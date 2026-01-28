//! C-callable exports for all libc functions
//!
//! This module provides #[no_mangle] extern "C" wrappers around
//! the Rust implementations so C code (like CPython) can link against them.

use crate::errno;
use crate::syscall;
use crate::fcntl::*;

/// Compute length of a C string (not including the null terminator)
unsafe fn cstr_len(s: *const u8) -> usize {
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    len
}

// ============ errno ============

static mut ERRNO_VAR: i32 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn __errno_location() -> *mut i32 {
    unsafe { &raw mut ERRNO_VAR }
}

// ============ string basics ============

#[unsafe(no_mangle)]
pub extern "C" fn strlen(s: *const u8) -> usize {
    crate::string::strlen(s)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcpy(dst: *mut u8, src: *const u8) -> *mut u8 {
    crate::string::strcpy(dst, src)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    crate::string::strncpy(dst, src, n)
}

#[unsafe(no_mangle)]
pub extern "C" fn strcmp(s1: *const u8, s2: *const u8) -> i32 {
    crate::string::strcmp(s1, s2)
}

#[unsafe(no_mangle)]
pub extern "C" fn strncmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    crate::string::strncmp(s1, s2, n)
}

#[unsafe(no_mangle)]
pub extern "C" fn strchr(s: *const u8, c: i32) -> *const u8 {
    crate::string::strchr(s, c)
}

#[unsafe(no_mangle)]
pub extern "C" fn strrchr(s: *const u8, c: i32) -> *const u8 {
    crate::string::strrchr(s, c)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    crate::string::memcpy(dst, src, n)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    crate::string::memmove(dst, src, n)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dst: *mut u8, c: i32, n: usize) -> *mut u8 {
    crate::string::memset(dst, c, n)
}

#[unsafe(no_mangle)]
pub extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    crate::string::memcmp(s1, s2, n)
}

