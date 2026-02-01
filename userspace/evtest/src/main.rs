//! OXIDE Input Event Tester (evtest)
//!
//! Reads and displays input events from /dev/input/ device nodes.
//!
//! Usage:
//!   evtest                       List available input devices
//!   evtest /dev/input/event0     Monitor keyboard events
//!   evtest /dev/input/event1     Monitor mouse events
//!   evtest /dev/input/mice       Monitor aggregated mouse packets

#![no_std]
#![no_main]
#![allow(unused)]

use libc::*;

/// InputEvent struct matching kernel's repr(C) layout (24 bytes)
#[repr(C)]
#[derive(Clone, Copy)]
struct InputEvent {
    /// Seconds
    sec: u64,
    /// Microseconds
    usec: u64,
    /// Event type
    type_: u16,
    /// Event code
    code: u16,
    /// Event value
    value: i32,
}

const INPUT_EVENT_SIZE: usize = core::mem::size_of::<InputEvent>();

// Event types
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const EV_ABS: u16 = 0x03;
const EV_MSC: u16 = 0x04;
const EV_SW: u16 = 0x05;
const EV_LED: u16 = 0x11;
const EV_SND: u16 = 0x12;
const EV_REP: u16 = 0x14;
const EV_FF: u16 = 0x15;

// IOCTL numbers
const EVIOCGNAME: u64 = 0x06;
const EVIOCGID: u64 = 0x02;
const EVIOCFLUSH: u64 = 0x100;

/// Print a string to stdout
fn print(s: &str) {
    write(1, s.as_bytes());
}

/// Print a string to stderr
fn eprint(s: &str) {
    write(2, s.as_bytes());
}

/// Print a decimal number to stdout
fn print_num(n: i32) {
    if n < 0 {
        print("-");
        print_num_unsigned((-n) as u32);
    } else {
        print_num_unsigned(n as u32);
    }
}

fn print_num_unsigned(n: u32) {
    let mut buf = [0u8; 12];
    let mut pos = buf.len();
    let mut val = n;

    if val == 0 {
        print("0");
        return;
    }

    while val > 0 {
        pos -= 1;
        buf[pos] = b'0' + (val % 10) as u8;
        val /= 10;
    }

    if let Ok(s) = core::str::from_utf8(&buf[pos..]) {
        print(s);
    }
}

/// Print a hex number (0xNNNN)
fn print_hex(mut val: u64) {
    let hex_chars = b"0123456789abcdef";
    let mut buf = [0u8; 16];
    for i in 0..16 {
        buf[15 - i] = hex_chars[(val & 0xF) as usize];
        val >>= 4;
    }
    for &ch in &buf {
        putchar(ch);
    }
}

fn print_hex16(n: u16) {
    let hex = b"0123456789abcdef";
    let mut buf = [b'0', b'x', 0, 0, 0, 0];
    buf[2] = hex[((n >> 12) & 0xF) as usize];
    buf[3] = hex[((n >> 8) & 0xF) as usize];
    buf[4] = hex[((n >> 4) & 0xF) as usize];
    buf[5] = hex[(n & 0xF) as usize];
    write(1, &buf);
}

/// Get event type name
fn event_type_name(type_: u16) -> &'static str {
    match type_ {
        EV_SYN => "EV_SYN",
        EV_KEY => "EV_KEY",
        EV_REL => "EV_REL",
        EV_ABS => "EV_ABS",
        EV_MSC => "EV_MSC",
        EV_SW => "EV_SW",
        EV_LED => "EV_LED",
        EV_SND => "EV_SND",
        EV_REP => "EV_REP",
        EV_FF => "EV_FF",
        _ => "UNKNOWN",
    }
}

/// Get synchronization code name
fn syn_code_name(code: u16) -> &'static str {
    match code {
        0 => "SYN_REPORT",
        1 => "SYN_CONFIG",
        2 => "SYN_MT_REPORT",
        3 => "SYN_DROPPED",
        _ => "SYN_?",
    }
}

