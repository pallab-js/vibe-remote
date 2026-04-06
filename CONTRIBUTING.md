# Contributing to VibeRemote

Thank you for your interest in contributing to VibeRemote! This document provides guidelines and instructions for contributing.

## Code of Conduct

Please note that this project adheres to the [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## Getting Started

### Prerequisites

- **Rust** 1.85+ (`rustup default stable`)
- **Node.js** 20+ and **pnpm**
- **Xcode Command Line Tools** (macOS): `xcode-select --install`

### Development Setup

```bash
# Clone the repository
git clone https://github.com/pallab-js/vibe-remote.git
cd vibe-remote

# Install frontend dependencies
pnpm install

# Run in development mode
pnpm tauri dev
```

## Project Architecture

VibeRemote is built with:

- **Backend**: Rust (Tauri 2.0) — screen capture, QUIC transport, input injection
- **Frontend**: SvelteKit 5 with TypeScript + Tailwind CSS v4
- **Transport**: QUIC (quinn) with TLS 1.3
- **Screen Capture**: ScreenCaptureKit (macOS), DXGI (Windows)
- **Input Injection**: enigo (cross-platform)

### Key Modules

| Module | Path | Purpose |
|--------|------|---------|
| `transport.rs` | `src-tauri/src/` | QUIC networking, certificate pinning |
| `capture.rs` | `src-tauri/src/` | macOS screen capture |
| `input.rs` | `src-tauri/src/` | Mouse/keyboard injection |
| `encoder.rs` | `src-tauri/src/` | JPEG+zlib frame compression |
| `session.rs` | `src-tauri/src/` | Session state, auto-reconnect |
| `auth.rs` | `src-tauri/src/` | Ed25519 identity, Noise Protocol |
| `+page.svelte` | `src/routes/` | Main UI |

## How to Contribute

### Reporting Bugs

Before creating bug reports, please check [existing issues](https://github.com/pallab-js/vibe-remote/issues).

When filing a bug report, include:

- **Platform**: macOS version / Windows version
- **VibeRemote version**: `0.1.0`
- **Steps to reproduce**: Clear, numbered steps
- **Expected vs actual behavior**
- **Logs**: Run with `VIBE_LOG_LEVEL=debug` and share relevant output

### Suggesting Features

- Describe the problem the feature solves
- Explain how it fits into the project scope
- Consider implementation complexity

### Pull Requests

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes
4. Run tests and linting:
   ```bash
   cd src-tauri && cargo clippy -- -D warnings
   cd src-tauri && cargo test
   pnpm check
   ```
5. Commit with clear messages: `git commit -m "feat: add display selection UI"`
6. Push and open a Pull Request

### Commit Message Convention

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add hardware H.264 encoder support
fix: resolve certificate pinning on Windows
docs: update contributing guide
test: add integration tests for QUIC transport
refactor: simplify frame encoding pipeline
chore: update dependencies
```

## Code Style

### Rust

- Follow `rustfmt` defaults (`cargo fmt`)
- Use `cargo clippy -- -D warnings` before committing
- Error handling: `VibeResult<T>` with `VibeError` for library code
- Async: Use `tokio` runtime; release Mutex locks before `.await`
- Logging: Use `tracing` macros (`info!`, `debug!`, `error!`, `warn!`)

### Frontend

- Svelte 5 runes: use `$state()` for reactive state
- TypeScript: strict mode, no `any` unless necessary
- Tailwind CSS: use utility classes, custom CSS only when needed
- Event handlers: use attribute syntax (`onclick`, not `on:click`)

## Security

- Never log user content (clipboard text, keystrokes, screen data)
- Always enforce consent checks for remote control operations
- Rate limit all security-sensitive commands
- Report security vulnerabilities privately before public disclosure

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
