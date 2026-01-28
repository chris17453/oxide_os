//! Wide character support

/// Wide character type (32-bit)
pub type WcharT = i32;

/// Wide int type for wint_t
pub type WintT = u32;

/// End of file for wide characters
pub const WEOF: WintT = 0xFFFFFFFF;

/// Multibyte conversion state
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct MbState {
    /// Conversion state
    pub count: i32,
    /// Partial character
    pub value: WcharT,
}

impl MbState {
    /// Create new state
    pub const fn new() -> Self {
        MbState { count: 0, value: 0 }
    }

    /// Check if state is initial
    pub fn is_initial(&self) -> bool {
        self.count == 0
    }

    /// Reset state
    pub fn reset(&mut self) {
        self.count = 0;
        self.value = 0;
    }
}

/// Get length of wide string
pub fn wcslen(s: *const WcharT) -> usize {
    if s.is_null() {
        return 0;
    }

    let mut len = 0;
    unsafe {
        while *s.add(len) != 0 {
            len += 1;
        }
    }
    len
}

/// Copy wide string
pub unsafe fn wcscpy(dest: *mut WcharT, src: *const WcharT) -> *mut WcharT {
    if dest.is_null() || src.is_null() {
        return dest;
    }

    let mut i = 0;
    loop {
        let c = *src.add(i);
        *dest.add(i) = c;
        if c == 0 {
            break;
        }
        i += 1;
    }

    dest
}

/// Copy wide string with length limit
pub unsafe fn wcsncpy(dest: *mut WcharT, src: *const WcharT, n: usize) -> *mut WcharT {
    if dest.is_null() || src.is_null() {
        return dest;
    }

    let mut i = 0;
    while i < n {
        let c = *src.add(i);
        *dest.add(i) = c;
        if c == 0 {
            // Pad with nulls
            while i < n {
                *dest.add(i) = 0;
                i += 1;
            }
            break;
        }
        i += 1;
    }

    dest
}

/// Concatenate wide strings
pub unsafe fn wcscat(dest: *mut WcharT, src: *const WcharT) -> *mut WcharT {
    if dest.is_null() || src.is_null() {
        return dest;
    }

    let dest_len = wcslen(dest);
    wcscpy(dest.add(dest_len), src);
    dest
}

/// Compare wide strings
pub unsafe fn wcscmp(s1: *const WcharT, s2: *const WcharT) -> i32 {
    if s1.is_null() || s2.is_null() {
        return 0;
    }

    let mut i = 0;
    loop {
        let c1 = *s1.add(i);
        let c2 = *s2.add(i);

        if c1 != c2 {
            return c1 - c2;
        }
        if c1 == 0 {
            return 0;
        }
        i += 1;
    }
}

/// Compare wide strings with length limit
pub unsafe fn wcsncmp(s1: *const WcharT, s2: *const WcharT, n: usize) -> i32 {
    if s1.is_null() || s2.is_null() || n == 0 {
        return 0;
    }

    for i in 0..n {
        let c1 = *s1.add(i);
        let c2 = *s2.add(i);

        if c1 != c2 {
            return c1 - c2;
        }
        if c1 == 0 {
            return 0;
        }
    }

    0
}

/// Find character in wide string
pub unsafe fn wcschr(s: *const WcharT, c: WcharT) -> *const WcharT {
    if s.is_null() {
        return core::ptr::null();
    }

    let mut i = 0;
    loop {
        let ch = *s.add(i);
        if ch == c {
            return s.add(i);
        }
        if ch == 0 {
            return core::ptr::null();
        }
        i += 1;
    }
}

/// Find last occurrence of character in wide string
pub unsafe fn wcsrchr(s: *const WcharT, c: WcharT) -> *const WcharT {
    if s.is_null() {
        return core::ptr::null();
    }

    let mut last: *const WcharT = core::ptr::null();
    let mut i = 0;
    loop {
        let ch = *s.add(i);
        if ch == c {
            last = s.add(i);
        }
        if ch == 0 {
            break;
        }
        i += 1;
    }

    last
}

/// Convert multibyte to wide character
pub unsafe fn mbtowc(pwc: *mut WcharT, s: *const u8, n: usize) -> i32 {
    if s.is_null() {
        return 0; // No state-dependent encoding
    }

    if n == 0 {
        return -1;
    }

    let c = *s;

    // ASCII (single byte)
    if c < 0x80 {
        if !pwc.is_null() {
            *pwc = c as WcharT;
        }
        return if c == 0 { 0 } else { 1 };
    }

    // UTF-8 decoding
    let (len, mut wc) = if c & 0xE0 == 0xC0 {
        (2, (c & 0x1F) as WcharT)
    } else if c & 0xF0 == 0xE0 {
        (3, (c & 0x0F) as WcharT)
    } else if c & 0xF8 == 0xF0 {
        (4, (c & 0x07) as WcharT)
    } else {
        return -1; // Invalid UTF-8
    };

    if n < len {
        return -1;
    }

    for i in 1..len {
        let b = *s.add(i);
        if b & 0xC0 != 0x80 {
            return -1; // Invalid continuation byte
        }
        wc = (wc << 6) | (b & 0x3F) as WcharT;
    }

    if !pwc.is_null() {
        *pwc = wc;
    }

    len as i32
}

