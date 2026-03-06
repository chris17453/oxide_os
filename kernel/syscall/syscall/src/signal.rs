//! Signal-related syscalls
//!
//! Implements kill, sigaction, sigprocmask, sigpending, etc.

use arch_x86_64 as arch;
use signal::{SIGKILL, SIGSTOP, SigAction, SigHow, SigInfo, SigSet, can_catch, is_valid};
use vfs::permission;

use crate::errno;
use crate::{current_pid, get_current_meta, get_meta, with_current_meta, with_current_meta_mut};

/// sys_kill - Send signal to process
///
/// # Arguments
/// * `pid` - Target process (>0 = specific, 0 = process group, -1 = all, <-1 = group)
/// * `sig` - Signal number (0 = null signal, just check permissions)
pub fn sys_kill(pid: i32, sig: i32) -> i64 {
    // Signal 0 is a null signal (just checks permissions)
    if sig != 0 && !is_valid(sig) {
        return errno::EINVAL;
    }

    let cur_pid = current_pid();

    // Get sender info for siginfo AND permission checking
    // — EmberLock: We need euid for permission checks, uid for SigInfo.
    // A process running as setuid-root has euid=0 even if uid!=0 — use euid.
    let (sender_pid, sender_uid, sender_euid) =
        with_current_meta(|m| (m.tgid, m.credentials.uid, m.credentials.euid))
            .unwrap_or((0, 0, 0));

    if pid > 0 {
        // Send to specific process
        send_signal_to_pid(pid as u32, sig, sender_pid, sender_uid, sender_euid)
    } else if pid == 0 {
        // Send to current process group
        let pgid = match with_current_meta(|m| m.pgid) {
            Some(p) => p,
            None => return errno::ESRCH,
        };
        send_signal_to_pgrp(pgid, sig, sender_pid, sender_uid, sender_euid)
    } else if pid == -1 {
        // Send to all processes (except init and self)
        // — EmberLock: For broadcast kill, skip processes we can't signal.
        // POSIX: succeed if at least one signal was delivered.
        let mut sent = false;
        for p in sched::all_pids() {
            if p != 1 && p != cur_pid {
                if send_signal_to_pid(p, sig, sender_pid, sender_uid, sender_euid) == 0 {
                    sent = true;
                }
            }
        }
        if sent { 0 } else { errno::ESRCH }
    } else {
        // Send to process group |pid|
        let pgid = (-pid) as u32;
        send_signal_to_pgrp(pgid, sig, sender_pid, sender_uid, sender_euid)
    }
}

/// Send signal to a specific PID
///
/// — GraveShift: After queuing the signal, wake the target if it's sleeping in
/// TASK_INTERRUPTIBLE (nanosleep, blocking read, waitpid HLT loop). Linux does
/// this in complete_signal() → signal_wake_up(). Without it, signals queue up
/// but sleeping processes never notice them until their blocking call times out.
///
/// — EmberLock: Now enforces POSIX permission checks. You don't own the process,
/// you don't get to kill it. Revolutionary concept, I know.
fn send_signal_to_pid(
    pid: u32,
    sig: i32,
    sender_pid: u32,
    sender_uid: u32,
    sender_euid: u32,
) -> i64 {
    if let Some(meta) = get_meta(pid) {
        // — EmberLock: Check permission before doing anything with the signal.
        // Get target's real uid — POSIX checks sender euid vs target real uid.
        let target_uid = meta.lock().credentials.uid;
        if !permission::can_signal(sender_euid, target_uid) {
            return errno::EPERM;
        }

        // Signal 0 just checks if process exists (and now also permissions — which we just checked)
        if sig == 0 {
            return 0;
        }

        let info = SigInfo::kill(sig, sender_pid, sender_uid);
        meta.lock().send_signal(sig, Some(info));
        // — GraveShift: Wake the process so it can notice the signal.
        sched::wake_up(pid);
        0
    } else {
        errno::ESRCH
    }
}

