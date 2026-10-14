# VibeRemote

A lightweight remote desktop for macOS, built with Rust and SvelteKit. Streams your screen over QUIC with TLS 1.3.

[![Rust](https://img.shields.io/badge/rust-1.85+-blue.svg)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-blue.svg)](https://tauri.app/)
[![SvelteKit](https://img.shields.io/badge/SvelteKit-5.0-orange.svg)](https://kit.svelte.dev/)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macos-lightgrey.svg)](https://github.com/pallab-js/vibe-remote)

## Features

- **Native capture** — ScreenCaptureKit (macOS 13+)
- **QUIC transport** — QUIC with native TLS 1.3 via [quinn](https://github.com/quinn-rs/quinn)
- **Small binary** — software JPEG + zlib encoding, no heavyweight codecs
- **Consent-based control** — view-only by default; input and clipboard require explicit grant
- **Rate-limited input** — 100 mouse/sec, 50 keys/sec, clipboard capped
- **Certificate pinning** — SHA256 fingerprint pinning with TOFU mode
- **Ed25519 identity** — persistent keys stored with 0o600 permissions, zeroized in memory
- **Clean UI** — dark glass theme, onboarding wizard, multi-display selection

## Install

Requires [Rust 1.85+](https://rustup.rs), Node.js 20+, pnpm, and Xcode Command Line Tools.

```bash
git clone https://github.com/pallab-js/vibe-remote.git
cd vibe-remote
pnpm install
pnpm tauri dev      # development
pnpm tauri build    # production
```

On first run, grant **Screen Recording** and (for input) **Accessibility** in System Settings > Privacy & Security.

## Usage

1. **Host**: launch the app, choose Host, set a port (default 4567), start the server.
2. **Client**: choose Connect, enter the host's `ip:port`, connect. The remote screen appears in view-only mode.

To test locally, run two instances and connect to `127.0.0.1:4567`.

## Architecture

```
src/            SvelteKit frontend (single-page UI)
src-tauri/
  src/
    lib.rs         Tauri commands + app state
    capture.rs     ScreenCaptureKit frames
    encoder.rs     JPEG + zlib frame encoding
    transport.rs   QUIC server/client, cert pinning
    input.rs       enigo input injection
    session.rs     session state + frame streaming
    auth.rs        Ed25519 identity
    protocol.rs    MessagePack wire types (not yet wired)
    error.rs       error types
    logging.rs     tracing
  tests/         integration tests
```

## Known Limitations

Current status is honest and narrow:

- Video streams end-to-end (capture → JPEG → QUIC → render), but every frame is a full keyframe — no delta frames or adaptive bitrate yet.
- Remote input is not delivered over the network yet; input commands work on the local path only.
- LAN/direct IP only — no NAT traversal or relay.
- macOS only. No Windows build.
- File transfer is not implemented.

See [Roadmap](#roadmap) for priorities.

## Development

```bash
pnpm tauri dev                                  # dev mode with hot reload
cd src-tauri && cargo check                    # type-check Rust
cd src-tauri && cargo clippy -- -D warnings    # lint
cd src-tauri && cargo test                     # tests
pnpm check                                     # type-check frontend
```

Verbose logging: `VIBE_LOG_LEVEL=debug` or `VIBE_VERBOSE=1`.

## Security

Transport encryption, pinning, identity, consent, and rate limiting are layered — see [SECURITY.md](SECURITY.md) for the model and vulnerability reporting.

## Roadmap

- [ ] Deliver remote input over the QUIC data path
- [ ] Display and verify the server certificate fingerprint in the UI
- [ ] Delta frames or hardware H.264 for higher FPS at lower bandwidth
- [ ] Adaptive bitrate
- [ ] NAT traversal / relay
- [ ] File transfer
- [ ] Windows capture

## Contributing

PRs welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). Run `cargo clippy -- -D warnings` and `cargo test` before opening one.

## License

MIT — see [LICENSE](LICENSE).
