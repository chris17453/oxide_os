//! printf formatting engine
//!
//! Implements vsnprintf which is the core of all printf variants.

use core::ffi::VaList;

pub unsafe fn vsnprintf_impl(
    buf: *mut u8,
    size: usize,
    fmt: *const u8,
    ap: &mut VaList<'_>,
) -> i32 {
    let mut out = 0usize;
    let mut i = 0usize;

    let write_char = |buf: *mut u8, out: &mut usize, size: usize, c: u8| {
        if *out < size.saturating_sub(1) {
            *buf.add(*out) = c;
        }
        *out += 1;
    };

    let write_str = |buf: *mut u8, out: &mut usize, size: usize, s: &[u8]| {
        for &c in s {
            if *out < size.saturating_sub(1) {
                *buf.add(*out) = c;
            }
            *out += 1;
        }
    };

    while *fmt.add(i) != 0 {
        if *fmt.add(i) != b'%' {
            write_char(buf, &mut out, size, *fmt.add(i));
            i += 1;
            continue;
        }

        i += 1; // skip %

        // Flags
        let mut flag_minus = false;
        let mut flag_plus = false;
        let mut flag_space = false;
        let mut flag_zero = false;
        let mut flag_hash = false;

        loop {
            match *fmt.add(i) {
                b'-' => flag_minus = true,
                b'+' => flag_plus = true,
                b' ' => flag_space = true,
                b'0' => flag_zero = true,
                b'#' => flag_hash = true,
                _ => break,
            }
            i += 1;
        }

        // Width
        let mut width: i32 = 0;
        if *fmt.add(i) == b'*' {
            width = ap.arg::<i32>();
            if width < 0 {
                flag_minus = true;
                width = -width;
            }
            i += 1;
        } else {
            while *fmt.add(i) >= b'0' && *fmt.add(i) <= b'9' {
                width = width * 10 + (*fmt.add(i) - b'0') as i32;
                i += 1;
            }
        }

        // Precision
        let mut precision: i32 = -1;
        if *fmt.add(i) == b'.' {
            i += 1;
            precision = 0;
            if *fmt.add(i) == b'*' {
                precision = ap.arg::<i32>();
                i += 1;
            } else {
                while *fmt.add(i) >= b'0' && *fmt.add(i) <= b'9' {
                    precision = precision * 10 + (*fmt.add(i) - b'0') as i32;
                    i += 1;
                }
            }
        }

        // Length modifier
        let mut length = 0u8; // 0=none, 'h'=short, 'H'=char, 'l'=long, 'L'=long long, 'z'=size_t, 'j'=intmax
        match *fmt.add(i) {
            b'h' => {
                i += 1;
                if *fmt.add(i) == b'h' {
                    length = b'H';
                    i += 1;
                } else {
                    length = b'h';
                }
            }
            b'l' => {
                i += 1;
                if *fmt.add(i) == b'l' {
                    length = b'L';
                    i += 1;
                } else {
                    length = b'l';
                }
            }
            b'z' | b'j' | b't' => {
                length = b'l'; // On 64-bit, size_t/intmax_t/ptrdiff_t are 64-bit
                i += 1;
            }
            b'L' => {
                length = b'L';
                i += 1;
            }
            _ => {}
        }

        // Conversion
        let conv = *fmt.add(i);
        i += 1;

        match conv {
            b'd' | b'i' => {
                let val: i64 = match length {
                    b'l' | b'L' => ap.arg::<i64>(),
                    b'h' => ap.arg::<i32>() as i16 as i64,
                    b'H' => ap.arg::<i32>() as i8 as i64,
                    _ => ap.arg::<i32>() as i64,
                };
                let mut tmp = [0u8; 24];
                let neg = val < 0;
                let uval = if neg { (-(val as i128)) as u64 } else { val as u64 };
                let len = format_uint(uval, 10, false, &mut tmp);
                let numstr = &tmp[24 - len..24];

                let prefix_len = if neg || flag_plus || flag_space { 1 } else { 0 };
                let num_len = if precision >= 0 {
                    core::cmp::max(len, precision as usize)
                } else {
                    len
                };
                let total = prefix_len + num_len;
                let pad = if width > total as i32 {
                    (width - total as i32) as usize
                } else {
                    0
                };
                let pad_char = if flag_zero && !flag_minus && precision < 0 {
                    b'0'
                } else {
                    b' '
                };

                if !flag_minus && pad_char == b' ' {
                    for _ in 0..pad {
                        write_char(buf, &mut out, size, b' ');
                    }
                }

                if neg {
                    write_char(buf, &mut out, size, b'-');
                } else if flag_plus {
                    write_char(buf, &mut out, size, b'+');
                } else if flag_space {
                    write_char(buf, &mut out, size, b' ');
                }

                if !flag_minus && pad_char == b'0' {
                    for _ in 0..pad {
                        write_char(buf, &mut out, size, b'0');
                    }
                }

                // Zero-pad for precision
                if precision >= 0 && (precision as usize) > len {
                    for _ in 0..(precision as usize - len) {
                        write_char(buf, &mut out, size, b'0');
                    }
                }

                write_str(buf, &mut out, size, numstr);

                if flag_minus {
                    for _ in 0..pad {
                        write_char(buf, &mut out, size, b' ');
                    }
                }
            }

            b'u' | b'o' | b'x' | b'X' => {
                let val: u64 = match length {
                    b'l' | b'L' => ap.arg::<u64>(),
                    b'h' => ap.arg::<u32>() as u16 as u64,
                    b'H' => ap.arg::<u32>() as u8 as u64,
                    _ => ap.arg::<u32>() as u64,
                };

                let base = match conv {
                    b'o' => 8,
                    b'x' | b'X' => 16,
                    _ => 10,
                };
                let upper = conv == b'X';

                let mut tmp = [0u8; 24];
                let len = format_uint(val, base, upper, &mut tmp);
                let numstr = &tmp[24 - len..24];

                let prefix_len = if flag_hash && val != 0 {
                    match conv {
                        b'o' => 1,
                        b'x' | b'X' => 2,
                        _ => 0,
                    }
                } else {
                    0
                };

                let num_len = if precision >= 0 {
                    core::cmp::max(len, precision as usize)
                } else {
                    len
                };
                let total = prefix_len + num_len;
                let pad = if width > total as i32 {
                    (width - total as i32) as usize
                } else {
                    0
                };
                let pad_char = if flag_zero && !flag_minus && precision < 0 {
                    b'0'
                } else {
                    b' '
                };

                if !flag_minus && pad_char == b' ' {
                    for _ in 0..pad {
                        write_char(buf, &mut out, size, b' ');
                    }
                }

                if flag_hash && val != 0 {
                    match conv {
                        b'o' => write_char(buf, &mut out, size, b'0'),
                        b'x' => {
                            write_char(buf, &mut out, size, b'0');
                            write_char(buf, &mut out, size, b'x');
                        }
                        b'X' => {
                            write_char(buf, &mut out, size, b'0');
                            write_char(buf, &mut out, size, b'X');
                        }
                        _ => {}
                    }
                }

                if !flag_minus && pad_char == b'0' {
                    for _ in 0..pad {
                        write_char(buf, &mut out, size, b'0');
                    }
                }

                if precision >= 0 && (precision as usize) > len {
                    for _ in 0..(precision as usize - len) {
                        write_char(buf, &mut out, size, b'0');
                    }
                }

                write_str(buf, &mut out, size, numstr);

                if flag_minus {
                    for _ in 0..pad {
                        write_char(buf, &mut out, size, b' ');
                    }
                }
            }

            b'c' => {
                let c = ap.arg::<i32>() as u8;
                let pad = if width > 1 { width - 1 } else { 0 };

                if !flag_minus {
                    for _ in 0..pad {
                        write_char(buf, &mut out, size, b' ');
                    }
                }
                write_char(buf, &mut out, size, c);
                if flag_minus {
                    for _ in 0..pad {
                        write_char(buf, &mut out, size, b' ');
                    }
                }
            }

            b's' => {
                let s: *const u8 = ap.arg::<*const u8>();
                let s = if s.is_null() { b"(null)\0".as_ptr() } else { s };

                let mut slen = 0;
                while *s.add(slen) != 0 {
                    slen += 1;
                }
                if precision >= 0 && (precision as usize) < slen {
                    slen = precision as usize;
                }

                let pad = if width > slen as i32 {
                    (width - slen as i32) as usize
                } else {
                    0
                };

                if !flag_minus {
                    for _ in 0..pad {
                        write_char(buf, &mut out, size, b' ');
                    }
                }
                for j in 0..slen {
                    write_char(buf, &mut out, size, *s.add(j));
                }
                if flag_minus {
                    for _ in 0..pad {
                        write_char(buf, &mut out, size, b' ');
                    }
                }
            }

            b'p' => {
                let p: usize = ap.arg::<usize>();
                let mut tmp = [0u8; 24];
                let len = format_uint(p as u64, 16, false, &mut tmp);

                write_char(buf, &mut out, size, b'0');
                write_char(buf, &mut out, size, b'x');
                write_str(buf, &mut out, size, &tmp[24 - len..24]);
            }

            b'f' | b'F' => {
                let val: f64 = ap.arg::<f64>();
                let prec = if precision < 0 { 6 } else { precision as usize };

                if val != val {
                    // NaN
                    write_str(buf, &mut out, size, if conv == b'F' { b"NAN" } else { b"nan" });
                } else if val == f64::INFINITY {
                    if flag_plus {
                        write_char(buf, &mut out, size, b'+');
                    }
                    write_str(buf, &mut out, size, if conv == b'F' { b"INF" } else { b"inf" });
                } else if val == f64::NEG_INFINITY {
                    write_char(buf, &mut out, size, b'-');
                    write_str(buf, &mut out, size, if conv == b'F' { b"INF" } else { b"inf" });
                } else {
                    let neg = val < 0.0;
                    let val = if neg { -val } else { val };

                    if neg {
                        write_char(buf, &mut out, size, b'-');
                    } else if flag_plus {
                        write_char(buf, &mut out, size, b'+');
                    } else if flag_space {
                        write_char(buf, &mut out, size, b' ');
                    }

                    let int_part = val as u64;
                    let mut frac = val - int_part as f64;

                    // Round
                    let mut mult = 1.0f64;
                    for _ in 0..prec {
                        mult *= 10.0;
                    }
                    frac = (frac * mult + 0.5) as u64 as f64;
                    let frac_int = frac as u64;

                    let mut tmp = [0u8; 24];
                    let ilen = format_uint(int_part, 10, false, &mut tmp);
                    write_str(buf, &mut out, size, &tmp[24 - ilen..24]);

                    if prec > 0 || flag_hash {
                        write_char(buf, &mut out, size, b'.');
                        let flen = format_uint(frac_int, 10, false, &mut tmp);
                        // Zero pad fractional part
                        for _ in flen..prec {
                            write_char(buf, &mut out, size, b'0');
                        }
                        let start = if flen > prec { flen - prec } else { 0 };
                        write_str(buf, &mut out, size, &tmp[24 - flen + start..24]);
                    }
                }
            }

            b'e' | b'E' => {
                let val: f64 = ap.arg::<f64>();
                format_scientific(buf, &mut out, size, val, precision, conv, flag_plus, flag_space);
            }

            b'g' | b'G' => {
                let val: f64 = ap.arg::<f64>();
                let prec = if precision < 0 { 6 } else if precision == 0 { 1 } else { precision };
                // Use %e if exponent < -4 or >= precision, else %f
                if val == 0.0 || val != val || val == f64::INFINITY || val == f64::NEG_INFINITY {
                    format_scientific(buf, &mut out, size, val, prec - 1, if conv == b'G' { b'E' } else { b'e' }, flag_plus, flag_space);
                } else {
                    let abs_val = if val < 0.0 { -val } else { val };
                    let exp = if abs_val > 0.0 {
                        crate::math::ln(abs_val) / crate::math::consts::LN_10
                    } else {
                        0.0
                    };
                    let exp = crate::math::floor(exp) as i32;
                    if exp < -4 || exp >= prec {
                        format_scientific(buf, &mut out, size, val, prec - 1, if conv == b'G' { b'E' } else { b'e' }, flag_plus, flag_space);
                    } else {
                        // Use %f with adjusted precision
                        let neg = val < 0.0;
                        let val = if neg { -val } else { val };
                        if neg {
                            write_char(buf, &mut out, size, b'-');
                        } else if flag_plus {
                            write_char(buf, &mut out, size, b'+');
                        }
                        let int_part = val as u64;
                        let frac = val - int_part as f64;
                        let mut tmp = [0u8; 24];
                        let ilen = format_uint(int_part, 10, false, &mut tmp);
                        write_str(buf, &mut out, size, &tmp[24 - ilen..24]);

                        let frac_prec = (prec - 1 - exp).max(0) as usize;
                        if frac_prec > 0 {
                            write_char(buf, &mut out, size, b'.');
                            let mut mult = 1.0f64;
                            for _ in 0..frac_prec {
                                mult *= 10.0;
                            }
                            let frac_int = (frac * mult + 0.5) as u64;
                            let flen = format_uint(frac_int, 10, false, &mut tmp);
                            for _ in flen..frac_prec {
                                write_char(buf, &mut out, size, b'0');
                            }
                            write_str(buf, &mut out, size, &tmp[24 - flen..24]);
                        }
                    }
                }
            }

            b'n' => {
                let p: *mut i32 = ap.arg::<*mut i32>();
                if !p.is_null() {
                    *p = out as i32;
                }
            }

            b'%' => {
                write_char(buf, &mut out, size, b'%');
            }

            _ => {
                write_char(buf, &mut out, size, b'%');
                write_char(buf, &mut out, size, conv);
            }
        }
    }

    // Null-terminate
    if size > 0 {
        let pos = core::cmp::min(out, size - 1);
        *buf.add(pos) = 0;
    }

    out as i32
}

