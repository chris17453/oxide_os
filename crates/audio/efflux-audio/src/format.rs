//! Audio Formats

/// Sample format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// Unsigned 8-bit
    U8,
    /// Signed 8-bit
    S8,
    /// Signed 16-bit little-endian
    S16LE,
    /// Signed 16-bit big-endian
    S16BE,
    /// Signed 24-bit little-endian (packed)
    S24LE,
    /// Signed 24-bit big-endian (packed)
    S24BE,
    /// Signed 24-bit in 32-bit container little-endian
    S24LE32,
    /// Signed 24-bit in 32-bit container big-endian
    S24BE32,
    /// Signed 32-bit little-endian
    S32LE,
    /// Signed 32-bit big-endian
    S32BE,
    /// Float 32-bit little-endian
    F32LE,
    /// Float 32-bit big-endian
    F32BE,
    /// Float 64-bit little-endian
    F64LE,
    /// Float 64-bit big-endian
    F64BE,
    /// IMA ADPCM
    ImaAdpcm,
    /// Mu-law
    MuLaw,
    /// A-law
    ALaw,
}

impl SampleFormat {
    /// Get bytes per sample
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            SampleFormat::U8 | SampleFormat::S8 => 1,
            SampleFormat::S16LE | SampleFormat::S16BE => 2,
            SampleFormat::S24LE | SampleFormat::S24BE => 3,
            SampleFormat::S24LE32 | SampleFormat::S24BE32 |
            SampleFormat::S32LE | SampleFormat::S32BE |
            SampleFormat::F32LE | SampleFormat::F32BE => 4,
            SampleFormat::F64LE | SampleFormat::F64BE => 8,
            SampleFormat::ImaAdpcm => 1,  // Variable, approximate
            SampleFormat::MuLaw | SampleFormat::ALaw => 1,
        }
    }

    /// Check if format is floating point
    pub fn is_float(&self) -> bool {
        matches!(self,
            SampleFormat::F32LE | SampleFormat::F32BE |
            SampleFormat::F64LE | SampleFormat::F64BE)
    }

    /// Check if format is signed
    pub fn is_signed(&self) -> bool {
        !matches!(self, SampleFormat::U8)
    }

    /// Check if format is little-endian
    pub fn is_little_endian(&self) -> bool {
        matches!(self,
            SampleFormat::U8 | SampleFormat::S8 |
            SampleFormat::S16LE | SampleFormat::S24LE | SampleFormat::S24LE32 |
            SampleFormat::S32LE | SampleFormat::F32LE | SampleFormat::F64LE |
            SampleFormat::ImaAdpcm | SampleFormat::MuLaw | SampleFormat::ALaw)
    }

    /// Get bits per sample
    pub fn bits_per_sample(&self) -> u32 {
        match self {
            SampleFormat::U8 | SampleFormat::S8 |
            SampleFormat::MuLaw | SampleFormat::ALaw => 8,
            SampleFormat::S16LE | SampleFormat::S16BE => 16,
            SampleFormat::S24LE | SampleFormat::S24BE => 24,
            SampleFormat::S24LE32 | SampleFormat::S24BE32 |
            SampleFormat::S32LE | SampleFormat::S32BE |
            SampleFormat::F32LE | SampleFormat::F32BE => 32,
            SampleFormat::F64LE | SampleFormat::F64BE => 64,
            SampleFormat::ImaAdpcm => 4,
        }
    }
}

/// Channel layout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelLayout {
    /// Mono
    Mono,
    /// Stereo (left, right)
    Stereo,
    /// 2.1 (left, right, LFE)
    Surround21,
    /// Quad (front left, front right, rear left, rear right)
    Quad,
    /// 5.1 (front left, front right, center, LFE, rear left, rear right)
    Surround51,
    /// 7.1 (5.1 + side left, side right)
    Surround71,
}

impl ChannelLayout {
    /// Get number of channels
    pub fn channels(&self) -> u8 {
        match self {
            ChannelLayout::Mono => 1,
            ChannelLayout::Stereo => 2,
            ChannelLayout::Surround21 => 3,
            ChannelLayout::Quad => 4,
            ChannelLayout::Surround51 => 6,
            ChannelLayout::Surround71 => 8,
        }
    }

