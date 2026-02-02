//! MIPS64 syscall interface
//!
//! Raw syscall wrappers using the MIPS64 SYSCALL instruction.
//! ABI: syscall number in $v0 ($2), args in $a0-$a5 ($4-$9), return in $v0
//!
//! MIPS64 calling convention (N64 ABI):
//! - $a0-$a7 ($4-$11): argument registers
//! - $v0-$v1 ($2-$3): return value registers
//! - $t0-$t3 ($8-$11): temporary caller-saved (overlap with $a4-$a7)
//! - $t4-$t9 ($12-$15, $24-$25): temporary caller-saved
//! - $s0-$s7 ($16-$23): saved registers (callee-saved)
//! - $gp ($28): global pointer
//! - $sp ($29): stack pointer
//! - $fp ($30): frame pointer
//! - $ra ($31): return address
//!
//! ⚠️ BIG-ENDIAN: SGI MIPS systems are big-endian
//!
//! — GraveShift

use core::arch::asm;

/// Raw syscall with 0 arguments
#[inline(always)]
pub fn syscall0(nr: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("$2") nr,      // $v0 = syscall number
            lateout("$2") ret, // $v0 = return value
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
            "syscall",
            in("$2") nr,      // $v0
            in("$4") arg1,    // $a0
            lateout("$2") ret,
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
            "syscall",
            in("$2") nr,      // $v0
            in("$4") arg1,    // $a0
            in("$5") arg2,    // $a1
            lateout("$2") ret,
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
            "syscall",
            in("$2") nr,      // $v0
            in("$4") arg1,    // $a0
            in("$5") arg2,    // $a1
            in("$6") arg3,    // $a2
            lateout("$2") ret,
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
            "syscall",
            in("$2") nr,      // $v0
            in("$4") arg1,    // $a0
            in("$5") arg2,    // $a1
            in("$6") arg3,    // $a2
            in("$7") arg4,    // $a3
            lateout("$2") ret,
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
            "syscall",
            in("$2") nr,      // $v0
            in("$4") arg1,    // $a0
            in("$5") arg2,    // $a1
            in("$6") arg3,    // $a2
            in("$7") arg4,    // $a3
            in("$8") arg5,    // $a4 (also $t0)
            lateout("$2") ret,
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
            "syscall",
            in("$2") nr,      // $v0
            in("$4") arg1,    // $a0
            in("$5") arg2,    // $a1
            in("$6") arg3,    // $a2
            in("$7") arg4,    // $a3
            in("$8") arg5,    // $a4
            in("$9") arg6,    // $a5 (also $t1)
            lateout("$2") ret,
            options(nostack),
        );
    }
    ret
}
