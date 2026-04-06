//! NAT Traversal using STUN/TURN
//!
//! Provides peer discovery and hole-punching for internet-scale connections
//! when direct P2P is not possible due to NAT.

use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;
use rand::rngs::OsRng;
use rand::RngCore;
use tracing::{info, debug, warn, error};
use zeroize::Zeroize;

use crate::error::{VibeResult, VibeError};

/// TURN server configuration with secure credential storage
#[derive(Clone)]
pub struct TurnServer {
    pub address: String,
    pub port: u16,
    username: zeroize::Zeroizing<String>,
    password: zeroize::Zeroizing<String>,
}

impl TurnServer {
    pub fn new(address: String, port: u16, username: String, password: String) -> Self {
        Self {
            address,
            port,
            username: zeroize::Zeroizing::new(username),
            password: zeroize::Zeroizing::new(password),
        }
    }
    
    // Credentials are only accessible via borrow, preventing accidental copies
    pub fn username(&self) -> &str {
        &self.username
    }
    
    pub fn password(&self) -> &str {
        &self.password
    }
}

impl std::fmt::Debug for TurnServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnServer")
            .field("address", &self.address)
            .field("port", &self.port)
            .field("username", &"***REDACTED***")
            .field("password", &"***REDACTED***")
            .finish()
    }
}

/// STUN server configuration
#[derive(Clone, Debug)]
pub struct StunServer {
    pub address: String,
    pub port: u16,
}

impl Default for StunServer {
    fn default() -> Self {
        Self {
            address: "stun.l.google.com".to_string(),
            port: 19302,
        }
    }
}

/// TURN server configuration - REMOVED, use TurnServer::new() instead
// Old struct definition replaced by secure version above

/// NAT type detected
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NatType {
    /// No NAT (public IP)
    Open,
    /// Full-cone NAT
    FullCone,
    /// Restricted-cone NAT
    RestrictedCone,
    /// Port-restricted cone NAT
    PortRestrictedCone,
    /// Symmetric NAT (hardest to traverse)
    Symmetric,
    /// UDP blocked
    Blocked,
}

/// ICE candidate for peer exchange
#[derive(Clone, Debug)]
pub struct IceCandidate {
    pub foundation: String,
    pub component_id: u16,
    pub protocol: String,
    pub priority: u32,
    pub ip: String,
    pub port: u16,
    pub typ: String,
    pub related_address: Option<String>,
    pub related_port: Option<u16>,
}

/// NAT traversal manager
pub struct NatTraversal {
    stun_servers: Vec<StunServer>,
    turn_servers: Vec<TurnServer>,
    public_address: Option<SocketAddr>,
    nat_type: Option<NatType>,
}

impl NatTraversal {
    /// Create new NAT traversal manager
    pub fn new(
        stun_servers: Vec<StunServer>,
        turn_servers: Vec<TurnServer>,
    ) -> Self {
        Self {
            stun_servers,
            turn_servers,
            public_address: None,
            nat_type: None,
        }
    }

    /// Discover public IP address using STUN
    pub fn discover_public_address(&mut self) -> VibeResult<SocketAddr> {
        info!("Discovering public address via STUN...");

        for server in &self.stun_servers {
            match self.stun_request(&server.address, server.port) {
                Ok(addr) => {
                    info!("Public address discovered: {}", addr);
                    self.public_address = Some(addr);
                    return Ok(addr);
                }
                Err(e) => {
                    warn!("STUN server {}:{} failed: {}", server.address, server.port, e);
                }
            }
        }

        Err(VibeError::Connection(
            "All STUN servers failed".to_string()
        ))
    }

    /// Perform STUN binding request
    fn stun_request(&self, server: &str, port: u16) -> VibeResult<SocketAddr> {
        use stun::client::Client;
        use stun::codec::Message;
        use stun::message::{MessageBuilder, MessageClass, MessageType, BIND_REQUEST, MAPPED_ADDRESS, XOR_MAPPED_ADDRESS};

        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| VibeError::Connection(format!("Failed to bind UDP socket: {}", e)))?;
        
