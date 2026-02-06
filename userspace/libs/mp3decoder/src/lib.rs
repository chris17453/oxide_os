//! MP3 Decoder for OXIDE OS
//!
//! Thin wrapper around minimp3 for decoding MP3 audio files.
//! Provides a simple API for feeding MP3 data and getting PCM samples.
//! — EchoFrame: Audio codec layer

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use minimp3::{Decoder, Frame, Error};

/// MP3 decoder wrapper
pub struct Mp3Decoder {
    decoder: Decoder<&'static [u8]>,
}

impl Mp3Decoder {
    /// Create a new MP3 decoder
    pub fn new() -> Self {
        Mp3Decoder {
            decoder: Decoder::new(&[]),
        }
    }

    /// Decode MP3 data from a byte slice
    ///
    /// Returns decoded PCM samples and sample rate.
    /// Each call processes one MP3 frame.
    pub fn decode(&mut self, data: &[u8]) -> Result<DecodedFrame, DecodeError> {
        // Create new decoder with this data
        self.decoder = Decoder::new(data);
        
        match self.decoder.next_frame() {
            Ok(Frame {
                data,
                sample_rate,
                channels,
                ..
            }) => Ok(DecodedFrame {
                samples: data,
                sample_rate,
                channels,
            }),
            Err(Error::Eof) => Err(DecodeError::Eof),
            Err(Error::SkippedData) => Err(DecodeError::SkippedData),
            Err(Error::InsufficientData) => Err(DecodeError::InsufficientData),
            Err(_) => Err(DecodeError::InvalidData),
        }
    }

    /// Decode entire MP3 file at once
    ///
    /// Returns all decoded PCM samples, sample rate, and channel count.
    /// This reads the entire file into memory - use for small files only.
    pub fn decode_all(data: &[u8]) -> Result<DecodedAudio, DecodeError> {
        let mut decoder = Decoder::new(data);
        let mut all_samples = Vec::new();
        let mut sample_rate = 0;
        let mut channels = 0;

        loop {
            match decoder.next_frame() {
                Ok(Frame {
                    data,
                    sample_rate: sr,
                    channels: ch,
                    ..
                }) => {
                    sample_rate = sr;
                    channels = ch;
                    all_samples.extend_from_slice(&data);
                }
                Err(Error::Eof) => break,
                Err(Error::SkippedData) => continue,
                Err(Error::InsufficientData) => break,
                Err(_) => return Err(DecodeError::InvalidData),
            }
        }

        if sample_rate == 0 {
            return Err(DecodeError::NoFrames);
        }

        Ok(DecodedAudio {
            samples: all_samples,
            sample_rate,
            channels,
        })
    }
}

impl Default for Mp3Decoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Decoded MP3 frame
#[derive(Debug)]
pub struct DecodedFrame {
    /// PCM samples (interleaved if stereo)
    pub samples: Vec<i16>,
    /// Sample rate in Hz
    pub sample_rate: i32,
    /// Number of channels (1 = mono, 2 = stereo)
    pub channels: usize,
}

/// Decoded MP3 audio (complete file)
#[derive(Debug)]
pub struct DecodedAudio {
    /// All PCM samples (interleaved if stereo)
    pub samples: Vec<i16>,
    /// Sample rate in Hz
    pub sample_rate: i32,
    /// Number of channels (1 = mono, 2 = stereo)
    pub channels: usize,
}

/// Decode error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// End of file
    Eof,
    /// Invalid or corrupted MP3 data
    InvalidData,
    /// Not enough data to decode a frame
    InsufficientData,
    /// Skipped invalid data
    SkippedData,
    /// No frames found in the data
    NoFrames,
}
