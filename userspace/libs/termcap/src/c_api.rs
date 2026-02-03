//! # C-Compatible API for Termcap
//!
//! Provides traditional termcap C API functions for linking with existing programs.
//!
//! -- NeonRoot: C ABI bridge - seamless integration with legacy codebases

use crate::{current_terminal, expand, load_terminal, set_current_terminal};
use core::ffi::{c_char, c_int};
use core::sync::atomic::{AtomicUsize, Ordering};

/// ── NeonRoot: Static buffer for capability strings ──
/// Accessed via raw pointers to comply with Rust 2024 static mut rules.
static mut CAP_BUFFER: [u8; 2048] = [0; 2048];
static CAP_BUFFER_POS: AtomicUsize = AtomicUsize::new(0);

/// ── NeonRoot: Raw pointer helpers for the cap buffer ──
#[inline]
unsafe fn cap_buf_ptr() -> *mut u8 {
    core::ptr::addr_of_mut!(CAP_BUFFER) as *mut u8
}

#[inline]
fn cap_buf_len() -> usize {
    2048
}

/// Load terminal entry (traditional tgetent API)
///
/// # Safety
/// This function uses static buffers and is not thread-safe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tgetent(_bp: *mut c_char, name: *const c_char) -> c_int {
    if name.is_null() {
        return -1;
    }

    let name_str = match core::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match load_terminal(name_str) {
        Ok(entry) => {
            set_current_terminal(entry);
            CAP_BUFFER_POS.store(0, Ordering::Relaxed);
            1
        }
        Err(_) => 0,
    }
}

/// Get numeric capability value
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tgetnum(id: *const c_char) -> c_int {
    if id.is_null() {
        return -1;
    }

    let id_str = match core::ffi::CStr::from_ptr(id).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let cap_name = crate::capabilities::termcap_to_terminfo(id_str).unwrap_or(id_str);

    if let Some(term) = current_terminal() {
        term.get_number(cap_name).unwrap_or(-1)
    } else {
        -1
    }
}

/// Get boolean capability flag
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tgetflag(id: *const c_char) -> c_int {
    if id.is_null() {
        return 0;
    }

    let id_str = match core::ffi::CStr::from_ptr(id).to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let cap_name = crate::capabilities::termcap_to_terminfo(id_str).unwrap_or(id_str);

    if let Some(term) = current_terminal() {
        if term.get_flag(cap_name) { 1 } else { 0 }
    } else {
        0
    }
}

/// Get string capability
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tgetstr(id: *const c_char, area: *mut *mut c_char) -> *mut c_char {
    if id.is_null() {
        return core::ptr::null_mut();
    }

    let id_str = match core::ffi::CStr::from_ptr(id).to_str() {
        Ok(s) => s,
        Err(_) => return core::ptr::null_mut(),
    };

    let cap_name = crate::capabilities::termcap_to_terminfo(id_str).unwrap_or(id_str);

    if let Some(term) = current_terminal() {
        if let Some(cap_str) = term.get_string(cap_name) {
            let bytes = cap_str.as_bytes();
            let buf_start = CAP_BUFFER_POS.load(Ordering::Relaxed);
            let buf = cap_buf_ptr();

            if buf_start + bytes.len() + 1 > cap_buf_len() {
                return core::ptr::null_mut();
            }

            // ── NeonRoot: Copy via raw pointers ──
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.add(buf_start), bytes.len());
            *buf.add(buf_start + bytes.len()) = 0; // Null terminate

            let result_ptr = buf.add(buf_start) as *mut c_char;
            CAP_BUFFER_POS.store(buf_start + bytes.len() + 1, Ordering::Relaxed);

            if !area.is_null() {
                *area = result_ptr;
            }

            return result_ptr;
        }
    }

    core::ptr::null_mut()
}

/// Cursor positioning (old termcap API)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tgoto(cap: *const c_char, col: c_int, line: c_int) -> *mut c_char {
    if cap.is_null() {
        return core::ptr::null_mut();
    }

    let cap_str = match core::ffi::CStr::from_ptr(cap).to_str() {
        Ok(s) => s,
        Err(_) => return core::ptr::null_mut(),
    };

    let expanded = match expand::tgoto(cap_str, col, line) {
        Ok(s) => s,
        Err(_) => return core::ptr::null_mut(),
    };

    let bytes = expanded.as_bytes();
    let buf_start = CAP_BUFFER_POS.load(Ordering::Relaxed);
    let buf = cap_buf_ptr();

    if buf_start + bytes.len() + 1 > cap_buf_len() {
        return core::ptr::null_mut();
    }

    core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.add(buf_start), bytes.len());
    *buf.add(buf_start + bytes.len()) = 0;

    let result_ptr = buf.add(buf_start) as *mut c_char;
    CAP_BUFFER_POS.store(buf_start + bytes.len() + 1, Ordering::Relaxed);

    result_ptr
}

/// Output capability string with padding
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tputs(
    str: *const c_char,
    affcnt: c_int,
    putc: Option<unsafe extern "C" fn(c_int) -> c_int>,
) -> c_int {
    if str.is_null() || putc.is_none() {
        return -1;
    }

    let str_rust = match core::ffi::CStr::from_ptr(str).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let putc_fn = putc.unwrap();

    let (output, _delay_ms) = expand::parse_padding(str_rust);

    for ch in output.chars() {
        putc_fn(ch as c_int);
    }

    let _ = affcnt;

    0
}

/// Get terminal type from environment
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ttytype() -> *const c_char {
    "xterm\0".as_ptr() as *const c_char
}
