//! String manipulation functions
//!
//! Basic string operations like strlen, strcpy, strcmp, etc.

/// Calculate length of null-terminated string
pub fn strlen(s: *const u8) -> usize {
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

/// Find substring
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
