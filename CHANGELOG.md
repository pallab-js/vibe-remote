# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-04-06

### Added

- QUIC-powered remote desktop with TLS 1.3 encryption
- Native macOS screen capture via ScreenCaptureKit
- Hardware-accelerated frame encoding (JPEG + zlib compression ~40:1)
- Real-time mouse and keyboard input injection
- Ed25519 identity system with secure key storage
- Certificate pinning (SHA256 fingerprint) with TOFU (Trust On First Use) mode
- Noise Protocol XX handshake infrastructure
- Consent-based remote control (secure by default, view-only mode)
- Rate limiting on all input and clipboard commands
- Connection approval workflow
- Clipboard sync between host and client
- Auto-reconnection with exponential backoff
- STUN/TURN NAT traversal support
- Multi-display support
- Deep Slate dark theme with glassmorphism UI
- Buffer pool management for optimized memory usage
- Custom congestion control (BBR-like) for real-time video
- Comprehensive audit logging
- Content Security Policy enforcement
- CI/CD pipeline with GitHub Actions
- Self-host relay server documentation

### Security

- Backend-enforced consent for remote control and clipboard access
- Certificate pinning prevents man-in-the-middle attacks
- Ed25519 keys stored with 0o600 permissions + memory zeroization
- CSPRNG for all cryptographic randomness (OsRng)
- Path sanitization for file transfer operations
- No user content in production logs
- Dependency vulnerability scanning via `cargo-audit`

### Known Issues

- Windows DXGI capture module is implemented but not yet tested
- Hardware H.264 encoder (VideoToolbox) infrastructure is ready but not wired
- Noise Protocol handshake exists but not integrated into connection flow (future enhancement)

[Unreleased]: https://github.com/pallab-js/vibe-remote/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/pallab-js/vibe-remote/releases/tag/v0.1.0
