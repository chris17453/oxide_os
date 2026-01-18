//! Audio Subsystem for EFFLUX OS
//!
//! Provides audio device abstraction, PCM playback/capture, and mixing.

#![no_std]

extern crate alloc;

pub mod device;
pub mod format;
pub mod mixer;
pub mod buffer;

pub use device::{AudioDevice, AudioDeviceInfo, StreamConfig, StreamState};
pub use format::{SampleFormat, AudioFormat, ChannelLayout};
pub use mixer::{Mixer, MixerChannel};
pub use buffer::RingBuffer;

use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

/// Audio error type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioError {
    /// Device not found
    DeviceNotFound,
    /// Invalid configuration
    InvalidConfig,
    /// Buffer overflow
    BufferOverflow,
    /// Buffer underrun
    BufferUnderrun,
    /// Device busy
    DeviceBusy,
    /// Not supported
    NotSupported,
    /// I/O error
    IoError,
    /// Invalid state
    InvalidState,
}

/// Audio result type
pub type AudioResult<T> = Result<T, AudioError>;

/// Global audio device registry
static DEVICES: Mutex<Vec<Arc<dyn AudioDevice>>> = Mutex::new(Vec::new());

/// Global mixer
static MIXER: Mutex<Option<Mixer>> = Mutex::new(None);

/// Initialize audio subsystem
pub fn init() {
    *MIXER.lock() = Some(Mixer::new());
}

/// Register an audio device
pub fn register_device(device: Arc<dyn AudioDevice>) -> usize {
    let mut devices = DEVICES.lock();
    let id = devices.len();
    devices.push(device);
    id
}

/// Get device by index
pub fn get_device(index: usize) -> Option<Arc<dyn AudioDevice>> {
    DEVICES.lock().get(index).cloned()
}

/// Get all devices
pub fn devices() -> Vec<Arc<dyn AudioDevice>> {
    DEVICES.lock().clone()
}

/// Get the mixer
pub fn mixer() -> &'static Mutex<Option<Mixer>> {
    &MIXER
}

/// Set master volume (0-100)
pub fn set_master_volume(volume: u8) {
    if let Some(ref mut mixer) = *MIXER.lock() {
        mixer.set_master_volume(volume.min(100));
    }
}

/// Get master volume
pub fn get_master_volume() -> u8 {
    MIXER.lock().as_ref().map(|m| m.master_volume()).unwrap_or(100)
}
