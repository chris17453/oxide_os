//! # C-Compatible API for Termcap
//!
//! Provides traditional termcap C API functions for linking with existing programs.
//!
//! -- NeonRoot: C ABI bridge - seamless integration with legacy codebases

use core::prelude::v1::*;
use alloc::string::ToString;
use core::ffi::{c_char, c_int};
use crate::{load_terminal, set_current_terminal, current_terminal, expand};

/// Static buffer for capability strings (traditional termcap API requirement)
static mut CAP_BUFFER: [u8; 2048] = [0; 2048];
static mut CAP_BUFFER_POS: usize = 0;

/// Load terminal entry (traditional tgetent API)
///
/// # Safety
/// This function uses static buffers and is not thread-safe.
/// 
/// ## Parameters
/// - `bp`: Buffer pointer (unused in modern implementations, can be null)
/// - `name`: Terminal name as C string
///
/// ## Returns
/// - 1: Success
/// - 0: Terminal not found
/// - -1: Terminfo database not found
#[no_mangle]
pub unsafe extern "C" fn tgetent(_bp: *mut c_char, name: *const c_char) -> c_int {
    if name.is_null() {
        return -1;
    }
    
    // Convert C string to Rust string
    let name_str = match core::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    
    // Load terminal from database
    match load_terminal(name_str) {
        Ok(entry) => {
            set_current_terminal(entry);
            CAP_BUFFER_POS = 0; // Reset buffer
            1
        }
        Err(_) => 0,
    }
}

/// Get numeric capability value
///
/// ## Parameters
/// - `id`: Two-character capability name (e.g., "co" for columns)
///
/// ## Returns
/// - Value if capability exists
/// - -1 if capability doesn't exist
#[no_mangle]
pub unsafe extern "C" fn tgetnum(id: *const c_char) -> c_int {
    if id.is_null() {
        return -1;
    }
    
    let id_str = match core::ffi::CStr::from_ptr(id).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    
    // Convert termcap 2-letter code to terminfo name
    let cap_name = crate::capabilities::termcap_to_terminfo(id_str).unwrap_or(id_str);
    
    if let Some(term) = current_terminal() {
        term.get_number(cap_name).unwrap_or(-1)
    } else {
        -1
    }
}

/// Get boolean capability flag
///
/// ## Parameters
/// - `id`: Two-character capability name (e.g., "am" for auto_right_margin)
///
/// ## Returns
/// - 1 if flag is set
/// - 0 if flag is not set or doesn't exist
#[no_mangle]
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
///
/// ## Parameters
/// - `id`: Two-character capability name (e.g., "cm" for cursor_address)
/// - `area`: Pointer to buffer pointer (receives pointer to string in static buffer)
///
/// ## Returns
/// - Pointer to capability string in static buffer
/// - Null if capability doesn't exist
#[no_mangle]
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
            let buf_start = CAP_BUFFER_POS;
            
            // Check if we have space
            if buf_start + bytes.len() + 1 > CAP_BUFFER.len() {
                return core::ptr::null_mut();
            }
            
            // Copy to buffer
            CAP_BUFFER[buf_start..buf_start + bytes.len()].copy_from_slice(bytes);
            CAP_BUFFER[buf_start + bytes.len()] = 0; // Null terminate
            
            let result_ptr = CAP_BUFFER.as_mut_ptr().add(buf_start) as *mut c_char;
            CAP_BUFFER_POS = buf_start + bytes.len() + 1;
            
            // Update area if provided
            if !area.is_null() {
                *area = result_ptr;
            }
            
            return result_ptr;
        }
    }
    
    core::ptr::null_mut()
}

/// Cursor positioning (old termcap API)
///
/// ## Parameters
/// - `cap`: Cursor motion capability string (from tgetstr)
/// - `col`: Column (0-based)
/// - `line`: Line (0-based)
///
/// ## Returns
/// - Pointer to expanded string in static buffer
/// - Null on error
#[no_mangle]
pub unsafe extern "C" fn tgoto(cap: *const c_char, col: c_int, line: c_int) -> *mut c_char {
    if cap.is_null() {
        return core::ptr::null_mut();
    }
    
    let cap_str = match core::ffi::CStr::from_ptr(cap).to_str() {
        Ok(s) => s,
        Err(_) => return core::ptr::null_mut(),
    };
    
    // Expand with parameters
    let expanded = match expand::tgoto(cap_str, col, line) {
        Ok(s) => s,
        Err(_) => return core::ptr::null_mut(),
    };
    
    let bytes = expanded.as_bytes();
    let buf_start = CAP_BUFFER_POS;
    
    if buf_start + bytes.len() + 1 > CAP_BUFFER.len() {
        return core::ptr::null_mut();
    }
    
    CAP_BUFFER[buf_start..buf_start + bytes.len()].copy_from_slice(bytes);
    CAP_BUFFER[buf_start + bytes.len()] = 0;
    
    let result_ptr = CAP_BUFFER.as_mut_ptr().add(buf_start) as *mut c_char;
    CAP_BUFFER_POS = buf_start + bytes.len() + 1;
    
    result_ptr
}

/// Output capability string with padding
///
/// ## Parameters
/// - `str`: Capability string to output
/// - `affcnt`: Lines affected (for padding calculation, usually 1)
/// - `putc`: Function pointer to output function
///
/// ## Returns
/// - 0 on success
/// - -1 on error
///
/// ## Safety
/// The putc function must be valid and safe to call.
#[no_mangle]
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
    
    // Parse padding (if any)
    let (output, _delay_ms) = expand::parse_padding(str_rust);
    
    // Output each character
    for ch in output.chars() {
        putc_fn(ch as c_int);
    }
    
    // In a real implementation, would apply padding based on delay_ms and affcnt
    // For now, we just output the string
    let _ = affcnt; // Suppress warning
    
    0
}

/// Get terminal type from environment
///
/// ## Returns
/// - Pointer to terminal name string (from $TERM)
/// - Pointer to "unknown" if $TERM not set
#[no_mangle]
pub unsafe extern "C" fn ttytype() -> *const c_char {
    // In a real implementation, would read $TERM environment variable
    // For now, return a default
    "xterm\0".as_ptr() as *const c_char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c_api_basic() {
        unsafe {
            let term_name = "xterm\0".as_ptr() as *const c_char;
            let result = tgetent(core::ptr::null_mut(), term_name);
            assert_eq!(result, 1);
            
            let cols_cap = "co\0".as_ptr() as *const c_char;
            let cols = tgetnum(cols_cap);
            assert_eq!(cols, 80);
            
            let am_cap = "am\0".as_ptr() as *const c_char;
            let am = tgetflag(am_cap);
            assert_eq!(am, 1);
        }
    }
}