// ============ string extras ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strndup(s: *const u8, n: usize) -> *mut u8 {
    if s.is_null() {
        return core::ptr::null_mut();
    }
    let mut len = 0;
    while len < n && *s.add(len) != 0 {
        len += 1;
    }
    let p = malloc(len + 1) as *mut u8;
    if p.is_null() {
        return core::ptr::null_mut();
    }
    core::ptr::copy_nonoverlapping(s, p, len);
    *p.add(len) = 0;
    p
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strsignal(_sig: i32) -> *const u8 {
    b"Unknown signal\0".as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strnlen(s: *const u8, maxlen: usize) -> usize {
    let mut len = 0;
    while len < maxlen && *s.add(len) != 0 {
        len += 1;
    }
    len
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memrchr(s: *const u8, c: i32, n: usize) -> *mut u8 {
    let byte = c as u8;
    let mut i = n;
    while i > 0 {
        i -= 1;
        if *s.add(i) == byte {
            return s.add(i) as *mut u8;
        }
    }
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn explicit_bzero(s: *mut u8, n: usize) {
    core::ptr::write_bytes(s, 0, n);
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bcopy(src: *const u8, dest: *mut u8, n: usize) {
    core::ptr::copy(src, dest, n);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bzero(s: *mut u8, n: usize) {
    core::ptr::write_bytes(s, 0, n);
}

// ============ memory allocation ============

use core::alloc::{GlobalAlloc, Layout};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut u8 {
    if size == 0 {
        return core::ptr::null_mut();
    }
    // Store size before allocation for realloc
    let total = size + 16;
    let layout = Layout::from_size_align_unchecked(total, 16);
    let ptr = alloc::alloc::alloc(layout);
    if ptr.is_null() {
        return core::ptr::null_mut();
    }
    *(ptr as *mut usize) = size;
    ptr.add(16)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn calloc(nmemb: usize, size: usize) -> *mut u8 {
    let total = nmemb.saturating_mul(size);
    let ptr = malloc(total);
    if !ptr.is_null() {
        core::ptr::write_bytes(ptr, 0, total);
    }
    ptr
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn realloc(ptr: *mut u8, size: usize) -> *mut u8 {
    if ptr.is_null() {
        return malloc(size);
    }
    if size == 0 {
        free(ptr);
        return core::ptr::null_mut();
    }
    let old_size = *(ptr.sub(16) as *const usize);
    let new_ptr = malloc(size);
    if new_ptr.is_null() {
        return core::ptr::null_mut();
    }
    let copy_size = core::cmp::min(old_size, size);
    core::ptr::copy_nonoverlapping(ptr, new_ptr, copy_size);
    free(ptr);
    new_ptr
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(_ptr: *mut u8) {
    // Bump allocator doesn't free
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_memalign(memptr: *mut *mut u8, _align: usize, size: usize) -> i32 {
    let p = malloc(size);
    if p.is_null() {
        return errno::ENOMEM;
    }
    *memptr = p;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aligned_alloc(align: usize, size: usize) -> *mut u8 {
    let _ = align;
    malloc(size)
}

// ============ stdlib ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn abort() -> ! {
    syscall::sys_exit(134); // 128 + SIGABRT(6)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn exit(status: i32) -> ! {
    // Flush stdio
    crate::filestream::fflush(core::ptr::null_mut());
    syscall::sys_exit(status);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _exit(status: i32) -> ! {
    syscall::sys_exit(status);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _Exit(status: i32) -> ! {
    syscall::sys_exit(status);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn atexit(_func: extern "C" fn()) -> i32 {
    0 // stub - accept but don't call
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cxa_atexit(
    _func: extern "C" fn(*mut u8),
    _arg: *mut u8,
    _dso_handle: *mut u8,
) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn system(_cmd: *const u8) -> i32 {
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn realpath(path: *const u8, resolved: *mut u8) -> *mut u8 {
    if path.is_null() {
        return core::ptr::null_mut();
    }
    // Simple: just copy path if it starts with /
    if *path == b'/' {
        let out = if resolved.is_null() {
            malloc(4096)
        } else {
            resolved
        };
        if out.is_null() {
            return core::ptr::null_mut();
        }
        let mut i = 0;
        while *path.add(i) != 0 && i < 4095 {
            *out.add(i) = *path.add(i);
            i += 1;
        }
        *out.add(i) = 0;
        return out;
    }
    // For relative paths, prepend cwd
    let mut cwd = [0u8; 4096];
    let ret = syscall::syscall2(syscall::nr::GETCWD, cwd.as_mut_ptr() as usize, 4096) as i32;
    if ret < 0 {
        return core::ptr::null_mut();
    }
    let out = if resolved.is_null() {
        malloc(4096)
    } else {
        resolved
    };
    if out.is_null() {
        return core::ptr::null_mut();
    }
    let mut i = 0;
    while cwd[i] != 0 && i < 4095 {
        *out.add(i) = cwd[i];
        i += 1;
    }
    if i > 0 && *out.add(i - 1) != b'/' {
        *out.add(i) = b'/';
        i += 1;
    }
    let mut j = 0;
    while *path.add(j) != 0 && i < 4095 {
        *out.add(i) = *path.add(j);
        i += 1;
        j += 1;
    }
    *out.add(i) = 0;
    out
}

// ============ qsort / bsearch ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsort(
    base: *mut u8,
    nmemb: usize,
    size: usize,
    compar: unsafe extern "C" fn(*const u8, *const u8) -> i32,
) {
    // Simple insertion sort (good enough for moderate sizes)
    if nmemb <= 1 {
        return;
    }
    let mut tmp = alloc::vec![0u8; size];
    for i in 1..nmemb {
        let mut j = i;
        while j > 0 {
            let a = base.add(j * size);
            let b = base.add((j - 1) * size);
            if compar(a, b) < 0 {
                // swap
                core::ptr::copy_nonoverlapping(a, tmp.as_mut_ptr(), size);
                core::ptr::copy_nonoverlapping(b, a, size);
                core::ptr::copy_nonoverlapping(tmp.as_ptr(), b, size);
                j -= 1;
            } else {
                break;
            }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bsearch(
    key: *const u8,
    base: *const u8,
    nmemb: usize,
    size: usize,
    compar: unsafe extern "C" fn(*const u8, *const u8) -> i32,
) -> *mut u8 {
    let mut lo = 0usize;
    let mut hi = nmemb;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let elem = base.add(mid * size);
        let cmp = compar(key, elem);
        if cmp == 0 {
            return elem as *mut u8;
        } else if cmp < 0 {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    core::ptr::null_mut()
}

// ============ strtod / strtof ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtod(nptr: *const u8, endptr: *mut *mut u8) -> f64 {
    if nptr.is_null() {
        return 0.0;
    }

    let mut i = 0usize;
    // Skip whitespace
    while *nptr.add(i) == b' ' || *nptr.add(i) == b'\t' || *nptr.add(i) == b'\n' {
        i += 1;
    }

    let negative = if *nptr.add(i) == b'-' {
        i += 1;
        true
    } else {
        if *nptr.add(i) == b'+' {
            i += 1;
        }
        false
    };

    // Check for inf/nan
    if (*nptr.add(i) == b'i' || *nptr.add(i) == b'I')
        && (*nptr.add(i + 1) == b'n' || *nptr.add(i + 1) == b'N')
        && (*nptr.add(i + 2) == b'f' || *nptr.add(i + 2) == b'F')
    {
        i += 3;
        if (*nptr.add(i) == b'i' || *nptr.add(i) == b'I') {
            i += 5; // "inity"
        }
        if !endptr.is_null() {
            *endptr = nptr.add(i) as *mut u8;
        }
        return if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }

    if (*nptr.add(i) == b'n' || *nptr.add(i) == b'N')
        && (*nptr.add(i + 1) == b'a' || *nptr.add(i + 1) == b'A')
        && (*nptr.add(i + 2) == b'n' || *nptr.add(i + 2) == b'N')
    {
        i += 3;
        if !endptr.is_null() {
            *endptr = nptr.add(i) as *mut u8;
        }
        return f64::NAN;
    }

    // Check for hex float
    if *nptr.add(i) == b'0' && (*nptr.add(i + 1) == b'x' || *nptr.add(i + 1) == b'X') {
        i += 2;
        return strtod_hex(nptr, &mut i, negative, endptr);
    }

    let start = i;
    let mut result: f64 = 0.0;

    // Integer part
    while *nptr.add(i) >= b'0' && *nptr.add(i) <= b'9' {
        result = result * 10.0 + (*nptr.add(i) - b'0') as f64;
        i += 1;
    }

    // Fractional part
    if *nptr.add(i) == b'.' {
        i += 1;
        let mut frac = 0.1;
        while *nptr.add(i) >= b'0' && *nptr.add(i) <= b'9' {
            result += (*nptr.add(i) - b'0') as f64 * frac;
            frac *= 0.1;
            i += 1;
        }
    }

    if i == start {
        if !endptr.is_null() {
            *endptr = nptr as *mut u8;
        }
        return 0.0;
    }

    // Exponent
    if *nptr.add(i) == b'e' || *nptr.add(i) == b'E' {
        i += 1;
        let exp_neg = if *nptr.add(i) == b'-' {
            i += 1;
            true
        } else {
            if *nptr.add(i) == b'+' {
                i += 1;
            }
            false
        };
        let mut exp: i32 = 0;
        while *nptr.add(i) >= b'0' && *nptr.add(i) <= b'9' {
            exp = exp * 10 + (*nptr.add(i) - b'0') as i32;
            i += 1;
        }
        if exp_neg {
            exp = -exp;
        }
        result *= crate::math::pow(10.0, exp as f64);
    }

    if !endptr.is_null() {
        *endptr = nptr.add(i) as *mut u8;
    }

    if negative { -result } else { result }
}

unsafe fn strtod_hex(
    nptr: *const u8,
    i: &mut usize,
    negative: bool,
    endptr: *mut *mut u8,
) -> f64 {
    let mut result: f64 = 0.0;

    while hex_val(*nptr.add(*i)) >= 0 {
        result = result * 16.0 + hex_val(*nptr.add(*i)) as f64;
        *i += 1;
    }

    if *nptr.add(*i) == b'.' {
        *i += 1;
        let mut frac = 1.0 / 16.0;
        while hex_val(*nptr.add(*i)) >= 0 {
            result += hex_val(*nptr.add(*i)) as f64 * frac;
            frac /= 16.0;
            *i += 1;
        }
    }

    // Binary exponent
    if *nptr.add(*i) == b'p' || *nptr.add(*i) == b'P' {
        *i += 1;
        let exp_neg = if *nptr.add(*i) == b'-' {
            *i += 1;
            true
        } else {
            if *nptr.add(*i) == b'+' {
                *i += 1;
            }
            false
        };
        let mut exp: i32 = 0;
        while *nptr.add(*i) >= b'0' && *nptr.add(*i) <= b'9' {
            exp = exp * 10 + (*nptr.add(*i) - b'0') as i32;
            *i += 1;
        }
        if exp_neg {
            exp = -exp;
        }
        result *= crate::math::pow(2.0, exp as f64);
    }

    if !endptr.is_null() {
        *endptr = nptr.add(*i) as *mut u8;
    }

    if negative { -result } else { result }
}

fn hex_val(c: u8) -> i32 {
    match c {
        b'0'..=b'9' => (c - b'0') as i32,
        b'a'..=b'f' => (c - b'a' + 10) as i32,
        b'A'..=b'F' => (c - b'A' + 10) as i32,
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtof(nptr: *const u8, endptr: *mut *mut u8) -> f32 {
    strtod(nptr, endptr) as f32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtold(nptr: *const u8, endptr: *mut *mut u8) -> f64 {
    strtod(nptr, endptr) // long double = double on x86_64
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn atof(nptr: *const u8) -> f64 {
    strtod(nptr, core::ptr::null_mut())
}

// ============ assert ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __assert_fail(
    expr: *const u8,
    file: *const u8,
    line: i32,
    func: *const u8,
) -> ! {
    // Print assertion failure to stderr
    let msg = b"Assertion failed: \0";
    syscall::sys_write(2, &msg[..msg.len() - 1]);
    if !expr.is_null() {
        let mut len = 0;
        while *expr.add(len) != 0 {
            len += 1;
        }
        syscall::sys_write(2, core::slice::from_raw_parts(expr, len));
    }
    syscall::sys_write(2, b"\n");
    abort();
}

// ============ process stubs ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn raise(sig: i32) -> i32 {
    let pid = syscall::sys_getpid();
    syscall::sys_kill(pid, sig)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn signal(
    _signum: i32,
    handler: usize,
) -> usize {
    0 // SIG_DFL - stub
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigemptyset(set: *mut u64) -> i32 {
    if !set.is_null() {
        *set = 0;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigfillset(set: *mut u64) -> i32 {
    if !set.is_null() {
        *set = !0u64;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigaddset(set: *mut u64, signum: i32) -> i32 {
    if !set.is_null() && signum > 0 && signum < 64 {
        *set |= 1u64 << (signum - 1);
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigdelset(set: *mut u64, signum: i32) -> i32 {
    if !set.is_null() && signum > 0 && signum < 64 {
        *set &= !(1u64 << (signum - 1));
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigismember(set: *const u64, signum: i32) -> i32 {
    if set.is_null() || signum <= 0 || signum >= 64 {
        return 0;
    }
    ((*set >> (signum - 1)) & 1) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigaction(
    _signum: i32,
    _act: *const u8,
    _oldact: *mut u8,
) -> i32 {
    0 // stub
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigprocmask(
    _how: i32,
    _set: *const u64,
    _oldset: *mut u64,
) -> i32 {
    0 // stub
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_sigmask(
    _how: i32,
    _set: *const u64,
    _oldset: *mut u64,
) -> i32 {
    0
}

// ============ unistd wrappers ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn read(fd: i32, buf: *mut u8, count: usize) -> isize {
    syscall::syscall3(syscall::nr::READ, fd as usize, buf as usize, count) as isize
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn write(fd: i32, buf: *const u8, count: usize) -> isize {
    syscall::syscall3(syscall::nr::WRITE, fd as usize, buf as usize, count) as isize
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn close(fd: i32) -> i32 {
    syscall::sys_close(fd)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    syscall::sys_lseek(fd, offset, whence)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn open(path: *const u8, flags: i32, mode: i32) -> i32 {
    syscall::syscall4(
        syscall::nr::OPEN,
        path as usize,
        cstr_len(path),
        flags as usize,
        mode as usize,
    ) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn dup(oldfd: i32) -> i32 {
    syscall::sys_dup(oldfd)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn dup2(oldfd: i32, newfd: i32) -> i32 {
    syscall::sys_dup2(oldfd, newfd)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pipe(pipefd: *mut i32) -> i32 {
    let mut fd_pair = [0i32; 2];
    let ret = syscall::sys_pipe(&mut fd_pair);
    if ret == 0 {
        *pipefd = fd_pair[0];
        *pipefd.add(1) = fd_pair[1];
    }
    ret
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fork() -> i32 {
    syscall::sys_fork()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn execve(
    path: *const u8,
    argv: *const *const u8,
    envp: *const *const u8,
) -> i32 {
    syscall::syscall4(
        syscall::nr::EXECVE,
        path as usize,
        cstr_len(path),
        argv as usize,
        envp as usize,
    ) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn execv(path: *const u8, argv: *const *const u8) -> i32 {
    syscall::syscall4(
        syscall::nr::EXECVE,
        path as usize,
        cstr_len(path),
        argv as usize,
        core::ptr::null::<*const u8>() as usize,
    ) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn execvp(_file: *const u8, _argv: *const *const u8) -> i32 {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32 {
    let mut stat: i32 = 0;
    let ret = syscall::sys_waitpid(pid, &mut stat, options);
    if !status.is_null() {
        *status = stat;
    }
    ret
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wait(status: *mut i32) -> i32 {
    waitpid(-1, status, 0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpid() -> i32 {
    syscall::sys_getpid()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getppid() -> i32 {
    syscall::sys_getppid()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getuid() -> u32 {
    syscall::syscall0(syscall::nr::GETUID) as u32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn geteuid() -> u32 {
    syscall::syscall0(syscall::nr::GETEUID) as u32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getgid() -> u32 {
    syscall::syscall0(syscall::nr::GETGID) as u32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getegid() -> u32 {
    syscall::syscall0(syscall::nr::GETEGID) as u32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setuid(_uid: u32) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setgid(_gid: u32) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seteuid(_uid: u32) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setegid(_gid: u32) -> i32 { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chdir(path: *const u8) -> i32 {
    syscall::syscall2(syscall::nr::CHDIR, path as usize, cstr_len(path)) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getcwd(buf: *mut u8, size: usize) -> *mut u8 {
    let ret = syscall::syscall2(syscall::nr::GETCWD, buf as usize, size) as i32;
    if ret < 0 {
        return core::ptr::null_mut();
    }
    buf
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn access(path: *const u8, _mode: i32) -> i32 {
    // Check if file exists by trying to stat it
    let mut stat_buf = crate::stat::Stat::zeroed();
    let ret = syscall::syscall3(
        syscall::nr::STAT,
        path as usize,
        cstr_len(path),
        &mut stat_buf as *mut crate::stat::Stat as usize,
    ) as i32;
    if ret < 0 { -1 } else { 0 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn unlink(path: *const u8) -> i32 {
    syscall::syscall2(syscall::nr::UNLINK, path as usize, cstr_len(path)) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rmdir(path: *const u8) -> i32 {
    syscall::syscall2(syscall::nr::RMDIR, path as usize, cstr_len(path)) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mkdir(path: *const u8, mode: u32) -> i32 {
    syscall::syscall3(syscall::nr::MKDIR, path as usize, cstr_len(path), mode as usize) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rename(oldpath: *const u8, newpath: *const u8) -> i32 {
    syscall::syscall4(
        syscall::nr::RENAME,
        oldpath as usize,
        cstr_len(oldpath),
        newpath as usize,
        cstr_len(newpath),
    ) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn link(_oldpath: *const u8, _newpath: *const u8) -> i32 {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn symlink(_target: *const u8, _linkpath: *const u8) -> i32 {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn readlink(_path: *const u8, _buf: *mut u8, _bufsiz: usize) -> isize {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ftruncate(fd: i32, length: i64) -> i32 {
    syscall::syscall2(syscall::nr::FTRUNCATE, fd as usize, length as usize) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn truncate(_path: *const u8, _length: i64) -> i32 {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fsync(_fd: i32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fdatasync(_fd: i32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn isatty(fd: i32) -> i32 {
    // Try tcgetattr-style ioctl
    let mut termios = [0u8; 60];
    let ret = syscall::sys_ioctl(fd, 0x5401, termios.as_mut_ptr() as u64);
    if ret == 0 { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ttyname(_fd: i32) -> *mut u8 {
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ttyname_r(_fd: i32, _buf: *mut u8, _buflen: usize) -> i32 {
    errno::ENOSYS
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysconf(name: i32) -> i64 {
    match name {
        2 => 100,     // _SC_CLK_TCK
        30 => 4096,   // _SC_PAGESIZE
        84 => 1,      // _SC_NPROCESSORS_ONLN
        4 => 256,     // _SC_OPEN_MAX
        0 => 131072,  // _SC_ARG_MAX
        180 => 64,    // _SC_HOST_NAME_MAX
        70 => 1024,   // _SC_GETPW_R_SIZE_MAX
        69 => 1024,   // _SC_GETGR_R_SIZE_MAX
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pathconf(_path: *const u8, name: i32) -> i64 {
    match name {
        4 => 4096,  // _PC_PATH_MAX
        3 => 255,   // _PC_NAME_MAX
        5 => 4096,  // _PC_PIPE_BUF
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fpathconf(_fd: i32, name: i32) -> i64 {
    pathconf(core::ptr::null(), name)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn confstr(_name: i32, _buf: *mut u8, _len: usize) -> usize {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sleep(seconds: u32) -> u32 {
    crate::time::sleep(seconds);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn usleep(usec: u32) -> i32 {
    crate::time::usleep(usec);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn alarm(_seconds: u32) -> u32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pause() -> i32 {
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpagesize() -> i32 {
    4096
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gethostname(name: *mut u8, len: usize) -> i32 {
    let hostname = b"oxide\0";
    let copy_len = core::cmp::min(len, hostname.len());
    core::ptr::copy_nonoverlapping(hostname.as_ptr(), name, copy_len);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getlogin() -> *mut u8 {
    b"root\0".as_ptr() as *mut u8
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getlogin_r(buf: *mut u8, bufsize: usize) -> i32 {
    let name = b"root\0";
    if bufsize < name.len() {
        return errno::ERANGE;
    }
    core::ptr::copy_nonoverlapping(name.as_ptr(), buf, name.len());
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getgroups(_size: i32, _list: *mut u32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setgroups(_size: usize, _list: *const u32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nice(_inc: i32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn brk(_addr: *mut u8) -> i32 {
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sbrk(_increment: isize) -> *mut u8 {
    (-1isize) as *mut u8
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpgid(pid: i32) -> i32 {
    syscall::sys_getpgid(pid)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpgrp() -> i32 {
    getpgid(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setpgid(pid: i32, pgid: i32) -> i32 {
    syscall::sys_setpgid(pid, pgid)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setsid() -> i32 {
    syscall::sys_setsid()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getsid(_pid: i32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcgetpgrp(fd: i32) -> i32 {
    let mut pgrp: i32 = 0;
    let ret = syscall::sys_ioctl(fd, 0x540F, &mut pgrp as *mut i32 as u64);
    if ret < 0 { -1 } else { pgrp }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcsetpgrp(fd: i32, pgrp: i32) -> i32 {
    syscall::sys_ioctl(fd, 0x5410, &pgrp as *const i32 as u64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pread(fd: i32, buf: *mut u8, count: usize, offset: i64) -> isize {
    // Save position, seek, read, restore
    let old = lseek(fd, 0, 1); // SEEK_CUR
    lseek(fd, offset, 0); // SEEK_SET
    let n = read(fd, buf, count);
    lseek(fd, old, 0);
    n
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pwrite(fd: i32, buf: *const u8, count: usize, offset: i64) -> isize {
    let old = lseek(fd, 0, 1);
    lseek(fd, offset, 0);
    let n = write(fd, buf, count);
    lseek(fd, old, 0);
    n
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn writev(fd: i32, iov: *const u8, iovcnt: i32) -> isize {
    // iovec: { void *base; size_t len; } = 16 bytes each
    let mut total: isize = 0;
    for i in 0..iovcnt as usize {
        let entry = iov.add(i * 16);
        let base = *(entry as *const *const u8);
        let len = *(entry.add(8) as *const usize);
        if !base.is_null() && len > 0 {
            let n = write(fd, base, len);
            if n < 0 {
                return if total > 0 { total } else { n };
            }
            total += n;
        }
    }
    total
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn readv(fd: i32, iov: *const u8, iovcnt: i32) -> isize {
    let mut total: isize = 0;
    for i in 0..iovcnt as usize {
        let entry = iov.add(i * 16);
        let base = *(entry as *const *mut u8);
        let len = *(entry.add(8) as *const usize);
        if !base.is_null() && len > 0 {
            let n = read(fd, base, len);
            if n < 0 {
                return if total > 0 { total } else { n };
            }
            total += n;
            if (n as usize) < len {
                break;
            }
        }
    }
    total
}

// ============ mmap ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmap(
    addr: *mut u8,
    length: usize,
    prot: i32,
    flags: i32,
    fd: i32,
    offset: i64,
) -> *mut u8 {
    let ret = syscall::sys_mmap(addr, length, prot, flags, fd, offset);
    if ret == syscall::MAP_FAILED {
        (-1isize) as *mut u8 // MAP_FAILED
    } else {
        ret
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn munmap(addr: *mut u8, length: usize) -> i32 {
    syscall::sys_munmap(addr, length)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mprotect(addr: *mut u8, len: usize, prot: i32) -> i32 {
    syscall::sys_mprotect(addr, len, prot)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn madvise(_addr: *mut u8, _length: usize, _advice: i32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mlock(_addr: *const u8, _len: usize) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn munlock(_addr: *const u8, _len: usize) -> i32 {
    0
}

// ============ time ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn time(tloc: *mut i64) -> i64 {
    let mut tv = crate::time::Timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    crate::time::gettimeofday(&mut tv, None);
    if !tloc.is_null() {
        *tloc = tv.tv_sec;
    }
    tv.tv_sec
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gettimeofday(tv: *mut crate::time::Timeval, _tz: *mut u8) -> i32 {
    crate::time::gettimeofday(&mut *tv, None);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn clock_gettime(clk_id: i32, tp: *mut crate::time::Timespec) -> i32 {
    crate::time::clock_gettime(clk_id, &mut *tp)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn clock_getres(clk_id: i32, res: *mut crate::time::Timespec) -> i32 {
    crate::time::clock_getres(clk_id, &mut *res)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn clock() -> i64 {
    let mut ts = crate::time::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    clock_gettime(2, &mut ts); // CLOCK_PROCESS_CPUTIME_ID
    ts.tv_sec * 1_000_000 + ts.tv_nsec / 1000
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nanosleep(
    req: *const crate::time::Timespec,
    rem: *mut crate::time::Timespec,
) -> i32 {
    crate::time::nanosleep(&*req, if rem.is_null() { None } else { Some(&mut *rem) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn difftime(time1: i64, time0: i64) -> f64 {
    (time1 - time0) as f64
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mktime(tm: *mut crate::time::Tm) -> i64 {
    crate::time::mktime(&mut *tm)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gmtime(timep: *const i64) -> *mut crate::time::Tm {
    static mut TM_BUF: crate::time::Tm = crate::time::Tm {
        tm_sec: 0, tm_min: 0, tm_hour: 0, tm_mday: 0, tm_mon: 0,
        tm_year: 0, tm_wday: 0, tm_yday: 0, tm_isdst: 0,
        tm_gmtoff: 0, tm_zone: core::ptr::null(),
    };
    gmtime_r(timep, &raw mut TM_BUF);
    &raw mut TM_BUF
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gmtime_r(
    timep: *const i64,
    result: *mut crate::time::Tm,
) -> *mut crate::time::Tm {
    crate::time::gmtime_r(&*timep, &mut *result);
    result
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn localtime(timep: *const i64) -> *mut crate::time::Tm {
    gmtime(timep) // No timezone support
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn localtime_r(
    timep: *const i64,
    result: *mut crate::time::Tm,
) -> *mut crate::time::Tm {
    gmtime_r(timep, result)
}

static UTC_ZONE: [u8; 4] = *b"UTC\0";

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strftime(
    s: *mut u8,
    maxsize: usize,
    format: *const u8,
    tm: *const crate::time::Tm,
) -> usize {
    if s.is_null() || maxsize == 0 || format.is_null() || tm.is_null() {
        return 0;
    }

    let mut out = 0usize;
    let mut i = 0usize;

    while *format.add(i) != 0 && out < maxsize - 1 {
        if *format.add(i) == b'%' {
            i += 1;
            match *format.add(i) {
                b'Y' => out += write_num4(s.add(out), maxsize - out, (*tm).tm_year + 1900),
                b'm' => out += write_num2(s.add(out), maxsize - out, (*tm).tm_mon + 1),
                b'd' => out += write_num2(s.add(out), maxsize - out, (*tm).tm_mday),
                b'H' => out += write_num2(s.add(out), maxsize - out, (*tm).tm_hour),
                b'M' => out += write_num2(s.add(out), maxsize - out, (*tm).tm_min),
                b'S' => out += write_num2(s.add(out), maxsize - out, (*tm).tm_sec),
                b'Z' => {
                    let z = b"UTC";
                    for &b in z {
                        if out < maxsize - 1 {
                            *s.add(out) = b;
                            out += 1;
                        }
                    }
                }
                b'%' => {
                    *s.add(out) = b'%';
                    out += 1;
                }
                b'j' => {
                    out += write_num3(s.add(out), maxsize - out, (*tm).tm_yday + 1);
                }
                b'w' => {
                    *s.add(out) = b'0' + (*tm).tm_wday as u8;
                    out += 1;
                }
                b'n' => {
                    *s.add(out) = b'\n';
                    out += 1;
                }
                b't' => {
                    *s.add(out) = b'\t';
                    out += 1;
                }
                _ => {
                    *s.add(out) = b'%';
                    out += 1;
                    if out < maxsize - 1 {
                        *s.add(out) = *format.add(i);
                        out += 1;
                    }
                }
            }
            i += 1;
        } else {
            *s.add(out) = *format.add(i);
            out += 1;
            i += 1;
        }
    }

    *s.add(out) = 0;
    out
}

unsafe fn write_num2(buf: *mut u8, max: usize, val: i32) -> usize {
    if max < 2 {
        return 0;
    }
    *buf = b'0' + (val / 10 % 10) as u8;
    *buf.add(1) = b'0' + (val % 10) as u8;
    2
}

unsafe fn write_num3(buf: *mut u8, max: usize, val: i32) -> usize {
    if max < 3 {
        return 0;
    }
    *buf = b'0' + (val / 100 % 10) as u8;
    *buf.add(1) = b'0' + (val / 10 % 10) as u8;
    *buf.add(2) = b'0' + (val % 10) as u8;
    3
}

unsafe fn write_num4(buf: *mut u8, max: usize, val: i32) -> usize {
    if max < 4 {
        return 0;
    }
    *buf = b'0' + (val / 1000 % 10) as u8;
    *buf.add(1) = b'0' + (val / 100 % 10) as u8;
    *buf.add(2) = b'0' + (val / 10 % 10) as u8;
    *buf.add(3) = b'0' + (val % 10) as u8;
    4
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strptime(
    _s: *const u8,
    _format: *const u8,
    _tm: *mut crate::time::Tm,
) -> *mut u8 {
    core::ptr::null_mut() // stub
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn asctime(_tm: *const crate::time::Tm) -> *mut u8 {
    b"Thu Jan  1 00:00:00 1970\n\0".as_ptr() as *mut u8
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn asctime_r(
    _tm: *const crate::time::Tm,
    buf: *mut u8,
) -> *mut u8 {
    let s = b"Thu Jan  1 00:00:00 1970\n\0";
    core::ptr::copy_nonoverlapping(s.as_ptr(), buf, s.len());
    buf
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ctime(timep: *const i64) -> *mut u8 {
    asctime(gmtime(timep))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ctime_r(timep: *const i64, buf: *mut u8) -> *mut u8 {
    static mut TM: crate::time::Tm = crate::time::Tm {
        tm_sec: 0, tm_min: 0, tm_hour: 0, tm_mday: 0, tm_mon: 0,
        tm_year: 0, tm_wday: 0, tm_yday: 0, tm_isdst: 0,
        tm_gmtoff: 0, tm_zone: core::ptr::null(),
    };
    gmtime_r(timep, &raw mut TM);
    asctime_r(&raw const TM, buf)
}

#[unsafe(no_mangle)]
pub static mut timezone: i64 = 0;

#[unsafe(no_mangle)]
pub static mut daylight: i32 = 0;

static mut TZNAME_UTC: [u8; 4] = *b"UTC\0";
static mut TZNAME_UTC2: [u8; 4] = *b"UTC\0";

#[unsafe(no_mangle)]
pub static mut tzname: [*mut u8; 2] = unsafe {
    [(&raw const TZNAME_UTC) as *mut u8, (&raw const TZNAME_UTC2) as *mut u8]
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tzset() {
    // No-op, always UTC
}

// ============ stat ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn stat(path: *const u8, buf: *mut crate::stat::Stat) -> i32 {
    let ret = syscall::syscall3(
        syscall::nr::STAT,
        path as usize,
        cstr_len(path),
        buf as usize,
    ) as i32;
    if ret < 0 {
        ERRNO_VAR = -ret;
        -1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fstat(fd: i32, buf: *mut crate::stat::Stat) -> i32 {
    let ret = syscall::syscall2(
        syscall::nr::FSTAT,
        fd as usize,
        buf as usize,
    ) as i32;
    if ret < 0 {
        ERRNO_VAR = -ret;
        -1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lstat(path: *const u8, buf: *mut crate::stat::Stat) -> i32 {
    stat(path, buf) // No symlink support yet
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fstatat(
    _dirfd: i32,
    path: *const u8,
    buf: *mut crate::stat::Stat,
    _flags: i32,
) -> i32 {
    stat(path, buf)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chmod(_path: *const u8, _mode: u32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fchmod(_fd: i32, _mode: u32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn umask(_mask: u32) -> u32 {
    0o022
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chown(_path: *const u8, _owner: u32, _group: u32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fchown(_fd: i32, _owner: u32, _group: u32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn utimensat(
    _dirfd: i32,
    _path: *const u8,
    _times: *const u8,
    _flags: i32,
) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn futimens(_fd: i32, _times: *const u8) -> i32 {
    0
}

// ============ fcntl ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fcntl(fd: i32, cmd: i32, arg: i64) -> i32 {
    match cmd {
        1 => 0,    // F_GETFD -> no flags
        2 => 0,    // F_SETFD -> ok
        3 => 0,    // F_GETFL -> no flags
        4 => 0,    // F_SETFL -> ok
        0 => dup(fd), // F_DUPFD
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn flock(_fd: i32, _operation: i32) -> i32 {
    0
}

// ============ ioctl ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ioctl(fd: i32, request: u64, arg: u64) -> i32 {
    syscall::sys_ioctl(fd, request, arg)
}

// ============ poll/select ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn poll(fds: *mut u8, nfds: u64, timeout: i32) -> i32 {
    // Stub: indicate all ready
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn select(
    _nfds: i32,
    _readfds: *mut u8,
    _writefds: *mut u8,
    _exceptfds: *mut u8,
    _timeout: *mut u8,
) -> i32 {
    0
}

// ============ dirent ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendir(name: *const u8) -> *mut u8 {
    let fd = syscall::syscall4(
        syscall::nr::OPEN,
        name as usize,
        cstr_len(name),
        (O_RDONLY | O_DIRECTORY) as usize,
        0,
    ) as i32;
    if fd < 0 {
        return core::ptr::null_mut();
    }
    // Allocate DIR struct: fd (4) + buffer offset
    let dir = malloc(4096 + 16);
    if dir.is_null() {
        syscall::sys_close(fd);
        return core::ptr::null_mut();
    }
    *(dir as *mut i32) = fd;
    *(dir.add(4) as *mut i32) = 0; // buf_pos
    *(dir.add(8) as *mut i32) = 0; // buf_len
    dir
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn closedir(dirp: *mut u8) -> i32 {
    if dirp.is_null() {
        return -1;
    }
    let fd = *(dirp as *const i32);
    close(fd);
    free(dirp);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn readdir(dirp: *mut u8) -> *mut u8 {
    if dirp.is_null() {
        return core::ptr::null_mut();
    }
    let fd = *(dirp as *const i32);
    let buf_pos = dirp.add(4) as *mut i32;
    let buf_len = dirp.add(8) as *mut i32;
    let buf = dirp.add(16);

    if *buf_pos >= *buf_len {
        let n = syscall::syscall3(syscall::nr::GETDENTS, fd as usize, buf as usize, 4096) as i32;
        if n <= 0 {
            return core::ptr::null_mut();
        }
        *buf_len = n as i32;
        *buf_pos = 0;
    }

    let entry = buf.add(*buf_pos as usize);
    let reclen = *(entry.add(16) as *const u16) as i32;
    *buf_pos += reclen;
    entry
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn dirfd(dirp: *mut u8) -> i32 {
    if dirp.is_null() {
        return -1;
    }
    *(dirp as *const i32)
}

// ============ pwd/grp ============

static mut PW_BUF: [u8; 256] = [0; 256];

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpwuid(_uid: u32) -> *mut u8 {
    // Return a simple passwd struct for root
    crate::pwd::getpwuid(_uid) as *mut u8
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpwnam(name: *const u8) -> *mut u8 {
    crate::pwd::getpwnam(name) as *mut u8
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpwuid_r(
    uid: u32,
    _pwd: *mut u8,
    _buf: *mut u8,
    _buflen: usize,
    result: *mut *mut u8,
) -> i32 {
    let pw = getpwuid(uid);
    *result = pw;
    if pw.is_null() { errno::ENOENT } else { 0 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpwnam_r(
    name: *const u8,
    _pwd: *mut u8,
    _buf: *mut u8,
    _buflen: usize,
    result: *mut *mut u8,
) -> i32 {
    let pw = getpwnam(name);
    *result = pw;
    if pw.is_null() { errno::ENOENT } else { 0 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getgrgid(_gid: u32) -> *mut u8 {
    crate::pwd::getgrgid(_gid) as *mut u8
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getgrnam(name: *const u8) -> *mut u8 {
    crate::pwd::getgrnam(name) as *mut u8
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getgrgid_r(
    gid: u32,
    _grp: *mut u8,
    _buf: *mut u8,
    _buflen: usize,
    result: *mut *mut u8,
) -> i32 {
    let gr = getgrgid(gid);
    *result = gr;
    if gr.is_null() { errno::ENOENT } else { 0 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getgrnam_r(
    name: *const u8,
    _grp: *mut u8,
    _buf: *mut u8,
    _buflen: usize,
    result: *mut *mut u8,
) -> i32 {
    let gr = getgrnam(name);
    *result = gr;
    if gr.is_null() { errno::ENOENT } else { 0 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getgrouplist(
    _user: *const u8,
    _group: u32,
    _groups: *mut u32,
    ngroups: *mut i32,
) -> i32 {
    *ngroups = 0;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn initgroups(_user: *const u8, _group: u32) -> i32 {
    0
}

// ============ locale ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setlocale(category: i32, locale: *const u8) -> *const u8 {
    crate::locale::setlocale(category, locale)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn localeconv() -> *mut crate::locale::Lconv {
    crate::locale::localeconv()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn newlocale(
    _category_mask: i32,
    _locale: *const u8,
    _base: *mut u8,
) -> *mut u8 {
    // Return a non-null "locale" pointer
    static mut FAKE_LOCALE: i32 = 0;
    &raw mut FAKE_LOCALE as *mut u8
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uselocale(_newloc: *mut u8) -> *mut u8 {
    static mut FAKE_LOCALE: i32 = 0;
    &raw mut FAKE_LOCALE as *mut u8
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn freelocale(_locobj: *mut u8) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn duplocale(_locobj: *mut u8) -> *mut u8 {
    _locobj
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nl_langinfo(item: i32) -> *const u8 {
    crate::locale::nl_langinfo(item)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nl_langinfo_l(item: i32, _locale: *mut u8) -> *const u8 {
    // Ignore locale parameter, use default implementation
    crate::locale::nl_langinfo(item)
}

// ============ uname ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uname(buf: *mut u8) -> i32 {
    // Fill utsname struct (5 * 65 bytes)
    let copy_field = |offset: usize, val: &[u8]| {
        let dest = buf.add(offset);
        let len = core::cmp::min(val.len(), 64);
        core::ptr::copy_nonoverlapping(val.as_ptr(), dest, len);
        *dest.add(len) = 0;
    };

    copy_field(0, b"OXIDE");
    copy_field(65, b"oxide");
    copy_field(130, b"0.1.0");
    copy_field(195, b"#1");
    copy_field(260, b"x86_64");
    copy_field(325, b"");
    0
}

// ============ resource ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getrlimit(_resource: i32, rlim: *mut u8) -> i32 {
    // Set both cur and max to RLIM_INFINITY
    let ptr = rlim as *mut u64;
    *ptr = !0u64;
    *ptr.add(1) = !0u64;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setrlimit(_resource: i32, _rlim: *const u8) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getrusage(_who: i32, usage: *mut u8) -> i32 {
    core::ptr::write_bytes(usage, 0, 144);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpriority(_which: i32, _who: i32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setpriority(_which: i32, _who: i32, _prio: i32) -> i32 {
    0
}

// ============ socket stubs ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn socket(_domain: i32, _type: i32, _protocol: i32) -> i32 {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn socketpair(_d: i32, _t: i32, _p: i32, _sv: *mut i32) -> i32 {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bind(_fd: i32, _addr: *const u8, _len: u32) -> i32 {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn listen(_fd: i32, _backlog: i32) -> i32 {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn accept(_fd: i32, _addr: *mut u8, _len: *mut u32) -> i32 {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn connect(_fd: i32, _addr: *const u8, _len: u32) -> i32 {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn send(_fd: i32, _buf: *const u8, _len: usize, _flags: i32) -> isize {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn recv(_fd: i32, _buf: *mut u8, _len: usize, _flags: i32) -> isize {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sendto(
    _fd: i32, _buf: *const u8, _len: usize, _flags: i32,
    _addr: *const u8, _addrlen: u32,
) -> isize {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn recvfrom(
    _fd: i32, _buf: *mut u8, _len: usize, _flags: i32,
    _addr: *mut u8, _addrlen: *mut u32,
) -> isize {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn shutdown(_fd: i32, _how: i32) -> i32 {
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getsockname(
    _fd: i32, _addr: *mut u8, _len: *mut u32,
) -> i32 {
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpeername(
    _fd: i32, _addr: *mut u8, _len: *mut u32,
) -> i32 {
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setsockopt(
    _fd: i32, _level: i32, _optname: i32,
    _optval: *const u8, _optlen: u32,
) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getsockopt(
    _fd: i32, _level: i32, _optname: i32,
    _optval: *mut u8, _optlen: *mut u32,
) -> i32 {
    -1
}

// ============ netdb ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getaddrinfo(
    _node: *const u8,
    _service: *const u8,
    _hints: *const u8,
    _res: *mut *mut u8,
) -> i32 {
    -2 // EAI_NONAME
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn freeaddrinfo(_res: *mut u8) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gai_strerror(_errcode: i32) -> *const u8 {
    b"Name resolution not supported\0".as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getnameinfo(
    _sa: *const u8, _salen: u32,
    _host: *mut u8, _hostlen: u32,
    _serv: *mut u8, _servlen: u32,
    _flags: i32,
) -> i32 {
    -2
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gethostbyname(_name: *const u8) -> *mut u8 {
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gethostbyaddr(
    _addr: *const u8, _len: u32, _type_: i32,
) -> *mut u8 {
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn htons(x: u16) -> u16 {
    x.to_be()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ntohs(x: u16) -> u16 {
    u16::from_be(x)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn htonl(x: u32) -> u32 {
    x.to_be()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ntohl(x: u32) -> u32 {
    u32::from_be(x)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inet_addr(_cp: *const u8) -> u32 {
    0xFFFFFFFF
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inet_ntoa(_in_addr: u32) -> *const u8 {
    b"0.0.0.0\0".as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inet_pton(
    _af: i32, _src: *const u8, _dst: *mut u8,
) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inet_ntop(
    _af: i32, _src: *const u8, dst: *mut u8, _size: u32,
) -> *const u8 {
    let s = b"0.0.0.0\0";
    core::ptr::copy_nonoverlapping(s.as_ptr(), dst, s.len());
    dst
}

// ============ pthread stubs ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_create(
    _thread: *mut u64,
    _attr: *const u8,
    _start: *const u8,
    _arg: *mut u8,
) -> i32 {
    errno::ENOSYS
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_join(_thread: u64, _retval: *mut *mut u8) -> i32 {
    errno::ENOSYS
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_detach(_thread: u64) -> i32 {
    0
}

static mut FAKE_THREAD_ID: u64 = 1;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_self() -> u64 {
    FAKE_THREAD_ID
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_equal(t1: u64, t2: u64) -> i32 {
    (t1 == t2) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_exit(_retval: *mut u8) -> ! {
    _exit(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutex_init(_mutex: *mut u8, _attr: *const u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutex_destroy(_mutex: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutex_lock(_mutex: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutex_trylock(_mutex: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutex_unlock(_mutex: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutex_timedlock(_m: *mut u8, _t: *const u8) -> i32 { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutexattr_init(_attr: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutexattr_destroy(_attr: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutexattr_settype(_attr: *mut u8, _type: i32) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutexattr_gettype(_a: *const u8, t: *mut i32) -> i32 {
    *t = 0;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_cond_init(_cond: *mut u8, _attr: *const u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_cond_destroy(_cond: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_cond_signal(_cond: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_cond_broadcast(_cond: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_cond_wait(_c: *mut u8, _m: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_cond_timedwait(_c: *mut u8, _m: *mut u8, _t: *const u8) -> i32 {
    errno::ETIMEDOUT
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_condattr_init(_attr: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_condattr_destroy(_attr: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_condattr_setclock(_a: *mut u8, _c: i32) -> i32 { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_rwlock_init(_rw: *mut u8, _attr: *const u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_rwlock_destroy(_rw: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_rwlock_rdlock(_rw: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_rwlock_wrlock(_rw: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_rwlock_tryrdlock(_rw: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_rwlock_trywrlock(_rw: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_rwlock_unlock(_rw: *mut u8) -> i32 { 0 }

// Thread-local storage
const MAX_KEYS: usize = 128;
static mut TLS_DATA: [*mut u8; MAX_KEYS] = [core::ptr::null_mut(); MAX_KEYS];
static mut TLS_DTORS: [Option<unsafe extern "C" fn(*mut u8)>; MAX_KEYS] = [None; MAX_KEYS];
static mut TLS_NEXT_KEY: i32 = 0;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_key_create(
    key: *mut i32,
    destructor: Option<unsafe extern "C" fn(*mut u8)>,
) -> i32 {
    if TLS_NEXT_KEY >= MAX_KEYS as i32 {
        return errno::EAGAIN;
    }
    let k = TLS_NEXT_KEY;
    TLS_NEXT_KEY += 1;
    TLS_DTORS[k as usize] = destructor;
    *key = k;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_key_delete(_key: i32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_getspecific(key: i32) -> *mut u8 {
    if key < 0 || key >= MAX_KEYS as i32 {
        return core::ptr::null_mut();
    }
    TLS_DATA[key as usize]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_setspecific(key: i32, value: *const u8) -> i32 {
    if key < 0 || key >= MAX_KEYS as i32 {
        return errno::EINVAL;
    }
    TLS_DATA[key as usize] = value as *mut u8;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_once(
    once_control: *mut i32,
    init_routine: unsafe extern "C" fn(),
) -> i32 {
    if *once_control == 0 {
        *once_control = 1;
        init_routine();
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_attr_init(_attr: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_attr_destroy(_attr: *mut u8) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_attr_setdetachstate(_a: *mut u8, _s: i32) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_attr_getdetachstate(_a: *const u8, s: *mut i32) -> i32 {
    *s = 0;
    0
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_attr_setstacksize(_a: *mut u8, _s: usize) -> i32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_attr_getstacksize(_a: *const u8, s: *mut usize) -> i32 {
    *s = 8 * 1024 * 1024;
    0
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_attr_setscope(_a: *mut u8, _s: i32) -> i32 { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_atfork(
    _prepare: Option<unsafe extern "C" fn()>,
    _parent: Option<unsafe extern "C" fn()>,
    _child: Option<unsafe extern "C" fn()>,
) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_cancel(_thread: u64) -> i32 {
    errno::ENOSYS
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_setcancelstate(_state: i32, _oldstate: *mut i32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_setcanceltype(_type: i32, _oldtype: *mut i32) -> i32 {
    0
}

// ============ dlfcn stubs ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlopen(_filename: *const u8, _flags: i32) -> *mut u8 {
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlsym(_handle: *mut u8, _symbol: *const u8) -> *mut u8 {
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlclose(_handle: *mut u8) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlerror() -> *const u8 {
    b"Dynamic loading not supported\0".as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn dladdr(_addr: *const u8, _info: *mut u8) -> i32 {
    0
}

// ============ statvfs ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn statvfs(_path: *const u8, buf: *mut u8) -> i32 {
    core::ptr::write_bytes(buf, 0, 88);
    // Set reasonable defaults
    let p = buf as *mut u64;
    *p = 4096;       // f_bsize
    *p.add(1) = 4096; // f_frsize
    *p.add(2) = 1024 * 1024; // f_blocks
    *p.add(3) = 512 * 1024;  // f_bfree
    *p.add(4) = 512 * 1024;  // f_bavail
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fstatvfs(_fd: i32, buf: *mut u8) -> i32 {
    statvfs(core::ptr::null(), buf)
}

// ============ termios ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcgetattr(fd: i32, termios_p: *mut u8) -> i32 {
    ioctl(fd, 0x5401, termios_p as u64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcsetattr(fd: i32, _action: i32, termios_p: *const u8) -> i32 {
    ioctl(fd, 0x5402, termios_p as u64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cfgetospeed(_termios_p: *const u8) -> u32 {
    38400 // B38400
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cfgetispeed(_termios_p: *const u8) -> u32 {
    38400
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cfsetospeed(_termios_p: *mut u8, _speed: u32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cfsetispeed(_termios_p: *mut u8, _speed: u32) -> i32 {
    0
}

// ============ misc ============

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getrandom(buf: *mut u8, buflen: usize, _flags: u32) -> isize {
    syscall::getrandom(buf, buflen, 0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getentropy(buf: *mut u8, buflen: usize) -> i32 {
    if buflen > 256 {
        return -1;
    }
    let ret = getrandom(buf, buflen, 0);
    if ret < 0 { -1 } else { 0 }
}

// Environ
#[unsafe(no_mangle)]
pub static mut environ: *mut *mut u8 = core::ptr::null_mut();

static mut EMPTY_ENVIRON: [*mut u8; 1] = [core::ptr::null_mut()];

pub unsafe fn init_environ() {
    environ = (&raw mut EMPTY_ENVIRON) as *mut *mut u8;
}

// These are sometimes needed
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getenv(name: *const u8) -> *mut u8 {
    if name.is_null() {
        return core::ptr::null_mut();
    }
    let len = cstr_len(name);
    let name_str = core::str::from_utf8_unchecked(core::slice::from_raw_parts(name, len));
    match crate::env::getenv(name_str) {
        Some(val) => val.as_ptr() as *mut u8,
        None => core::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setenv(name: *const u8, value: *const u8, _overwrite: i32) -> i32 {
    if name.is_null() || value.is_null() {
        return -1;
    }
    let name_len = cstr_len(name);
    let value_len = cstr_len(value);
    let name_str = core::str::from_utf8_unchecked(core::slice::from_raw_parts(name, name_len));
    let value_str = core::str::from_utf8_unchecked(core::slice::from_raw_parts(value, value_len));
    crate::env::setenv(name_str, value_str)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn unsetenv(name: *const u8) -> i32 {
    if name.is_null() {
        return -1;
    }
    let len = cstr_len(name);
    let name_str = core::str::from_utf8_unchecked(core::slice::from_raw_parts(name, len));
    crate::env::unsetenv(name_str)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn putenv(_string: *mut u8) -> i32 {
    0
}

// strtol/strtoul/atoi C exports
#[unsafe(no_mangle)]
pub unsafe extern "C" fn atoi(nptr: *const u8) -> i32 {
    strtol(nptr, core::ptr::null_mut(), 10) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn atol(nptr: *const u8) -> i64 {
    strtol(nptr, core::ptr::null_mut(), 10)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn atoll(nptr: *const u8) -> i64 {
    strtol(nptr, core::ptr::null_mut(), 10)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtol(nptr: *const u8, endptr: *mut *mut u8, base: i32) -> i64 {
    if nptr.is_null() {
        return 0;
    }
    let mut i = 0;
    while *nptr.add(i) == b' ' || *nptr.add(i) == b'\t' || *nptr.add(i) == b'\n' {
        i += 1;
    }
    let neg = *nptr.add(i) == b'-';
    if neg || *nptr.add(i) == b'+' {
        i += 1;
    }

    let mut base = base;
    if base == 0 {
        if *nptr.add(i) == b'0' {
            if *nptr.add(i + 1) == b'x' || *nptr.add(i + 1) == b'X' {
                base = 16;
                i += 2;
            } else {
                base = 8;
                i += 1;
            }
        } else {
            base = 10;
        }
    } else if base == 16 && *nptr.add(i) == b'0'
        && (*nptr.add(i + 1) == b'x' || *nptr.add(i + 1) == b'X')
    {
        i += 2;
    }

    let mut result: i64 = 0;
    loop {
        let c = *nptr.add(i);
        let digit = if c >= b'0' && c <= b'9' {
            (c - b'0') as i64
        } else if c >= b'a' && c <= b'z' {
            (c - b'a' + 10) as i64
        } else if c >= b'A' && c <= b'Z' {
            (c - b'A' + 10) as i64
        } else {
            break;
        };
        if digit >= base as i64 {
            break;
        }
        result = result.wrapping_mul(base as i64).wrapping_add(digit);
        i += 1;
    }

    if !endptr.is_null() {
        *endptr = nptr.add(i) as *mut u8;
    }

    if neg { -result } else { result }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtoul(nptr: *const u8, endptr: *mut *mut u8, base: i32) -> u64 {
    strtol(nptr, endptr, base) as u64
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtoll(nptr: *const u8, endptr: *mut *mut u8, base: i32) -> i64 {
    strtol(nptr, endptr, base)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtoull(nptr: *const u8, endptr: *mut *mut u8, base: i32) -> u64 {
    strtol(nptr, endptr, base) as u64
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtoimax(nptr: *const u8, endptr: *mut *mut u8, base: i32) -> i64 {
    strtol(nptr, endptr, base)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtoumax(nptr: *const u8, endptr: *mut *mut u8, base: i32) -> u64 {
    strtol(nptr, endptr, base) as u64
}

// abs/labs/llabs
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abs(j: i32) -> i32 {
    if j < 0 { -j } else { j }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn labs(j: i64) -> i64 {
    if j < 0 { -j } else { j }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn llabs(j: i64) -> i64 {
    if j < 0 { -j } else { j }
}

// rand
static mut RAND_SEED: u64 = 12345;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rand() -> i32 {
    RAND_SEED = RAND_SEED.wrapping_mul(6364136223846793005).wrapping_add(1);
    ((RAND_SEED >> 33) as i32) & 0x7FFFFFFF
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn srand(seed: u32) {
    RAND_SEED = seed as u64;
}

// snprintf family - delegate to the printf module
#[unsafe(no_mangle)]
pub unsafe extern "C" fn printf(fmt: *const u8, args: ...) -> i32 {
    vfprintf(crate::filestream::stdout, fmt, args)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fprintf(
    stream: *mut crate::filestream::FILE,
    fmt: *const u8,
    args: ...
) -> i32 {
    vfprintf(stream, fmt, args)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sprintf(s: *mut u8, fmt: *const u8, args: ...) -> i32 {
    vsprintf(s, fmt, args)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn snprintf(s: *mut u8, n: usize, fmt: *const u8, args: ...) -> i32 {
    vsnprintf(s, n, fmt, args)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vprintf(fmt: *const u8, ap: core::ffi::VaList) -> i32 {
    vfprintf(crate::filestream::stdout, fmt, ap)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vfprintf(
    stream: *mut crate::filestream::FILE,
    fmt: *const u8,
    ap: core::ffi::VaList,
) -> i32 {
    let mut buf = [0u8; 4096];
    let n = vsnprintf(buf.as_mut_ptr(), buf.len(), fmt, ap);
    if n > 0 {
        crate::filestream::fwrite(buf.as_ptr(), 1, n as usize, stream);
    }
    n
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vsprintf(s: *mut u8, fmt: *const u8, ap: core::ffi::VaList) -> i32 {
    vsnprintf(s, usize::MAX, fmt, ap)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vsnprintf(
    s: *mut u8,
    n: usize,
    fmt: *const u8,
    mut ap: core::ffi::VaList,
) -> i32 {
    crate::printf::vsnprintf_impl(s, n, fmt, &mut ap)
}

// sscanf stub
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sscanf(_str: *const u8, _fmt: *const u8, _args: ...) -> i32 {
    0
}

// perror
#[unsafe(no_mangle)]
pub unsafe extern "C" fn perror(s: *const u8) {
    if !s.is_null() && *s != 0 {
        let mut len = 0;
        while *s.add(len) != 0 {
            len += 1;
        }
        syscall::sys_write(2, core::slice::from_raw_parts(s, len));
        syscall::sys_write(2, b": ");
    }
    let err = ERRNO_VAR;
    let msg = crate::string::strerror_rust(err);
    syscall::sys_write(2, msg.as_bytes());
    syscall::sys_write(2, b"\n");
}

// strerror C export
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strerror(errnum: i32) -> *const u8 {
    crate::string::strerror_rust(errnum).as_ptr()
}

// strdup
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strdup(s: *const u8) -> *mut u8 {
    if s.is_null() {
        return core::ptr::null_mut();
    }
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    let p = malloc(len + 1) as *mut u8;
    if !p.is_null() {
        core::ptr::copy_nonoverlapping(s, p, len + 1);
    }
    p
}

// fchdir
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fchdir(_fd: i32) -> i32 {
    ERRNO_VAR = errno::ENOSYS;
    -1
}

// posix_spawn stubs
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawn(
    _pid: *mut i32, _path: *const u8,
    _file_actions: *const u8, _attrp: *const u8,
    _argv: *const *mut u8, _envp: *const *mut u8,
) -> i32 {
    errno::ENOSYS
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawnp(
    _pid: *mut i32, _file: *const u8,
    _file_actions: *const u8, _attrp: *const u8,
    _argv: *const *mut u8, _envp: *const *mut u8,
) -> i32 {
    errno::ENOSYS
}

// Misc stubs CPython needs
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getservbyname(_name: *const u8, _proto: *const u8) -> *mut u8 {
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getservbyport(_port: i32, _proto: *const u8) -> *mut u8 {
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getprotobyname(_name: *const u8) -> *mut u8 {
    core::ptr::null_mut()
}

// wctype/wchar C exports
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswalpha(wc: u32) -> i32 {
    crate::wchar::iswalpha(wc) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswdigit(wc: u32) -> i32 {
    crate::wchar::iswdigit(wc) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswalnum(wc: u32) -> i32 {
    crate::wchar::iswalnum(wc) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswspace(wc: u32) -> i32 {
    crate::wchar::iswspace(wc) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswupper(wc: u32) -> i32 {
    crate::wchar::iswupper(wc) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswlower(wc: u32) -> i32 {
    crate::wchar::iswlower(wc) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswprint(wc: u32) -> i32 {
    crate::wchar::iswprint(wc) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswpunct(wc: u32) -> i32 {
    crate::wchar::iswpunct(wc) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswcntrl(wc: u32) -> i32 {
    crate::wchar::iswcntrl(wc) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswxdigit(wc: u32) -> i32 {
    crate::wchar::iswxdigit(wc) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswgraph(wc: u32) -> i32 {
    crate::wchar::iswgraph(wc) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswblank(wc: u32) -> i32 {
    crate::wchar::iswblank(wc) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn towupper(wc: u32) -> u32 {
    crate::wchar::towupper(wc)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn towlower(wc: u32) -> u32 {
    crate::wchar::towlower(wc)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wctype(_name: *const u8) -> u64 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswctype(_wc: u32, _desc: u64) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wctrans(_name: *const u8) -> *const i32 {
    core::ptr::null()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn towctrans(wc: u32, _desc: *const i32) -> u32 {
    wc
}

// _l locale variants
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswalpha_l(wc: u32, _l: *mut u8) -> i32 { iswalpha(wc) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswdigit_l(wc: u32, _l: *mut u8) -> i32 { iswdigit(wc) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswalnum_l(wc: u32, _l: *mut u8) -> i32 { iswalnum(wc) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswspace_l(wc: u32, _l: *mut u8) -> i32 { iswspace(wc) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswupper_l(wc: u32, _l: *mut u8) -> i32 { iswupper(wc) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswlower_l(wc: u32, _l: *mut u8) -> i32 { iswlower(wc) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswprint_l(wc: u32, _l: *mut u8) -> i32 { iswprint(wc) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswcntrl_l(wc: u32, _l: *mut u8) -> i32 { iswcntrl(wc) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswxdigit_l(wc: u32, _l: *mut u8) -> i32 { iswxdigit(wc) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn towupper_l(wc: u32, _l: *mut u8) -> u32 { towupper(wc) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn towlower_l(wc: u32, _l: *mut u8) -> u32 { towlower(wc) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wctype_l(_n: *const u8, _l: *mut u8) -> u64 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswctype_l(wc: u32, desc: u64, _l: *mut u8) -> i32 { iswctype(wc, desc) }

// wchar string C exports
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcslen(s: *const i32) -> usize {
    crate::wchar::wcslen(s)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcscpy(dest: *mut i32, src: *const i32) -> *mut i32 {
    crate::wchar::wcscpy(dest, src)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsncpy(dest: *mut i32, src: *const i32, n: usize) -> *mut i32 {
    crate::wchar::wcsncpy(dest, src, n)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcscmp(s1: *const i32, s2: *const i32) -> i32 {
    crate::wchar::wcscmp(s1, s2)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsncmp(s1: *const i32, s2: *const i32, n: usize) -> i32 {
    crate::wchar::wcsncmp(s1, s2, n)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcschr(s: *const i32, c: i32) -> *const i32 {
    crate::wchar::wcschr(s, c)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsrchr(s: *const i32, c: i32) -> *const i32 {
    crate::wchar::wcsrchr(s, c)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbtowc(pwc: *mut i32, s: *const u8, n: usize) -> i32 {
    crate::wchar::mbtowc(pwc, s, n)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wctomb(s: *mut u8, wc: i32) -> i32 {
    crate::wchar::wctomb(s, wc)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbstowcs(dest: *mut i32, src: *const u8, n: usize) -> usize {
    if src.is_null() {
        return 0;
    }
    let mut i = 0usize;
    let mut j = 0usize;
    while i < n {
        let c = *src.add(j);
        if c == 0 {
            if !dest.is_null() {
                *dest.add(i) = 0;
            }
            return i;
        }
        let mut wc: i32 = 0;
        let len = mbtowc(&mut wc, src.add(j), 4);
        if len < 0 {
            return usize::MAX;
        }
        if !dest.is_null() {
            *dest.add(i) = wc;
        }
        j += len as usize;
        i += 1;
    }
    i
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstombs(dest: *mut u8, src: *const i32, n: usize) -> usize {
    if src.is_null() {
        return 0;
    }
    let mut i = 0usize;
    let mut j = 0usize;
    while j < n {
        let wc = *src.add(i);
        if wc == 0 {
            if !dest.is_null() {
                *dest.add(j) = 0;
            }
            return j;
        }
        let mut buf = [0u8; 4];
        let len = wctomb(buf.as_mut_ptr(), wc);
        if len < 0 {
            return usize::MAX;
        }
        if j + len as usize > n {
            break;
        }
        if !dest.is_null() {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), dest.add(j), len as usize);
        }
        j += len as usize;
        i += 1;
    }
    j
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbrtowc(
    pwc: *mut i32,
    s: *const u8,
    n: usize,
    _ps: *mut u8,
) -> usize {
    if s.is_null() {
        return 0;
    }
    let ret = mbtowc(pwc, s, n);
    if ret < 0 {
        usize::MAX
    } else {
        ret as usize
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcrtomb(s: *mut u8, wc: i32, _ps: *mut u8) -> usize {
    if s.is_null() {
        return 1;
    }
    let ret = wctomb(s, wc);
    if ret < 0 {
        usize::MAX
    } else {
        ret as usize
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbrlen(s: *const u8, n: usize, _ps: *mut u8) -> usize {
    mbrtowc(core::ptr::null_mut(), s, n, core::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbsinit(_ps: *const u8) -> i32 {
    1 // Always in initial state
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn btowc(c: i32) -> i32 {
    if c < 0 || c > 127 { -1 } else { c }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wctob(c: i32) -> i32 {
    if c < 0 || c > 127 { -1 } else { c }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wmemcpy(dest: *mut i32, src: *const i32, n: usize) -> *mut i32 {
    core::ptr::copy_nonoverlapping(src, dest, n);
    dest
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wmemmove(dest: *mut i32, src: *const i32, n: usize) -> *mut i32 {
    core::ptr::copy(src, dest, n);
    dest
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wmemset(s: *mut i32, c: i32, n: usize) -> *mut i32 {
    for i in 0..n {
        *s.add(i) = c;
    }
    s
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wmemcmp(s1: *const i32, s2: *const i32, n: usize) -> i32 {
    for i in 0..n {
        let diff = *s1.add(i) - *s2.add(i);
        if diff != 0 {
            return diff;
        }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wmemchr(s: *const i32, c: i32, n: usize) -> *mut i32 {
    for i in 0..n {
        if *s.add(i) == c {
            return s.add(i) as *mut i32;
        }
    }
    core::ptr::null_mut()
}

// wcstol/wcstod
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstol(_nptr: *const i32, _endptr: *mut *mut i32, _base: i32) -> i64 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstoul(_nptr: *const i32, _endptr: *mut *mut i32, _base: i32) -> u64 {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstod(_nptr: *const i32, _endptr: *mut *mut i32) -> f64 {
    0.0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn swprintf(
    _s: *mut i32, _n: usize, _fmt: *const i32, _args: ...
) -> i32 {
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsdup(_s: *const i32) -> *mut i32 {
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcscoll(s1: *const i32, s2: *const i32) -> i32 {
    wcscmp(s1, s2)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsxfrm(dest: *mut i32, src: *const i32, n: usize) -> usize {
    let len = wcslen(src);
    if n > 0 {
        let copy = core::cmp::min(len, n - 1);
        core::ptr::copy_nonoverlapping(src, dest, copy);
        *dest.add(copy) = 0;
    }
    len
}

// syscall() - generic syscall wrapper
#[unsafe(no_mangle)]
pub unsafe extern "C" fn syscall(number: i64, args: ...) -> i64 {
    use crate::arch::x86_64::syscall as raw;
    let mut ap = args;
    match number {
        186 => { // SYS_gettid
            raw::syscall0(number as u64)
        }
        318 => { // SYS_getrandom
            let buf: usize = ap.arg();
            let buflen: usize = ap.arg();
            let flags: usize = ap.arg();
            raw::syscall3(number as u64, buf, buflen, flags)
        }
        _ => {
            -38 // -ENOSYS
        }
    }
}

// mbsrtowcs/wcsrtombs
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbsrtowcs(
    dest: *mut i32,
    src: *mut *const u8,
    len: usize,
    _ps: *mut u8,
) -> usize {
    if src.is_null() || (*src).is_null() {
        return 0;
    }
    let ret = mbstowcs(dest, *src, len);
    if !dest.is_null() && ret != usize::MAX {
        *src = core::ptr::null();
    }
    ret
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsrtombs(
    dest: *mut u8,
    src: *mut *const i32,
    len: usize,
    _ps: *mut u8,
) -> usize {
    if src.is_null() || (*src).is_null() {
        return 0;
    }
    let ret = wcstombs(dest, *src, len);
    if !dest.is_null() && ret != usize::MAX {
        *src = core::ptr::null();
    }
    ret
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbsnrtowcs(
    dest: *mut i32,
    src: *mut *const u8,
    _nms: usize,
    len: usize,
    ps: *mut u8,
) -> usize {
    mbsrtowcs(dest, src, len, ps)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsnrtombs(
    dest: *mut u8,
    src: *mut *const i32,
    _nwc: usize,
    len: usize,
    ps: *mut u8,
) -> usize {
    wcsrtombs(dest, src, len, ps)
}

// puts - print string with newline
#[unsafe(no_mangle)]
pub unsafe extern "C" fn puts(s: *const u8) -> i32 {
    crate::stdio::puts(s)
}

// strstr - find substring
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strstr(haystack: *const u8, needle: *const u8) -> *mut u8 {
    crate::string::strstr_c(haystack, needle) as *mut u8
}

// wcstok - tokenize wide string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstok(s: *mut i32, delim: *const i32, ptr: *mut *mut i32) -> *mut i32 {
    crate::wchar::wcstok(s, delim, ptr)
}

// System logging functions - minimal stubs (require kernel support)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn openlog(_ident: *const u8, _option: i32, _facility: i32) {
    // Stub: syslog functionality not implemented yet
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn syslog(_priority: i32, _format: *const u8, _args: ...) {
    // Stub: syslog functionality not implemented yet
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn closelog() {
    // Stub: syslog functionality not implemented yet
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setlogmask(_mask: i32) -> i32 {
    0 // Stub: return dummy value
}

// chroot - change root directory (requires kernel support)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chroot(_path: *const u8) -> i32 {
    -1 // Not implemented
}

// Terminal I/O control functions (require kernel support)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcsendbreak(_fd: i32, _duration: i32) -> i32 {
    -1 // Not implemented
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcdrain(_fd: i32) -> i32 {
    -1 // Not implemented
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcflush(_fd: i32, _queue_selector: i32) -> i32 {
    -1 // Not implemented
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcflow(_fd: i32, _action: i32) -> i32 {
    -1 // Not implemented
}

// utime - set file access and modification times
#[unsafe(no_mangle)]
pub unsafe extern "C" fn utime(_filename: *const u8, _times: *const u8) -> i32 {
    -1 // Not implemented
}

// Group database functions (require /etc/group file)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setgrent() {
    // Stub: group database not implemented
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getgrent() -> *mut u8 {
    core::ptr::null_mut() // Stub: return null
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn endgrent() {
    // Stub: group database not implemented
}

// Additional string functions
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memchr(s: *const u8, c: i32, n: usize) -> *mut u8 {
    crate::string::memchr(s, c, n) as *mut u8
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcspn(s: *const u8, reject: *const u8) -> usize {
    crate::string::strcspn(s, reject)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strpbrk(s: *const u8, accept: *const u8) -> *mut u8 {
    crate::string::strpbrk(s, accept) as *mut u8
}

// putchar - already exists in stdio but needs C export
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putchar(c: i32) -> i32 {
    crate::stdio::putchar(c as u8);
    c // Return the character written
}

// times - get process times (stub, requires kernel support)
#[repr(C)]
pub struct tms {
    tms_utime: i64,  // User CPU time
    tms_stime: i64,  // System CPU time
    tms_cutime: i64, // User CPU time of children
    tms_cstime: i64, // System CPU time of children
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn times(buf: *mut tms) -> i64 {
    if !buf.is_null() {
        (*buf).tms_utime = 0;
        (*buf).tms_stime = 0;
        (*buf).tms_cutime = 0;
        (*buf).tms_cstime = 0;
    }
    0 // Stub: return 0 (elapsed time)
}
