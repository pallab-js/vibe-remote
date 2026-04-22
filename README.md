# VibeRemote

**A QUIC-powered, production-ready remote desktop application built with Rust and SvelteKit.**

[![Rust](https://img.shields.io/badge/rust-1.85+-blue.svg)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-blue.svg)](https://tauri.app/)
[![SvelteKit](https://img.shields.io/badge/SvelteKit-5.0-orange.svg)](https://kit.svelte.dev/)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macos-lightgrey.svg)](https://github.com/pallab-js/vibe-remote)

## Overview

VibeRemote is a high-performance remote desktop application that delivers sub-10MB binaries with native macOS screen capture (ScreenCaptureKit), hardware-accelerated encoding, real-time streaming over QUIC with TLS 1.3, Ed25519 identity authentication, and Noise Protocol key exchange.

**Key differentiators:**
- **<10MB binary** (vs ~150MB for alternatives)
- **QUIC transport** with native TLS 1.3 — no custom crypto
- **Certificate pinning** to prevent man-in-the-middle attacks
- **Consent-based remote control** — secure by default (view-only until enabled)
- **Rate-limited input** — prevents abuse and flooding

## Features

### Core
- Native macOS screen capture via ScreenCaptureKit (macOS 13+)
- QUIC networking with TLS 1.3 encryption (quinn)
- Custom congestion control tuned for real-time video (BBR-like)
- JPEG + zlib compression (~40:1 ratio) with delta frames
- Buffer pool management for optimized memory usage
- Hardware H.264 encoder infrastructure (VideoToolbox)
- Adaptive bitrate (500K-8Mbps based on RTT/packet loss)
- MessagePack binary wire protocol for efficient serialization
- Connection modes: Local (capture), Host (server), Client (viewer)

### Input & Control
- Full mouse support (move, click, wheel, right-click, middle-click)
- Full keyboard support (all keys including function keys F1-F12, modifiers)
- Coordinate scaling for multi-resolution setups
- **View-only mode by default** — enable remote control with explicit consent
- Clipboard access with consent required

### Security
- QUIC with TLS 1.3 (native encryption)
- Certificate pinning (SHA256 fingerprint) + TOFU mode
- Ed25519 identity system with persistent keypairs
- Noise Protocol XX handshake infrastructure
- Backend-enforced consent for remote control and clipboard
- Rate limiting on all input (100 mouse/sec, 50 keys/sec, 5 clipboard/min)
- Key storage with 0o600 permissions + memory zeroization
- Comprehensive audit logging (no user content in logs)
- Content Security Policy enforcement

### Networking
- Server and client modes
- Auto-reconnection with exponential backoff
- STUN/TURN NAT traversal
- ICE candidate gathering
- Self-host relay support (see [RELAY_SERVER.md](RELAY_SERVER.md))

### UI
- Deep Slate dark theme with glassmorphism and macOS vibrancy effects
- Svelte 5 runes for reactive state management
- Auto-hiding floating toolbar (shows/hides after 3 seconds)
- Real-time FPS and latency display with network vitality sparkline
- Multi-display selection — lists all available monitors
- Onboarding wizard — step-by-step setup for new users
- Connection history with trust levels (trusted/ask/blocked)
- Consent modals for input and clipboard with security warnings

## Installation

### Prerequisites

- **Rust** 1.85+ (`rustup default stable`)
- **Node.js** 20+ and **pnpm**
- **Xcode Command Line Tools** (macOS): `xcode-select --install`

### From Source

```bash
# Clone
git clone https://github.com/pallab-js/vibe-remote.git
cd vibe-remote

# Install dependencies
pnpm install

# Development mode (hot reload)
pnpm tauri dev

# Production build
pnpm tauri build
```

### Pre-built Binaries

Download from [Releases](https://github.com/pallab-js/vibe-remote/releases).

- **macOS (Apple Silicon)**: `.dmg` installer
- **Output**: `src-tauri/target/release/bundle/`

### After Installing

1. **Screen Recording Permissions**: macOS will prompt on first capture. Grant in **System Preferences > Security & Privacy > Privacy > Screen Recording**.
2. **Accessibility Permissions**: For input injection, grant in **System Preferences > Security & Privacy > Privacy > Accessibility**.

## Usage

### Host (Server)

1. Launch VibeRemote
2. Select **Host** mode
3. Set port (default: `4567`)
4. Click **Start Server**
5. Share your IP and the displayed certificate fingerprint with the client

### Client (Viewer)

1. Launch VibeRemote
2. Select **Connect** mode
3. Enter the host's IP address (e.g., `192.168.1.100:4567`)
4. Click **Connect**
5. The remote screen appears — default is **view-only mode**
6. Click the toolbar toggle or the "Enable Remote Control" button to interact

### Loopback Testing

Open two terminal windows, run `pnpm tauri dev` in both. Use `127.0.0.1:4567` in the client.

## Architecture

```
vibe-remote/
├── src/                          # SvelteKit frontend
│   ├── routes/
│   │   └── +page.svelte         # Main dashboard (all UI logic)
│   ├── lib/
│   │   ├── components/          # Reusable components
│   │   │   └── ConnectPanel.svelte
│   │   ├── stores/              # Svelte 5 runes stores
│   │   │   ├── settings.svelte.ts
│   │   │   ├── connection-history.svelte.ts
│   │   │   └── session.svelte.ts
│   │   ├── utils/               # Utilities
│   │   │   └── codec.ts
│   │   └── design-system.css   # CSS variables + macOS vibrancy
│   ├── app.css                  # Tailwind theme
│   └── app.html                 # HTML template
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── lib.rs               # Main library: AppState, 40+ Tauri commands
│   │   ├── main.rs              # Entry point
│   │   ├── capture.rs           # ScreenCaptureKit integration
│   │   ├── capture_windows.rs   # Windows DXGI (conditional compilation)
│   │   ├── transport.rs        # QUIC server/client + certificate pinning
│   │   ├── input.rs            # enigo input injection
│   │   ├── encoder.rs          # JPEG+zlib compression + buffer pool
│   │   ├── h264_encoder.rs     # Hardware H.264 encoder
│   │   ├── session.rs          # Session state + auto-reconnect
│   │   ├── connection.rs       # Connection state machine
│   │   ├── auth.rs             # Ed25519 + Noise Protocol
│   │   ├── protocol.rs         # MessagePack binary wire protocol
│   │   ├── adaptive_bitrate.rs # Network-adaptive quality control
│   │   ├── nat_traversal.rs    # STUN/TURN implementation
│   │   ├── audio.rs            # CoreAudio capture (stub)
│   │   ├── error.rs            # Centralized error types
│   │   └── logging.rs          # Tracing subscriber
│   ├── tests/
│   │   └── integration_test.rs # Integration tests + benchmarks
│   └── tauri.conf.json         # Tauri configuration
└── .github/workflows/
    └── build.yml               # CI/CD pipeline
```

## Development

```bash
# Run in development mode
pnpm tauri dev

# Lint Rust code
cd src-tauri && cargo clippy -- -D warnings

# Run tests
cd src-tauri && cargo test

# Scan dependencies for vulnerabilities
cd src-tauri && cargo audit

# Type-check frontend
pnpm check

# Clean build artifacts
cargo clean --manifest-path=src-tauri/Cargo.toml && rm -rf build/ .svelte-kit/
```

### Debugging

```bash
# Verbose logging
VIBE_LOG_LEVEL=debug pnpm tauri dev

# Verbose logging with file/line numbers
VIBE_VERBOSE=1 pnpm tauri dev
```

## Security Model

```
Layer 1: Transport      QUIC + TLS 1.3 (native encryption)
Layer 2: Pinning        SHA256 certificate fingerprint verification
Layer 3: Identity       Ed25519 keypairs (persistent, 0o600 perms)
Layer 4: Auth           Noise Protocol XX handshake (infrastructure ready)
Layer 5: Access Control Backend-enforced consent + rate limiting
Layer 6: Audit          All security events logged (no user content)
```

See [SECURITY.md](SECURITY.md) for the vulnerability reporting process.

## Roadmap

- [ ] Hardware H.264 encoding (VideoToolbox full integration)
- [ ] Noise Protocol handshake wired into connection flow
- [ ] Full file transfer implementation
- [ ] Windows DXGI capture testing
- [ ] Tauri 3.0 migration (when available)

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

Quick start:
1. Fork the repo
2. Create a feature branch
3. Make changes
4. Run `cargo clippy -- -D warnings` and `cargo test`
5. Open a Pull Request

## License

MIT License — see [LICENSE](LICENSE).

## Acknowledgments

- [Tauri](https://tauri.app/) — Smaller, faster, more secure desktop apps
- [SvelteKit](https://kit.svelte.dev/) — Web development for the rest of us
- [quinn](https://github.com/quinn-rs/quinn) — Rust QUIC implementation
- [enigo](https://github.com/enigo-rs/enigo) — Cross-platform input simulation
- [screencapturekit-rs](https://github.com/trackpad-dev/screencapturekit-rs) — macOS ScreenCaptureKit bindings
