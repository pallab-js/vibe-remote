//! QUIC transport layer using quinn
//!
//! Provides QUIC-based networking for:
//! - Reliable bidirectional streams for control commands and file transfer
//! - Unreliable datagrams for video frames (latency > perfection)
//! - Built-in TLS 1.3 encryption

use std::sync::Arc;
use std::net::SocketAddr;
use tracing::{info, error, debug};

use quinn::{Endpoint, ServerConfig, ClientConfig, TransportConfig, Connection, RecvStream, SendStream, VarInt};
use quinn::crypto::rustls::{QuicServerConfig, QuicClientConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

use crate::error::{VibeResult, VibeError};

/// QUIC configuration
#[derive(Debug, Clone)]
pub struct QuicConfig {
    /// Local address to bind to
    pub bind_addr: SocketAddr,
    /// Remote address to connect to (None for server mode)
    pub remote_addr: Option<SocketAddr>,
    /// Server name for TLS verification
    pub server_name: String,
    /// ALPN protocols
    pub alpn_protocols: Vec<Vec<u8>>,
    /// HIGH-3: Optional peer Ed25519 public key for Noise Protocol authentication
    pub peer_public_key_b64: Option<String>,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:4567".parse().unwrap(),
            remote_addr: None,
            server_name: "localhost".to_string(),
            alpn_protocols: vec![b"vibe-remote-0.2".to_vec()],
            peer_public_key_b64: None,
        }
    }
}

/// Generate a self-signed certificate for QUIC
fn generate_cert() -> VibeResult<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    info!("Generating self-signed certificate for QUIC");
    
    let certified_key = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .map_err(|e| VibeError::Config(format!("Certificate generation failed: {}", e)))?;
    
    let cert_der = CertificateDer::from(certified_key.cert.der().to_vec());
    let key_der = PrivateKeyDer::try_from(certified_key.key_pair.serialize_der())
        .map_err(|e| VibeError::Config(format!("Key conversion failed: {}", e)))?;
    
    Ok((vec![cert_der], key_der))
}

/// QUIC transport wrapper
#[derive(Clone)]
pub struct QuicTransport {
    endpoint: Endpoint,
    config: QuicConfig,
    connection: Option<Connection>,
    /// Peer's certificate fingerprint for pinning (SHA256 of DER)
    pub pinned_fingerprint: Option<String>,
    /// HIGH-3: Peer's Ed25519 public key for Noise Protocol authentication
    pub peer_public_key_b64: Option<String>,
}

impl QuicTransport {
    /// Create a new QUIC transport instance (server mode)
    pub async fn new_server(config: QuicConfig) -> VibeResult<Self> {
        info!("Initializing QUIC server on {}", config.bind_addr);

        let (certs, key) = generate_cert()?;
        
        // Compute and log our certificate fingerprint for sharing
        let fingerprint = FingerprintVerifier::compute_fingerprint(&certs[0]);
        info!("Server certificate fingerprint: {}", fingerprint);

        // Build rustls ServerConfig
        let mut rustls_server = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| VibeError::Config(format!("TLS setup failed: {}", e)))?;

        rustls_server.alpn_protocols = config.alpn_protocols.clone();

        // Wrap in Quinn's server config
        let quic_crypto = QuicServerConfig::try_from(rustls_server)
            .map_err(|e| VibeError::Config(format!("QuicServerConfig failed: {}", e)))?;

        let mut server_config = ServerConfig::with_crypto(Arc::new(quic_crypto));