    /// Get layout from channel count
    pub fn from_channels(channels: u8) -> Self {
        match channels {
            1 => ChannelLayout::Mono,
            2 => ChannelLayout::Stereo,
            3 => ChannelLayout::Surround21,
            4 => ChannelLayout::Quad,
            6 => ChannelLayout::Surround51,
            8 => ChannelLayout::Surround71,
            _ => ChannelLayout::Stereo,
        }
    }
}

/// Complete audio format specification
#[derive(Debug, Clone, Copy)]
pub struct AudioFormat {
    /// Sample format
    pub sample_format: SampleFormat,
    /// Channel layout
    pub channel_layout: ChannelLayout,
    /// Sample rate in Hz
    pub sample_rate: u32,
}

impl AudioFormat {
    /// Create a new audio format
    pub fn new(sample_format: SampleFormat, channel_layout: ChannelLayout, sample_rate: u32) -> Self {
        AudioFormat {
            sample_format,
            channel_layout,
            sample_rate,
        }
    }

    /// Create CD quality format (16-bit stereo 44100 Hz)
    pub fn cd_quality() -> Self {
        AudioFormat::new(SampleFormat::S16LE, ChannelLayout::Stereo, 44100)
    }

    /// Create DVD quality format (24-bit stereo 48000 Hz)
    pub fn dvd_quality() -> Self {
        AudioFormat::new(SampleFormat::S24LE, ChannelLayout::Stereo, 48000)
    }

    /// Get bytes per frame
    pub fn bytes_per_frame(&self) -> usize {
        self.channel_layout.channels() as usize * self.sample_format.bytes_per_sample()
    }

    /// Get bytes per second
    pub fn bytes_per_second(&self) -> usize {
        self.bytes_per_frame() * self.sample_rate as usize
    }

    /// Get bit rate in bits per second
    pub fn bit_rate(&self) -> u32 {
        (self.bytes_per_second() * 8) as u32
    }
}

impl Default for AudioFormat {
    fn default() -> Self {
        AudioFormat::cd_quality()
    }
}

/// OSS/ALSA format constants for compatibility
pub mod oss {
    pub const AFMT_U8: u32 = 0x00000008;
    pub const AFMT_S8: u32 = 0x00000040;
    pub const AFMT_S16_LE: u32 = 0x00000010;
    pub const AFMT_S16_BE: u32 = 0x00000020;
    pub const AFMT_S24_LE: u32 = 0x00008000;
    pub const AFMT_S24_BE: u32 = 0x00010000;
    pub const AFMT_S32_LE: u32 = 0x00001000;
    pub const AFMT_S32_BE: u32 = 0x00002000;
    pub const AFMT_MU_LAW: u32 = 0x00000001;
    pub const AFMT_A_LAW: u32 = 0x00000002;
}

/// Convert OSS format to SampleFormat
pub fn from_oss_format(oss_fmt: u32) -> Option<SampleFormat> {
    match oss_fmt {
        oss::AFMT_U8 => Some(SampleFormat::U8),
        oss::AFMT_S8 => Some(SampleFormat::S8),
        oss::AFMT_S16_LE => Some(SampleFormat::S16LE),
        oss::AFMT_S16_BE => Some(SampleFormat::S16BE),
        oss::AFMT_S24_LE => Some(SampleFormat::S24LE),
        oss::AFMT_S24_BE => Some(SampleFormat::S24BE),
        oss::AFMT_S32_LE => Some(SampleFormat::S32LE),
        oss::AFMT_S32_BE => Some(SampleFormat::S32BE),
        oss::AFMT_MU_LAW => Some(SampleFormat::MuLaw),
        oss::AFMT_A_LAW => Some(SampleFormat::ALaw),
        _ => None,
    }
}

/// Convert SampleFormat to OSS format
pub fn to_oss_format(fmt: SampleFormat) -> u32 {
    match fmt {
        SampleFormat::U8 => oss::AFMT_U8,
        SampleFormat::S8 => oss::AFMT_S8,
        SampleFormat::S16LE => oss::AFMT_S16_LE,
        SampleFormat::S16BE => oss::AFMT_S16_BE,
        SampleFormat::S24LE => oss::AFMT_S24_LE,
        SampleFormat::S24BE => oss::AFMT_S24_BE,
        SampleFormat::S32LE => oss::AFMT_S32_LE,
        SampleFormat::S32BE => oss::AFMT_S32_BE,
        SampleFormat::MuLaw => oss::AFMT_MU_LAW,
        SampleFormat::ALaw => oss::AFMT_A_LAW,
        _ => oss::AFMT_S16_LE,
    }
}
