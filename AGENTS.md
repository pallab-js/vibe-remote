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
- Transport: QUIC via Quinn + binary MessagePack
- Capture: ScreenCaptureKit (macOS 13+)
- Auth: Ed25519 + Noise Protocol

## Key Invariants
1. All pixel data is RGBA (not BGRA) in transit
2. Frame IPC uses base64-encoded data via Tauri events  
3. Input requires explicit consent before execution
4. No secrets logged - only metadata

## New Modules
- `protocol.rs` - Binary MessagePack wire protocol
- `adaptive_bitrate.rs` - Network-adaptive encoding
- `h264_encoder.rs` - Hardware encoding (stub)

## Design System
- `src/lib/design-system.css` - CSS variables
- `src/lib/stores/` - Svelte 5 runes stores
- `src/lib/components/` - Reusable components

## Common Issues
- encoder.rs: Avoid zlib on JPEG - causes size bloat
- InputHandler: Uses enigo (CGEventSource not Send)
- capture_windows.rs: macOS-only, not built on other platforms