/// Get relative axis code name
fn rel_code_name(code: u16) -> &'static str {
    match code {
        0x00 => "REL_X",
        0x01 => "REL_Y",
        0x02 => "REL_Z",
        0x06 => "REL_HWHEEL",
        0x07 => "REL_DIAL",
        0x08 => "REL_WHEEL",
        0x09 => "REL_MISC",
        0x0B => "REL_WHEEL_HI_RES",
        0x0C => "REL_HWHEEL_HI_RES",
        _ => "REL_?",
    }
}

/// Get absolute axis code name
fn abs_code_name(code: u16) -> &'static str {
    match code {
        0x00 => "ABS_X",
        0x01 => "ABS_Y",
        0x02 => "ABS_Z",
        0x03 => "ABS_RX",
        0x04 => "ABS_RY",
        0x05 => "ABS_RZ",
        0x06 => "ABS_THROTTLE",
        0x07 => "ABS_RUDDER",
        0x08 => "ABS_WHEEL",
        0x18 => "ABS_PRESSURE",
        _ => "ABS_?",
    }
}

/// Get key/button code name
fn key_code_name(code: u16) -> &'static str {
    match code {
        0 => "KEY_RESERVED",
        1 => "KEY_ESC",
        2 => "KEY_1",
        3 => "KEY_2",
        4 => "KEY_3",
        5 => "KEY_4",
        6 => "KEY_5",
        7 => "KEY_6",
        8 => "KEY_7",
        9 => "KEY_8",
        10 => "KEY_9",
        11 => "KEY_0",
        12 => "KEY_MINUS",
        13 => "KEY_EQUAL",
        14 => "KEY_BACKSPACE",
        15 => "KEY_TAB",
        16 => "KEY_Q",
        17 => "KEY_W",
        18 => "KEY_E",
        19 => "KEY_R",
        20 => "KEY_T",
        21 => "KEY_Y",
        22 => "KEY_U",
        23 => "KEY_I",
        24 => "KEY_O",
        25 => "KEY_P",
        26 => "KEY_LEFTBRACE",
        27 => "KEY_RIGHTBRACE",
        28 => "KEY_ENTER",
        29 => "KEY_LEFTCTRL",
        30 => "KEY_A",
        31 => "KEY_S",
        32 => "KEY_D",
        33 => "KEY_F",
        34 => "KEY_G",
        35 => "KEY_H",
        36 => "KEY_J",
        37 => "KEY_K",
        38 => "KEY_L",
        39 => "KEY_SEMICOLON",
        40 => "KEY_APOSTROPHE",
        41 => "KEY_GRAVE",
        42 => "KEY_LEFTSHIFT",
        43 => "KEY_BACKSLASH",
        44 => "KEY_Z",
        45 => "KEY_X",
        46 => "KEY_C",
        47 => "KEY_V",
        48 => "KEY_B",
        49 => "KEY_N",
        50 => "KEY_M",
        51 => "KEY_COMMA",
        52 => "KEY_DOT",
        53 => "KEY_SLASH",
        54 => "KEY_RIGHTSHIFT",
        55 => "KEY_KPASTERISK",
        56 => "KEY_LEFTALT",
        57 => "KEY_SPACE",
        58 => "KEY_CAPSLOCK",
        59 => "KEY_F1",
        60 => "KEY_F2",
        61 => "KEY_F3",
        62 => "KEY_F4",
        63 => "KEY_F5",
        64 => "KEY_F6",
        65 => "KEY_F7",
        66 => "KEY_F8",
        67 => "KEY_F9",
        68 => "KEY_F10",
        69 => "KEY_NUMLOCK",
        70 => "KEY_SCROLLLOCK",
        87 => "KEY_F11",
        88 => "KEY_F12",
        96 => "KEY_KPENTER",
        97 => "KEY_RIGHTCTRL",
        100 => "KEY_RIGHTALT",
        102 => "KEY_HOME",
        103 => "KEY_UP",
        104 => "KEY_PAGEUP",
        105 => "KEY_LEFT",
        106 => "KEY_RIGHT",
        107 => "KEY_END",
        108 => "KEY_DOWN",
        109 => "KEY_PAGEDOWN",
        110 => "KEY_INSERT",
        111 => "KEY_DELETE",
        125 => "KEY_LEFTMETA",
        126 => "KEY_RIGHTMETA",
        // Mouse buttons
        0x110 => "BTN_LEFT",
        0x111 => "BTN_RIGHT",
        0x112 => "BTN_MIDDLE",
        0x113 => "BTN_SIDE",
        0x114 => "BTN_EXTRA",
        0x115 => "BTN_FORWARD",
        0x116 => "BTN_BACK",
        0x117 => "BTN_TASK",
        _ => "KEY_?",
    }
}