/// Convert wide character to multibyte
pub unsafe fn wctomb(s: *mut u8, wc: WcharT) -> i32 {
    if s.is_null() {
        return 0; // No state-dependent encoding
    }

    // ASCII
    if wc < 0x80 {
        *s = wc as u8;
        return 1;
    }

    // UTF-8 encoding
    if wc < 0x800 {
        *s = (0xC0 | (wc >> 6)) as u8;
        *s.add(1) = (0x80 | (wc & 0x3F)) as u8;
        return 2;
    }

    if wc < 0x10000 {
        *s = (0xE0 | (wc >> 12)) as u8;
        *s.add(1) = (0x80 | ((wc >> 6) & 0x3F)) as u8;
        *s.add(2) = (0x80 | (wc & 0x3F)) as u8;
        return 3;
    }

    if wc < 0x110000 {
        *s = (0xF0 | (wc >> 18)) as u8;
        *s.add(1) = (0x80 | ((wc >> 12) & 0x3F)) as u8;
        *s.add(2) = (0x80 | ((wc >> 6) & 0x3F)) as u8;
        *s.add(3) = (0x80 | (wc & 0x3F)) as u8;
        return 4;
    }

    -1 // Invalid wide character
}

/// Maximum bytes per character
pub const MB_CUR_MAX: usize = 4; // UTF-8 max
pub const MB_LEN_MAX: usize = 4;

/// Check if wide character is alphanumeric
pub fn iswalnum(wc: WintT) -> bool {
    iswalpha(wc) || iswdigit(wc)
}

/// Check if wide character is alphabetic
pub fn iswalpha(wc: WintT) -> bool {
    iswupper(wc) || iswlower(wc)
}

/// Check if wide character is blank
pub fn iswblank(wc: WintT) -> bool {
    wc == ' ' as WintT || wc == '\t' as WintT
}

/// Check if wide character is control
pub fn iswcntrl(wc: WintT) -> bool {
    wc < 32 || wc == 127
}

/// Check if wide character is digit
pub fn iswdigit(wc: WintT) -> bool {
    wc >= '0' as WintT && wc <= '9' as WintT
}

/// Check if wide character is graphical
pub fn iswgraph(wc: WintT) -> bool {
    iswprint(wc) && !iswspace(wc)
}

/// Check if wide character is lowercase
pub fn iswlower(wc: WintT) -> bool {
    wc >= 'a' as WintT && wc <= 'z' as WintT
}

/// Check if wide character is printable
pub fn iswprint(wc: WintT) -> bool {
    wc >= 32 && wc < 127
}

/// Check if wide character is punctuation
pub fn iswpunct(wc: WintT) -> bool {
    iswgraph(wc) && !iswalnum(wc)
}

/// Check if wide character is whitespace
pub fn iswspace(wc: WintT) -> bool {
    wc == ' ' as WintT
        || wc == '\t' as WintT
        || wc == '\n' as WintT
        || wc == '\r' as WintT
        || wc == '\x0b' as WintT
        || wc == '\x0c' as WintT
}

/// Check if wide character is uppercase
pub fn iswupper(wc: WintT) -> bool {
    wc >= 'A' as WintT && wc <= 'Z' as WintT
}

/// Check if wide character is hex digit
pub fn iswxdigit(wc: WintT) -> bool {
    iswdigit(wc)
        || (wc >= 'A' as WintT && wc <= 'F' as WintT)
        || (wc >= 'a' as WintT && wc <= 'f' as WintT)
}

/// Convert wide character to lowercase
pub fn towlower(wc: WintT) -> WintT {
    if iswupper(wc) { wc + 32 } else { wc }
}

/// Convert wide character to uppercase
pub fn towupper(wc: WintT) -> WintT {
    if iswlower(wc) { wc - 32 } else { wc }
}

/// Thread-local storage for wcstok state
static mut WCSTOK_STATE: *mut WcharT = core::ptr::null_mut();

/// Split wide string into tokens (C wcstok function)
pub unsafe fn wcstok(s: *mut WcharT, delim: *const WcharT, ptr: *mut *mut WcharT) -> *mut WcharT {
    // Use provided pointer for state if available, otherwise use static
    let state = if !ptr.is_null() {
        ptr
    } else {
        &raw mut WCSTOK_STATE
    };

    // Determine starting position
    let mut str_ptr = if !s.is_null() {
        s
    } else {
        *state
    };

    // If null or end of string, return null
    if str_ptr.is_null() || *str_ptr == 0 {
        *state = core::ptr::null_mut();
        return core::ptr::null_mut();
    }

    // Skip leading delimiters
    loop {
        let ch = *str_ptr;
        if ch == 0 {
            *state = core::ptr::null_mut();
            return core::ptr::null_mut();
        }

        let mut is_delim = false;
        let mut d = delim;
        while *d != 0 {
            if ch == *d {
                is_delim = true;
                break;
            }
            d = d.add(1);
        }

        if !is_delim {
            break;
        }
        str_ptr = str_ptr.add(1);
    }

    // Mark token start
    let token = str_ptr;

    // Find end of token
    loop {
        let ch = *str_ptr;
        if ch == 0 {
            *state = core::ptr::null_mut();
            return token;
        }

        let mut is_delim = false;
        let mut d = delim;
        while *d != 0 {
            if ch == *d {
                is_delim = true;
                break;
            }
            d = d.add(1);
        }

        if is_delim {
            *str_ptr = 0;
            *state = str_ptr.add(1);
            return token;
        }

        str_ptr = str_ptr.add(1);
    }
}
