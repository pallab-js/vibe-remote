# VibeRemote

**A QUIC-powered remote desktop application built with Rust and SvelteKit.**

[![Rust](https://img.shields.io/badge/rust-1.85+-blue.svg)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-blue.svg)](https://tauri.app/)
[![SvelteKit](https://img.shields.io/badge/SvelteKit-5.0-orange.svg)](https://kit.svelte.dev/)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macos-lightgrey.svg)](https://github.com/pallab-js/vibe-remote)

## Overview

VibeRemote is a remote desktop application with native macOS screen capture (ScreenCaptureKit), JPEG compression, real-time streaming over QUIC with TLS 1.3, and Ed25519 identity keys.

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
- JPEG + zlib compression with buffer pool management
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
- Backend-enforced consent for remote control and clipboard
- Rate limiting on all input (100 mouse/sec, 50 keys/sec, 5 clipboard/min)
- Key storage with 0o600 permissions + memory zeroization
- Metadata-only audit logging (no user content in logs)
- Content Security Policy enforcement

### Networking
- Server and client modes
- Client-side reconnection events with error detection
- Self-host relay support (see [RELAY_SERVER.md](RELAY_SERVER.md))

### UI
- Deep Slate dark theme with glassmorphism and macOS vibrancy effects
- Svelte 5 runes for reactive state management
- Auto-hiding floating toolbar (shows/hides after 3 seconds)
- Multi-display selection — lists all available monitors
- Onboarding wizard — step-by-step setup for new users
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
5. Share your IP address with the client (see Known Limitations: the certificate fingerprint is not yet shown in the UI)

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
│   │   └── design-system.css    # CSS variables + macOS vibrancy
│   ├── app.css                  # Tailwind theme
│   └── app.html                 # HTML template
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── lib.rs               # Main library: AppState, Tauri commands
│   │   ├── main.rs              # Entry point
│   │   ├── capture.rs           # ScreenCaptureKit integration
│   │   ├── transport.rs         # QUIC server/client + certificate pinning
│   │   ├── input.rs             # enigo input injection
│   │   ├── encoder.rs           # JPEG+zlib compression + buffer pool
│   │   ├── session.rs           # Session state + frame streaming
│   │   ├── auth.rs              # Ed25519 identity
│   │   ├── protocol.rs          # MessagePack wire types (not yet wired)
│   │   ├── error.rs             # Centralized error types
│   │   └── logging.rs           # Tracing subscriber
│   ├── tests/
│   │   └── integration_test.rs  # Integration tests + benchmarks
│   └── tauri.conf.json          # Tauri configuration
└── .github/workflows/
    └── build.yml               # CI/CD pipeline (macOS)
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
Layer 4: Access Control Backend-enforced consent + rate limiting
Layer 5: Audit          Security events logged (no user content)
```

See [SECURITY.md](SECURITY.md) for the vulnerability reporting process.

## Known Limitations

Honest status of the current implementation:

- **Video streaming works end-to-end** (capture → JPEG → QUIC → render) but every frame is a full JPEG keyframe; delta frames and adaptive bitrate are not implemented.
- **Remote input is not delivered over the network yet.** The host streams video out, but incoming QUIC datagrams are not routed to the input handler. Input commands work on the local command path only.
- **No NAT traversal** — LAN/direct-IP use only.
- **macOS only.** Windows DXGI capture was never completed; there is no Windows build.
- **No hardware H.264 encoding** — software JPEG only.
- **File transfer is not implemented** (commands acknowledge but do nothing).
- **The server certificate fingerprint is not yet displayed in the UI** — TOFU mode accepts the first certificate seen and logs it.

## Roadmap

- [ ] Deliver remote input over the QUIC data path
- [ ] Display and verify the server certificate fingerprint in the UI
- [ ] Delta frames or hardware H.264 encoding for higher FPS at lower bandwidth
- [ ] Adaptive bitrate wired to real RTT/loss measurements
- [ ] NAT traversal / relay
- [ ] File transfer
- [ ] Windows capture (from scratch)

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