fn format_uint(mut val: u64, base: u32, upper: bool, buf: &mut [u8; 24]) -> usize {
    let digits = if upper {
        b"0123456789ABCDEF"
    } else {
        b"0123456789abcdef"
    };

    if val == 0 {
        buf[23] = b'0';
        return 1;
    }

    let mut len = 0;
    while val > 0 {
        buf[23 - len] = digits[(val % base as u64) as usize];
        val /= base as u64;
        len += 1;
    }
    len
}

unsafe fn format_scientific(
    buf: *mut u8,
    out: &mut usize,
    size: usize,
    val: f64,
    precision: i32,
    conv: u8,
    flag_plus: bool,
    flag_space: bool,
) {
    let write_char = |buf: *mut u8, out: &mut usize, size: usize, c: u8| {
        if *out < size.saturating_sub(1) {
            *buf.add(*out) = c;
        }
        *out += 1;
    };

    let write_str = |buf: *mut u8, out: &mut usize, size: usize, s: &[u8]| {
        for &c in s {
            if *out < size.saturating_sub(1) {
                *buf.add(*out) = c;
            }
            *out += 1;
        }
    };

    let prec = if precision < 0 { 6 } else { precision as usize };

    if val != val {
        write_str(buf, out, size, if conv == b'E' { b"NAN" } else { b"nan" });
        return;
    }
    if val == f64::INFINITY {
        if flag_plus { write_char(buf, out, size, b'+'); }
        write_str(buf, out, size, if conv == b'E' { b"INF" } else { b"inf" });
        return;
    }
    if val == f64::NEG_INFINITY {
        write_char(buf, out, size, b'-');
        write_str(buf, out, size, if conv == b'E' { b"INF" } else { b"inf" });
        return;
    }

    let neg = val < 0.0;
    let mut val = if neg { -val } else { val };

    if neg {
        write_char(buf, out, size, b'-');
    } else if flag_plus {
        write_char(buf, out, size, b'+');
    } else if flag_space {
        write_char(buf, out, size, b' ');
    }

    let mut exp: i32 = 0;
    if val > 0.0 {
        exp = crate::math::floor(crate::math::ln(val) / crate::math::consts::LN_10) as i32;
        val /= crate::math::pow(10.0, exp as f64);
        // Normalize to [1, 10)
        while val >= 10.0 {
            val /= 10.0;
            exp += 1;
        }
        while val < 1.0 && val > 0.0 {
            val *= 10.0;
            exp -= 1;
        }
    }

    let int_part = val as u8;
    let frac = val - int_part as f64;
    write_char(buf, out, size, b'0' + int_part);

    if prec > 0 {
        write_char(buf, out, size, b'.');
        let mut mult = 1.0f64;
        for _ in 0..prec {
            mult *= 10.0;
        }
        let frac_int = (frac * mult + 0.5) as u64;
        let mut tmp = [0u8; 24];
        let flen = format_uint(frac_int, 10, false, &mut tmp);
        for _ in flen..prec {
            write_char(buf, out, size, b'0');
        }
        write_str(buf, out, size, &tmp[24 - flen..24]);
    }

    write_char(buf, out, size, if conv == b'E' { b'E' } else { b'e' });
    if exp >= 0 {
        write_char(buf, out, size, b'+');
    } else {
        write_char(buf, out, size, b'-');
        exp = -exp;
    }
    if exp < 10 {
        write_char(buf, out, size, b'0');
    }
    let mut tmp = [0u8; 24];
    let elen = format_uint(exp as u64, 10, false, &mut tmp);
    write_str(buf, out, size, &tmp[24 - elen..24]);
}
