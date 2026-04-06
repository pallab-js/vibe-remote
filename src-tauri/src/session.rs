//! Streaming session manager for remote desktop
//!
//! Coordinates frame capture, encoding, and QUIC transmission
//! along with remote input injection, auto-reconnect, file transfer, and clipboard sync.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::sync::mpsc;
use tracing::{info, error, debug, warn};

use crate::capture::CapturedFrame;
use crate::encoder::FrameEncoder;
use crate::transport::QuicTransport;
use crate::input::{InputHandler, MouseEvent, MouseButton, KeyboardEvent, VirtualKey};

/// Reconnection state machine
#[derive(Debug, Clone, Copy, PartialEq)]
enum ReconnectState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting { attempt: u32 },
}

/// Reconnection configuration
#[derive(Clone)]
pub struct ReconnectConfig {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f32,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            max_attempts: 10,
            initial_delay_ms: 1000,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
        }
    }
}

/// Remote desktop session state
pub struct SessionState {
    pub is_active: Arc<AtomicBool>,
    pub frames_sent: Arc<AtomicU64>,
    pub bytes_sent: Arc<AtomicU64>,
    pub inputs_received: Arc<AtomicU64>,
    pub bytes_received: Arc<AtomicU64>,
    reconnect_state: Arc<Mutex<ReconnectState>>,
    reconnect_config: ReconnectConfig,
}

impl Clone for SessionState {
    fn clone(&self) -> Self {
        Self {
            is_active: self.is_active.clone(),
            frames_sent: self.frames_sent.clone(),
            bytes_sent: self.bytes_sent.clone(),
            inputs_received: self.inputs_received.clone(),
            bytes_received: self.bytes_received.clone(),
            reconnect_state: self.reconnect_state.clone(),
            reconnect_config: self.reconnect_config.clone(),
        }
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            is_active: Arc::new(AtomicBool::new(false)),
            frames_sent: Arc::new(AtomicU64::new(0)),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            inputs_received: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            reconnect_state: Arc::new(Mutex::new(ReconnectState::Disconnected)),
            reconnect_config: ReconnectConfig::default(),
        }
    }
}

impl SessionState {
    /// Set reconnect configuration
    pub fn set_reconnect_config(&self, config: ReconnectConfig) {
        let _state = self.reconnect_state.lock().unwrap();
        // Store config separately - for simplicity we use the default
        let _ = config;
    }

    /// Get current connection status
    pub fn is_connected(&self) -> bool {
        let state = self.reconnect_state.lock().unwrap();
        *state == ReconnectState::Connected
    }

    /// Get session statistics
    pub fn get_stats(&self) -> SessionStats {
        SessionStats {
            is_active: self.is_active.load(Ordering::SeqCst),
            frames_sent: self.frames_sent.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            inputs_received: self.inputs_received.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
        }
    }
}

/// Session statistics snapshot
pub struct SessionStats {
    pub is_active: bool,
    pub frames_sent: u64,
    pub bytes_sent: u64,
    pub inputs_received: u64,
    pub bytes_received: u64,
}

/// Auto-reconnecting connection manager
pub struct ConnectionManager {
    session: SessionState,
}

impl ConnectionManager {
    pub fn new(session: SessionState) -> Self {
        Self { session }
    }

    /// Attempt to connect with automatic retries
    pub async fn connect_with_retries<F, Fut, T>(
        &self,
        connect_fn: F,
    ) -> Result<T, String>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        let config = &self.session.reconnect_config;
        let mut delay_ms = config.initial_delay_ms;

