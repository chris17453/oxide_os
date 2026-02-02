//! Audio Device Trait

use crate::{AudioError, AudioResult, SampleFormat};
use alloc::string::String;
use alloc::vec::Vec;

/// Audio device information
#[derive(Debug, Clone)]
pub struct AudioDeviceInfo {
    /// Device name
    pub name: String,
    /// Description
    pub description: String,
    /// Supported sample rates
    pub sample_rates: Vec<u32>,
    /// Supported channel counts
    pub channels: Vec<u8>,
    /// Supported sample formats
    pub formats: Vec<SampleFormat>,
    /// Maximum buffer size in frames
    pub max_buffer_frames: u32,
    /// Minimum buffer size in frames
    pub min_buffer_frames: u32,
}

impl Default for AudioDeviceInfo {
    fn default() -> Self {
        AudioDeviceInfo {
            name: String::new(),
            description: String::new(),
            sample_rates: Vec::new(),
            channels: Vec::new(),
            formats: Vec::new(),
            max_buffer_frames: 8192,
            min_buffer_frames: 64,
        }
    }
}

/// Stream configuration
#[derive(Debug, Clone, Copy)]
pub struct StreamConfig {
    /// Number of channels (1=mono, 2=stereo, etc.)
    pub channels: u8,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Sample format
    pub format: SampleFormat,
    /// Buffer size in frames
    pub buffer_frames: u32,
    /// Period size in frames (for interrupts)
    pub period_frames: u32,
}

impl Default for StreamConfig {
    fn default() -> Self {
        StreamConfig {
            channels: 2,
            sample_rate: 44100,
            format: SampleFormat::S16LE,
            buffer_frames: 4096,
            period_frames: 1024,
        }
    }
}

impl StreamConfig {
    /// Create a new stream configuration
    pub fn new(channels: u8, sample_rate: u32, format: SampleFormat) -> Self {
        StreamConfig {
            channels,
            sample_rate,
            format,
            ..Default::default()
        }
    }

    /// Get bytes per frame
    pub fn bytes_per_frame(&self) -> usize {
        self.channels as usize * self.format.bytes_per_sample()
    }

    /// Get bytes per second
    pub fn bytes_per_second(&self) -> usize {
        self.bytes_per_frame() * self.sample_rate as usize
    }

    /// Get buffer size in bytes
    pub fn buffer_bytes(&self) -> usize {
        self.buffer_frames as usize * self.bytes_per_frame()
    }
}

/// Stream state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    /// Initial state, not configured
    Setup,
    /// Configured and ready to start
    Prepared,
    /// Playing or recording
    Running,
    /// Stopped
    Stopped,
    /// Paused
    Paused,
}

/// Audio device trait
pub trait AudioDevice: Send + Sync {
    /// Get device information
    fn info(&self) -> AudioDeviceInfo;

    /// Check if device supports playback
    fn supports_playback(&self) -> bool;

    /// Check if device supports capture
    fn supports_capture(&self) -> bool;

    /// Get current stream state
    fn state(&self) -> StreamState;

    /// Configure stream for playback
    fn configure(&self, config: StreamConfig) -> AudioResult<()>;

    /// Prepare stream for operation
    fn prepare(&self) -> AudioResult<()>;

    /// Start playback/capture
    fn start(&self) -> AudioResult<()>;

    /// Stop playback/capture
    fn stop(&self) -> AudioResult<()>;

    /// Pause stream
    fn pause(&self) -> AudioResult<()> {
        Err(AudioError::NotSupported)
    }

    /// Resume stream
    fn resume(&self) -> AudioResult<()> {
        Err(AudioError::NotSupported)
    }

    /// Release/reset stream
    fn release(&self) -> AudioResult<()>;

    /// Write PCM data for playback
    fn write(&self, data: &[u8]) -> AudioResult<usize>;

    /// Read PCM data from capture
    fn read(&self, data: &mut [u8]) -> AudioResult<usize>;

    /// Get available space for writing (in bytes)
    fn write_available(&self) -> usize;

    /// Get available data for reading (in bytes)
    fn read_available(&self) -> usize;

    /// Get/set volume (0-100)
    fn get_volume(&self) -> u8 {
        100
    }

    fn set_volume(&self, _volume: u8) -> AudioResult<()> {
        Err(AudioError::NotSupported)
    }

    /// Get/set mute state
    fn is_muted(&self) -> bool {
        false
    }

    fn set_mute(&self, _mute: bool) -> AudioResult<()> {
        Err(AudioError::NotSupported)
    }

    /// Get current stream position in frames
    fn get_position(&self) -> u64 {
        0
    }

    /// Get stream latency in frames
    fn get_latency(&self) -> u32 {
        0
    }
}