/// Get key value name
fn key_value_name(value: i32) -> &'static str {
    match value {
        0 => "RELEASED",
        1 => "PRESSED",
        2 => "REPEAT",
        _ => "?",
    }
}

/// Get code name based on event type
fn code_name(type_: u16, code: u16) -> &'static str {
    match type_ {
        EV_SYN => syn_code_name(code),
        EV_KEY => key_code_name(code),
        EV_REL => rel_code_name(code),
        EV_ABS => abs_code_name(code),
        _ => "?",
    }
}

/// Print a single input event
fn print_event(ev: &InputEvent) {
    print("Event: type=");
    print(event_type_name(ev.type_));
    print("(");
    print_hex16(ev.type_);
    print(") code=");
    print(code_name(ev.type_, ev.code));
    print("(");
    print_hex16(ev.code);
    print(") value=");
    print_num(ev.value);

    // Extra annotation for key events
    if ev.type_ == EV_KEY {
        print(" [");
        print(key_value_name(ev.value));
        print("]");
    }

    print("\n");
}

/// Print /dev/input/mice raw packets (3-byte PS/2 format)
fn monitor_mice(fd: i32) {
    print("Monitoring /dev/input/mice (3-byte PS/2 packets)\n");
    print("Format: buttons dx dy\n");
    print("Press Ctrl+C to stop.\n\n");

    let mut buf = [0u8; 3];
    loop {
        let n = read(fd, &mut buf);
        if n < 0 {
            eprint("evtest: read error\n");
            break;
        }
        if n < 3 {
            continue;
        }

        let buttons = buf[0];
        let dx = buf[1] as i8;
        let dy = buf[2] as i8;

        print("Mice: buttons=");
        // Print button bits
        if buttons & 1 != 0 {
            print("L");
        } else {
            print("-");
        }
        if buttons & 4 != 0 {
            print("M");
        } else {
            print("-");
        }
        if buttons & 2 != 0 {
            print("R");
        } else {
            print("-");
        }
        print(" dx=");
        print_num(dx as i32);
        print(" dy=");
        print_num(dy as i32);
        print("\n");
    }
}

/// List available input devices in /dev/input/
fn list_devices() {
    print("Available input devices:\n");

    if let Some(mut dir) = dirent::opendir("/dev/input") {
        while let Some(entry) = dirent::readdir(&mut dir) {
            let name = entry.name();
            // Skip . and ..
            if name == "." || name == ".." {
                continue;
            }

            print("  /dev/input/");
            print(name);

            // Try to get device name via ioctl
            let mut path_buf = [0u8; 64];
            let prefix = b"/dev/input/";
            let name_bytes = name.as_bytes();
            let total_len = prefix.len() + name_bytes.len();
            if total_len < path_buf.len() {
                path_buf[..prefix.len()].copy_from_slice(prefix);
                path_buf[prefix.len()..prefix.len() + name_bytes.len()].copy_from_slice(name_bytes);

                // Null-terminate for safety
                if let Ok(path_str) = core::str::from_utf8(&path_buf[..total_len]) {
                    let fd = open(path_str, fcntl::O_RDONLY, 0);
                    if fd >= 0 {
                        let mut name_buf = [0u8; 128];
                        let ret = sys_ioctl(fd, EVIOCGNAME, name_buf.as_mut_ptr() as u64);
                        if ret > 0 {
                            let len = (ret as usize).min(name_buf.len());
                            if let Ok(dev_name) = core::str::from_utf8(&name_buf[..len]) {
                                let trimmed = dev_name.trim_end_matches('\0');
                                if !trimmed.is_empty() {
                                    print("  ");
                                    print(trimmed);
                                }
                            }
                        }
                        close(fd);
                    }
                }
            }

            print("\n");
        }
        dirent::closedir(dir);
    } else {
        eprint("evtest: cannot open /dev/input/\n");
        eprint("Make sure the input subsystem is initialized.\n");
    }
}

