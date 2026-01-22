//! Performance monitoring for framebuffer operations

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Performance statistics for framebuffer operations
pub struct PerfStats {
    /// Total frames rendered
    pub frames: AtomicU64,
    /// Total pixels written
    pub pixels_written: AtomicU64,
    /// Total flush operations
    pub flushes: AtomicU64,
    /// Last frame timestamp (in timer ticks)
    pub last_frame_time: AtomicU64,
    /// Frame time accumulator for FPS calculation
    pub frame_time_accum: AtomicU64,
    /// Current FPS (updated every second)
    pub current_fps: AtomicU32,
}

impl PerfStats {
    /// Create new performance statistics
    pub const fn new() -> Self {
        PerfStats {
            frames: AtomicU64::new(0),
            pixels_written: AtomicU64::new(0),
            flushes: AtomicU64::new(0),
            last_frame_time: AtomicU64::new(0),
            frame_time_accum: AtomicU64::new(0),
            current_fps: AtomicU32::new(0),
        }
    }

    /// Record a frame rendered
    pub fn record_frame(&self, time: u64) {
        self.frames.fetch_add(1, Ordering::Relaxed);

        let last_time = self.last_frame_time.swap(time, Ordering::Relaxed);
        if last_time > 0 {
            let frame_time = time.saturating_sub(last_time);
            let accum = self
                .frame_time_accum
                .fetch_add(frame_time, Ordering::Relaxed);

            // Update FPS every ~1000 frames or ~1 second worth of frames
            let frames_count = self.frames.load(Ordering::Relaxed);
            if frames_count % 100 == 0 || accum > 1000 {
                // Calculate FPS: frames / (time_in_ms / 1000)
                if accum > 0 {
                    let fps = (100 * 1000) / accum;
                    self.current_fps.store(fps as u32, Ordering::Relaxed);
                    self.frame_time_accum.store(0, Ordering::Relaxed);
                }
            }
        }
    }

    /// Record pixels written
    pub fn record_pixels(&self, count: u64) {
        self.pixels_written.fetch_add(count, Ordering::Relaxed);
    }

    /// Record a flush operation
    pub fn record_flush(&self) {
        self.flushes.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current FPS
    pub fn fps(&self) -> u32 {
        self.current_fps.load(Ordering::Relaxed)
    }

    /// Get total frames
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    /// Get total pixels written
    pub fn pixels(&self) -> u64 {
        self.pixels_written.load(Ordering::Relaxed)
    }

    /// Get total flushes
    pub fn flushes(&self) -> u64 {
        self.flushes.load(Ordering::Relaxed)
    }

    /// Reset statistics
    pub fn reset(&self) {
        self.frames.store(0, Ordering::Relaxed);
        self.pixels_written.store(0, Ordering::Relaxed);
        self.flushes.store(0, Ordering::Relaxed);
        self.last_frame_time.store(0, Ordering::Relaxed);
        self.frame_time_accum.store(0, Ordering::Relaxed);
        self.current_fps.store(0, Ordering::Relaxed);
    }
}

/// Global performance statistics
static PERF_STATS: PerfStats = PerfStats::new();

/// Record a frame rendered
pub fn record_frame(time_ms: u64) {
    PERF_STATS.record_frame(time_ms);
}

/// Record pixels written
pub fn record_pixels(count: u64) {
    PERF_STATS.record_pixels(count);
}

/// Record a flush operation
pub fn record_flush() {
    PERF_STATS.record_flush();
}

/// Get current FPS
pub fn get_fps() -> u32 {
    PERF_STATS.fps()
}

/// Get statistics
pub fn get_stats() -> (u64, u64, u64, u32) {
    (
        PERF_STATS.frames(),
        PERF_STATS.pixels(),
        PERF_STATS.flushes(),
        PERF_STATS.fps(),
    )
}

/// Reset statistics
pub fn reset_stats() {
    PERF_STATS.reset();
}
