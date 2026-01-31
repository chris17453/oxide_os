//! String manipulation functions
//!
//! Basic string operations like strlen, strcpy, strcmp, etc.

/// Calculate length of null-terminated string
pub fn strlen(s: *const u8) -> usize {
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

/// Copy string
pub unsafe fn strcpy(dst: *mut u8, src: *const u8) -> *mut u8 {
    let mut i = 0;
    loop {
        let c = *src.add(i);
        *dst.add(i) = c;
        if c == 0 {
            break;
        }
        i += 1;
    }
    dst
}

/// Copy string with length limit
pub unsafe fn strncpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        let c = *src.add(i);
        *dst.add(i) = c;
        if c == 0 {
            break;
        }
        i += 1;
    }
    while i < n {
        *dst.add(i) = 0;
        i += 1;
    }
    dst
}

/// Compare strings
pub fn strcmp(s1: *const u8, s2: *const u8) -> i32 {
    let mut i = 0;
    unsafe {
        loop {
            let c1 = *s1.add(i);
            let c2 = *s2.add(i);
            if c1 != c2 {
                return (c1 as i32) - (c2 as i32);
            }
            if c1 == 0 {
                return 0;
            }
            i += 1;
        }
    }
}

/// Compare strings with length limit
pub fn strncmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    let mut i = 0;
    unsafe {
        while i < n {
            let c1 = *s1.add(i);
            let c2 = *s2.add(i);
            if c1 != c2 {
                return (c1 as i32) - (c2 as i32);
            }
            if c1 == 0 {
                return 0;
            }
            i += 1;
        }
    }
    0
}

/// Find character in string
pub fn strchr(s: *const u8, c: i32) -> *const u8 {
    let c = c as u8;
    let mut i = 0;
    unsafe {
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
}

/// Find last occurrence of character in string
pub fn strrchr(s: *const u8, c: i32) -> *const u8 {
    let c = c as u8;
    let mut last: *const u8 = core::ptr::null();
    let mut i = 0;
    unsafe {
        loop {
            let ch = *s.add(i);
            if ch == c {
                last = s.add(i);
            }
            if ch == 0 {
                return last;
            }
            i += 1;
        }
    }
}

/// Find byte in memory
pub unsafe fn memchr(s: *const u8, c: i32, n: usize) -> *const u8 {
    let c = c as u8;
    for i in 0..n {
        if *s.add(i) == c {
            return s.add(i);
        }
    }
    core::ptr::null()
}

/// Get length of prefix substring not containing any character from reject
pub unsafe fn strcspn(s: *const u8, reject: *const u8) -> usize {
    let mut i = 0;
    loop {
        let ch = *s.add(i);
        if ch == 0 {
            return i;
        }

        // Check if ch is in reject set
        let mut j = 0;
        loop {
            let r = *reject.add(j);
            if r == 0 {
                break;
            }
            if ch == r {
                return i;
            }
            j += 1;
        }
        i += 1;
    }
}

/// Find first occurrence in string of any character from accept
pub unsafe fn strpbrk(s: *const u8, accept: *const u8) -> *const u8 {
    let mut i = 0;
    loop {
        let ch = *s.add(i);
        if ch == 0 {
            return core::ptr::null();
        }

        // Check if ch is in accept set
        let mut j = 0;
        loop {
            let a = *accept.add(j);
            if a == 0 {
                break;
            }
            if ch == a {
                return s.add(i);
            }
            j += 1;
        }
        i += 1;
    }
}

/// Copy memory
pub unsafe fn memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    for i in 0..n {
        *dst.add(i) = *src.add(i);
    }
    dst
}

/// Move memory (handles overlapping regions)
pub unsafe fn memmove(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if (dst as usize) < (src as usize) {
        // Copy forward
        for i in 0..n {
            *dst.add(i) = *src.add(i);
        }
    } else {
        // Copy backward
        for i in (0..n).rev() {
            *dst.add(i) = *src.add(i);
        }
    }
    dst
}

/// Set memory
pub unsafe fn memset(dst: *mut u8, c: i32, n: usize) -> *mut u8 {
    for i in 0..n {
        *dst.add(i) = c as u8;
    }
    dst
}

/// Compare memory
pub fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    unsafe {
        for i in 0..n {
            let c1 = *s1.add(i);
            let c2 = *s2.add(i);
            if c1 != c2 {
                return (c1 as i32) - (c2 as i32);
            }
        }
    }
    0
}

/// Compare string slices
pub fn str_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let a = a.as_bytes();
    let b = b.as_bytes();
    for i in 0..a.len() {
        if a[i] != b[i] {
            return false;
        }
    }
    true
}

/// Return error string for errno value
pub fn strerror_rust(errnum: i32) -> &'static str {
    match errnum {
        0 => "Success",
        1 => "Operation not permitted",
        2 => "No such file or directory",
        3 => "No such process",
        4 => "Interrupted system call",
        5 => "Input/output error",
        9 => "Bad file descriptor",
        10 => "No child processes",
        11 => "Resource temporarily unavailable",
        12 => "Cannot allocate memory",
        13 => "Permission denied",
        14 => "Bad address",
        16 => "Device or resource busy",
        17 => "File exists",
        19 => "No such device",
        20 => "Not a directory",
        21 => "Is a directory",
        22 => "Invalid argument",
        23 => "Too many open files in system",
        24 => "Too many open files",
        25 => "Inappropriate ioctl for device",
        27 => "File too large",
        28 => "No space left on device",
        29 => "Illegal seek",
        30 => "Read-only file system",
        32 => "Broken pipe",
        33 => "Numerical argument out of domain",
        34 => "Numerical result out of range",
        36 => "File name too long",
        38 => "Function not implemented",
        39 => "Directory not empty",
        75 => "Value too large for defined data type",
        84 => "Invalid or incomplete multibyte or wide character",
        _ => "Unknown error",
    }
}

/// Find substring (Rust version)
pub fn strstr(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.len() > h.len() {
        return None;
    }
    for i in 0..=(h.len() - n.len()) {
        let mut found = true;
        for j in 0..n.len() {
            if h[i + j] != n[j] {
                found = false;
                break;
            }
        }
        if found {
            return Some(i);
        }
    }
    None
}

/// Find substring in null-terminated string (C strstr)
pub unsafe fn strstr_c(haystack: *const u8, needle: *const u8) -> *const u8 {
    if haystack.is_null() || needle.is_null() {
        return core::ptr::null();
    }

    // Get needle length
    let mut needle_len = 0;
    while *needle.add(needle_len) != 0 {
        needle_len += 1;
    }

    // Empty needle matches at beginning
    if needle_len == 0 {
        return haystack;
    }

    // Search for needle in haystack
    let mut i = 0;
    loop {
        let h_char = *haystack.add(i);
        if h_char == 0 {
            break; // End of haystack
        }

        // Check if needle matches at this position
        let mut matches = true;
        for j in 0..needle_len {
            if *haystack.add(i + j) != *needle.add(j) {
                matches = false;
                break;
            }
        }

        if matches {
            return haystack.add(i);
        }

        i += 1;
    }

    core::ptr::null()
}

/// Fill memory with a byte value that won't be optimized away
///
/// Uses volatile writes to ensure the compiler doesn't elide the fill.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn explicit_memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    let byte = c as u8;
    for i in 0..n {
        unsafe {
            core::ptr::write_volatile(s.add(i), byte);
        }
    }
    s
}