/// Send signal to all processes in a process group
///
/// — GraveShift: Same wake-on-signal pattern as send_signal_to_pid. Every process
/// in the group gets woken so they can die together like a proper tragedy.
///
/// — EmberLock: Permission check per process. Root sends to all. Peasants only
/// reach their own. The group kill returns EPERM only if every target was denied.
fn send_signal_to_pgrp(
    pgid: u32,
    sig: i32,
    sender_pid: u32,
    sender_uid: u32,
    sender_euid: u32,
) -> i64 {
    let mut sent = false;
    let mut perm_denied = false;

    for pid in sched::all_pids() {
        if let Some(meta) = get_meta(pid) {
            let meta_guard = meta.lock();
            if meta_guard.pgid == pgid {
                // — EmberLock: Per-process permission check inside the group.
                // One bad apple doesn't spoil the whole group kill.
                let target_uid = meta_guard.credentials.uid;
                drop(meta_guard);

                if !permission::can_signal(sender_euid, target_uid) {
                    perm_denied = true;
                    continue;
                }

                if sig != 0 {
                    let info = SigInfo::kill(sig, sender_pid, sender_uid);
                    meta.lock().send_signal(sig, Some(info));
                    // — GraveShift: Wake sleeping processes so they notice the signal.
                    sched::wake_up(pid);
                }
                sent = true;
            }
        }
    }

    if sent {
        0
    } else if perm_denied {
        // — EmberLock: Found processes in the group but couldn't signal any of them.
        errno::EPERM
    } else {
        errno::ESRCH
    }
}

/// sys_sigaction - Get/set signal handler
///
/// # Arguments
/// * `sig` - Signal number
/// * `act_ptr` - Pointer to new action (or 0)
/// * `oldact_ptr` - Pointer to store old action (or 0)
pub fn sys_sigaction(sig: i32, act_ptr: u64, oldact_ptr: u64) -> i64 {
    // Cannot catch SIGKILL or SIGSTOP
    if !can_catch(sig) {
        return errno::EINVAL;
    }

    if !is_valid(sig) {
        return errno::EINVAL;
    }

    if let Some(meta) = get_current_meta() {
        let mut m = meta.lock();

        // Store old action if requested
        if oldact_ptr != 0 && oldact_ptr < 0x0000_8000_0000_0000 {
            if let Some(old_action) = m.sigaction(sig) {
                unsafe {
                    let out = oldact_ptr as *mut SigAction;
                    *out = *old_action;
                }
            }
        }

        // Set new action if provided
        if act_ptr != 0 && act_ptr < 0x0000_8000_0000_0000 {
            let action = unsafe { *(act_ptr as *const SigAction) };
            m.set_sigaction(sig, action);
        }

        0
    } else {
        errno::ESRCH
    }
}

/// sys_sigprocmask - Get/set signal mask
///
/// # Arguments
/// * `how` - How to modify mask (SIG_BLOCK, SIG_UNBLOCK, SIG_SETMASK)
/// * `set_ptr` - Pointer to new mask (or 0)
/// * `oldset_ptr` - Pointer to store old mask (or 0)
pub fn sys_sigprocmask(how: i32, set_ptr: u64, oldset_ptr: u64) -> i64 {
    if let Some(meta) = get_current_meta() {
        let mut m = meta.lock();

        // Store old mask if requested
        if oldset_ptr != 0 && oldset_ptr < 0x0000_8000_0000_0000 {
            unsafe {
                let out = oldset_ptr as *mut SigSet;
                *out = m.signal_mask.clone();
            }
        }

        // Modify mask if requested
        if set_ptr != 0 && set_ptr < 0x0000_8000_0000_0000 {
            let new_set = unsafe { *(set_ptr as *const SigSet) };

            let how_enum = match SigHow::from_i32(how) {
                Some(h) => h,
                None => return errno::EINVAL,
            };

            let current = m.signal_mask.clone();
            let mut new_mask = match how_enum {
                SigHow::Block => current.union(&new_set),
                SigHow::Unblock => current.difference(&new_set),
                SigHow::SetMask => new_set,
            };

            // Cannot block SIGKILL or SIGSTOP
            new_mask.remove(SIGKILL);
            new_mask.remove(SIGSTOP);

            m.signal_mask = new_mask;
        }

        0
    } else {
        errno::ESRCH
    }
}

/// sys_sigpending - Get pending signals
///
/// # Arguments
/// * `set_ptr` - Pointer to store pending signals
pub fn sys_sigpending(set_ptr: u64) -> i64 {
    if set_ptr == 0 || set_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    if let Some(meta) = get_current_meta() {
        let m = meta.lock();
        let pending = m.pending_signals.set();

        unsafe {
            let out = set_ptr as *mut SigSet;
            *out = pending;
        }

        0
    } else {
        errno::ESRCH
    }
}

