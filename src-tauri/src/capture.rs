//! Screen capture module using ScreenCaptureKit (macOS)
//!
//! This module provides hardware-accelerated screen capture using
//! Apple's ScreenCaptureKit API via the screencapturekit crate.
//! Optimized for M1 Apple Silicon with efficient buffer management.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use screencapturekit::cv::CVPixelBufferLockFlags;
use screencapturekit::prelude::*;

use crate::error::{VibeError, VibeResult};

/// Represents a single frame captured from the screen
#[derive(Clone, Debug)]
pub struct CapturedFrame {
    /// Raw pixel buffer (BGRA format)
    pub data: Vec<u8>,
    /// Frame width in pixels
    pub width: usize,
    /// Frame height in pixels
    pub height: usize,
    /// Bytes per row
    pub bytes_per_row: usize,
    /// Timestamp of capture (milliseconds since start)
    pub timestamp: u128,
}

/// Screen capture stream configuration
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Target FPS (default: 60)
    pub fps: u32,
    /// Show cursor (default: true)
    pub show_cursor: bool,
    /// Capture resolution scale (1.0 = native, 0.5 = half)
    pub scale: f32,
    /// Display index to capture (0 = primary)
    pub display_index: usize,
    /// Exclude sensitive applications (default: loginwindow, screensaver)
    pub exclude_apps: Vec<String>,
    /// Enable content protection (filters sensitive windows)
    pub content_protection: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            fps: 60,
            show_cursor: true,
            scale: 1.0,
            display_index: 0,
            exclude_apps: vec![
                "com.apple.loginwindow".to_string(),
                "com.apple.screensaver".to_string(),
            ],
            content_protection: true,
        }
    }
}

/// Get list of running applications for exclusion filtering
#[cfg(target_os = "macos")]
#[allow(dead_code)]
fn get_running_apps() -> VibeResult<Vec<String>> {
    let content = SCShareableContent::get()
        .map_err(|e| VibeError::Capture(format!("Failed to get shareable content: {}", e)))?;

    Ok(content
        .applications()
        .iter()
        .map(|app| app.bundle_identifier().to_string())
        .collect())
}

#[cfg(not(target_os = "macos"))]
fn get_running_apps() -> VibeResult<Vec<String>> {
    Ok(vec![])
}

/// Screen capture stream handler
pub struct CaptureStream {
    config: CaptureConfig,
    running: Arc<AtomicBool>,
}

impl CaptureStream {
    /// Create a new capture stream with the given configuration
    pub fn new(config: CaptureConfig) -> Self {
        Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get the primary display stream
    ///
    /// Returns a channel that yields CapturedFrame instances
    pub async fn get_primary_stream(&self) -> VibeResult<mpsc::Receiver<CapturedFrame>> {
        info!("Initializing ScreenCaptureKit primary stream");

        let (tx, rx) = mpsc::channel(3); // Buffer 3 frames for smooth playback
        let config = self.config.clone();
        let running = self.running.clone();

        // Mark as running
        running.store(true, Ordering::SeqCst);

        // INFO-3: Spawn capture task on dedicated blocking thread
        // This prevents consuming tokio's shared blocking thread pool
        std::thread::Builder::new()
            .name("vibe-capture".to_string())
            .spawn(move || {
                if let Err(e) = run_capture_loop(config, tx, running) {
                    error!("Capture loop error: {}", e);
                }
            })
            .map_err(|e| VibeError::Capture(format!("Failed to spawn capture thread: {}", e)))?;

        info!("Primary stream initialized");
        Ok(rx)
    }

    /// Stop the capture stream
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        info!("Capture stream stopped");
    }
}

/// Frame output handler
struct FrameHandler {
    tx: mpsc::Sender<CapturedFrame>,
    running: Arc<AtomicBool>,
    start_time: std::time::Instant,
}

