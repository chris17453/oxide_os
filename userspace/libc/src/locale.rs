//! Locale support

/// Locale categories
pub mod category {
    pub const LC_CTYPE: i32 = 0;
    pub const LC_NUMERIC: i32 = 1;
    pub const LC_TIME: i32 = 2;
    pub const LC_COLLATE: i32 = 3;
    pub const LC_MONETARY: i32 = 4;
    pub const LC_MESSAGES: i32 = 5;
    pub const LC_ALL: i32 = 6;
}

/// Locale conversion info
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Lconv {
    /// Decimal point character
    pub decimal_point: *const u8,
    /// Thousands separator
    pub thousands_sep: *const u8,
    /// Grouping
    pub grouping: *const u8,
    /// International currency symbol
    pub int_curr_symbol: *const u8,
    /// Local currency symbol
    pub currency_symbol: *const u8,
    /// Monetary decimal point
    pub mon_decimal_point: *const u8,
    /// Monetary thousands separator
    pub mon_thousands_sep: *const u8,
    /// Monetary grouping
    pub mon_grouping: *const u8,
    /// Positive sign
    pub positive_sign: *const u8,
    /// Negative sign
    pub negative_sign: *const u8,
    /// International fractional digits
    pub int_frac_digits: i8,
    /// Local fractional digits
    pub frac_digits: i8,
    /// Currency symbol precedes positive value
    pub p_cs_precedes: i8,
    /// Space separates currency symbol and positive value
    pub p_sep_by_space: i8,
    /// Currency symbol precedes negative value
    pub n_cs_precedes: i8,
    /// Space separates currency symbol and negative value
    pub n_sep_by_space: i8,
    /// Position of positive sign
    pub p_sign_posn: i8,
    /// Position of negative sign
    pub n_sign_posn: i8,
    /// Int currency symbol precedes positive value
    pub int_p_cs_precedes: i8,
    /// Int space separates currency symbol and positive value
    pub int_p_sep_by_space: i8,
    /// Int currency symbol precedes negative value
    pub int_n_cs_precedes: i8,
    /// Int space separates currency symbol and negative value
    pub int_n_sep_by_space: i8,
    /// Int position of positive sign
    pub int_p_sign_posn: i8,
    /// Int position of negative sign
    pub int_n_sign_posn: i8,
}

/// Default C locale strings
static DECIMAL_POINT: &[u8] = b".\0";
static THOUSANDS_SEP: &[u8] = b"\0";
static GROUPING: &[u8] = b"\0";
static EMPTY: &[u8] = b"\0";
#[allow(dead_code)]
static CHAR_MAX: i8 = 127;

/// Default lconv for C locale
static mut C_LCONV: Lconv = Lconv {
    decimal_point: DECIMAL_POINT.as_ptr(),
    thousands_sep: THOUSANDS_SEP.as_ptr(),
    grouping: GROUPING.as_ptr(),
    int_curr_symbol: EMPTY.as_ptr(),
    currency_symbol: EMPTY.as_ptr(),
    mon_decimal_point: EMPTY.as_ptr(),
    mon_thousands_sep: EMPTY.as_ptr(),
    mon_grouping: EMPTY.as_ptr(),
    positive_sign: EMPTY.as_ptr(),
    negative_sign: EMPTY.as_ptr(),
    int_frac_digits: 127,
    frac_digits: 127,
    p_cs_precedes: 127,
    p_sep_by_space: 127,
    n_cs_precedes: 127,
    n_sep_by_space: 127,
    p_sign_posn: 127,
    n_sign_posn: 127,
    int_p_cs_precedes: 127,
    int_p_sep_by_space: 127,
    int_n_cs_precedes: 127,
    int_n_sep_by_space: 127,
    int_p_sign_posn: 127,
    int_n_sign_posn: 127,
};

/// Current locale name
static mut CURRENT_LOCALE: &[u8] = b"C\0";

/// Set locale
///
/// # Safety
/// locale must be a valid null-terminated string or null
pub unsafe fn setlocale(_category: i32, locale: *const u8) -> *const u8 {
    let current_ptr = &raw mut CURRENT_LOCALE;

    // If locale is null, return current locale
    if locale.is_null() {
        return (*current_ptr).as_ptr();
    }

    // Get locale string
    let mut len = 0;
    while *locale.add(len) != 0 {
        len += 1;
    }

    // Only "C" and "POSIX" are supported
    let locale_str = core::slice::from_raw_parts(locale, len);
    if locale_str == b"C" || locale_str == b"POSIX" || locale_str == b"" {
        *current_ptr = b"C\0";
        return (*current_ptr).as_ptr();
    }

    // Unsupported locale
    core::ptr::null()
}

/// Get locale conversion info
pub fn localeconv() -> *mut Lconv {
    unsafe { &raw mut C_LCONV }
}

/// Locale-aware character classification
pub mod ctype {
    /// Check if character is alphanumeric
    pub fn isalnum(c: i32) -> bool {
        isalpha(c) || isdigit(c)
    }

    /// Check if character is alphabetic
    pub fn isalpha(c: i32) -> bool {
        (c >= b'A' as i32 && c <= b'Z' as i32) || (c >= b'a' as i32 && c <= b'z' as i32)
    }

    /// Check if character is ASCII
    pub fn isascii(c: i32) -> bool {
        c >= 0 && c <= 127
    }

    /// Check if character is blank (space or tab)
    pub fn isblank(c: i32) -> bool {
        c == b' ' as i32 || c == b'\t' as i32
    }

    /// Check if character is control character
    pub fn iscntrl(c: i32) -> bool {
        c >= 0 && c < 32 || c == 127
    }

    /// Check if character is digit
    pub fn isdigit(c: i32) -> bool {
        c >= b'0' as i32 && c <= b'9' as i32
    }

    /// Check if character is graphical (printable, not space)
    pub fn isgraph(c: i32) -> bool {
        c > 32 && c < 127
    }

    /// Check if character is lowercase
    pub fn islower(c: i32) -> bool {
        c >= b'a' as i32 && c <= b'z' as i32
    }

    /// Check if character is printable
    pub fn isprint(c: i32) -> bool {
        c >= 32 && c < 127
    }

    /// Check if character is punctuation
    pub fn ispunct(c: i32) -> bool {
        isgraph(c) && !isalnum(c)
    }

    /// Check if character is whitespace
    pub fn isspace(c: i32) -> bool {
        c == b' ' as i32 || c == b'\t' as i32 || c == b'\n' as i32
            || c == b'\r' as i32 || c == b'\x0b' as i32 || c == b'\x0c' as i32
    }

    /// Check if character is uppercase
    pub fn isupper(c: i32) -> bool {
        c >= b'A' as i32 && c <= b'Z' as i32
    }

    /// Check if character is hexadecimal digit
    pub fn isxdigit(c: i32) -> bool {
        isdigit(c) || (c >= b'A' as i32 && c <= b'F' as i32)
            || (c >= b'a' as i32 && c <= b'f' as i32)
    }

    /// Convert to lowercase
    pub fn tolower(c: i32) -> i32 {
        if isupper(c) {
            c + 32
        } else {
            c
        }
    }

    /// Convert to uppercase
    pub fn toupper(c: i32) -> i32 {
        if islower(c) {
            c - 32
        } else {
            c
        }
    }
}
