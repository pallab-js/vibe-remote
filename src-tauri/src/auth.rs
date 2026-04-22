//! Authentication module using Ed25519 + Noise Protocol
//!
//! Provides zero-knowledge identity verification for VibeRemote connections.
//! Each peer has an Ed25519 keypair for identity, and uses the Noise Protocol
//! for authenticated key exchange.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::Signer;
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use snow::HandshakeState;
use std::fs;
use std::path::Path;

use crate::error::{VibeError, VibeResult};

/// VibeRemote peer identity
#[derive(Clone, Debug)]
pub struct PeerIdentity {
    /// Ed25519 signing keypair
    signing_key: SigningKey,
    /// Ed25519 verifying key (public)
    verifying_key: VerifyingKey,
    /// Human-readable name
    name: String,
}

/// Public identity info (shared with peers)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicIdentity {
    /// Base64-encoded verifying key
    pub verifying_key_b64: String,
    /// Human-readable name
    pub name: String,
}

impl PublicIdentity {
    /// Get verifying key bytes
    pub fn verifying_key_bytes(&self) -> Vec<u8> {
        base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &self.verifying_key_b64,
        )
        .unwrap_or_default()
    }
}

impl PeerIdentity {
    /// Generate a new random identity
    pub fn generate(name: String) -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        Self {
            signing_key,
            verifying_key,
            name,
        }
    }

    /// Load identity from file, or generate if not exists
    pub fn load_or_generate(path: &Path, name: String) -> VibeResult<Self> {
        if path.exists() {
            Self::load(path)
        } else {
            let identity = Self::generate(name);
            identity.save(path)?;
            Ok(identity)
        }
    }

    /// Load identity from file
    pub fn load(path: &Path) -> VibeResult<Self> {
        const KEY_SIZE: usize = 32; // Ed25519 secret key is 32 bytes

        let data = fs::read(path)
            .map_err(|e| VibeError::Config(format!("Failed to read identity file: {}", e)))?;

        // Decode signing key from file
        let signing_key = SigningKey::from_bytes(
            &data[..KEY_SIZE]
                .try_into()
                .map_err(|_| VibeError::Config("Invalid key data".to_string()))?,
        );
        let verifying_key = signing_key.verifying_key();

        // Read name from rest of file
        let name = String::from_utf8_lossy(&data[KEY_SIZE..])
            .trim()
            .to_string();

        Ok(Self {
            signing_key,
            verifying_key,
            name,
        })
    }

    /// Save identity to file with restricted permissions
    pub fn save(&self, path: &Path) -> VibeResult<()> {
        use zeroize::Zeroize;

        let mut data = self.signing_key.to_bytes().to_vec();
        data.extend_from_slice(self.name.as_bytes());

        // Write file with restricted permissions (owner-only read/write)
        #[cfg(unix)]
        {
            use std::fs::File;
            use std::os::unix::fs::OpenOptionsExt;

            let file = File::options()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600) // Owner read/write only
                .open(path)
                .map_err(|e| VibeError::Config(format!("Failed to create key file: {}", e)))?;

            use std::io::Write;
            let mut writer = std::io::BufWriter::new(file);
            writer
                .write_all(&data)
                .map_err(|e| VibeError::Config(format!("Failed to write key: {}", e)))?;
        }

        #[cfg(not(unix))]
        {
            fs::write(path, &data)
                .map_err(|e| VibeError::Config(format!("Failed to save identity: {}", e)))?;
        }

        // Clear key material from memory
        data.zeroize();

        Ok(())
    }

    /// Get public identity
    pub fn public_identity(&self) -> PublicIdentity {
        PublicIdentity {
            verifying_key_b64: BASE64.encode(self.verifying_key.to_bytes()),
            name: self.name.clone(),
        }
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    /// Verify a signature
    pub fn verify_signature(
        verifying_key_b64: &str,
        message: &[u8],
        signature_b64: &str,
    ) -> VibeResult<()> {
        let key_bytes = BASE64
            .decode(verifying_key_b64)
            .map_err(|e| VibeError::Config(format!("Invalid key encoding: {}", e)))?;

        let verifying_key = VerifyingKey::from_bytes(
            &key_bytes[..32]
                .try_into()
                .map_err(|_| VibeError::Config("Invalid key length".to_string()))?,
        )
        .map_err(|e| VibeError::Config(format!("Invalid verifying key: {}", e)))?;

        let sig_bytes = BASE64
            .decode(signature_b64)
            .map_err(|e| VibeError::Config(format!("Invalid signature encoding: {}", e)))?;

        let signature = Signature::from_bytes(
            &sig_bytes[..64]
                .try_into()
                .map_err(|_| VibeError::Config("Invalid signature length".to_string()))?,
        );

        verifying_key
            .verify_strict(message, &signature)
            .map_err(|e| VibeError::Config(format!("Signature verification failed: {}", e)))?;

        Ok(())
    }

    /// Get verifying key as base64
    pub fn verifying_key_b64(&self) -> String {
        BASE64.encode(self.verifying_key.to_bytes())
    }

    /// Get name
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Noise Protocol handshake manager
/// NOTE: QUIC already provides TLS 1.3 encryption. This is provided for future
/// application-layer authentication if desired. Currently unused.
#[allow(dead_code)]
pub struct NoiseHandshake {
    /// Noise handshake state
    state: HandshakeState,
    /// Our identity
    identity: PeerIdentity,
}

impl NoiseHandshake {
    /// Create new noise handshake for initiator (client)
    pub fn new_initiator(identity: PeerIdentity, remote_public_key_b64: &str) -> VibeResult<Self> {
        // Parse remote public key
        let remote_key_bytes = BASE64
            .decode(remote_public_key_b64)
            .map_err(|e| VibeError::Config(format!("Invalid remote key: {}", e)))?;

        let pub_identity = identity.public_identity();
        let key_bytes = pub_identity.verifying_key_bytes();

        // Noise protocol pattern: XX handshake (mutual auth)
        let builder = snow::Builder::new(
            "Noise_XX_25519_ChaChaPoly_BLAKE2s"
                .parse()
                .map_err(|e| VibeError::Config(format!("Invalid noise pattern: {}", e)))?,
        )
        .prologue(&key_bytes)
        .prologue(&remote_key_bytes);

        let state = builder
            .build_initiator()
            .map_err(|e| VibeError::Config(format!("Failed to build initiator: {}", e)))?;

        Ok(Self { state, identity })
    }

    /// Create new noise handshake for responder (server)
    pub fn new_responder(identity: PeerIdentity) -> VibeResult<Self> {
        let pub_identity = identity.public_identity();
        let key_bytes = pub_identity.verifying_key_bytes();

        let builder = snow::Builder::new(
            "Noise_XX_25519_ChaChaPoly_BLAKE2s"
                .parse()
                .map_err(|e| VibeError::Config(format!("Invalid noise pattern: {}", e)))?,
        )
        .prologue(&key_bytes);

        let state = builder
            .build_responder()
            .map_err(|e| VibeError::Config(format!("Failed to build responder: {}", e)))?;

        Ok(Self { state, identity })
    }

    /// Write handshake message
    pub fn write_message(&mut self, payload: &[u8], out: &mut Vec<u8>) -> VibeResult<bool> {
        out.clear();
        out.reserve(payload.len() + 100);
        out.extend_from_slice(payload);

        let written = self
            .state
            .write_message(payload, out)
            .map_err(|e| VibeError::Config(format!("Handshake write failed: {}", e)))?;

        out.truncate(written);

        // Check if handshake is complete
        Ok(self.state.is_handshake_finished())
    }

    /// Read handshake message
    pub fn read_message(&mut self, message: &[u8], out: &mut Vec<u8>) -> VibeResult<bool> {
        out.clear();

        let written = self
            .state
            .read_message(message, out)
            .map_err(|e| VibeError::Config(format!("Handshake read failed: {}", e)))?;

        out.truncate(written);

        Ok(self.state.is_handshake_finished())
    }

    /// Get handshake completion status
    pub fn is_finished(&self) -> bool {
        self.state.is_handshake_finished()
    }
}

/// Authentication result
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResult {
    pub success: bool,
    pub peer_name: String,
    pub peer_key_b64: String,
    pub error: Option<String>,
}

/// Generate a new peer identity and save it
pub fn generate_identity(name: String, key_path: &Path) -> VibeResult<PublicIdentity> {
    let identity = PeerIdentity::load_or_generate(key_path, name)?;
    let public = identity.public_identity();
    Ok(public)
}

/// Load existing identity
pub fn load_identity(key_path: &Path) -> VibeResult<PublicIdentity> {
    let identity = PeerIdentity::load(key_path)?;
    Ok(identity.public_identity())
}
