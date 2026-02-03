//! Sound Client - Test utility for OXIDE Sound Manager
//!
//! Demonstrates how to interact with the soundd daemon to:
//! - Query sound system status
//! - Control volume
//! - Play audio (future)
//!
//! — EchoFrame: Audio + media subsystems

#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use libc::*;

const SOUNDD_SOCKET: &str = "/run/soundd.sock";

/// Send command to sound daemon
fn send_command(cmd: &str) -> bool {
    let fd = open2(SOUNDD_SOCKET, O_RDWR);
    if fd < 0 {
        prints("Error: Sound daemon not running\n");
        return false;
    }

    let _ = write(fd, cmd.as_bytes());

    // Wait for response if it's a query
    // TODO: Implement proper timeout-based read instead of fixed delay
    // — ThreadRogue: Runtime + process model engineer
    if cmd.starts_with("STATUS") {
        usleep(100_000); // 100ms
        let mut buf = [0u8; 512];
        let n = read(fd, &mut buf);
        if n > 0 {
            if let Ok(response) = core::str::from_utf8(&buf[..n as usize]) {
                prints(response);
            }
        }
    }

    close(fd);
    true
}

/// Play a simple sine wave tone (demonstration)
fn play_tone(frequency: u32, duration_ms: u32) {
    prints("Playing ");
    print_i64(frequency as i64);
    prints("Hz tone for ");
    print_i64(duration_ms as i64);
    prints("ms (direct to /dev/dsp)\n");

    // Open audio device directly
    let fd = open2("/dev/dsp", O_WRONLY);
    if fd < 0 {
        prints("Error: Cannot open /dev/dsp\n");
        return;
    }

    // In a real implementation, generate PCM samples and write them
    // For now, just demonstrate the interface

    prints("Note: Actual audio playback requires hardware driver support\n");

    close(fd);
}

/// Show usage
fn show_usage() {
    prints("Usage: sndclient <command> [args]\n");
    prints("\n");
    prints("Commands:\n");
    prints("  status         Show sound system status\n");
    prints("  volume <0-100> Set master volume\n");
    prints("  mute           Mute audio\n");
    prints("  unmute         Unmute audio\n");
    prints("  tone <hz> <ms> Play a tone (demo)\n");
    prints("  help           Show this help\n");
    prints("\n");
    prints("Examples:\n");
    prints("  sndclient status\n");
    prints("  sndclient volume 75\n");
    prints("  sndclient tone 440 1000\n");
}

fn parse_u32(s: &str) -> Option<u32> {
    let mut val: u32 = 0;
    for c in s.bytes() {
        if c.is_ascii_digit() {
            val = val.checked_mul(10)?;
            val = val.checked_add((c - b'0') as u32)?;
        } else {
            return None;
        }
    }
    Some(val)
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        show_usage();
        return 1;
    }

    let cmd = cstr_to_str(unsafe { *argv.add(1) });

    match cmd {
        "status" => {
            if !send_command("STATUS") {
                return 1;
            }
            0
        }
        "volume" => {
            if argc < 3 {
                prints("Error: volume requires an argument (0-100)\n");
                return 1;
            }
            let vol_str = cstr_to_str(unsafe { *argv.add(2) });
            if let Some(vol) = parse_u32(vol_str) {
                if vol <= 100 {
                    let mut cmd_buf = [0u8; 32];
                    let mut pos = 0;
                    for &b in b"VOLUME:" {
                        cmd_buf[pos] = b;
                        pos += 1;
                    }
                    // Format volume
                    let vol_bytes = format_u32(vol);
                    for &b in vol_bytes.as_bytes() {
                        cmd_buf[pos] = b;
                        pos += 1;
                    }
                    let cmd_str = core::str::from_utf8(&cmd_buf[..pos]).unwrap();
                    if send_command(cmd_str) {
                        prints("Volume set to ");
                        print_i64(vol as i64);
                        prints("\n");
                        return 0;
                    }
                } else {
                    prints("Error: volume must be 0-100\n");
                }
            } else {
                prints("Error: invalid volume value\n");
            }
            1
        }
        "mute" => {
            if send_command("MUTE") {
                prints("Audio muted\n");
                0
            } else {
                1
            }
        }
        "unmute" => {
            if send_command("UNMUTE") {
                prints("Audio unmuted\n");
                0
            } else {
                1
            }
        }
        "tone" => {
            if argc < 4 {
                prints("Error: tone requires frequency and duration\n");
                prints("Usage: sndclient tone <hz> <ms>\n");
                return 1;
            }
            let freq_str = cstr_to_str(unsafe { *argv.add(2) });
            let dur_str = cstr_to_str(unsafe { *argv.add(3) });

            if let (Some(freq), Some(dur)) = (parse_u32(freq_str), parse_u32(dur_str)) {
                if freq > 0 && freq < 20000 && dur > 0 && dur < 10000 {
                    play_tone(freq, dur);
                    return 0;
                } else {
                    prints("Error: frequency must be 1-20000 Hz, duration 1-10000 ms\n");
                }
            } else {
                prints("Error: invalid frequency or duration\n");
            }
            1
        }
        "help" | "--help" | "-h" => {
            show_usage();
            0
        }
        _ => {
            prints("Error: unknown command: ");
            prints(cmd);
            prints("\n");
            show_usage();
            1
        }
    }
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

fn format_u32(mut val: u32) -> String {
    if val == 0 {
        return String::from("0");
    }

    let mut digits = [0u8; 10];
    let mut len = 0;
    while val > 0 {
        digits[len] = b'0' + (val % 10) as u8;
        val /= 10;
        len += 1;
    }

    let mut result = String::new();
    for i in (0..len).rev() {
        result.push(digits[i] as char);
    }
    result
}

fn usleep(us: u32) {
    libc::time::usleep(us);
}
