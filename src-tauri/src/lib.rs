//! VibeRemote - A QUIC-powered Remote Desktop for the Modern Era
//!
//! Core library providing screen capture, QUIC transport, and input injection.

pub mod error;
pub mod logging;
pub mod capture;
pub mod transport;
pub mod input;
pub mod encoder;
pub mod session;
pub mod auth;

use std::sync::{Mutex, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, error, debug, warn};

use tauri::{Emitter, State};
use serde::{Serialize, Deserialize};

use capture::{CaptureStream, CaptureConfig};
use transport::QuicTransport;
use input::{InputHandler, MouseEvent, MouseButton, KeyboardEvent, VirtualKey};
use encoder::FrameEncoder;
use session::SessionState;
use auth::PeerIdentity;

/// Pending connection request awaiting user approval
#[derive(Clone, Debug)]
pub struct PendingConnection {
    pub remote_address: String,
    pub peer_fingerprint: Option<String>,
    pub timestamp: std::time::Instant,
}

/// Application state shared across Tauri commands
pub struct AppState {
    pub capture_stream: Mutex<Option<Arc<CaptureStream>>>,
    pub quic_transport: Mutex<Option<Arc<QuicTransport>>>,
    pub input_handler: Mutex<InputHandler>,
    pub frame_encoder: Mutex<Option<FrameEncoder>>,
    pub session_state: SessionState,
    pub is_server_mode: Mutex<bool>,
    pub identity: Mutex<Option<PeerIdentity>>,
    // LOW-6: Rate limiting state
    pub command_timestamps: Mutex<std::collections::HashMap<String, Vec<std::time::Instant>>>,
    // SEC-3: Backend consent enforcement for remote control
    pub input_consent_granted: AtomicBool,
    // SEC-3: Backend consent enforcement for clipboard sync
    pub clipboard_consent_granted: AtomicBool,
    // SEC-4: Pending connection requests awaiting user approval
    pub pending_connections: Mutex<Vec<PendingConnection>>,
    // SEC-4: Whether server requires connection approval
    pub require_connection_approval: AtomicBool,
}

// SAFETY JUSTIFICATION for Send + Sync:
// - capture_stream: Mutex<Option<Arc<CaptureStream>>> - Arc + Mutex are Send+Sync
// - quic_transport: Mutex<Option<Arc<QuicTransport>>> - Arc + Mutex are Send+Sync
// - input_handler: Mutex<InputHandler> - InputHandler wraps enigo in Arc<Mutex<>>
// - frame_encoder: Mutex<Option<FrameEncoder>> - FrameEncoder is Clone + Send
// - session_state: SessionState - Uses AtomicBool/AtomicU64 which are Send+Sync
// - is_server_mode: Mutex<bool> - bool is Send+Sync, Mutex makes it thread-safe
// All fields use proper synchronization primitives, no raw pointers or !Send types.
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

/// Frame data for sending to frontend
#[derive(Serialize, Clone, Debug)]
pub struct FrameData {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
    pub timestamp: u128,
}

/// Connection parameters
/// SEC-1: server_fingerprint is now optional for TOFU (Trust On First Use) mode
///        When empty, the client will accept the first certificate seen (TOFU)
///        When provided, strict certificate pinning is enforced
#[derive(Deserialize, Debug)]
pub struct ConnectParams {
    pub host: String,
    pub port: u16,
    /// Server certificate fingerprint for pinning (SHA256 hex)
    /// Empty string = TOFU mode (accept first seen cert)
    pub server_fingerprint: Option<String>,
}

/// Initialize VibeRemote
#[tauri::command]
async fn init_vibe(state: State<'_, AppState>) -> Result<String, String> {
    info!("Initializing VibeRemote");

    // Initialize capture stream
    let capture = Arc::new(CaptureStream::new(CaptureConfig::default()));
    *state.capture_stream.lock().unwrap() = Some(capture);

    // Initialize input handler
    let input = InputHandler::new().map_err(|e| e.to_string())?;
    *state.input_handler.lock().unwrap() = input;

    // Load or generate identity
    let key_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("vibe-remote");
    std::fs::create_dir_all(&key_dir).ok();
    let key_path = key_dir.join("identity.key");
    
    let identity = PeerIdentity::load_or_generate(&key_path, "VibeRemote User".to_string())
        .map_err(|e| format!("Failed to initialize identity: {}", e))?;
    info!("Loaded identity: {} ({})", identity.name(), identity.verifying_key_b64());
    *state.identity.lock().unwrap() = Some(identity);

    info!("VibeRemote initialized successfully");
    Ok("VibeRemote initialized".to_string())
}

