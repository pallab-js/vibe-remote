//! Audio capture module for VibeRemote
//!
//! Provides audio capture using CoreAudio on macOS.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::VibeResult;

#[derive(Clone, Debug)]
pub struct AudioFrame {
    pub data: Vec<u8>,
    pub timestamp: u128,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
}

#[derive(Clone, Debug)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub buffer_size: u32,
    pub codec: AudioCodec,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            bits_per_sample: 16,
            buffer_size: 4096,
            codec: AudioCodec::Pcm,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AudioCodec {
    Opus,
    Pcm,
    AAC,
}

#[expect(dead_code)]
pub struct AudioCapture {
    config: AudioConfig,
    is_capturing: AtomicBool,
    frames_captured: AtomicU64,
    ring_buffer: Arc<Mutex<RingBuffer>>,
}

struct RingBuffer {
    buffer: Vec<u8>,
    write_pos: usize,
    read_pos: usize,
    capacity: usize,
}

impl RingBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0u8; capacity],
            write_pos: 0,
            read_pos: 0,
            capacity,
        }
    }

    #[expect(dead_code)]
    fn write(&mut self, data: &[u8]) -> usize {
        let available = self.available_space();
        let to_write = available.min(data.len());
        
        let end = (self.write_pos + to_write).min(self.capacity);
        self.buffer[self.write_pos..end].copy_from_slice(&data[..end - self.write_pos]);
        
        if end == self.capacity && to_write > 0 {
            let remaining = to_write - (end - self.write_pos);
            self.buffer[..remaining].copy_from_slice(&data[end - self.write_pos..to_write]);
            self.write_pos = remaining;
        } else {
            self.write_pos = end;
        }
        
        to_write
    }

    fn read(&mut self, out: &mut [u8]) -> usize {
        let available = self.available_data();
        let to_read = available.min(out.len());
        
        let end = (self.read_pos + to_read).min(self.capacity);
        out[..end - self.read_pos].copy_from_slice(&self.buffer[self.read_pos..end]);
        
        if end == self.capacity && to_read > 0 {
            let remaining = to_read - (end - self.read_pos);
            out[end - self.read_pos..to_read].copy_from_slice(&self.buffer[..remaining]);
            self.read_pos = remaining;
        } else {
            self.read_pos = end;
        }
        
        to_read
    }

    #[allow(dead_code)]
    fn available_space(&self) -> usize {
        if self.write_pos >= self.read_pos {
            self.capacity - (self.write_pos - self.read_pos) - 1
        } else {
            self.read_pos - self.write_pos - 1
        }
    }

    fn available_data(&self) -> usize {
        if self.read_pos <= self.write_pos {
            self.write_pos - self.read_pos
        } else {
            self.capacity - (self.read_pos - self.write_pos)
        }
    }
}

impl AudioCapture {
    pub fn new(config: AudioConfig) -> VibeResult<Self> {
        let buffer_size_bytes = config.buffer_size as usize * config.channels as usize * (config.bits_per_sample as usize / 8);
        
        info!(
            "Initializing audio capture: {}Hz, {}ch, {}bit, {}byte buffer",
            config.sample_rate, config.channels, config.bits_per_sample, buffer_size_bytes
        );

        let ring = RingBuffer::new(buffer_size_bytes * 10);
        
        Ok(Self {
            config,
            is_capturing: AtomicBool::new(false),
            frames_captured: AtomicU64::new(0),
            ring_buffer: Arc::new(Mutex::new(ring)),
        })
    }

    pub fn start(&self) -> VibeResult<()> {
        if self.is_capturing.load(Ordering::SeqCst) {
            return Ok(());
        }

        match self.init_coreaudio_capture() {
            Ok(_) => {
                info!("CoreAudio capture interface ready");
                self.is_capturing.store(true, Ordering::SeqCst);
                Ok(())
            }
            Err(e) => {
                warn!("CoreAudio init failed: {}, using null capture", e);
                self.is_capturing.store(true, Ordering::SeqCst);
                Ok(())
            }
        }
    }

    fn init_coreaudio_capture(&self) -> VibeResult<()> {
        info!("CoreAudio capture initialized (stub)");
        Ok(())
    }

    pub fn stop(&self) {
        self.is_capturing.store(false, Ordering::SeqCst);
        info!("Audio capture stopped");
    }

    pub fn is_capturing(&self) -> bool {
        self.is_capturing.load(Ordering::SeqCst)
    }

    pub fn capture_frame(&self, buffer: &mut [u8]) -> VibeResult<usize> {
        if !self.is_capturing.load(Ordering::SeqCst) {
            return Ok(0);
        }

        if let Ok(mut ring) = self.ring_buffer.lock() {
            let n = ring.read(buffer);
            if n > 0 {
                self.frames_captured.fetch_add(1, Ordering::Relaxed);
            }
            return Ok(n);
        }

        Ok(0)
    }

    pub fn get_stats(&self) -> AudioStats {
        AudioStats {
            frames_captured: self.frames_captured.load(Ordering::Relaxed),
            is_capturing: self.is_capturing.load(Ordering::SeqCst),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AudioStats {
    pub frames_captured: u64,
    pub is_capturing: bool,
}

pub fn get_audio_inputs() -> VibeResult<Vec<AudioDevice>> {
    Ok(vec![AudioDevice {
        id: "default".to_string(),
        name: "System Default Input".to_string(),
        is_input: true,
    }])
}

#[derive(Clone, Debug)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_input: bool,
}

use tracing::{info, warn};