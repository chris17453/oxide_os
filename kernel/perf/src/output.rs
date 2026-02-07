//! Performance statistics output helpers
//!
//! Provides ISR-safe output functions for writing performance stats

/// Write string (delegates to os_log for ISR-safe bounded output)
#[inline]
pub fn write_str(s: &str) {
    unsafe {
        os_log::write_str_raw(s);
    }
}

/// Write byte (delegates to os_log)
#[inline]
pub fn write_byte(b: u8) {
    unsafe {
        os_log::write_byte_raw(b);
    }
}

/// Print decimal number (ISR-safe)
#[inline]
pub fn print_decimal(mut n: u64) {
    if n == 0 {
        write_byte(b'0');
        return;
    }

    let mut buf = [0u8; 20];
    let mut pos = 0;
    while n > 0 {
        buf[pos] = b'0' + (n % 10) as u8;
        n /= 10;
        pos += 1;
    }

    for i in (0..pos).rev() {
        write_byte(buf[i]);
    }
}

/// Get width of decimal number in characters
#[inline]
pub fn decimal_width(mut n: u64) -> usize {
    if n == 0 {
        return 1;
    }
    let mut width = 0;
    while n > 0 {
        width += 1;
        n /= 10;
    }
    width
}

/// Print padding spaces
#[inline]
pub fn print_padding(count: usize) {
    for _ in 0..count {
        write_byte(b' ');
    }
}
