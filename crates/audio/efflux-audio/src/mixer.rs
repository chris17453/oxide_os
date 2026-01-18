//! Audio Mixer

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Audio mixer
pub struct Mixer {
    /// Master volume (0-100)
    master_volume: AtomicU8,
    /// Master mute
    master_mute: AtomicBool,
    /// Mixer channels
    channels: Vec<MixerChannel>,
}

impl Mixer {
    /// Create a new mixer
    pub fn new() -> Self {
        Mixer {
            master_volume: AtomicU8::new(100),
            master_mute: AtomicBool::new(false),
            channels: Vec::new(),
        }
    }

    /// Get master volume
    pub fn master_volume(&self) -> u8 {
        self.master_volume.load(Ordering::SeqCst)
    }

    /// Set master volume
    pub fn set_master_volume(&mut self, volume: u8) {
        self.master_volume.store(volume.min(100), Ordering::SeqCst);
    }

    /// Check if master is muted
    pub fn is_master_muted(&self) -> bool {
        self.master_mute.load(Ordering::SeqCst)
    }

    /// Set master mute
    pub fn set_master_mute(&mut self, mute: bool) {
        self.master_mute.store(mute, Ordering::SeqCst);
    }

    /// Add a mixer channel
    pub fn add_channel(&mut self, name: String) -> usize {
        let id = self.channels.len();
        self.channels.push(MixerChannel::new(name));
        id
    }

    /// Get channel by index
    pub fn channel(&self, index: usize) -> Option<&MixerChannel> {
        self.channels.get(index)
    }

    /// Get channel by index (mutable)
    pub fn channel_mut(&mut self, index: usize) -> Option<&mut MixerChannel> {
        self.channels.get_mut(index)
    }

    /// Get all channels
    pub fn channels(&self) -> &[MixerChannel] {
        &self.channels
    }

    /// Apply master volume to a sample (0-32767 range for S16)
    pub fn apply_master(&self, sample: i16) -> i16 {
        if self.is_master_muted() {
            return 0;
        }

        let volume = self.master_volume() as i32;
        ((sample as i32 * volume) / 100) as i16
    }

    /// Apply master volume to stereo samples
    pub fn apply_master_stereo(&self, left: i16, right: i16) -> (i16, i16) {
        (self.apply_master(left), self.apply_master(right))
    }

    /// Mix multiple audio streams
    pub fn mix_streams(&self, streams: &[&[i16]], output: &mut [i16]) {
        // Clear output
        for sample in output.iter_mut() {
            *sample = 0;
        }

        // Mix all streams
        for stream in streams {
            for (i, &sample) in stream.iter().enumerate() {
                if i < output.len() {
                    // Saturating add to prevent overflow
                    output[i] = output[i].saturating_add(sample);
                }
            }
        }

        // Apply master volume
        for sample in output.iter_mut() {
            *sample = self.apply_master(*sample);
        }
    }
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

/// Mixer channel
pub struct MixerChannel {
    /// Channel name
    name: String,
    /// Volume (0-100)
    volume: AtomicU8,
    /// Left volume for stereo (0-100)
    volume_left: AtomicU8,
    /// Right volume for stereo (0-100)
    volume_right: AtomicU8,
    /// Muted
    muted: AtomicBool,
}

impl MixerChannel {
    /// Create a new mixer channel
    pub fn new(name: String) -> Self {
        MixerChannel {
            name,
            volume: AtomicU8::new(100),
            volume_left: AtomicU8::new(100),
            volume_right: AtomicU8::new(100),
            muted: AtomicBool::new(false),
        }
    }

    /// Get channel name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get volume
    pub fn volume(&self) -> u8 {
        self.volume.load(Ordering::SeqCst)
    }

    /// Set volume
    pub fn set_volume(&self, volume: u8) {
        let v = volume.min(100);
        self.volume.store(v, Ordering::SeqCst);
        self.volume_left.store(v, Ordering::SeqCst);
        self.volume_right.store(v, Ordering::SeqCst);
    }

    /// Get stereo volume
    pub fn stereo_volume(&self) -> (u8, u8) {
        (
            self.volume_left.load(Ordering::SeqCst),
            self.volume_right.load(Ordering::SeqCst),
        )
    }

    /// Set stereo volume
    pub fn set_stereo_volume(&self, left: u8, right: u8) {
        self.volume_left.store(left.min(100), Ordering::SeqCst);
        self.volume_right.store(right.min(100), Ordering::SeqCst);
        // Update mono volume to average
        self.volume.store((left.min(100) + right.min(100)) / 2, Ordering::SeqCst);
    }

    /// Check if muted
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::SeqCst)
    }

    /// Set mute
    pub fn set_mute(&self, mute: bool) {
        self.muted.store(mute, Ordering::SeqCst);
    }

    /// Apply volume to a mono sample
    pub fn apply_volume(&self, sample: i16) -> i16 {
        if self.is_muted() {
            return 0;
        }

        let volume = self.volume() as i32;
        ((sample as i32 * volume) / 100) as i16
    }

    /// Apply volume to stereo samples
    pub fn apply_stereo_volume(&self, left: i16, right: i16) -> (i16, i16) {
        if self.is_muted() {
            return (0, 0);
        }

        let (vol_l, vol_r) = self.stereo_volume();
        (
            ((left as i32 * vol_l as i32) / 100) as i16,
            ((right as i32 * vol_r as i32) / 100) as i16,
        )
    }
}

/// OSS mixer ioctl constants
pub mod oss_mixer {
    pub const SOUND_MIXER_VOLUME: u32 = 0;
    pub const SOUND_MIXER_BASS: u32 = 1;
    pub const SOUND_MIXER_TREBLE: u32 = 2;
    pub const SOUND_MIXER_SYNTH: u32 = 3;
    pub const SOUND_MIXER_PCM: u32 = 4;
    pub const SOUND_MIXER_SPEAKER: u32 = 5;
    pub const SOUND_MIXER_LINE: u32 = 6;
    pub const SOUND_MIXER_MIC: u32 = 7;
    pub const SOUND_MIXER_CD: u32 = 8;
    pub const SOUND_MIXER_IMIX: u32 = 9;
    pub const SOUND_MIXER_ALTPCM: u32 = 10;
    pub const SOUND_MIXER_RECLEV: u32 = 11;
    pub const SOUND_MIXER_IGAIN: u32 = 12;
    pub const SOUND_MIXER_OGAIN: u32 = 13;

    pub const SOUND_MIXER_READ_VOLUME: u32 = 0x80044D00;
    pub const SOUND_MIXER_WRITE_VOLUME: u32 = 0xC0044D00;
}

/// Encode stereo volume for OSS (left in low byte, right in high byte)
pub fn encode_oss_stereo(left: u8, right: u8) -> u32 {
    ((right as u32) << 8) | (left as u32)
}

/// Decode OSS stereo volume
pub fn decode_oss_stereo(value: u32) -> (u8, u8) {
    ((value & 0xFF) as u8, ((value >> 8) & 0xFF) as u8)
}
