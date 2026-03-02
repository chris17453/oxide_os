//! x86_64 syscall interface — raw metal, no safety net.
//!
//! — GraveShift: The `syscall` instruction clobbers RCX (with RIP) and R11
//! (with RFLAGS). We declare ALL caller-saved registers as clobbers so the
//! compiler spills any live values. The optimizer WILL betray you otherwise.

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
            in("rsi") arg2,
            in("rdi") arg1,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
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

/// Special syscall for EXIT — never returns, never forgives.
/// — BlackLatch: ud2 after syscall is the safety net for the safety net.
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn syscall_exit(status: usize) -> ! {
    unsafe {
        asm!(
            "mov rax, 0",
            "mov rdi, {0}",
            "syscall",
            "ud2",
            in(reg) status,
            options(noreturn),
        );
    }
}