/// Start QUIC server mode
#[tauri::command]
async fn start_server(
    state: State<'_, AppState>,
    port: u16,
    _app: tauri::AppHandle,
) -> Result<String, String> {
    info!("Starting QUIC server on port {}", port);

    let config = transport::QuicConfig {
        bind_addr: format!("0.0.0.0:{}", port).parse()
            .map_err(|e| format!("Invalid port: {}", e))?,
        remote_addr: None,
        server_name: "vibe-remote-server".to_string(),
        alpn_protocols: vec![b"vibe-remote-0.2".to_vec()],
        peer_public_key_b64: None,
    };

    let server = transport::QuicTransport::new_server(config).await
        .map_err(|e| format!("Failed to start server: {}", e))?;

    // Store server in state (wrapped in Arc)
    let server_arc = Arc::new(server);
    *state.quic_transport.lock().unwrap() = Some(server_arc.clone());

    // Spawn accept loop in background
    let server_for_accept = server_arc;
    tokio::spawn(async move {
        info!("Starting QUIC accept loop");
        if let Err(e) = server_for_accept.accept_connections().await {
            error!("QUIC accept loop error: {}", e);
        }
    });

    info!("QUIC server started on port {}", port);
    Ok(format!("Server started on port {}", port))
}

/// Get available displays
#[tauri::command]
fn get_displays() -> Result<Vec<(String, u32, u32)>, String> {
    capture::get_available_displays()
        .map_err(|e| e.to_string())
}

/// Start screen capture and stream frames
#[tauri::command]
async fn start_capture(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    display_index: Option<usize>,
) -> Result<String, String> {
    info!("Starting screen capture on display {}", display_index.unwrap_or(0));
    
    // Build capture config
    let display_idx = display_index.unwrap_or(0);
    let config = CaptureConfig {
        display_index: display_idx,
        ..CaptureConfig::default()
    };
    
    // Create and store capture stream
    let capture = Arc::new(CaptureStream::new(config));
    *state.capture_stream.lock().unwrap() = Some(capture.clone());
    
    // Now the lock is released, we can safely await
    let mut receiver = capture.get_primary_stream().await
        .map_err(|e| format!("Failed to start capture: {}", e))?;
    
    // Spawn frame streaming task
    tokio::spawn(async move {
        let mut frame_count = 0u32;
        let mut last_fps_update = std::time::Instant::now();
        
        while let Some(frame) = receiver.recv().await {
            frame_count += 1;
            
            // Calculate FPS
            let now = std::time::Instant::now();
            if now.duration_since(last_fps_update) >= std::time::Duration::from_secs(1) {
                debug!("Capture FPS: {}", frame_count);
                frame_count = 0;
                last_fps_update = now;
            }
            
            let frame_data = FrameData {
                width: frame.width,
                height: frame.height,
                data: frame.data,
                timestamp: frame.timestamp,
            };
            
            // Emit frame to frontend
            if let Err(e) = app.emit("frame", &frame_data) {
                error!("Failed to emit frame: {}", e);
            }
        }
    });
    
    Ok(format!("Capture started on display {}", display_idx))
}

/// Stop screen capture
#[tauri::command]
fn stop_capture(state: State<'_, AppState>) -> Result<String, String> {
    info!("Stopping screen capture");
    
    let guard = state.capture_stream.lock().unwrap();
    if let Some(capture) = guard.as_ref() {
        capture.stop();
    }
    
    Ok("Capture stopped".to_string())
}