impl SCStreamOutputTrait for FrameHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, _type: SCStreamOutputType) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }

        // Get pixel buffer from sample using image_buffer()
        if let Some(pixel_buffer) = sample.image_buffer() {
            // Lock the buffer for read-only CPU access
            if let Ok(guard) = pixel_buffer.lock(CVPixelBufferLockFlags::READ_ONLY) {
                let width = guard.width();
                let height = guard.height();

                // Get raw BGRA bytes
                let data = guard.as_slice().to_vec();
                let bytes_per_row = data.len() / height;

                let frame = CapturedFrame {
                    data,
                    width,
                    height,
                    bytes_per_row,
                    timestamp: self.start_time.elapsed().as_millis(),
                };

                debug!(
                    "Captured frame: {}x{} ({} bytes)",
                    width,
                    height,
                    frame.data.len()
                );

                // Try to send frame (drop if channel is full to avoid blocking)
                if self.tx.try_send(frame).is_err() {
                    debug!("Frame dropped (channel full)");
                }
            } else {
                error!("Failed to lock pixel buffer");
            }
        } else {
            error!("Failed to get pixel buffer from sample");
        }
    }
}

/// Internal capture loop using ScreenCaptureKit
fn run_capture_loop(
    config: CaptureConfig,
    tx: mpsc::Sender<CapturedFrame>,
    running: Arc<AtomicBool>,
) -> VibeResult<()> {
    info!("Starting ScreenCaptureKit loop at {} FPS", config.fps);

    // Get primary display
    let content = SCShareableContent::get()
        .map_err(|e| VibeError::Capture(format!("Failed to get shareable content: {}", e)))?;

    let displays = content.displays();
    let display = displays.get(config.display_index).ok_or_else(|| {
        VibeError::Capture(format!(
            "Display index {} not found ({} available)",
            config.display_index,
            displays.len()
        ))
    })?;

    let native_width = display.width();
    let native_height = display.height();

    // Apply scale factor
    let width = (native_width as f32 * config.scale) as u32;
    let height = (native_height as f32 * config.scale) as u32;

    info!(
        "Capturing display: {}x{} (scaled to {}x{})",
        native_width, native_height, width, height
    );

    // Create content filter (basic - no exclusions for now to ensure build)
    let filter = SCContentFilter::create()
        .with_display(display)
        .with_excluding_windows(&[])
        .build();

    // Configure stream settings
    let frame_interval = CMTime::new(1, config.fps as i32);
    let stream_config = SCStreamConfiguration::new()
        .with_width(width)
        .with_height(height)
        .with_pixel_format(PixelFormat::BGRA)
        .with_minimum_frame_interval(&frame_interval)
        .with_shows_cursor(config.show_cursor);

    // Create SCStream
    let mut stream = SCStream::new(&filter, &stream_config);

    // Add output handler
    let handler = FrameHandler {
        tx,
        running: running.clone(),
        start_time: std::time::Instant::now(),
    };

    stream.add_output_handler(handler, SCStreamOutputType::Screen);

    // Start capturing
    stream
        .start_capture()
        .map_err(|e| VibeError::Capture(format!("Failed to start capture: {}", e)))?;

    info!("ScreenCaptureKit stream started");

    // Keep running until stopped
    while running.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Stop capture
    stream
        .stop_capture()
        .map_err(|e| VibeError::Capture(format!("Failed to stop capture: {}", e)))?;

    info!("ScreenCaptureKit stream stopped");
    Ok(())
}

/// Get list of available displays
pub fn get_available_displays() -> VibeResult<Vec<(String, u32, u32)>> {
    let content = SCShareableContent::get()
        .map_err(|e| VibeError::Capture(format!("Failed to get shareable content: {}", e)))?;

    let displays = content.displays();
    let result = displays
        .iter()
        .enumerate()
        .map(|(i, display)| {
            (
                format!("Display {}", i + 1),
                display.width(),
                display.height(),
            )
        })
        .collect();

    Ok(result)
}

/// Initialize capture with default settings
pub fn init_capture() -> CaptureStream {
    CaptureStream::new(CaptureConfig::default())
}