        socket.set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| VibeError::Connection(format!("Failed to set timeout: {}", e)))?;

        let server_addr = format!("{}:{}", server, port);
        socket.connect(&server_addr)
            .map_err(|e| VibeError::Connection(format!("Failed to connect to STUN: {}", e)))?;

        // Create STUN binding request with explicit CSPRNG
        use stun::message::{MessageBuilder, MessageClass, MessageType, BIND_REQUEST};
        
        let mut msg = Message::new();
        msg.message_type = MessageType {
            method: BIND_REQUEST,
            class: MessageClass::Request,
        };
        let mut transaction_id = [0u8; 12];
        OsRng.fill_bytes(&mut transaction_id);
        msg.transaction_id = transaction_id;

        // Encode and send
        let buf = msg.encode().unwrap();
        socket.send(&buf)
            .map_err(|e| VibeError::Connection(format!("STUN send failed: {}", e)))?;

        // Receive response
        let mut resp_buf = [0u8; 1024];
        let n = socket.recv(&mut resp_buf)
            .map_err(|e| VibeError::Connection(format!("STUN recv failed: {}", e)))?;

        // Decode response
        let resp = Message::decode(&resp_buf[..n])
            .map_err(|e| VibeError::Connection(format!("STUN decode failed: {}", e)))?;

        // Extract mapped address
        if let Some(attr) = resp.get_attr(XOR_MAPPED_ADDRESS) {
            // Parse XOR-mapped address
            let (ip, port) = parse_xor_mapped_address(&attr, &msg.transaction_id)
                .ok_or_else(|| VibeError::Connection("Failed to parse XOR address".to_string()))?;
            
            Ok(SocketAddr::new(ip, port))
        } else if let Some(attr) = resp.get_attr(MAPPED_ADDRESS) {
            // Parse regular mapped address
            let (ip, port) = parse_mapped_address(&attr)
                .ok_or_else(|| VibeError::Connection("Failed to parse mapped address".to_string()))?;
            
            Ok(SocketAddr::new(ip, port))
        } else {
            Err(VibeError::Connection("No address in STUN response".to_string()))
        }
    }

    /// Detect NAT type
    pub fn detect_nat_type(&mut self) -> VibeResult<NatType> {
        info!("Detecting NAT type...");

        // Test 1: Binding test
        let addr1 = match self.discover_public_address() {
            Ok(addr) => addr,
            Err(_) => return Ok(NatType::Blocked),
        };

        // Test 2: Change IP test (use different STUN server)
        let addr2 = self.stun_servers.iter()
            .skip(1)
            .find_map(|s| self.stun_request(&s.address, s.port).ok());

        let nat_type = match (addr1, addr2) {
            (a1, Some(a2)) if a1 == a2 => {
                // Same address = cone NAT
                NatType::FullCone // Simplified detection
            }
            (a1, Some(a2)) if a1 != a2 => {
                // Different address = symmetric NAT
                NatType::Symmetric
            }
            _ => NatType::RestrictedCone, // Default assumption
        };

        info!("NAT type detected: {:?}", nat_type);
        self.nat_type = Some(nat_type);
        Ok(nat_type)
    }

    /// Get gathered ICE candidates
    pub fn gather_candidates(&self) -> VibeResult<Vec<IceCandidate>> {
        let mut candidates = Vec::new();

        // Host candidate (local address)
        if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
            if let Ok(local_addr) = socket.local_addr() {
                candidates.push(IceCandidate {
                    foundation: "1".to_string(),
                    component_id: 1,
                    protocol: "udp".to_string(),
                    priority: 2130706431,
                    ip: local_addr.ip().to_string(),
                    port: local_addr.port(),
                    typ: "host".to_string(),
                    related_address: None,
                    related_port: None,
                });
            }
        }

        // Server-reflexive candidate (public address)
        if let Some(public_addr) = self.public_address {
            candidates.push(IceCandidate {
                foundation: "2".to_string(),
                component_id: 1,
                protocol: "udp".to_string(),
                priority: 1694498815,
                ip: public_addr.ip().to_string(),
                port: public_addr.port(),
                typ: "srflx".to_string(),
                related_address: None,
                related_port: None,
            });
        }

        // Relay candidate (TURN) - if available
        for (i, turn) in self.turn_servers.iter().enumerate() {
            candidates.push(IceCandidate {
                foundation: format!("{}", 3 + i),
                component_id: 1,
                protocol: "udp".to_string(),
                priority: 16777215,
                ip: turn.address.clone(),
                port: turn.port,
                typ: "relay".to_string(),
                related_address: Some(turn.address.clone()),
                related_port: Some(turn.port),
            });
        }

        debug!("Gathered {} ICE candidates", candidates.len());
        Ok(candidates)
    }

    /// Get public address
    pub fn public_address(&self) -> Option<SocketAddr> {
        self.public_address
    }

    /// Get NAT type
    pub fn nat_type(&self) -> Option<NatType> {
        self.nat_type
    }
}

/// Parse XOR-mapped address attribute
fn parse_xor_mapped_address(data: &[u8], transaction_id: &[u8; 12]) -> Option<(std::net::IpAddr, u16)> {
    if data.len() < 8 {
        return None;
    }

    let family = u16::from_be_bytes([data[0], data[1]]);
    let port_xor = u16::from_be_bytes([data[2], data[3]]);
    let transaction_id_magic: [u8; 4] = [0x21, 0x12, 0xA4, 0x42];

    // XOR port with magic cookie
    let magic_cookie = u32::from_be_bytes(transaction_id_magic);
    let port = (port_xor as u32 ^ (magic_cookie >> 16)) as u16;

    if family == 0x01 {
        // IPv4
        if data.len() < 8 {
            return None;
        }
        let mut ip_bytes = [0u8; 4];
        for i in 0..4 {
            ip_bytes[i] = data[4 + i] ^ transaction_id_magic[i % 4];
        }
        Some((std::net::IpAddr::V4(std::net::Ipv4Addr::from(ip_bytes)), port))
    } else {
        None // IPv6 not supported yet
    }
}

/// Parse regular mapped address attribute
fn parse_mapped_address(data: &[u8]) -> Option<(std::net::IpAddr, u16)> {
    if data.len() < 8 {
        return None;
    }

    let family = u16::from_be_bytes([data[0], data[1]]);
    let port = u16::from_be_bytes([data[2], data[3]]);

    if family == 0x01 {
        // IPv4
        let mut ip_bytes = [0u8; 4];
        ip_bytes.copy_from_slice(&data[4..8]);
        Some((std::net::IpAddr::V4(std::net::Ipv4Addr::from(ip_bytes)), port))
    } else {
        None
    }
}

/// Create default NAT traversal with Google STUN servers
pub fn default_nat_traversal() -> NatTraversal {
    NatTraversal::new(
        vec![
            StunServer {
                address: "stun.l.google.com".to_string(),
                port: 19302,
            },
            StunServer {
                address: "stun1.l.google.com".to_string(),
                port: 19302,
            },
            StunServer {
                address: "stun2.l.google.com".to_string(),
                port: 19302,
            },
        ],
        vec![], // No TURN servers by default - configure via TurnServer::new()
    )
}
