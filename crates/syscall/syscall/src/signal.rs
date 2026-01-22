//! Signal-related syscalls
//!
//! Implements kill, sigaction, sigprocmask, sigpending, etc.

use proc::process_table;
use signal::{SIGKILL, SIGSTOP, SigAction, SigHow, SigInfo, SigSet, can_catch, is_valid};

use crate::errno;

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

    let table = process_table();
    let current_pid = table.current_pid();

    // Get sender info for siginfo
    let (sender_pid, sender_uid) = if let Some(proc) = table.get(current_pid) {
        let p = proc.lock();
        (p.pid(), p.credentials().uid)
    } else {
        (0, 0)
    };

    if pid > 0 {
        // Send to specific process
        send_signal_to_pid(pid as u32, sig, sender_pid, sender_uid)
    } else if pid == 0 {
        // Send to current process group
        let pgid = if let Some(proc) = table.get(current_pid) {
            proc.lock().pgid()
        } else {
            return errno::ESRCH;
        };
        send_signal_to_pgrp(pgid, sig, sender_pid, sender_uid)
    } else if pid == -1 {
        // Send to all processes (except init and self)
        let mut sent = false;
        for p in table.all_pids() {
            if p != 1 && p != current_pid {
                if send_signal_to_pid(p, sig, sender_pid, sender_uid) == 0 {
                    sent = true;
                }
            }
        }
        if sent { 0 } else { errno::ESRCH }
    } else {
        // Send to process group |pid|
        let pgid = (-pid) as u32;
        send_signal_to_pgrp(pgid, sig, sender_pid, sender_uid)
    }
}

/// Send signal to a specific PID
fn send_signal_to_pid(pid: u32, sig: i32, sender_pid: u32, sender_uid: u32) -> i64 {
    let table = process_table();

    if let Some(proc) = table.get(pid) {
        // Signal 0 just checks if process exists
        if sig == 0 {
            return 0;
        }

        let info = SigInfo::kill(sig, sender_pid, sender_uid);
        proc.lock().send_signal(sig, Some(info));
        0
    } else {
        errno::ESRCH
    }
}

/// Send signal to all processes in a process group
fn send_signal_to_pgrp(pgid: u32, sig: i32, sender_pid: u32, sender_uid: u32) -> i64 {
    let table = process_table();
    let mut sent = false;

    for pid in table.all_pids() {
        if let Some(proc) = table.get(pid) {
            if proc.lock().pgid() == pgid {
                if sig != 0 {
                    let info = SigInfo::kill(sig, sender_pid, sender_uid);
                    proc.lock().send_signal(sig, Some(info));
                }
                sent = true;
            }
        }
    }

    if sent { 0 } else { errno::ESRCH }
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

    let table = process_table();
    let current_pid = table.current_pid();

    if let Some(proc) = table.get(current_pid) {
        let mut p = proc.lock();

        // Store old action if requested
        if oldact_ptr != 0 && oldact_ptr < 0x0000_8000_0000_0000 {
            if let Some(old_action) = p.sigaction(sig) {
                unsafe {
                    let out = oldact_ptr as *mut SigAction;
                    *out = *old_action;
                }
            }
        }

        // Set new action if provided
        if act_ptr != 0 && act_ptr < 0x0000_8000_0000_0000 {
            let action = unsafe { *(act_ptr as *const SigAction) };
            p.set_sigaction(sig, action);
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
    let table = process_table();
    let current_pid = table.current_pid();

    if let Some(proc) = table.get(current_pid) {
        let mut p = proc.lock();

        // Store old mask if requested
        if oldset_ptr != 0 && oldset_ptr < 0x0000_8000_0000_0000 {
            unsafe {
                let out = oldset_ptr as *mut SigSet;
                *out = *p.signal_mask();
            }
        }

        // Modify mask if requested
        if set_ptr != 0 && set_ptr < 0x0000_8000_0000_0000 {
            let new_set = unsafe { *(set_ptr as *const SigSet) };

            let how_enum = match SigHow::from_i32(how) {
                Some(h) => h,
                None => return errno::EINVAL,
            };

            let current = *p.signal_mask();
            let mut new_mask = match how_enum {
                SigHow::Block => current.union(&new_set),
                SigHow::Unblock => current.difference(&new_set),
                SigHow::SetMask => new_set,
            };

            // Cannot block SIGKILL or SIGSTOP
            new_mask.remove(SIGKILL);
            new_mask.remove(SIGSTOP);

            p.set_signal_mask(new_mask);
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

    let table = process_table();
    let current_pid = table.current_pid();

    if let Some(proc) = table.get(current_pid) {
        let p = proc.lock();
        let pending = p.pending_signals().set();

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
/// Note: This syscall always returns -EINTR when a signal is delivered
pub fn sys_sigsuspend(mask_ptr: u64) -> i64 {
    if mask_ptr == 0 || mask_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let table = process_table();
    let current_pid = table.current_pid();

    if let Some(proc) = table.get(current_pid) {
        let mut p = proc.lock();

        // Save old mask
        let old_mask = *p.signal_mask();

        // Set temporary mask
        let mut temp_mask = unsafe { *(mask_ptr as *const SigSet) };
        temp_mask.remove(SIGKILL);
        temp_mask.remove(SIGSTOP);
        p.set_signal_mask(temp_mask);

        // Drop lock before waiting
        drop(p);

        // Wait for a signal (in a real implementation, we'd block here)
        // For now, we just check if there's a pending signal
        // The actual waiting would be done by the scheduler

        // Restore old mask
        if let Some(proc) = table.get(current_pid) {
            proc.lock().set_signal_mask(old_mask);
        }

        // sigsuspend always returns -EINTR
        crate::errno::EINTR
    } else {
        errno::ESRCH
    }
}

/// sys_pause - Wait for any signal
///
/// Note: This syscall always returns -EINTR when a signal is delivered
pub fn sys_pause() -> i64 {
    // In a real implementation, we'd block until a signal is delivered
    // For now, just return -EINTR
    crate::errno::EINTR
}

/// sys_sigreturn - Return from signal handler
///
/// This restores the process context from the signal frame on the stack.
/// The actual implementation needs architecture-specific handling.
pub fn sys_sigreturn() -> i64 {
    // This needs to restore context from the signal frame
    // For now, just return success (actual implementation is arch-specific)
    0
}
