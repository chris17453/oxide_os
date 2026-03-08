//! x86_64 syscall interface
//!
//! Raw syscall wrappers using the x86_64 syscall instruction.
//! ABI: syscall number in rax, args in rdi, rsi, rdx, r10, r8, r9
//!
//! 🔥 GraveShift: The `syscall` instruction clobbers RCX (with RIP) and R11
//! (with RFLAGS). The kernel's syscall_entry preserves+restores all other
//! user registers. However, Rust's inline asm considers `in()` registers
//! dead after the asm block — so the compiler may reuse them as scratch
//! across inlined syscall sequences. We must declare ALL caller-saved
//! registers as clobbers so the compiler spills any live values before
//! the asm block. This prevents the optimizer from placing local variables
//! in registers that appear "free" between consecutive inlined syscalls. 🔥

use core::arch::asm;

/// Raw syscall with 0 arguments
#[inline(always)]
pub fn syscall0(nr: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") nr,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            // — GraveShift: clobber all syscall-convention scratch regs
            lateout("rdi") _,
            lateout("rsi") _,
            lateout("rdx") _,
            lateout("r8") _,
            lateout("r9") _,
            lateout("r10") _,
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
            in("rax") nr,
            in("rdi") arg1,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            // — GraveShift: clobber unused syscall-convention scratch regs
            lateout("rsi") _,
            lateout("rdx") _,
            lateout("r8") _,
            lateout("r9") _,
            lateout("r10") _,
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
            in("rax") nr,
            in("rdi") arg1,
            in("rsi") arg2,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            // — GraveShift: clobber unused syscall-convention scratch regs
            lateout("rdx") _,
            lateout("r8") _,
            lateout("r9") _,
            lateout("r10") _,
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
            in("rax") nr,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            // — GraveShift: clobber unused syscall-convention scratch regs
            lateout("r8") _,
            lateout("r9") _,
            lateout("r10") _,
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
            in("rax") nr,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            in("r10") arg4,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            // — GraveShift: clobber unused syscall-convention scratch regs
            lateout("r8") _,
            lateout("r9") _,
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
            in("rax") nr,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            in("r10") arg4,
            in("r8") arg5,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            // — GraveShift: clobber unused syscall-convention scratch regs
            lateout("r9") _,
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
            in("rax") nr,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            in("r10") arg4,
            in("r8") arg5,
            in("r9") arg6,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

/// Special syscall for EXIT - never returns
/// — GraveShift: RAX=60 is SYS_EXIT. The old code used RAX=0 (SYS_READ) which
/// returned instead of dying, then fell through to ud2 → SIGILL. Every process
/// that called exit() was committing suicide by illegal instruction instead of
/// dying with dignity. Classic off-by-sixty bug.
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn syscall_exit(status: usize) -> ! {
    unsafe {
        asm!(
            "mov rax, 60",     // EXIT syscall number (SYS_EXIT = 60 = 0x3c)
            "mov rdi, {0}",    // Exit status
            "syscall",         // Execute syscall
            "ud2",             // — BlackLatch: safety net, should never reach
            in(reg) status,
            options(noreturn),
        );
    }
}