/// sys_sigsuspend - Wait for signal with temporary mask
///
/// # Arguments
/// * `mask_ptr` - Pointer to temporary signal mask
///
/// Note: This syscall always returns -EINTR when a signal is delivered.
///
/// — WireSaint: The old implementation returned -EINTR immediately without
/// blocking. That's not sigsuspend — that's sigsurrender. We atomically swap
/// the signal mask, then HLT-loop until a deliverable signal wakes us. The
/// old mask is restored before returning. One way in, one way out: -EINTR.
pub fn sys_sigsuspend(mask_ptr: u64) -> i64 {
    if mask_ptr == 0 || mask_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let meta_arc = match get_current_meta() {
        Some(m) => m,
        None => return errno::ESRCH,
    };

    // Read the temporary mask from userspace and install it atomically.
    // — WireSaint: stac/clac bracket because mask_ptr is a user pointer.
    let temp_mask = unsafe {
        core::arch::asm!("stac", options(nostack));
        let m = core::ptr::read_volatile(mask_ptr as *const SigSet);
        core::arch::asm!("clac", options(nostack));
        m
    };

    let old_mask = {
        let mut m = meta_arc.lock();
        let old = m.signal_mask.clone();
        // Sanitize: SIGKILL and SIGSTOP can never be masked.
        let mut new_mask = temp_mask;
        new_mask.remove(SIGKILL);
        new_mask.remove(SIGSTOP);
        m.signal_mask = new_mask;
        old
    };

    // — WireSaint: Block using HLT+kpo until a deliverable signal arrives.
    // This is identical to the nanosleep/select pattern: allow preemption,
    // then sti+hlt. The signal sender calls wake_up() which re-enqueues us.
    // We check after each wakeup whether a signal would actually DO something
    // (not just queued and blocked/ignored) before returning -EINTR.
    loop {
        // Check for deliverable signal *before* sleeping, in case it arrived
        // between the mask swap and the first HLT.
        let interrupted = {
            let m = meta_arc.lock();
            signal::delivery::should_interrupt_for_signal(
                &m.pending_signals.set(),
                &m.signal_mask,
                &m.sigactions,
            )
        };

        if interrupted {
            break;
        }

        arch::allow_kernel_preempt();
        unsafe { core::arch::asm!("sti", "hlt", options(nomem, nostack)); }
        arch::disallow_kernel_preempt();
    }

    // Restore old signal mask before returning.
    // — WireSaint: POSIX requires this even though we're returning -EINTR.
    meta_arc.lock().signal_mask = old_mask;

    // sigsuspend always returns -EINTR — that is the contract.
    crate::errno::EINTR
}

/// sys_pause - Wait for any signal
///
/// Note: This syscall always returns -EINTR when a signal is delivered.
///
/// — WireSaint: The old implementation was literally one line: return -EINTR.
/// That's not pause — that's impatience. Real pause() HLT-loops until a
/// deliverable signal arrives (non-blocked, non-ignored). Unlike sigsuspend,
/// we don't touch the signal mask at all. Just sleep and wake on signal.
pub fn sys_pause() -> i64 {
    let meta_arc = match get_current_meta() {
        Some(m) => m,
        None => return errno::ESRCH,
    };

    // — WireSaint: HLT+kpo loop until a deliverable signal shows up.
    // Check before the first HLT in case the signal already arrived.
    loop {
        let interrupted = {
            let m = meta_arc.lock();
            signal::delivery::should_interrupt_for_signal(
                &m.pending_signals.set(),
                &m.signal_mask,
                &m.sigactions,
            )
        };

        if interrupted {
            break;
        }

        arch::allow_kernel_preempt();
        unsafe { core::arch::asm!("sti", "hlt", options(nomem, nostack)); }
        arch::disallow_kernel_preempt();
    }

    // pause() always returns -EINTR.
    crate::errno::EINTR
}

/// sys_sigreturn - Return from signal handler
///
/// — GraveShift: The previous "implementation" was literally `return 0`. That's not an
/// implementation, that's a confession. We can't modify SYSCALL_USER_CONTEXT here because
/// the asm resaves from the kernel stack AFTER this handler returns. Instead, we stash
/// the SignalFrame in a global and check_signals_on_syscall_return() applies it under CLI
/// where it sticks. Deferred restoration — the only way to beat the asm resave race.
pub fn sys_sigreturn() -> i64 {
    use signal::delivery::SignalFrame;

    // Get current user context — RSP points past the retaddr field (handler did `ret`)
    let ctx = unsafe { arch::syscall::get_user_context_mut() };

    // Frame starts 8 bytes below current RSP (retaddr was popped by `ret`)
    let frame_ptr = (ctx.rsp - 8) as *const SignalFrame;

    // Validate pointer is in user space
    if (frame_ptr as u64) >= 0x0000_8000_0000_0000 {
        return crate::errno::EFAULT;
    }

    // Enable user memory access (SMAP)
    unsafe { core::arch::asm!("stac", options(nostack)); }

    // Read the signal frame from user stack
    let frame = unsafe { core::ptr::read_volatile(frame_ptr) };

    // Disable user memory access
    unsafe { core::arch::asm!("clac", options(nostack)); }

    // Stash for deferred restoration in check_signals_on_syscall_return()
    unsafe { signal::delivery::set_sigreturn_frame(frame); }

    // Return value doesn't matter — check_signals_on_syscall_return will
    // overwrite ctx.rax with frame.saved_rax before sysretq
    0
}