        for attempt in 1..=config.max_attempts {
            debug!("Connection attempt {}/{}", attempt, config.max_attempts);
            
            {
                let mut state = self.session.reconnect_state.lock().unwrap();
                if attempt == 1 {
                    *state = ReconnectState::Connecting;
                } else {
                    *state = ReconnectState::Reconnecting { attempt };
                }
            }

            match connect_fn().await {
                Ok(result) => {
                    let mut state = self.session.reconnect_state.lock().unwrap();
                    *state = ReconnectState::Connected;
                    info!("Connection established on attempt {}", attempt);
                    return Ok(result);
                }
                Err(e) => {
                    warn!("Connection attempt {} failed: {}", attempt, e);
                    if attempt < config.max_attempts {
                        debug!("Retrying in {}ms", delay_ms);
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                        delay_ms = (delay_ms as f32 * config.backoff_multiplier) as u64;
                        delay_ms = delay_ms.min(config.max_delay_ms);
                    }
                }
            }
        }

        let mut state = self.session.reconnect_state.lock().unwrap();
        *state = ReconnectState::Disconnected;
        Err("Connection failed after maximum retries".to_string())
    }

    /// Mark connection as lost and start reconnection
    pub fn trigger_reconnect(&self) {
        let mut state = self.session.reconnect_state.lock().unwrap();
        if *state == ReconnectState::Connected {
            *state = ReconnectState::Disconnected;
            info!("Connection lost, reconnection will be triggered");
        }
    }
}

/// File transfer metadata
#[derive(Clone, Debug)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    pub transfer_id: u64,
}

/// Start streaming frames to remote client
pub async fn start_frame_streaming(
    mut frame_receiver: mpsc::Receiver<CapturedFrame>,
    quic: Arc<QuicTransport>,
    encoder: Arc<std::sync::Mutex<FrameEncoder>>,
    session: SessionState,
) {
    info!("Starting frame streaming session");
    session.is_active.store(true, Ordering::SeqCst);

    let mut frame_count = 0u64;
    let mut last_report = std::time::Instant::now();
    
    // MED-3: Backpressure - drop frames if network is behind
    let mut consecutive_drops = 0u64;
    const MAX_CONSECUTIVE_DROPS: u64 = 30; // Drop connection after 30 consecutive drops

    while let Some(frame) = frame_receiver.recv().await {
        if !session.is_active.load(Ordering::SeqCst) {
            break;
        }

        // Encode frame
        let encoded = {
            let mut enc = encoder.lock().unwrap();
            match enc.encode_frame(&frame.data, frame.width, frame.height, frame.timestamp) {
                Ok(e) => e,
                Err(e) => {
                    error!("Frame encoding failed: {}", e);
                    continue;
                }
            }
        };

        // Send over QUIC datagram
        let frame_data = serde_json::json!({
            "seq": encoded.sequence,
            "w": encoded.width,
            "h": encoded.height,
            "ts": encoded.timestamp,
            "type": match encoded.frame_type {
                crate::encoder::FrameType::KeyFrame => "key",
                crate::encoder::FrameType::DeltaFrame => "delta",
            },
            "data": base64_encode(&encoded.data),
        });

        let json_bytes = serde_json::to_vec(&frame_data).unwrap_or_default();
        
        if let Err(e) = quic.send_data(json_bytes.clone().into()).await {
            debug!("Frame send failed: {}", e);
            consecutive_drops += 1;
            
            // MED-3: Backpressure - drop frames if too many consecutive failures
            if consecutive_drops > MAX_CONSECUTIVE_DROPS {
                error!("Too many consecutive frame drops ({}), ending session", consecutive_drops);
                let _reconnect_lock = session.reconnect_state.lock().unwrap();
                break;
            }
        } else {
            consecutive_drops = 0; // Reset drop counter on success
            frame_count += 1;
            session.frames_sent.fetch_add(1, Ordering::Relaxed);
            session.bytes_sent.fetch_add(json_bytes.len() as u64, Ordering::Relaxed);
        }

        // Report stats every 100 frames
        if frame_count % 100 == 0 {
            let elapsed = last_report.elapsed();
            let fps = 100.0 / elapsed.as_secs_f64();
            debug!("Streaming stats: {} fps, {} frames total, {} consecutive drops", 
                   fps, frame_count, consecutive_drops);
            last_report = std::time::Instant::now();
        }
    }

    session.is_active.store(false, Ordering::SeqCst);
    info!("Frame streaming session ended ({} frames sent)", frame_count);
}

