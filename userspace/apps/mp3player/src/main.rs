//! MP3 Player for OXIDE OS
//!
//! Simple command-line MP3 player that decodes MP3 files and plays them
//! through the audio device (/dev/dsp).
//! — EchoFrame: User-facing audio playback

#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use libc::*;
use mp3decoder::{Mp3Decoder, DecodeError};

/// Read entire file into memory
fn read_file(path: &str) -> Option<Vec<u8>> {
    let fd = open(path, O_RDONLY);
    if fd < 0 {
        return None;
    }

    // Get file size
    let mut stat_buf = StatBuf::default();
    if fstat(fd, &mut stat_buf) < 0 {
        close(fd);
        return None;
    }

    let size = stat_buf.st_size as usize;
    let mut data = Vec::with_capacity(size);
    data.resize(size, 0);

    // Read file
    let mut offset = 0;
    while offset < size {
        let n = read(fd, &mut data[offset..]);
        if n <= 0 {
            close(fd);
            return None;
        }
        offset += n as usize;
    }

    close(fd);
    Some(data)
}

/// Play PCM audio samples to /dev/dsp
fn play_audio(samples: &[i16], sample_rate: i32, channels: usize) -> bool {
    prints("Opening /dev/dsp...\n");
    let fd = open2("/dev/dsp", O_WRONLY);
    if fd < 0 {
        prints("Error: Cannot open /dev/dsp\n");
        return false;
    }

    // Convert samples to bytes
    let mut audio_data = Vec::with_capacity(samples.len() * 2);
    for &sample in samples {
        audio_data.push((sample & 0xFF) as u8);
        audio_data.push(((sample >> 8) & 0xFF) as u8);
    }

    prints("Playing audio (");
    print_i64(sample_rate as i64);
    prints(" Hz, ");
    print_i64(channels as i64);
    prints(" channel");
    if channels != 1 {
        prints("s");
    }
    prints(")...\n");

    // Write audio data
    let chunk_size = 4096;
    let mut offset = 0;
    while offset < audio_data.len() {
        let end = (offset + chunk_size).min(audio_data.len());
        let n = write(fd, &audio_data[offset..end]);
        if n <= 0 {
            prints("Error writing to audio device\n");
            close(fd);
            return false;
        }
        offset += n as usize;
    }

    close(fd);
    prints("Playback complete.\n");
    true
}

/// Show usage
fn show_usage() {
    prints("Usage: mp3play <file.mp3>\n");
    prints("\n");
    prints("Plays an MP3 audio file through the system audio device.\n");
    prints("\n");
    prints("Examples:\n");
    prints("  mp3play music.mp3\n");
    prints("  mp3play /mnt/usb/song.mp3\n");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        show_usage();
        return 1;
    }

    let filename = cstr_to_str(unsafe { *argv.add(1) });
    prints("MP3 Player - OXIDE OS\n");
    prints("=====================\n\n");
    prints("File: ");
    prints(filename);
    prints("\n\n");

    // Read MP3 file
    prints("Reading file...\n");
    let data = match read_file(filename) {
        Some(d) => d,
        None => {
            prints("Error: Cannot read file\n");
            return 1;
        }
    };

    prints("File size: ");
    print_i64(data.len() as i64);
    prints(" bytes\n");

    // Decode MP3
    prints("Decoding MP3...\n");
    let decoded = match Mp3Decoder::decode_all(&data) {
        Ok(d) => d,
        Err(DecodeError::InvalidData) => {
            prints("Error: Invalid MP3 data\n");
            return 1;
        }
        Err(DecodeError::NoFrames) => {
            prints("Error: No audio frames found in file\n");
            return 1;
        }
        Err(_) => {
            prints("Error: Failed to decode MP3\n");
            return 1;
        }
    };

    prints("Decoded ");
    print_i64(decoded.samples.len() as i64);
    prints(" samples\n");

    // Play audio
    if !play_audio(&decoded.samples, decoded.sample_rate, decoded.channels) {
        return 1;
    }

    0
}

fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() {
        return "";
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}
