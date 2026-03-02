//! Thread syscall wrappers — clone, futex, yield, sleep.
//!
//! — ThreadRogue: Threads in a hobby OS. What could go wrong?
//! Everything. Everything could go wrong. But here we are.

use crate::syscall::*;
use crate::nr;
use crate::types::Timespec;

/// nanosleep — sleep for a duration
pub fn nanosleep(req: &Timespec, rem: Option<&mut Timespec>) -> i32 {
    let rem_ptr = match rem {
        Some(r) => r as *mut Timespec as usize,
        None => 0,
    };
    syscall2(nr::NANOSLEEP, req as *const Timespec as usize, rem_ptr) as i32
}

/// sleep_ms — convenience wrapper for millisecond sleep
pub fn sleep_ms(ms: u64) {
    let req = Timespec {
        tv_sec: (ms / 1000) as i64,
        tv_nsec: ((ms % 1000) * 1_000_000) as i64,
    };
    nanosleep(&req, None);
}

/// sched_yield — voluntarily give up the CPU
pub fn sched_yield() -> i32 {
    syscall0(nr::SCHED_YIELD) as i32
}

/// clone — create a new thread
/// flags: CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD etc.
/// stack: pointer to top of new thread's stack
/// Returns thread ID on success, negative errno on failure
pub fn clone(
    flags: u64,
    stack: *mut u8,
    parent_tid: *mut i32,
    child_tid: *mut i32,
    tls: usize,
) -> i64 {
    syscall5(
        nr::CLONE,
        flags as usize,
        stack as usize,
        parent_tid as usize,
        child_tid as usize,
        tls,
    ) as i64
}

/// gettid — get thread ID
pub fn gettid() -> i32 {
    syscall0(nr::GETTID) as i32
}

/// — ThreadRogue: spawn_thread — the real thread creation primitive.
/// Does the musl-style clone dance: puts entry+arg on the child stack,
/// calls clone, child pops and calls the entry function, then exits.
/// Parent gets the child TID back.
///
/// entry_fn: extern "C" fn(arg: usize) — called on the new thread
/// arg: passed to entry_fn
/// stack: mmap'd memory region (pointer to BASE, not top)
/// stack_size: size of the stack region
///
/// Returns: child TID on success, negative errno on failure
pub unsafe fn spawn_thread(
    entry_fn: extern "C" fn(usize),
    arg: usize,
    stack: *mut u8,
    stack_size: usize,
) -> i64 {
    use crate::types::clone_flags::*;

    let flags: u64 = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD;

    // — ThreadRogue: musl-style clone wrapper. We place the entry function and
    // argument on the child's stack before the syscall. After clone returns 0
    // in the child, inline asm pops the fn+arg and calls it, then exits.
    // Parent gets the TID and returns normally. Simple? Ha.
    let stack_top = unsafe { stack.add(stack_size) };
    // 16-byte align, then make room for 2 words (entry + arg)
    let aligned = ((stack_top as usize) & !15) - 16;
    let slot = aligned as *mut usize;
    unsafe {
        *slot = entry_fn as usize;       // [rsp]   = entry function
        *slot.add(1) = arg;              // [rsp+8] = argument
    }

    let ret: i64;
    unsafe {
        core::arch::asm!(
            // — ThreadRogue: musl-style clone. rdi = flags, rsi = child stack
            "xor edx, edx",            // parent_tid = NULL
            "xor r10d, r10d",          // child_tid = NULL
            "xor r8d, r8d",            // tls = 0
            "mov eax, 56",             // SYS_clone
            "syscall",
            "test eax, eax",
            "jnz 2f",
            // — ThreadRogue: Child path — rsp points at [entry_fn, arg]
            "xor ebp, ebp",            // clear frame pointer
            "pop rax",                 // entry_fn
            "pop rdi",                 // arg (first parameter)
            "call rax",                // call entry_fn(arg)
            "xor edi, edi",            // exit code = 0 (entry_fn returns void)
            "mov eax, 0",              // SYS_exit = 0
            "syscall",
            "ud2",                     // unreachable
            "2:",                      // Parent: rax = child TID or -errno
            in("rdi") flags,
            in("rsi") aligned,
            lateout("rax") ret,
            clobber_abi("C"),
        );
    }
    ret
}

/// set_tid_address — set pointer to thread ID
pub fn set_tid_address(tidptr: *mut i32) -> i32 {
    syscall1(nr::SET_TID_ADDRESS, tidptr as usize) as i32
}
