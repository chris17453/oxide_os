//! touch - change file timestamps or create files
//!
//! Enhanced implementation with:
//! - -a (change access time only)
//! - -m (change modification time only)
//! - -c (do not create file)
//! - -t (specify timestamp: [[CC]YY]MMDDhhmm[.ss])
//! - -r (use reference file's time)
//! - Multiple file arguments
//! - Full timestamp modification support

#![no_std]
#![no_main]

use libc::*;

const MAX_PATH: usize = 256;

struct TouchConfig {
    no_create: bool,
    change_atime: bool,
    change_mtime: bool,
    timestamp: Option<u64>,
    reference_file: Option<[u8; MAX_PATH]>,
}

impl TouchConfig {
    fn new() -> Self {
        TouchConfig {
            no_create: false,
            change_atime: true,
            change_mtime: true,
            timestamp: None,
            reference_file: None,
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

/// Parse timestamp from [[CC]YY]MMDDhhmm[.ss] format
/// Returns seconds since Unix epoch
fn parse_timestamp(s: &str) -> Option<u64> {
    let bytes = s.as_bytes();
    let len = bytes.len();

    // Minimum: MMDDhhmm (8 digits)
    // Maximum: CCYYMMDDhhmm.ss (15 chars with dot)
    if len < 8 || len > 15 {
        return None;
    }

    // Find decimal point for seconds
    let dot_pos = bytes.iter().position(|&b| b == b'.');
    let has_seconds = dot_pos.is_some();

    let main_part = if has_seconds {
        &s[..dot_pos.unwrap()]
    } else {
        s
    };

    let main_len = main_part.len();
    if main_len < 8 || main_len > 12 {
        return None;
    }

    // Parse based on length
    let (year, month_start) = match main_len {
        8 => {
            // MMDDhhmm - use current year
            // For simplicity, assume year 2024
            (2024, 0)
        }
        10 => {
            // YYMMDDhhmm - 20XX
            let yy = parse_digits(&main_part[0..2])?;
            (2000 + yy as i32, 2)
        }
        12 => {
            // CCYYMMDDhhmm
            let cc = parse_digits(&main_part[0..2])?;
            let yy = parse_digits(&main_part[2..4])?;
            (cc as i32 * 100 + yy as i32, 4)
        }
        _ => return None,
    };

    let month = parse_digits(&main_part[month_start..month_start + 2])?;
    let day = parse_digits(&main_part[month_start + 2..month_start + 4])?;
    let hour = parse_digits(&main_part[month_start + 4..month_start + 6])?;
    let minute = parse_digits(&main_part[month_start + 6..month_start + 8])?;

    let second = if has_seconds {
        let sec_start = dot_pos.unwrap() + 1;
        if s.len() >= sec_start + 2 {
            parse_digits(&s[sec_start..sec_start + 2])?
        } else {
            0
        }
    } else {
        0
    };

    // Validate ranges
    if month < 1 || month > 12 || day < 1 || day > 31 ||
       hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    // Convert to Unix timestamp (simplified, doesn't handle all edge cases)
    // Days since 1970-01-01
    let mut days = 0i64;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    let leap = is_leap_year(year);
    let days_in_months = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    for m in 0..(month as usize - 1) {
        if m < 12 {
            days += days_in_months[m] as i64;
        }
    }

    days += (day as i64) - 1;

    let timestamp = days * 86400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64;
    Some(timestamp as u64)
}

/// Parse 2-digit number from string
fn parse_digits(s: &str) -> Option<u32> {
    let mut result = 0u32;
    for b in s.bytes() {
        if b >= b'0' && b <= b'9' {
            result = result * 10 + (b - b'0') as u32;
        } else {
            return None;
        }
    }
    Some(result)
}

/// Check if year is leap year
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Get current time (simplified - returns 0 for now)
fn get_current_time() -> u64 {
    // TODO: implement actual time syscall
    0
}

/// Get file timestamps from reference file
fn get_reference_time(path: &str) -> Option<(u64, u64)> {
    let mut statbuf = Stat::zeroed();
    if stat(path, &mut statbuf) < 0 {
        return None;
    }
    Some((statbuf.atime, statbuf.mtime))
}

/// Check if file exists
fn file_exists(path: &str) -> bool {
    let mut statbuf = Stat::zeroed();
    stat(path, &mut statbuf) == 0
}

/// Touch a single file
fn touch_file(path: &str, config: &TouchConfig) -> i32 {
    let exists = file_exists(path);

    // If file doesn't exist and -c is specified, skip
    if !exists && config.no_create {
        return 0;
    }

    // Create file if it doesn't exist
    if !exists {
        let fd = open(path, O_WRONLY | O_CREAT, 0o644);
        if fd < 0 {
            eprints("touch: cannot touch '");
            prints(path);
            eprintlns("'");
            return 1;
        }
        close(fd);
    }

    // Determine timestamp to use
    let timestamp = if let Some(t) = config.timestamp {
        t
    } else if let Some(ref refpath_buf) = config.reference_file {
        // Extract path from buffer
        let len = refpath_buf.iter().position(|&c| c == 0).unwrap_or(MAX_PATH);
        let refpath = core::str::from_utf8(&refpath_buf[..len]).unwrap_or("");
        match get_reference_time(refpath) {
            Some((atime, mtime)) => {
                // Use both times from reference file
                let atime_val = if config.change_atime { atime } else { u64::MAX };
                let mtime_val = if config.change_mtime { mtime } else { u64::MAX };

                if sys_utimes(path, atime_val, mtime_val) < 0 {
                    eprints("touch: cannot set times for '");
                    prints(path);
                    eprintlns("'");
                    return 1;
                }
                return 0;
            }
            None => {
                eprints("touch: cannot stat reference file '");
                prints(refpath);
                eprintlns("'");
                return 1;
            }
        }
    } else {
        get_current_time()
    };

    // Set the appropriate timestamps
    let atime = if config.change_atime { timestamp } else { u64::MAX };
    let mtime = if config.change_mtime { timestamp } else { u64::MAX };

    if sys_utimes(path, atime, mtime) < 0 {
        eprints("touch: cannot set times for '");
        prints(path);
        eprintlns("'");
        return 1;
    }

    0
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: touch [options] FILE...");
        eprintlns("Options:");
        eprintlns("  -a        Change access time only");
        eprintlns("  -m        Change modification time only");
        eprintlns("  -c        Do not create file if it doesn't exist");
        eprintlns("  -t TIME   Use [[CC]YY]MMDDhhmm[.ss] instead of current time");
        eprintlns("  -r FILE   Use reference file's times");
        return 1;
    }

    let mut config = TouchConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg_ptr = unsafe { *argv.add(arg_idx as usize) };
        let arg = cstr_to_str(arg_ptr);

        if arg.starts_with('-') && arg.len() > 1 && arg != "--" {
            if arg == "-t" {
                // Timestamp option
                arg_idx += 1;
                if arg_idx >= argc {
                    eprintlns("touch: option -t requires an argument");
                    return 1;
                }
                let time_str = cstr_to_str(unsafe { *argv.add(arg_idx as usize) });
                match parse_timestamp(time_str) {
                    Some(t) => config.timestamp = Some(t),
                    None => {
                        eprints("touch: invalid date format: '");
                        prints(time_str);
                        eprintlns("'");
                        return 1;
                    }
                }
                arg_idx += 1;
            } else if arg == "-r" {
                // Reference file option
                arg_idx += 1;
                if arg_idx >= argc {
                    eprintlns("touch: option -r requires an argument");
                    return 1;
                }
                let refpath = cstr_to_str(unsafe { *argv.add(arg_idx as usize) });
                let mut buf = [0u8; MAX_PATH];
                let copy_len = if refpath.len() > MAX_PATH - 1 {
                    MAX_PATH - 1
                } else {
                    refpath.len()
                };
                buf[..copy_len].copy_from_slice(&refpath.as_bytes()[..copy_len]);
                config.reference_file = Some(buf);
                arg_idx += 1;
            } else {
                // Parse character flags
                for c in arg[1..].bytes() {
                    match c {
                        b'a' => {
                            config.change_atime = true;
                            config.change_mtime = false;
                        }
                        b'm' => {
                            config.change_atime = false;
                            config.change_mtime = true;
                        }
                        b'c' => config.no_create = true,
                        _ => {
                            eprints("touch: unknown option: -");
                            putchar(c);
                            printlns("");
                            return 1;
                        }
                    }
                }
                arg_idx += 1;
            }
        } else {
            break;
        }
    }

    if arg_idx >= argc {
        eprintlns("touch: missing file operand");
        return 1;
    }

    let mut status = 0;

    // Touch each specified file
    for i in arg_idx..argc {
        let path = cstr_to_str(unsafe { *argv.add(i as usize) });

        if touch_file(path, &config) != 0 {
            status = 1;
        }
    }

    status
}