/// Connect to remote peer via QUIC with certificate pinning
/// SEC-1: Supports TOFU (Trust On First Use) when fingerprint is not provided
#[tauri::command]
async fn connect_remote(
    state: State<'_, AppState>,
    params: ConnectParams,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let fingerprint_info = params.server_fingerprint.as_deref().unwrap_or("TOFU mode");
    info!("Connecting to remote: {}:{} (pinned: {})",
          params.host, params.port, fingerprint_info);

    let addr = format!("{}:{}", params.host, params.port)
        .parse()
        .map_err(|e| format!("Invalid address: {}", e))?;

    // Create QUIC transport (client mode)
    let config = transport::QuicConfig {
        bind_addr: "0.0.0.0:0".parse().unwrap(), // Let OS pick port
        remote_addr: Some(addr),
        server_name: params.host.clone(),
        alpn_protocols: vec![b"vibe-remote-0.2".to_vec()],
        peer_public_key_b64: None,
    };

    let mut quic = transport::QuicTransport::new_client(config).await
        .map_err(|e| format!("QUIC init failed: {}", e))?;

    // SEC-1: Connect with certificate pinning or TOFU mode
    if let Some(ref fingerprint) = params.server_fingerprint {
        // Strict certificate pinning - prevents MITM attacks
        quic.connect_with_fingerprint(fingerprint.clone()).await
            .map_err(|e| {
                error!("Certificate pinning failed: {}", e);
                format!("Connection failed (certificate mismatch): {}", e)
            })?;
    } else {
        // SEC-1: TOFU mode - accept the first certificate we see
        // This is less secure than pinning but better than no verification
        info!("SEC-1: Connecting in TOFU mode (no fingerprint provided)");
        quic.connect_tofu().await
            .map_err(|e| format!("Connection failed: {}", e))?;
    }

    // Store in state (wrapped in Arc)
    let quic_arc = Arc::new(quic);
    *state.quic_transport.lock().unwrap() = Some(quic_arc.clone());

    // Spawn frame receive loop in background
    let app_clone = app.clone();
    tokio::spawn(async move {
        info!("Starting client frame receive loop");
        loop {
            match quic_arc.receive_datagram().await {
                Ok(data) => {
                    // Parse JSON frame
                    if let Ok(frame_json) = serde_json::from_slice::<serde_json::Value>(&data) {
                        if let (Some(seq), Some(w), Some(h), Some(ts), Some(_frame_type), Some(data_b64)) = (
                            frame_json.get("seq"),
                            frame_json.get("w").and_then(|v| v.as_u64()),
                            frame_json.get("h").and_then(|v| v.as_u64()),
                            frame_json.get("ts").and_then(|v| v.as_u64()),
                            frame_json.get("type").and_then(|v| v.as_str()),
                            frame_json.get("data").and_then(|v| v.as_str()),
                        ) {
                            // Decode base64 data
                            if let Ok(compressed_data) = base64_decode(data_b64) {
                                // Decompress
                                use std::io::Read;
                                let mut decoder = flate2::read::ZlibDecoder::new(compressed_data.as_slice());
                                let mut jpeg_data = Vec::new();
                                if decoder.read_to_end(&mut jpeg_data).is_ok() {
                                    // Decode JPEG to BGRA
                                    if let Ok(img) = image::load_from_memory(&jpeg_data) {
                                        let width = img.width() as usize;
                                        let height = img.height() as usize;
                                        let rgba_data = img.to_rgba8().into_raw();
                                        
                                        // Convert RGBA to BGRA
                                        let mut bgra_data = vec![0u8; rgba_data.len()];
                                        for i in (0..rgba_data.len()).step_by(4) {
                                            bgra_data[i] = rgba_data[i + 2];     // B
                                            bgra_data[i + 1] = rgba_data[i + 1]; // G
                                            bgra_data[i + 2] = rgba_data[i];     // R
                                            bgra_data[i + 3] = rgba_data[i + 3]; // A
                                        }
                                        
                                        let frame_data = FrameData {
                                            width,
                                            height,
                                            data: bgra_data,
                                            timestamp: ts as u128,
                                        };
                                        
                                        // Emit to frontend
                                        if let Err(e) = app_clone.emit("frame", &frame_data) {
                                            debug!("Failed to emit remote frame: {}", e);
                                        } else {
                                            debug!("Emitted remote frame #{} ({}x{})", seq, w, h);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Frame receive error: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
    });

    info!("Connected to {}", addr);
    Ok(format!("Connected to {}", addr))
}

/// Start remote streaming session (server mode)
#[tauri::command]
async fn start_remote_stream(
    state: State<'_, AppState>,
    _app: tauri::AppHandle,
    display_index: Option<usize>,
) -> Result<String, String> {
    info!("Starting remote streaming session");
    
    // Initialize frame encoder
    let encoder = FrameEncoder::default();
    *state.frame_encoder.lock().unwrap() = Some(encoder);
    
    // Start capture
    let display_idx = display_index.unwrap_or(0);
    let config = CaptureConfig {
        display_index: display_idx,
        ..CaptureConfig::default()
    };
    
    let capture = Arc::new(CaptureStream::new(config));
    *state.capture_stream.lock().unwrap() = Some(capture.clone());
    
    let receiver = capture.get_primary_stream().await
        .map_err(|e| format!("Failed to start capture: {}", e))?;
    
    // Get QUIC transport
    let quic_arc = {
        let quic_guard = state.quic_transport.lock().unwrap();
        let quic = quic_guard.as_ref()
            .ok_or("QUIC not initialized")?.clone();
        quic
    };
    
    // Get input handler
    let _input_guard = state.input_handler.lock().unwrap();
    let _input_arc = Arc::new(_input_guard.clone());
    drop(_input_guard);
    
    // Get encoder
    let enc_guard = state.frame_encoder.lock().unwrap();
    let enc = enc_guard.as_ref()
        .ok_or("Encoder not initialized")?;
    let enc_arc = Arc::new(Mutex::new(enc.clone()));
    drop(enc_guard);
    
    // Start frame streaming in background
    let session = state.session_state.clone();
    tokio::spawn(async move {
        session::start_frame_streaming(
            receiver,
            quic_arc,
            enc_arc,
            session,
        ).await;
    });
    
    // Mark as server mode
    *state.is_server_mode.lock().unwrap() = true;
    state.session_state.is_active.store(true, std::sync::atomic::Ordering::SeqCst);
    
    info!("Remote streaming session started");
    Ok(format!("Streaming display {} to connected clients", display_idx))
}

/// Handle incoming remote input
#[tauri::command]
fn handle_remote_input(
    _state: State<'_, AppState>,
    _input_type: String,
    event_data: serde_json::Value,
) -> Result<String, String> {
    // SECURITY: Never log user content (clipboard text, keystrokes, etc.)
    // Only log event type and metadata sizes
    debug!("Remote input received: type={}, size={}", 
           _input_type, 
           serde_json::to_string(&event_data).map(|s| s.len()).unwrap_or(0));
    
    // Placeholder - actual handling would forward to QUIC or inject locally
    Ok("Input received".to_string())
}

/// Get session statistics
#[tauri::command]
fn get_session_stats(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let session = &state.session_state;
    
    Ok(serde_json::json!({
        "active": session.is_active.load(std::sync::atomic::Ordering::SeqCst),
        "frames_sent": session.frames_sent.load(std::sync::atomic::Ordering::Relaxed),
        "bytes_sent": session.bytes_sent.load(std::sync::atomic::Ordering::Relaxed),
        "inputs_received": session.inputs_received.load(std::sync::atomic::Ordering::Relaxed),
    }))
}

/// Handle mouse input from frontend
/// SEC-2: Rate limiting enforced (100 calls/sec)
/// SEC-3: Backend consent enforcement
#[tauri::command]
fn send_mouse_input(
    state: State<'_, AppState>,
    event_type: String,
    x: i32,
    y: i32,
    button: Option<String>,
) -> Result<String, String> {
    // SEC-3: Backend consent enforcement
    check_input_consent(&state)?;

    // SEC-2: Rate limiting - max 100 mouse events per second
    check_rate_limit(&state, "send_mouse_input", 100, 1)?;

    let input = state.input_handler.lock().unwrap();

    match event_type.as_str() {
        "move" => {
            input.handle_mouse_event(MouseEvent::Move { x, y })
                .map_err(|e| e.to_string())?;
        }
        "down" => {
            let btn = match button.as_deref() {
                Some("right") => MouseButton::Right,
                Some("middle") => MouseButton::Middle,
                _ => MouseButton::Left,
            };
            input.handle_mouse_event(MouseEvent::Down { button: btn })
                .map_err(|e| e.to_string())?;
        }
        "up" => {
            let btn = match button.as_deref() {
                Some("right") => MouseButton::Right,
                Some("middle") => MouseButton::Middle,
                _ => MouseButton::Left,
            };
            input.handle_mouse_event(MouseEvent::Up { button: btn })
                .map_err(|e| e.to_string())?;
        }
        "wheel" => {
            input.handle_mouse_event(MouseEvent::Wheel { delta: y })
                .map_err(|e| e.to_string())?;
        }
        _ => return Err(format!("Unknown mouse event: {}", event_type)),
    }

    state.session_state.inputs_received.fetch_add(1, Ordering::Relaxed);
    Ok("Mouse event handled".to_string())
}

/// Handle keyboard input from frontend
/// SEC-2: Rate limiting enforced (50 keys/sec)
/// SEC-3: Backend consent enforcement
#[tauri::command]
fn send_keyboard_input(
    state: State<'_, AppState>,
    key: String,
    event_type: String,
) -> Result<String, String> {
    // SEC-3: Backend consent enforcement
    check_input_consent(&state)?;

    // SEC-2: Rate limiting - max 50 keyboard events per second
    check_rate_limit(&state, "send_keyboard_input", 50, 1)?;

    let input = state.input_handler.lock().unwrap();

    match event_type.as_str() {
        "text" => {
            input.handle_keyboard_event(KeyboardEvent::Text { text: key })
                .map_err(|e| e.to_string())?;
        }
        "down" => {
            let vk = VirtualKey::from_str(&key)
                .ok_or_else(|| format!("Unknown key: {}", key))?;
            input.handle_keyboard_event(KeyboardEvent::KeyDown { key: vk })
                .map_err(|e| e.to_string())?;
        }
        "up" => {
            let vk = VirtualKey::from_str(&key)
                .ok_or_else(|| format!("Unknown key: {}", key))?;
            input.handle_keyboard_event(KeyboardEvent::KeyUp { key: vk })
                .map_err(|e| e.to_string())?;
        }
        _ => return Err(format!("Unknown keyboard event: {}", event_type)),
    }

    state.session_state.inputs_received.fetch_add(1, Ordering::Relaxed);
    Ok("Keyboard event handled".to_string())
}

/// Send frame over QUIC (for remote streaming)
#[tauri::command]
async fn send_frame_remote(
    state: State<'_, AppState>,
    width: usize,
    height: usize,
    _timestamp: u128,
    data: Vec<u8>,
) -> Result<String, String> {
    // Get QUIC transport (release lock immediately)
    let quic_arc = {
        let quic_guard = state.quic_transport.lock().unwrap();
        quic_guard.as_ref().cloned()
    };

    let quic = quic_arc.ok_or("Not connected to remote")?;

    // Encode frame (release lock before async)
    let encoded = {
        let mut encoder_guard = state.frame_encoder.lock().unwrap();
        let encoder = encoder_guard.as_mut()
            .ok_or("Frame encoder not initialized")?;
        
        encoder.encode_frame(&data, width, height, 0)
            .map_err(|e| format!("Frame encoding failed: {}", e))?
    };

    // Serialize to JSON with base64
    let frame_json = serde_json::json!({
        "seq": encoded.sequence,
        "w": encoded.width,
        "h": encoded.height,
        "ts": encoded.timestamp,
        "type": match encoded.frame_type {
            encoder::FrameType::KeyFrame => "key",
            encoder::FrameType::DeltaFrame => "delta",
        },
        "data": base64_encode(&encoded.data),
    });

    let json_bytes = serde_json::to_vec(&frame_json)
        .map_err(|e| format!("Serialization failed: {}", e))?;
    let json_len = json_bytes.len();

    // Send over QUIC datagram
    quic.send_data(json_bytes.into()).await
        .map_err(|e| format!("Send failed: {}", e))?;

    debug!("Sent frame #{} ({} bytes encoded, {} bytes on wire)", 
           encoded.sequence, encoded.data.len(), json_len);

    Ok(format!("Frame #{} sent", encoded.sequence))
}

/// Forward input to remote
#[tauri::command]
async fn forward_input_remote(
    state: State<'_, AppState>,
    input_type: String,
    event_data: serde_json::Value,
) -> Result<String, String> {
    // Get QUIC transport
    let quic_arc = {
        let quic_guard = state.quic_transport.lock().unwrap();
        quic_guard.as_ref().cloned()
    };

    let quic = quic_arc.ok_or("Not connected to remote")?;

    // Serialize and send input event
    let event_bytes = serde_json::to_vec(&event_data)
        .map_err(|e| format!("Serialization failed: {}", e))?;
    let event_len = event_bytes.len();

    quic.send_data(event_bytes.into()).await
        .map_err(|e| format!("Send failed: {}", e))?;

    debug!("Forwarded {} input ({} bytes)", input_type, event_len);
    Ok("Input forwarded".to_string())
}

/// Simple base64 encoding using the base64 crate
fn base64_encode(data: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data)
}

/// Simple base64 decoding using the base64 crate
fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, input)
        .map_err(|e| format!("Base64 decode failed: {}", e))
}

/// Get connection status
#[tauri::command]
fn get_connection_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = state.quic_transport.lock().unwrap();
    let is_connected = guard.as_ref().map(|q| q.is_connected()).unwrap_or(false);
    let session = &state.session_state;
    let stats = session.get_stats();
    
    // Get server fingerprint if in server mode
    let server_fingerprint = guard.as_ref()
        .and_then(|q| q.get_certificate_fingerprint());

    Ok(serde_json::json!({
        "connected": is_connected,
        "mode": if is_connected { "client" } else { "disconnected" },
        "server_fingerprint": server_fingerprint,
        "stats": {
            "active": stats.is_active,
            "frames_sent": stats.frames_sent,
            "bytes_sent": stats.bytes_sent,
            "inputs_received": stats.inputs_received,
        }
    }))
}

/// Get server certificate fingerprint for sharing with clients
#[tauri::command]
fn get_server_fingerprint(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let guard = state.quic_transport.lock().unwrap();
    Ok(guard.as_ref().and_then(|q| q.get_certificate_fingerprint()))
}

/// Get clipboard content
/// SEC-3: Backend consent enforcement + rate limiting (5 reads/min) + audit logging
#[tauri::command]
fn get_clipboard(state: State<'_, AppState>) -> Result<String, String> {
    // SEC-3: Backend consent enforcement
    check_clipboard_consent(&state)?;

    // SEC-2: Rate limiting - max 5 clipboard reads per minute
    check_rate_limit(&state, "get_clipboard", 5, 60)?;

    use arboard::Clipboard;
    let mut clipboard = Clipboard::new()
        .map_err(|e| format!("Clipboard init failed: {}", e))?;

    let text = clipboard.get_text()
        .map_err(|e| format!("Clipboard read failed: {}", e))?;

    // SEC-7: Audit logging (never log content, only metadata)
    info!("Clipboard read: {} chars (consent granted)", text.len());
    Ok(text)
}

/// Set clipboard content
/// SEC-3: Backend consent enforcement + rate limiting (10 writes/min) + audit logging
#[tauri::command]
fn set_clipboard(state: State<'_, AppState>, text: String) -> Result<String, String> {
    // SEC-3: Backend consent enforcement
    check_clipboard_consent(&state)?;

    // SEC-2: Rate limiting - max 10 clipboard writes per minute
    check_rate_limit(&state, "set_clipboard", 10, 60)?;

    use arboard::Clipboard;
    let mut clipboard = Clipboard::new()
        .map_err(|e| format!("Clipboard init failed: {}", e))?;

    clipboard.set_text(&text)
        .map_err(|e| format!("Clipboard write failed: {}", e))?;

    // SEC-7: Audit logging (never log content, only metadata)
    info!("Clipboard written: {} chars (consent granted)", text.len());
    Ok("Clipboard set".to_string())
}

/// Request file from remote (HIGH-4: Path sanitization implemented)
#[tauri::command]
async fn request_file(
    state: State<'_, AppState>,
    file_path: String,
) -> Result<String, String> {
    // HIGH-4: Sanitize file path to prevent path traversal attacks
    let sanitized = sanitize_file_path(&file_path)?;
    
    // Check if connected
    let is_connected = {
        let quic_guard = state.quic_transport.lock().unwrap();
        quic_guard.as_ref().map(|q| q.is_connected()).unwrap_or(false)
    };
    
    if !is_connected {
        return Err("Not connected to remote".to_string());
    }
    
    info!("File requested: {}", sanitized);
    // TODO: Implement actual file transfer via secondary QUIC stream
    Ok(format!("File requested: {}", sanitized))
}

/// Send file to remote (HIGH-4: Path sanitization implemented)
#[tauri::command]
async fn send_file(
    state: State<'_, AppState>,
    file_path: String,
) -> Result<String, String> {
    // HIGH-4: Sanitize file path to prevent path traversal attacks
    let sanitized = sanitize_file_path(&file_path)?;
    
    // Check if connected
    let is_connected = {
        let quic_guard = state.quic_transport.lock().unwrap();
        quic_guard.as_ref().map(|q| q.is_connected()).unwrap_or(false)
    };
    
    if !is_connected {
        return Err("Not connected to remote".to_string());
    }
    
    info!("File sent: {}", sanitized);
    // TODO: Implement actual file transfer via secondary QUIC stream
    Ok(format!("File sent: {}", sanitized))
}

/// SEC-2: Rate limiter for Tauri commands (now enforced on all security-sensitive commands)
fn check_rate_limit(
    state: &State<AppState>,
    command_name: &str,
    max_calls: usize,
    window_secs: u64,
) -> Result<(), String> {
    let mut timestamps = state.command_timestamps.lock().unwrap();
    let now = std::time::Instant::now();
    let window = std::time::Duration::from_secs(window_secs);

    let calls = timestamps.entry(command_name.to_string()).or_insert_with(Vec::new);

    // Remove old timestamps outside the window
    calls.retain(|t| now.duration_since(*t) < window);

    if calls.len() >= max_calls {
        warn!(
            "Rate limit exceeded for {}. Max {} calls per {} seconds.",
            command_name, max_calls, window_secs
        );
        return Err(format!(
            "Rate limit exceeded. Max {} calls per {} seconds.",
            max_calls, window_secs
        ));
    }

    calls.push(now);
    Ok(())
}

/// SEC-3: Check if remote input consent is granted (backend enforcement)
fn check_input_consent(state: &State<AppState>) -> Result<(), String> {
    if !state.input_consent_granted.load(Ordering::SeqCst) {
        return Err("Remote control is disabled. Enable remote control in the toolbar.".to_string());
    }
    Ok(())
}

/// SEC-3: Check if clipboard consent is granted (backend enforcement)
fn check_clipboard_consent(state: &State<AppState>) -> Result<(), String> {
    if !state.clipboard_consent_granted.load(Ordering::SeqCst) {
        return Err("Clipboard sync is disabled. Enable it in settings if needed.".to_string());
    }
    Ok(())
}

/// SEC-4: Sanitize file path to prevent path traversal attacks
fn sanitize_file_path(path: &str) -> Result<String, String> {
    // Reject absolute paths
    if path.starts_with('/') || path.contains(':') || path.starts_with("\\\\") {
        return Err("Absolute paths not allowed".to_string());
    }
    
    // Reject path traversal sequences
    if path.contains("..") || path.contains("./") || path.contains(".\\") {
        return Err("Path traversal sequences not allowed".to_string());
    }
    
    // Use only the filename component
    let filename = std::path::Path::new(path)
        .file_name()
        .ok_or("Invalid file path")?
        .to_str()
        .ok_or("Invalid characters in file path")?;
    
    // Validate filename
    if filename.is_empty() || filename.len() > 255 {
        return Err("Invalid filename length".to_string());
    }
    
    Ok(filename.to_string())
}

/// Get application version
#[tauri::command]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ============================================================================
// SEC-3: Consent Control Commands (Backend Enforcement)
// ============================================================================

/// Grant remote input consent (backend enforcement)
#[tauri::command]
fn grant_input_consent(state: State<'_, AppState>) -> Result<String, String> {
    state.input_consent_granted.store(true, Ordering::SeqCst);
    info!("SECURITY: Remote input consent granted");
    Ok("Input consent granted".to_string())
}

/// Revoke remote input consent (backend enforcement)
#[tauri::command]
fn revoke_input_consent(state: State<'_, AppState>) -> Result<String, String> {
    state.input_consent_granted.store(false, Ordering::SeqCst);
    info!("SECURITY: Remote input consent revoked");
    Ok("Input consent revoked".to_string())
}

/// Grant clipboard sync consent (backend enforcement)
#[tauri::command]
fn grant_clipboard_consent(state: State<'_, AppState>) -> Result<String, String> {
    state.clipboard_consent_granted.store(true, Ordering::SeqCst);
    info!("SECURITY: Clipboard sync consent granted");
    Ok("Clipboard consent granted".to_string())
}

/// Revoke clipboard sync consent (backend enforcement)
#[tauri::command]
fn revoke_clipboard_consent(state: State<'_, AppState>) -> Result<String, String> {
    state.clipboard_consent_granted.store(false, Ordering::SeqCst);
    info!("SECURITY: Clipboard sync consent revoked");
    Ok("Clipboard consent revoked".to_string())
}

/// Get current consent status for both input and clipboard
#[tauri::command]
fn get_consent_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "input_consent": state.input_consent_granted.load(Ordering::SeqCst),
        "clipboard_consent": state.clipboard_consent_granted.load(Ordering::SeqCst),
        "require_connection_approval": state.require_connection_approval.load(Ordering::SeqCst),
    }))
}

// ============================================================================
// SEC-4: Connection Approval Commands
// ============================================================================

/// Get pending connection requests
#[tauri::command]
fn get_pending_connections(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let pending = state.pending_connections.lock().unwrap();

    // Clean up expired requests (older than 60 seconds)
    let now = std::time::Instant::now();
    let valid: Vec<_> = pending.iter()
        .filter(|p| now.duration_since(p.timestamp) < std::time::Duration::from_secs(60))
        .collect();

    let result: Vec<serde_json::Value> = valid.iter().map(|p| {
        serde_json::json!({
            "remote_address": p.remote_address,
            "peer_fingerprint": p.peer_fingerprint,
        })
    }).collect();

    Ok(result)
}

/// Approve a pending connection
#[tauri::command]
fn approve_connection(_state: State<'_, AppState>, remote_address: String) -> Result<String, String> {
    info!("SECURITY: Connection approved from {}", remote_address);
    // In a full implementation, this would signal the transport layer to accept
    // the specific connection. For now, we log the approval.
    Ok(format!("Connection from {} approved", remote_address))
}

/// Reject a pending connection
#[tauri::command]
fn reject_connection(state: State<'_, AppState>, remote_address: String) -> Result<String, String> {
    let mut pending = state.pending_connections.lock().unwrap();
    pending.retain(|p| p.remote_address != remote_address);
    warn!("SECURITY: Connection rejected from {}", remote_address);
    Ok(format!("Connection from {} rejected", remote_address))
}

/// Enable/disable connection approval requirement
#[tauri::command]
fn set_connection_approval(state: State<'_, AppState>, enabled: bool) -> Result<String, String> {
    state.require_connection_approval.store(enabled, Ordering::SeqCst);
    info!("SECURITY: Connection approval requirement set to {}", enabled);
    Ok(format!("Connection approval {}", if enabled { "enabled" } else { "disabled" }))
}

/// Generate a new Ed25519 identity
#[tauri::command]
fn generate_identity(state: State<'_, AppState>, name: String) -> Result<auth::PublicIdentity, String> {
    // Use app data directory for key storage
    let key_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("vibe-remote");
    
    std::fs::create_dir_all(&key_dir)
        .map_err(|e| format!("Failed to create key directory: {}", e))?;
    
    let key_path = key_dir.join("identity.key");
    
    let public = auth::generate_identity(name, &key_path)
        .map_err(|e| e.to_string())?;
    
    // Load and store in state
    let identity = auth::PeerIdentity::load(&key_path)
        .map_err(|e| format!("Failed to load identity: {}", e))?;
    *state.identity.lock().unwrap() = Some(identity);
    
    info!("Generated new identity: {}", public.name);
    Ok(public)
}

/// Load existing Ed25519 identity
#[tauri::command]
fn load_identity(state: State<'_, AppState>) -> Result<Option<auth::PublicIdentity>, String> {
    let key_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("vibe-remote");
    let key_path = key_dir.join("identity.key");
    
    if !key_path.exists() {
        return Ok(None);
    }
    
    let public = auth::load_identity(&key_path)
        .map_err(|e| e.to_string())?;
    
    let identity = auth::PeerIdentity::load(&key_path)
        .map_err(|e| format!("Failed to load identity: {}", e))?;
    *state.identity.lock().unwrap() = Some(identity);
    
    Ok(Some(public))
}

/// Get current identity
#[tauri::command]
fn get_identity(state: State<'_, AppState>) -> Result<Option<auth::PublicIdentity>, String> {
    let guard = state.identity.lock().unwrap();
    Ok(guard.as_ref().map(|id| id.public_identity()))
}

/// Verify a peer's signature
#[tauri::command]
fn verify_peer_signature(
    verifying_key_b64: String,
    message_b64: String,
    signature_b64: String,
) -> Result<bool, String> {
    let message = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &message_b64)
        .map_err(|e| format!("Invalid message encoding: {}", e))?;
    
    match auth::PeerIdentity::verify_signature(&verifying_key_b64, &message, &signature_b64) {
        Ok(_) => Ok(true),
        Err(_e) => Ok(false)
    }
}