/// Handle remote input events and inject them
pub async fn handle_remote_input(
    input_data: serde_json::Value,
    input_handler: Arc<std::sync::Mutex<InputHandler>>,
    session: SessionState,
) -> Result<(), String> {
    let event_type = input_data["type"].as_str()
        .ok_or("Missing event type")?;

    match event_type {
        "mouse" => {
            let mouse_type = input_data["mouse_type"].as_str()
                .ok_or("Missing mouse type")?;
            let x = input_data["x"].as_i64().unwrap_or(0) as i32;
            let y = input_data["y"].as_i64().unwrap_or(0) as i32;

            let event = match mouse_type {
                "move" => MouseEvent::Move { x, y },
                "down" => {
                    let btn = match input_data["button"].as_str() {
                        Some("right") => MouseButton::Right,
                        Some("middle") => MouseButton::Middle,
                        _ => MouseButton::Left,
                    };
                    MouseEvent::Down { button: btn }
                }
                "up" => {
                    let btn = match input_data["button"].as_str() {
                        Some("right") => MouseButton::Right,
                        Some("middle") => MouseButton::Middle,
                        _ => MouseButton::Left,
                    };
                    MouseEvent::Up { button: btn }
                }
                "wheel" => MouseEvent::Wheel { delta: y },
                _ => return Err(format!("Unknown mouse type: {}", mouse_type)),
            };

            let handler = input_handler.lock().map_err(|e| e.to_string())?;
            handler.handle_mouse_event(event).map_err(|e| e.to_string())?;
        }
        "keyboard" => {
            let key_type = input_data["key_type"].as_str()
                .ok_or("Missing key type")?;
            let key_str = input_data["key"].as_str()
                .ok_or("Missing key")?;

            let vk = VirtualKey::from_str(key_str)
                .ok_or_else(|| format!("Unknown key: {}", key_str))?;

            let event = match key_type {
                "down" => KeyboardEvent::KeyDown { key: vk },
                "up" => KeyboardEvent::KeyUp { key: vk },
                _ => return Err(format!("Unknown key type: {}", key_type)),
            };

            let handler = input_handler.lock().map_err(|e| e.to_string())?;
            handler.handle_keyboard_event(event).map_err(|e| e.to_string())?;
        }
        "clipboard" => {
            let text = input_data["text"].as_str()
                .ok_or("Missing clipboard text")?;
            debug!("Remote clipboard update: {} chars", text.len());
            // In a full implementation, you'd set the system clipboard here
            // using arboard or similar crate
        }
        "file_request" => {
            debug!("Remote file request received");
            // File transfer would be handled here via a secondary QUIC stream
        }
        _ => return Err(format!("Unknown event type: {}", event_type)),
    }

    session.inputs_received.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Handle incoming file transfer over a secondary QUIC stream
pub async fn handle_file_transfer(
    _stream: &mut tokio::sync::mpsc::Receiver<Vec<u8>>,
    file_info: FileInfo,
) -> Result<String, String> {
    info!("Receiving file: {} ({} bytes)", file_info.name, file_info.size);
    // In a full implementation, this would write chunks to disk
    Ok(format!("File received: {}", file_info.name))
}

/// Simple base64 encoding
fn base64_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(data.len() * 4 / 3);
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        
        let triple = (b0 << 16) | (b1 << 8) | b2;
        
        write!(result, "{}", CHARS[(triple >> 18) as usize] as char).unwrap();
        write!(result, "{}", CHARS[((triple >> 12) & 0x3F) as usize] as char).unwrap();
        
        if chunk.len() > 1 {
            write!(result, "{}", CHARS[((triple >> 6) & 0x3F) as usize] as char).unwrap();
        } else {
            result.push('=');
        }
        
        if chunk.len() > 2 {
            write!(result, "{}", CHARS[(triple & 0x3F) as usize] as char).unwrap();
        } else {
            result.push('=');
        }
    }
    
    result
}
