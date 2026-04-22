//! Adaptive bitrate controller based on network conditions

use std::sync::atomic::{AtomicU32, Ordering};

/// Adaptive bitrate controller that adjusts encoding quality
/// based on measured RTT and packet loss
pub struct AdaptiveBitrateController {
    current_bitrate: AtomicU32,
    min_bitrate: u32,
    max_bitrate: u32,
    target_latency_ms: u32,
    pub consecutive_drops: AtomicU32,
}

impl AdaptiveBitrateController {
    pub fn new(min: u32, max: u32) -> Self {
        Self {
            current_bitrate: AtomicU32::new(max / 2),
            min_bitrate: min,
            max_bitrate: max,
            target_latency_ms: 80,
            consecutive_drops: AtomicU32::new(0),
        }
    }

    pub fn get_bitrate(&self) -> u32 {
        self.current_bitrate.load(Ordering::SeqCst)
    }

    pub fn update(&self, rtt_ms: u32, drop_rate: f32) -> u32 {
        let current = self.current_bitrate.load(Ordering::SeqCst);

        let new_bitrate = if rtt_ms > self.target_latency_ms || drop_rate > 0.02 {
            // Congestion detected - back off 25%
            let drops = self.consecutive_drops.fetch_add(1, Ordering::SeqCst) + 1;
            let reduction = if drops > 3 { 0.50 } else { 0.25 };
            ((current as f32 * (1.0 - reduction)) as u32).max(self.min_bitrate)
        } else {
            // Network healthy - increase 10%
            self.consecutive_drops.store(0, Ordering::SeqCst);
            ((current as f32 * 1.10) as u32).min(self.max_bitrate)
        };

        self.current_bitrate.store(new_bitrate, Ordering::SeqCst);
        new_bitrate
    }

    pub fn reset(&self) {
        self.current_bitrate
            .store(self.max_bitrate / 2, Ordering::SeqCst);
        self.consecutive_drops.store(0, Ordering::SeqCst);
    }
}

impl Default for AdaptiveBitrateController {
    fn default() -> Self {
        Self::new(500_000, 8_000_000) // 500K - 8Mbps
    }
}

// Quality presets
pub struct QualityPreset {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub bitrate: u32,
    pub framerate: u32,
}

impl QualityPreset {
    pub fn low() -> Self {
        Self {
            name: "low",
            width: 1280,
            height: 720,
            bitrate: 1_000_000,
            framerate: 30,
        }
    }

    pub fn medium() -> Self {
        Self {
            name: "medium",
            width: 1920,
            height: 1080,
            bitrate: 2_500_000,
            framerate: 30,
        }
    }

    pub fn high() -> Self {
        Self {
            name: "high",
            width: 1920,
            height: 1080,
            bitrate: 4_000_000,
            framerate: 60,
        }
    }

    pub fn ultra() -> Self {
        Self {
            name: "ultra",
            width: 1920,
            height: 1080,
            bitrate: 8_000_000,
            framerate: 60,
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "low" => Self::low(),
            "medium" => Self::medium(),
            "high" => Self::high(),
            "ultra" => Self::ultra(),
            _ => Self::high(),
        }
    }
}