/// Monitor an event device
fn monitor_device(path: &str) {
    let fd = open(path, fcntl::O_RDONLY, 0);
    if fd < 0 {
        eprint("evtest: cannot open ");
        eprint(path);
        eprint("\n");
        return;
    }

    // Print device name
    print("Input device: ");
    let mut name_buf = [0u8; 128];
    let ret = sys_ioctl(fd, EVIOCGNAME, name_buf.as_mut_ptr() as u64);
    if ret > 0 {
        let len = (ret as usize).min(name_buf.len());
        if let Ok(dev_name) = core::str::from_utf8(&name_buf[..len]) {
            let trimmed = dev_name.trim_end_matches('\0');
            print(trimmed);
        }
    } else {
        print(path);
    }
    print("\n");

    // Flush any old queued events before we start monitoring
    sys_ioctl(fd, EVIOCFLUSH, 0);

    print("Reading events from ");
    print(path);
    print("\nPress Ctrl+C to stop.\n\n");

    // Read events in a loop
    let mut buf = [0u8; INPUT_EVENT_SIZE * 16]; // Read up to 16 events at once
    loop {
        let n = read(fd, &mut buf);
        if n < 0 {
            eprint("evtest: read error\n");
            break;
        }
        if n == 0 {
            // EOF — device closed
            break;
        }

        // Process complete events
        let bytes_read = n as usize;
        let mut offset = 0;
        while offset + INPUT_EVENT_SIZE <= bytes_read {
            let ev = unsafe { &*(buf.as_ptr().add(offset) as *const InputEvent) };
            print_event(ev);
            offset += INPUT_EVENT_SIZE;
        }
    }

    close(fd);
}

fn usage() {
    print("Usage: evtest [DEVICE]\n\n");
    print("  evtest                   List available input devices\n");
    print("  evtest /dev/input/event0 Monitor keyboard events\n");
    print("  evtest /dev/input/event1 Monitor mouse events\n");
    print("  evtest /dev/input/mice   Monitor raw mouse packets\n");
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        list_devices();
        print("\nSelect a device to monitor, e.g.: evtest /dev/input/event0\n");
        return 0;
    }

    // Get device path from argv[1]
    let arg = unsafe { *argv.add(1) };
    if arg.is_null() {
        usage();
        return 1;
    }

    // Convert to &str
    let mut len = 0;
    unsafe {
        while *arg.add(len) != 0 {
            len += 1;
        }
    }
    let path = unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(arg, len)) };

    // DEBUG: print what we received
    print("[DEBUG] argc=");
    print_num(argc);
    print(" argv[1] ptr=0x");
    print_hex(arg as u64);
    print(" len=");
    print_num(len as i32);
    print(" path=\"");
    print(path);
    print("\"\n");

    // Check for help flag
    if path == "-h" || path == "--help" {
        usage();
        return 0;
    }

    // Check if this is the /dev/input/mice device
    if path.ends_with("/mice") {
        let fd = open(path, fcntl::O_RDONLY, 0);
        if fd < 0 {
            eprint("evtest: cannot open ");
            eprint(path);
            eprint("\n");
            return 1;
        }
        monitor_mice(fd);
        close(fd);
    } else {
        monitor_device(path);
    }

    0
}
