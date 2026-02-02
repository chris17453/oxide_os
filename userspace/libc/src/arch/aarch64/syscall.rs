//! ARM64 (aarch64) syscall interface
//!
//! Raw syscall wrappers using the ARM64 SVC (supervisor call) instruction.
//! ABI: syscall number in x8, args in x0-x5, return in x0
//!
//! ARM64 calling convention:
//! - x0-x7: argument/result registers
//! - x8: syscall number (indirect syscall ABI)
//! - x9-x15: caller-saved temporary registers
//! - x16-x17: intra-procedure-call scratch registers (IP0, IP1)
//! - x18: platform register (reserved, don't use)
//! - x19-x28: callee-saved registers
//! - x29: frame pointer (FP)
//! - x30: link register (LR)
//! - sp: stack pointer
//!
//! — NeonRoot

use core::arch::asm;

/// Raw syscall with 0 arguments
#[inline(always)]
pub fn syscall0(nr: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            lateout("x0") ret,
            options(nostack),
        );
    }
    ret
}

/// Raw syscall with 1 argument
#[inline(always)]
pub fn syscall1(nr: u64, arg1: usize) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            in("x0") arg1,
            lateout("x0") ret,
            options(nostack),
        );
    }
    ret
}

/// Raw syscall with 2 arguments
#[inline(always)]
pub fn syscall2(nr: u64, arg1: usize, arg2: usize) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            in("x0") arg1,
            in("x1") arg2,
            lateout("x0") ret,
            options(nostack),
        );
    }
    ret
}

/// Raw syscall with 3 arguments
#[inline(always)]
pub fn syscall3(nr: u64, arg1: usize, arg2: usize, arg3: usize) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            in("x0") arg1,
            in("x1") arg2,
            in("x2") arg3,
            lateout("x0") ret,
            options(nostack),
        );
    }
    ret
}

/// Raw syscall with 4 arguments
#[inline(always)]
pub fn syscall4(nr: u64, arg1: usize, arg2: usize, arg3: usize, arg4: usize) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            in("x0") arg1,
            in("x1") arg2,
            in("x2") arg3,
            in("x3") arg4,
            lateout("x0") ret,
            options(nostack),
        );
    }
    ret
}

/// Raw syscall with 5 arguments
#[inline(always)]
pub fn syscall5(nr: u64, arg1: usize, arg2: usize, arg3: usize, arg4: usize, arg5: usize) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            in("x0") arg1,
            in("x1") arg2,
            in("x2") arg3,
            in("x3") arg4,
            in("x4") arg5,
            lateout("x0") ret,
            options(nostack),
        );
    }
    ret
}

/// Raw syscall with 6 arguments
#[inline(always)]
pub fn syscall6(
    nr: u64,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
    arg6: usize,
) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            in("x0") arg1,
            in("x1") arg2,
            in("x2") arg3,
            in("x3") arg4,
            in("x4") arg5,
            in("x5") arg6,
            lateout("x0") ret,
            options(nostack),
        );
    }
    ret
}