/// stack_t structure for sigaltstack
#[repr(C)]
#[derive(Clone, Copy)]
struct StackT {
    ss_sp: u64,    // Base address of stack
    ss_flags: i32, // Flags (SS_DISABLE, SS_ONSTACK)
    _pad: i32,
    ss_size: usize, // Number of bytes in stack
}

/// sigaltstack flags
const SS_ONSTACK: i32 = 1;
const SS_DISABLE: i32 = 2;
const MINSIGSTKSZ: usize = 2048;

/// sys_sigaltstack - Set/get alternate signal stack
///
/// # Arguments
/// * `ss_ptr` - New alternate stack (NULL to query only)
/// * `old_ss_ptr` - Where to store current alternate stack (NULL to skip)
pub fn sys_sigaltstack(ss_ptr: u64, old_ss_ptr: u64) -> i64 {
    use crate::{errno, with_current_meta, with_current_meta_mut};

    // Return the old stack if requested
    if old_ss_ptr != 0 && old_ss_ptr < 0x0000_8000_0000_0000 {
        // We don't track alt stack in ProcessMeta yet, return disabled
        let old = StackT {
            ss_sp: 0,
            ss_flags: SS_DISABLE,
            _pad: 0,
            ss_size: 0,
        };
        unsafe {
            core::arch::asm!("stac", options(nostack));
            core::ptr::write_volatile(old_ss_ptr as *mut StackT, old);
            core::arch::asm!("clac", options(nostack));
        }
    }

    // Set the new stack if provided
    if ss_ptr != 0 && ss_ptr < 0x0000_8000_0000_0000 {
        let ss: StackT = unsafe {
            core::arch::asm!("stac", options(nostack));
            let val = core::ptr::read_volatile(ss_ptr as *const StackT);
            core::arch::asm!("clac", options(nostack));
            val
        };

        // Validate
        if ss.ss_flags & !SS_DISABLE != 0 {
            return errno::EINVAL;
        }
        if ss.ss_flags & SS_DISABLE == 0 && ss.ss_size < MINSIGSTKSZ {
            return errno::ENOMEM;
        }

        // Accept the values (stored for future signal delivery support)
        // Full implementation would save ss_sp/ss_size in ProcessMeta
    }

    0
}

/// Read a SigSet from user memory
///
/// Returns None if the pointer is invalid or in kernel space
pub fn read_sigset(ptr: usize) -> Option<SigSet> {
    if ptr == 0 || ptr >= 0x0000_8000_0000_0000 {
        return None;
    }

    unsafe {
        core::arch::asm!("stac", options(nostack));
        let sigset = core::ptr::read_volatile(ptr as *const SigSet);
        core::arch::asm!("clac", options(nostack));
        Some(sigset)
    }
}

/// Atomically swap the current process's signal mask
///
/// Sets the new mask and returns the old mask
pub fn swap_signal_mask(new_mask: SigSet) -> SigSet {
    if let Some(meta) = get_current_meta() {
        let mut m = meta.lock();
        let old_mask = m.signal_mask.clone();

        // Apply new mask (but never block SIGKILL or SIGSTOP)
        let mut sanitized = new_mask;
        sanitized.remove(SIGKILL);
        sanitized.remove(SIGSTOP);
        m.signal_mask = sanitized;

        old_mask
    } else {
        SigSet::empty()
    }
}

/// Set the current process's signal mask
pub fn set_signal_mask(mask: SigSet) {
    if let Some(meta) = get_current_meta() {
        let mut m = meta.lock();

        // Apply mask (but never block SIGKILL or SIGSTOP)
        let mut sanitized = mask;
        sanitized.remove(SIGKILL);
        sanitized.remove(SIGSTOP);
        m.signal_mask = sanitized;
    }
}