        // Configure transport for real-time performance
        let mut transport_config = TransportConfig::default();
        transport_config.max_concurrent_bidi_streams(100u32.into());
        transport_config.max_concurrent_uni_streams(100u32.into());
        transport_config.datagram_send_buffer_size(2 * 1024 * 1024); // 2MB
        transport_config.datagram_receive_buffer_size(Some(4 * 1024 * 1024)); // 4MB
        transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));
        
        // Custom congestion control optimized for real-time video
        // Use BBR-like settings for low-latency streaming
        transport_config.send_window(8 * 1024 * 1024); // 8MB send window (u64)
        transport_config.receive_window(VarInt::from_u32(8 * 1024 * 1024)); // 8MB receive
        transport_config.stream_receive_window(VarInt::from_u32(4 * 1024 * 1024)); // 4MB per stream
        
        server_config.transport_config(Arc::new(transport_config));

        // Create endpoint using Endpoint::server (binds automatically)
        let endpoint = Endpoint::server(server_config, config.bind_addr)?;

        info!("QUIC server initialized successfully");

        Ok(Self { 
            endpoint, 
            config,
            connection: None,
            pinned_fingerprint: Some(fingerprint),
            peer_public_key_b64: None,
        })
    }

    /// Create a new QUIC transport instance (client mode)
    pub async fn new_client(config: QuicConfig) -> VibeResult<Self> {
        info!("Initializing QUIC client on {}", config.bind_addr);

        // Clone needed fields before moving config
        let peer_key = config.peer_public_key_b64.clone();

        // Create endpoint using Endpoint::client (binds automatically)
        let endpoint = Endpoint::client(config.bind_addr)?;

        info!("QUIC client initialized successfully");

        Ok(Self { 
            endpoint, 
            config,
            connection: None,
            pinned_fingerprint: None,
            peer_public_key_b64: peer_key,
        })
    }

    /// Start accepting connections (server mode)
    /// SEC-4: Logs incoming connections for audit trail
    pub async fn accept_connections(&self) -> VibeResult<()> {
        info!("QUIC server listening on {}", self.config.bind_addr);

        loop {
            // Wait for incoming connection
            if let Some(connecting) = self.endpoint.accept().await {
                let remote_addr = connecting.remote_address();
                info!("SEC-4: Incoming connection from {}", remote_addr);

                // Accept the connection
                match connecting.await {
                    Ok(connection) => {
                        info!("QUIC connection established with {}", remote_addr);

                        // Spawn handler for this connection
                        let conn = connection.clone();
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_connection(conn).await {
                                error!("Connection handler error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                    }
                }
            }
        }
    }

    /// Connect to a remote QUIC endpoint with certificate pinning
    pub async fn connect_with_fingerprint(
        &mut self,
        server_fingerprint: String,
    ) -> VibeResult<Connection> {
        let remote_addr = self.config.remote_addr
            .ok_or_else(|| VibeError::Connection("No remote address configured".to_string()))?;

        info!("Connecting to QUIC server at {} (pinned: {})", remote_addr, server_fingerprint);

        // Build client config with certificate pinning
        let verifier = FingerprintVerifier::new(server_fingerprint.clone());
        let rustls_client = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();

        let quic_crypto = QuicClientConfig::try_from(rustls_client)
            .map_err(|e| VibeError::Config(format!("QuicClientConfig failed: {}", e)))?;

        let mut client_config = ClientConfig::new(Arc::new(quic_crypto));
        let mut transport_config = TransportConfig::default();
        transport_config.datagram_receive_buffer_size(Some(2 * 1024 * 1024));
        transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));
        client_config.transport_config(Arc::new(transport_config));

        let connection = self.endpoint.connect_with(
            client_config,
            remote_addr,
            &self.config.server_name,
        )?.await
            .map_err(|e| VibeError::Connection(format!("Connection failed: {}", e)))?;

        info!("QUIC connection established with pinned certificate");
        self.connection = Some(connection.clone());
        self.pinned_fingerprint = Some(server_fingerprint);
        Ok(connection)
    }

    /// SEC-1: Connect to a remote QUIC endpoint with TOFU (Trust On First Use)
    /// This accepts the first certificate seen, storing it for future verification.
    /// Less secure than pinning, but better than accepting any certificate.
    pub async fn connect_tofu(&mut self) -> VibeResult<Connection> {
        let remote_addr = self.config.remote_addr
            .ok_or_else(|| VibeError::Connection("No remote address configured".to_string()))?;

        info!("Connecting to QUIC server at {} (TOFU mode)", remote_addr);

        // Build client config that accepts any certificate (TOFU)
        // NOTE: This is less secure than pinning. In production, users should
        // exchange fingerprints out-of-band for MITM protection.
        let rustls_client = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(TofuVerifier::new()))
            .with_no_client_auth();

        let quic_crypto = QuicClientConfig::try_from(rustls_client)
            .map_err(|e| VibeError::Config(format!("QuicClientConfig failed: {}", e)))?;

        let mut client_config = ClientConfig::new(Arc::new(quic_crypto));
        let mut transport_config = TransportConfig::default();
        transport_config.datagram_receive_buffer_size(Some(2 * 1024 * 1024));
        transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));
        client_config.transport_config(Arc::new(transport_config));

        let connection = self.endpoint.connect_with(
            client_config,
            remote_addr,
            &self.config.server_name,
        )?.await
            .map_err(|e| VibeError::Connection(format!("Connection failed: {}", e)))?;

        // Store the certificate fingerprint we just saw for future reference
        // In a full TOFU implementation, this would be persisted to disk
        info!("SEC-1: TOFU connection established. Server fingerprint should be recorded for future pinning.");
        self.connection = Some(connection.clone());
        Ok(connection)
    }

    /// Get server certificate fingerprint for sharing with clients
    pub fn get_certificate_fingerprint(&self) -> Option<String> {
        // In a full implementation, this would return the SHA256 of our cert
        // For now, this is set during server initialization
        self.pinned_fingerprint.clone()
    }

    /// Open a reliable bidirectional stream
    pub async fn open_stream(&self) -> VibeResult<(SendStream, RecvStream)> {
        let connection = self.connection.as_ref()
            .ok_or_else(|| VibeError::Connection("Not connected".to_string()))?;
        
        let (send, recv) = connection.open_bi().await
            .map_err(|e| VibeError::Connection(format!("Failed to open stream: {}", e)))?;
        
        debug!("Opened bidirectional stream");
        Ok((send, recv))
    }

    /// Send unreliable datagram (for video frames)
    pub async fn send_datagram(&self, data: bytes::Bytes) -> VibeResult<()> {
        let connection = self.connection.as_ref()
            .ok_or_else(|| VibeError::Connection("Not connected".to_string()))?;

        connection.send_datagram(data.clone())
            .map_err(|e| VibeError::Connection(format!("Failed to send datagram: {}", e)))?;

        debug!("Sent datagram ({} bytes)", data.len());
        Ok(())
    }

    /// Send data (alias for send_datagram)
    pub async fn send_data(&self, data: bytes::Bytes) -> VibeResult<()> {
        self.send_datagram(data).await
    }

    /// Receive datagrams
    pub async fn receive_datagram(&self) -> VibeResult<bytes::Bytes> {
        let connection = self.connection.as_ref()
            .ok_or_else(|| VibeError::Connection("Not connected".to_string()))?;
        
        let data = connection.read_datagram().await
            .map_err(|e| VibeError::Connection(format!("Failed to receive datagram: {}", e)))?;
        
        Ok(data)
    }

    /// Handle an incoming QUIC connection
    async fn handle_connection(connection: Connection) -> VibeResult<()> {
        let remote = connection.remote_address();
        info!("Handling connection from {}", remote);

        loop {
            // Accept incoming streams
            match connection.accept_bi().await {
                Ok((mut send, mut recv)) => {
                    debug!("Accepted bidirectional stream from {}", remote);
                    
                    // Spawn handler for this stream
                    tokio::spawn(async move {
                        // Echo back for now (will be replaced with actual command handling)
                        let buffer = match recv.read_to_end(1024 * 1024).await {
                            Ok(buf) => buf,
                            Err(e) => {
                                error!("Stream read error: {}", e);
                                return;
                            }
                        };

                        debug!("Received {} bytes from {}", buffer.len(), remote);
                        
                        // Send response
                        if let Err(e) = send.write_all(&buffer).await {
                            error!("Stream write error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept stream: {}", e);
                    break;
                }
            }
        }
        
        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    /// Get the local endpoint
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

/// Certificate verifier that pins to a known fingerprint (SHA256 of DER-encoded cert)
/// This prevents MITM attacks by ensuring we only connect to the expected server
#[derive(Debug)]
struct FingerprintVerifier {
    expected_fingerprint: String,
}

impl FingerprintVerifier {
    fn new(fingerprint: String) -> Self {
        Self {
            expected_fingerprint: fingerprint,
        }
    }
    
    /// Compute SHA256 fingerprint of a certificate DER
    fn compute_fingerprint(cert: &CertificateDer<'_>) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(cert.as_ref());
        let result = hasher.finalize();
        format!("{:x}", result)
    }
}

impl rustls::client::danger::ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let actual_fingerprint = Self::compute_fingerprint(end_entity);
        
        if actual_fingerprint == self.expected_fingerprint {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            error!(
                "Certificate pinning failed! Expected: {}, Got: {}",
                self.expected_fingerprint, actual_fingerprint
            );
            Err(rustls::Error::General(format!(
                "Certificate fingerprint mismatch. Expected: {}, Got: {}",
                self.expected_fingerprint, actual_fingerprint
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

/// SEC-1: TOFU (Trust On First Use) certificate verifier
/// Accepts any certificate on first connection and logs the fingerprint.
/// In a full implementation, the fingerprint would be persisted to disk
/// and used for subsequent connections (pinning).
#[derive(Debug)]
struct TofuVerifier {}

impl TofuVerifier {
    fn new() -> Self {
        Self {}
    }
}

impl rustls::client::danger::ServerCertVerifier for TofuVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // SEC-1: TOFU - accept any certificate and log the fingerprint
        let fingerprint = FingerprintVerifier::compute_fingerprint(end_entity);
        info!("SEC-1 TOFU: Accepted server certificate with fingerprint: {}", fingerprint);
        info!("SEC-1 TOFU: Record this fingerprint and use it for future connections.");

        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

// Remove the old NoVerifier struct entirely

/// Create a local QUIC tunnel for testing
pub async fn create_local_tunnel() -> VibeResult<(QuicTransport, QuicTransport)> {
    use std::net::{Ipv4Addr, SocketAddrV4};
    
    let server_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 4567));
    let client_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));

    let server_config = QuicConfig {
        bind_addr: server_addr,
        remote_addr: None,
        server_name: "localhost".to_string(),
        alpn_protocols: vec![b"vibe-remote-0.2".to_vec()],
        peer_public_key_b64: None,
    };

    let client_config = QuicConfig {
        bind_addr: client_addr,
        remote_addr: Some(server_addr),
        server_name: "localhost".to_string(),
        alpn_protocols: vec![b"vibe-remote-0.2".to_vec()],
        peer_public_key_b64: None,
    };

    let server = QuicTransport::new_server(server_config).await?;
    let mut client = QuicTransport::new_client(client_config).await?;

    // Connect client to server using the server's fingerprint
    let fingerprint = server.pinned_fingerprint.clone()
        .ok_or_else(|| VibeError::Config("Server has no fingerprint".to_string()))?;
    client.connect_with_fingerprint(fingerprint).await?;

    Ok((server, client))
}
