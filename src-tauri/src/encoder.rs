//! Frame encoding and compression for network streaming
//!
//! Provides efficient frame encoding using JPEG compression
//! and delta encoding for reduced bandwidth.
//! Uses buffer pooling to minimize allocations.

use std::io::Cursor;
use image::{ImageBuffer, Rgba, ImageFormat};
use flate2::read::ZlibEncoder;
use flate2::Compression;
use std::io::Read;
use std::collections::VecDeque;

use crate::error::{VibeResult, VibeError};

/// Reusable buffer pool to minimize allocations
pub struct BufferPool {
    buffers: VecDeque<Vec<u8>>,
    max_pool_size: usize,
}

impl BufferPool {
    pub fn new(max_pool_size: usize) -> Self {
        Self {
            buffers: VecDeque::new(),
            max_pool_size,
        }
    }

    /// Get a buffer from the pool or allocate a new one
    pub fn get(&mut self, min_capacity: usize) -> Vec<u8> {
        if let Some(mut buf) = self.buffers.pop_front() {
            if buf.capacity() >= min_capacity {
                buf.clear();
                return buf;
            }
        }
        Vec::with_capacity(min_capacity)
    }

    /// Return a buffer to the pool
    pub fn put(&mut self, buf: Vec<u8>) {
        if self.buffers.len() < self.max_pool_size {
            self.buffers.push_back(buf);
        }
        // Otherwise drop it to limit memory usage
    }
}

/// Encoded frame data ready for network transmission
#[derive(Clone, Debug)]
pub struct EncodedFrame {
    /// Compressed frame data
    pub data: Vec<u8>,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Frame sequence number
    pub sequence: u64,
    /// Timestamp (ms since session start)
    pub timestamp: u128,
    /// Frame type (keyframe or delta)
    pub frame_type: FrameType,
}

/// Frame type for compression optimization
#[derive(Clone, Debug)]
pub enum FrameType {
    /// Full frame (I-frame)
    KeyFrame,
    /// Delta from previous frame (P-frame)
    DeltaFrame,
}

/// Frame encoder with compression
#[derive(Clone)]
pub struct FrameEncoder {
    /// Current sequence number
    sequence: u64,
    /// Previous frame for delta encoding (stored as compressed delta)
    previous_frame: Option<Vec<u8>>,
    /// JPEG quality (1-100)
    quality: u8,
    /// Target resolution scale (1.0 = full, 0.5 = half)
    scale: f32,
    /// Buffer pool for reuse
    pool: std::sync::Arc<std::sync::Mutex<BufferPool>>,
}

impl FrameEncoder {
    /// Create new encoder with default settings
    pub fn new(quality: u8, scale: f32) -> Self {
        Self {
            sequence: 0,
            previous_frame: None,
            quality,
            scale,
            pool: std::sync::Arc::new(std::sync::Mutex::new(BufferPool::new(8))),
        }
    }

    /// Encode a raw frame for network transmission
    pub fn encode_frame(&mut self, data: &[u8], width: usize, height: usize, timestamp: u128) -> VibeResult<EncodedFrame> {
        let scaled_width = (width as f32 * self.scale) as u32;
        let scaled_height = (height as f32 * self.scale) as u32;

        // Determine if this should be a keyframe
        let is_keyframe = self.previous_frame.is_none() || self.sequence % 30 == 0;

        let encoded_data = if is_keyframe {
            // Encode full frame as JPEG
            self.encode_jpeg_frame(data, width, height)?
        } else {
            // Encode delta from previous frame
            self.encode_delta_frame(data, width, height)?
        };

        let frame = EncodedFrame {
            data: encoded_data,
            width: scaled_width,
            height: scaled_height,
            sequence: self.sequence,
            timestamp,
            frame_type: if is_keyframe { FrameType::KeyFrame } else { FrameType::DeltaFrame },
        };

        self.sequence += 1;
        
        Ok(frame)
    }

