# VibeRemote Self-Hosted Relay Server Guide

This guide explains how to run your own VibeRemote signaling/relay server for internet-scale remote desktop connections.

## Architecture

VibeRemote uses **QUIC peer-to-peer** connections for the actual remote desktop streaming. However, when clients are behind NAT (which is most cases), a relay server is needed to:

1. **Introduce peers** - Exchange connection information between host and client
2. **Relay traffic** - Forward QUIC packets when direct P2P isn't possible
3. **STUN/TURN services** - Help discover public IP addresses for NAT traversal

## Option 1: Direct P2P (No Relay - Same Network Only)

If both machines are on the same local network, no relay is needed:

**Host:**
1. Find your local IP: `ifconfig | grep "inet "` (macOS) or `ip addr` (Linux)
2. Start VibeRemote in Host mode on port `4567`
3. Share your IP with the client

**Client:**
1. Connect to `HOST_IP:4567`

**Limitation:** This only works on the same network. Internet connections require NAT traversal.

## Option 2: Run a STUN Server

STUN helps clients discover their public IP address for hole-punching.

### Using coturn (Recommended)

```bash
# Install coturn
sudo apt-get install coturn  # Ubuntu/Debian
brew install coturn          # macOS

# Configure /etc/turnserver.conf
listening-port=3478
external-ip=YOUR_PUBLIC_IP

# Start
sudo systemctl start coturn
```

### VibeRemote Configuration

Currently, VibeRemote uses direct QUIC connections. STUN integration is planned for a future release.

## Option 3: Run a TURN Relay Server

When NAT hole-punching fails, TURN relays all traffic through a server.

### Using coturn as TURN

```bash
# /etc/turnserver.conf
listening-port=3478
tls-listening-port=5349
external-ip=YOUR_PUBLIC_IP

# User credentials
user=vibeuser:vibepass

# Realm
realm=vibe-remote.example.com

# Start
sudo systemctl enable coturn
sudo systemctl start coturn
```

**Firewall Rules:**
```bash
sudo ufw allow 3478/tcp   # STUN
sudo ufw allow 3478/udp   # STUN
sudo ufw allow 5349/tcp   # TURNS
sudo ufw allow 5349/udp   # TURNS
sudo ufw allow 49152:65535/udp  # Relay ports
```

## Option 4: Run a Signaling Server (Axum-based)

For production internet-scale deployments, you need a signaling server to coordinate connections.

### Prerequisites

```bash
cargo install cargo-generate
```

### Create Signaling Server

```bash
# Create new project
mkdir vibe-relay && cd vibe-relay
cargo init
```

### Add Dependencies

```toml
# Cargo.tom
[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
```

### Minimal Signaling Server

```rust
// src/main.rs
use axum::{
    extract::WebSocketUpgrade,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{info, error};

type RoomMap = Arc<Mutex<HashMap<String, Vec<String>>>>;

#[derive(Deserialize, Serialize, Debug)]
enum SignalMessage {
    Join { room: String, peer_id: String },
    Offer { room: String, sdp: String },
    Answer { room: String, sdp: String },
    IceCandidate { room: String, candidate: String },
}

#[derive(Deserialize)]
struct CreateRoom {
    room_id: String,
}

async fn create_room(
    rooms: axum::extract::State<RoomMap>,
    Json(payload): Json<CreateRoom>,
) -> impl IntoResponse {
    let mut map = rooms.lock().unwrap();
    map.entry(payload.room_id.clone()).or_insert_with(Vec::new);
    info!("Room created: {}", payload.room_id);
    Json(serde_json::json!({"room": payload.room_id}))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    let rooms: RoomMap = Arc::new(Mutex::new(HashMap::new()));
    
    let app = Router::new()
        .route("/room", axum::routing::post(create_room))
        .with_state(rooms);
    
    let addr = "0.0.0.0:3000";
    info!("Signaling server listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### Run the Server

```bash
RUST_LOG=info cargo run --release
```

### Deploy with Docker

```dockerfile
# Dockerfile
FROM rust:1.85 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/vibe-relay /usr/local/bin/
EXPOSE 3000
CMD ["vibe-relay"]
```

```bash
docker build -t vibe-relay .
docker run -d -p 3000:3000 --name vibe-relay vibe-relay
```

## Option 5: Use a Public TURN Service

If you don't want to self-host:

| Service | Free Tier | Pricing |
|---------|-----------|---------|
| Twilio Network Traversal | 500 mins/month | $0.004/min after |
| Xirsys | 5GB/month | Paid plans from $35/mo |
| Metered.ca | 100 connections | Paid plans available |

## Testing Your Setup

### Test Direct Connection

```bash
# On host machine
ping CLIENT_PUBLIC_IP

# If ping works, direct QUIC should work too
```

### Test Through Relay

1. Configure VibeRemote to use your TURN server
2. Test from different networks (e.g., mobile hotspot)
3. Verify traffic flows through relay when P2P fails

## Production Recommendations

1. **Use HTTPS/WSS** for signaling server
2. **Rate limit** connection requests to prevent abuse
3. **Authenticate** users before allowing room creation
4. **Monitor** bandwidth usage on TURN relay
5. **Use a CDN** for the VibeRemote app itself
6. **Set up monitoring** with Prometheus + Grafana

## Troubleshooting

### "Connection refused" error
- Check firewall allows port 4567
- Verify the host is actually running and listening

### "Connection timed out"
- NAT traversal is failing
- Try setting up a TURN server
- Check that UDP is not blocked

### "Relay traffic only"
- P2P hole-punching failed
- Verify STUN server is reachable
- Check for symmetric NAT (hardest to traverse)

## Cost Estimates

| Setup | Monthly Cost | Bandwidth | Latency |
|-------|-------------|-----------|---------|
| Direct P2P | $0 | Unlimited | Lowest |
| STUN only | $0 (self-hosted) | Unlimited | Low |
| TURN relay (self-hosted) | Server cost (~$5-20) | Depends on usage | Medium |
| TURN relay (Twilio) | $0.004/min | Metered | Medium |

---

**For support, open an issue on the VibeRemote GitHub repository.**