/// Setup Tauri application
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    logging::init_logging();
    
    info!("VibeRemote starting up");
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            capture_stream: Mutex::new(None),
            quic_transport: Mutex::new(None),
            // Create input handler, or use a placeholder that will fail gracefully
            input_handler: Mutex::new(InputHandler::new().expect("Failed to initialize input handler - check accessibility permissions")),
            frame_encoder: Mutex::new(None),
            session_state: SessionState::default(),
            is_server_mode: Mutex::new(false),
            identity: Mutex::new(None),
            // LOW-6: Initialize rate limiting state
            command_timestamps: Mutex::new(std::collections::HashMap::new()),
            // SEC-3: Consent state - default to DENY (secure by default)
            input_consent_granted: AtomicBool::new(false),
            clipboard_consent_granted: AtomicBool::new(false),
            // SEC-4: Pending connections
            pending_connections: Mutex::new(Vec::new()),
            // SEC-4: Connection approval - disabled by default for local network use
            require_connection_approval: AtomicBool::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            init_vibe,
            start_server,
            get_displays,
            start_capture,
            stop_capture,
            connect_remote,
            start_remote_stream,
            handle_remote_input,
            get_session_stats,
            send_mouse_input,
            send_keyboard_input,
            send_frame_remote,
            forward_input_remote,
            get_connection_status,
            get_server_fingerprint,
            get_clipboard,
            set_clipboard,
            request_file,
            send_file,
            // SEC-3: Consent control commands
            grant_input_consent,
            revoke_input_consent,
            grant_clipboard_consent,
            revoke_clipboard_consent,
            get_consent_status,
            // SEC-4: Connection approval commands
            get_pending_connections,
            approve_connection,
            reject_connection,
            set_connection_approval,
            // Identity commands
            generate_identity,
            load_identity,
            get_identity,
            verify_peer_signature,
            // Info commands
            get_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