    /// Encode as full JPEG frame
    fn encode_jpeg_frame(&mut self, data: &[u8], width: usize, height: usize) -> VibeResult<Vec<u8>> {
        // Convert BGRA to RGBA
        let mut rgba_data = vec![0u8; data.len()];
        for i in (0..data.len()).step_by(4) {
            rgba_data[i] = data[i + 2];     // R
            rgba_data[i + 1] = data[i + 1]; // G
            rgba_data[i + 2] = data[i];     // B
            rgba_data[i + 3] = data[i + 3]; // A
        }

        // Create image buffer
        let img = ImageBuffer::<Rgba<u8>, _>::from_raw(width as u32, height as u32, rgba_data)
            .ok_or_else(|| VibeError::Capture("Failed to create image buffer".to_string()))?;

        // Scale if needed
        let img = if self.scale != 1.0 {
            let scaled_width = (width as f32 * self.scale) as u32;
            let scaled_height = (height as f32 * self.scale) as u32;
            image::imageops::resize(&img, scaled_width, scaled_height, image::imageops::FilterType::Nearest)
        } else {
            img
        };

        // Encode to JPEG
        let mut jpeg_data = Vec::new();
        let mut cursor = Cursor::new(&mut jpeg_data);
        img.write_to(&mut cursor, ImageFormat::Jpeg)
            .map_err(|e| VibeError::Capture(format!("JPEG encoding failed: {}", e)))?;

        // Further compress with zlib
        let mut encoder = ZlibEncoder::new(jpeg_data.as_slice(), Compression::fast());
        let mut compressed = Vec::new();
        encoder.read_to_end(&mut compressed)
            .map_err(|e| VibeError::Capture(format!("Zlib compression failed: {}", e)))?;

        // Store for delta encoding
        self.previous_frame = Some(data.to_vec());

        Ok(compressed)
    }

    /// Encode delta from previous frame
    fn encode_delta_frame(&mut self, data: &[u8], width: usize, height: usize) -> VibeResult<Vec<u8>> {
        if let Some(ref prev) = self.previous_frame {
            // Calculate XOR delta
            let mut delta = vec![0u8; data.len()];
            let mut changed_pixels = 0;
            
            for i in 0..data.len() {
                delta[i] = data[i] ^ prev[i];
                if delta[i] != 0 {
                    changed_pixels += 1;
                }
            }

            // If more than 50% changed, send full frame instead
            let threshold = (data.len() as f32 * 0.5) as usize;
            if changed_pixels > threshold {
                return self.encode_jpeg_frame(data, width, height);
            }

            // Compress delta
            let mut encoder = ZlibEncoder::new(delta.as_slice(), Compression::fast());
            let mut compressed = Vec::new();
            encoder.read_to_end(&mut compressed)
                .map_err(|e| VibeError::Capture(format!("Delta compression failed: {}", e)))?;

            // Update previous frame
            self.previous_frame = Some(data.to_vec());

            Ok(compressed)
        } else {
            // No previous frame, send full frame
            self.encode_jpeg_frame(data, width, height)
        }
    }

    /// Decode a received frame
    pub fn decode_frame(&self, encoded: &EncodedFrame) -> VibeResult<(Vec<u8>, u32, u32)> {
        // Decompress zlib
        let mut decoder = flate2::read::ZlibDecoder::new(encoded.data.as_slice());
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)
            .map_err(|e| VibeError::Capture(format!("Zlib decompression failed: {}", e)))?;

        // Decode JPEG
        let img = image::load_from_memory(&decompressed)
            .map_err(|e| VibeError::Capture(format!("JPEG decoding failed: {}", e)))?;

        let width = img.width();
        let height = img.height();
        let rgba_data = img.to_rgba8().into_raw();

        // Convert RGBA to BGRA for display
        let mut bgra_data = vec![0u8; rgba_data.len()];
        for i in (0..rgba_data.len()).step_by(4) {
            bgra_data[i] = rgba_data[i + 2];     // B
            bgra_data[i + 1] = rgba_data[i + 1]; // G
            bgra_data[i + 2] = rgba_data[i];     // R
            bgra_data[i + 3] = rgba_data[i + 3]; // A
        }

        Ok((bgra_data, width, height))
    }

    /// Get compression ratio stats
    pub fn get_stats(&self) -> FrameEncoderStats {
        FrameEncoderStats {
            sequence: self.sequence,
            quality: self.quality,
            scale: self.scale,
        }
    }
}

/// Encoder statistics for monitoring
pub struct FrameEncoderStats {
    pub sequence: u64,
    pub quality: u8,
    pub scale: f32,
}

/// Default encoder for quick use
impl Default for FrameEncoder {
    fn default() -> Self {
        Self::new(85, 0.75) // 85% quality, 75% resolution
    }
}
