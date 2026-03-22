//! Raw syscall wrappers for ld-oxide
//!
//! — SableWire: the dynamic linker can't use libc — it IS the thing that loads libc.
//! Raw syscall instructions only. No dependencies, no allocator, no nothing.

/// SYS_READ = 0
const SYS_READ: u64 = 0;
/// SYS_WRITE = 1 (Linux-compatible)
const SYS_WRITE: u64 = 1;
/// SYS_OPEN = 2
const SYS_OPEN: u64 = 2;
/// SYS_CLOSE = 3
const SYS_CLOSE: u64 = 3;
/// SYS_FSTAT = 5
const SYS_FSTAT: u64 = 5;
/// SYS_MMAP = 9
const SYS_MMAP: u64 = 9;
/// SYS_MPROTECT = 10
const SYS_MPROTECT: u64 = 10;
/// SYS_EXIT = 60
const SYS_EXIT: u64 = 60;

// — SableWire: mmap protection flags
pub const PROT_READ: u32 = 1;
pub const PROT_WRITE: u32 = 2;
pub const PROT_EXEC: u32 = 4;

// mmap flags
pub const MAP_PRIVATE: u32 = 0x02;
pub const MAP_ANONYMOUS: u32 = 0x20;
pub const MAP_FIXED: u32 = 0x10;

// open flags
pub const O_RDONLY: u32 = 0;

/// Raw x86_64 syscall — 3 args
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall3(nr: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") nr,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack)
        );
    }
    ret
}

/// Raw x86_64 syscall — 6 args
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall6(nr: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") nr,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            in("r10") a4,
            in("r8") a5,
            in("r9") a6,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack)
        );
    }
    ret
}

/// Raw x86_64 syscall — 1 arg
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall1(nr: u64, a1: u64) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") nr,
            in("rdi") a1,
            out("rcx") _,
            out("r11") _,
            out("rsi") _,
            out("rdx") _,
            lateout("rax") ret,
            options(nostack)
        );
    }
    ret
}

/// Write bytes to fd
pub fn sys_write(fd: i32, buf: &[u8]) -> isize {
    unsafe { syscall3(SYS_WRITE, fd as u64, buf.as_ptr() as u64, buf.len() as u64) as isize }
}

/// Read bytes from fd
pub fn sys_read(fd: i32, buf: &mut [u8]) -> isize {
    unsafe { syscall3(SYS_READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) as isize }
}

/// Open a file
/// — SableWire: OXIDE's open syscall takes (path_ptr, path_len, flags, mode) — NOT
/// null-terminated like Linux. We pass the slice length as the second argument.
pub fn sys_open(path: &[u8], flags: u32, mode: u32) -> i32 {
    // — SableWire: strip trailing null if present (we null-terminate for safety,
    // but the kernel uses the length, not the null)
    let len = path.iter().position(|&b| b == 0).unwrap_or(path.len());
    unsafe { syscall6(SYS_OPEN, path.as_ptr() as u64, len as u64, flags as u64, mode as u64, 0, 0) as i32 }
}

/// Close a file descriptor
pub fn sys_close(fd: i32) -> i32 {
    unsafe { syscall1(SYS_CLOSE, fd as u64) as i32 }
}

/// Map memory
pub fn sys_mmap(addr: u64, length: u64, prot: u32, flags: u32, fd: i32, offset: u64) -> u64 {
    unsafe {
        syscall6(SYS_MMAP, addr, length, prot as u64, flags as u64, fd as u64, offset) as u64
    }
}

/// Exit the process
pub fn exit(code: i32) -> ! {
    unsafe { syscall1(SYS_EXIT, code as u64); }
    loop {} // unreachable
}

/// Write a string to stdout (fd 1)
/// — SableWire: for diagnostic output during dynamic linking
pub fn write_str(s: &str) {
    sys_write(1, s.as_bytes());
}

/// Write raw bytes to stdout
pub fn write_bytes(b: &[u8]) {
    sys_write(1, b);
}

/// Write a decimal number to stdout
pub fn write_num(mut n: u64) {
    if n == 0 {
        write_str("0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    sys_write(1, &buf[i..20]);
}

/// Write a hex number to stdout
pub fn write_hex(n: u64) {
    write_str("0x");
    let mut buf = [0u8; 16];
    for i in 0..16 {
        let nibble = ((n >> (60 - i * 4)) & 0xF) as u8;
        buf[i] = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
    }
    // Skip leading zeros
    let start = buf.iter().position(|&b| b != b'0').unwrap_or(15);
    sys_write(1, &buf[start..16]);
}
