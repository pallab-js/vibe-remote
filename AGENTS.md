# AGENTS.md — VibeRemote Development Guide

## Build Commands
```bash
pnpm tauri dev           # Start dev mode
cargo check             # Type-check Rust  
pnpm check              # Type-check Svelte/TS
cargo clippy -- -D warnings  # Lint Rust
cargo fmt              # Format Rust
```

## Architecture
- Frontend: Svelte 5 (runes API)
- Backend: Tauri v2 + Rust async (Tokio)
- Transport: QUIC via Quinn (frames currently JSON+base64 over datagrams)
- Capture: ScreenCaptureKit (macOS 13+)
- Auth: Ed25519 identity + QUIC/TLS 1.3

## Key Invariants
1. All pixel data is RGBA (not BGRA) in transit
2. Frame IPC uses base64-encoded data via Tauri events  
3. Input requires explicit consent before execution
4. No secrets logged - only metadata

## Modules
- `protocol.rs` - MessagePack wire types (not yet wired into the data path; frames currently go over JSON+base64)
- `encoder.rs` - JPEG+zlib frame encoding (delta frames removed - see encoder.rs header comment)

## Design System
- `src/lib/design-system.css` - CSS variables

## Common Issues
- encoder.rs: Avoid zlib on JPEG - causes size bloat
- InputHandler: Uses enigo (CGEventSource not Send)