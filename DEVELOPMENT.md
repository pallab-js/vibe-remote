# VibeRemote Development Guide

## Quick Start

### Running the Application

```bash
# Development mode (with hot reload)
pnpm tauri dev

# Build for production
pnpm tauri build
```

### Testing Features (Phase 1)

1. **Dashboard UI**: 
   - Launch the app
   - See the Deep Slate themed dashboard
   - Enter any text in the Partner ID field
   - Click "Connect"

2. **Test Pattern Capture**:
   - After clicking "Connect", the session view appears
   - You'll see an animated gradient test pattern
   - Move your mouse over the canvas to test input
   - Watch the FPS counter in the toolbar

3. **Logging**:
   - Check the terminal for detailed logs
   - Control log level with environment variable:
     ```bash
     VIBE_LOG_LEVEL=debug pnpm tauri dev
     ```

## Architecture Notes

### Current Limitations (Phase 1)

1. **Capture Mode**: 
   - Uses test pattern generator
   - Real ScreenCaptureKit integration in Phase 2
   
2. **QUIC Transport**:
   - Stub implementation
   - Full QUIC networking in Phase 2

3. **Input Handling**:
   - Mouse move events work
   - Keyboard input ready for Phase 2

### Thread Safety Model

The application uses `Arc<Mutex<>>` for shared state:
- `AppState` contains all shared resources
- Capture stream wrapped in `Arc` for cloning
- Input handler uses `Mutex<Enigo>` for thread safety
- All Tauri commands are `async` and `Send`-safe

### Adding New Features

#### New Tauri Command

```rust
// In src-tauri/src/lib.rs

#[tauri::command]
async fn my_new_command(state: State<'_, AppState>) -> Result<String, String> {
    info!("Executing my new command");
    // Implementation here
    Ok("Success".to_string())
}

// Add to invoke_handler:
.invoke_handler(tauri::generate_handler![
    // ... existing commands
    my_new_command,
])
```

#### New Frontend Event Listener

```typescript
// In +page.svelte
import { listen } from "@tauri-apps/api/event";

onMount(async () => {
  const unlisten = await listen("my-event", (event) => {
    console.log("Received:", event.payload);
  });
  
  return () => unlisten();
});
```

## Debugging

### Common Issues

**Issue**: App doesn't start
```bash
# Check Rust compilation
cd src-tauri && cargo check

# Check frontend
pnpm build
```

**Issue**: Frame rendering fails
- Verify canvas element is mounted
- Check frame data format (RGBA expected)
- Monitor console for errors

**Issue**: Input not working
- Check enigo permissions (macOS may need Accessibility access)
- Verify mouse coordinates are within bounds

### Performance Monitoring

Watch for these metrics in the UI:
- **FPS**: Should be ~30 in Phase 1 (test pattern)
- **Latency**: Should be <50ms for test pattern
- Phase 2 targets: 60 FPS, <30ms latency

## Build Optimizations

### Reducing Binary Size

```bash
# In src-tauri/Cargo.toml
[profile.release]
strip = true
lto = true
codegen-units = 1
opt-level = "z"
```

### Frontend Optimization

The SvelteKit setup already uses:
- Static adapter (no SSR overhead)
- Compiled-away framework (zero runtime)
- Minimal JavaScript footprint

## Next Steps (Phase 2 Implementation)

### 1. Real Screen Capture

Replace test pattern with ScreenCaptureKit:

```rust
// In capture.rs
use screencapturekit::shareable_content::SCShareableContent;

// Get actual display stream
let content = SCShareableContent::get()?;
let display = content.displays().first()?;
// ... setup capture pipeline
```

### 2. QUIC Connection

Enable quinn networking:

```rust
// In transport.rs
let connection = endpoint.connect(&remote_addr, "hostname")?.await?;
let (mut send, mut recv) = connection.open_bi().await?;
```

### 3. Video Encoding

Add hardware encoding (VideoToolbox on macOS):
- Use `videotoolbox` crate
- Encode frames to H.264
- Send via QUIC datagrams

## Testing Strategy (Phase 2)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_capture_initialization() {
        let stream = CaptureStream::new(CaptureConfig::default());
        assert!(stream.get_primary_stream().await.is_ok());
    }
    
    #[tokio::test]
    async fn test_quic_local_tunnel() {
        let (server, client) = create_local_tunnel().await.unwrap();
        // Test connection
    }
}
```

## CI/CD Setup (Phase 4)

GitHub Actions workflow will:
1. Build on macOS and Windows
2. Run tests
3. Create .dmg (macOS) and .msi (Windows)
4. Upload to GitHub Releases

Example workflow structure:
```yaml
name: Build
on: [push, pull_request]
jobs:
  build:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - run: pnpm install
      - run: pnpm tauri build
```

## Environment Variables

- `VIBE_LOG_LEVEL`: Set log level (debug, info, warn, error)
- `RUST_BACKTRACE`: Enable backtraces on panic (1 or full)
- `RUST_LOG`: Fine-grained logging control

## File Locations

- **Builds**: `src-tauri/target/release/bundle/`
- **Logs**: Terminal output (configurable in Phase 3)
- **Config**: `src-tauri/tauri.conf.json`
- **Icons**: `src-tauri/icons/`

## Useful Commands

```bash
# Format Rust code
cargo fmt --manifest-path=src-tauri/Cargo.toml

# Run clippy linter
cargo clippy --manifest-path=src-tauri/Cargo.toml -- -D warnings

# Check for security vulnerabilities
cargo audit --manifest-path=src-tauri/Cargo.toml

# Update dependencies
cargo update --manifest-path=src-tauri/Cargo.toml
pnpm update

# Clean build artifacts
cargo clean --manifest-path=src-tauri/Cargo.toml
rm -rf build/
```

## Getting Help

- Check terminal output for Rust errors
- Check browser console for frontend errors
- Review tracing logs with `VIBE_LOG_LEVEL=debug`
- Read module documentation in source files

---

Happy coding! 🚀
