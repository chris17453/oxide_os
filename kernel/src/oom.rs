//! OOM Killer — last line of defense against memory exhaustion
//!
//! — IronGhost: When the buddy allocator is bone-dry and the heap's about to
//! panic, this module picks the fattest process and sends it to the shadow realm.
//! SIGKILL, no mercy, no appeal. Better one process dies than the whole system
//! locks up in an alloc panic spiral. Linux has had this since 2.6 and we're
//! not about to pretend we're smarter than thirty years of OOM-kill wisdom.

use proc_traits::Pid;

/// — IronGhost: The OOM reaper. Called from mm-manager when alloc_frame() fails.
/// Returns true if we killed something (caller should retry the allocation).
/// Returns false if nothing killable was found (system is truly doomed).
pub fn try_oom_kill() -> bool {
    // — IronGhost: Collect all PIDs in the system. This allocates a Vec on the
    // heap, which is ironic during OOM. But the heap allocator uses a different
    // path (GlobalAlloc → slab/bump) and typically has slack. If THIS alloc
    // fails, we're already dead anyway — nothing left to kill will save us.
    let pids = sched::all_pids();

    if pids.is_empty() {
        return false;
    }

    // — IronGhost: Score each process by allocated_frames_count(). Higher = fatter.
    // Skip PID 0 (idle tasks) and PID 1 (init — killing init is suicide).
    let mut best_pid: Option<Pid> = None;
    let mut best_score: usize = 0;

    for pid in &pids {
        // — IronGhost: Never kill idle (PID 0) or init (PID 1). If init is the
        // memory hog, the operator has bigger problems than we can solve.
        if *pid <= 1 {
            continue;
        }

        // — IronGhost: Use try_get_task_meta (non-blocking) because we might be
        // called from an allocation path that already holds a run queue lock.
        // Blocking here = instant deadlock on single-CPU systems.
        let meta = match sched::try_get_task_meta(*pid) {
            Some(m) => m,
            None => continue,
        };

        let score = match meta.try_lock() {
            Some(m) => m.address_space.allocated_frames_count(),
            None => continue,
        };

        if score > best_score {
            best_score = score;
            best_pid = Some(*pid);
        }
    }

    let victim = match best_pid {
        Some(pid) => pid,
        None => {
            unsafe {
                os_log::write_str_raw("[OOM] No killable process found — system doomed\n");
            }
            return false;
        }
    };

    // — IronGhost: SIGKILL the victim. No signal handler, no cleanup, no mercy.
    // The scheduler will reap it on the next tick and Drop will free its frames.
    unsafe {
        os_log::write_str_raw("[OOM] Killing PID ");
        os_log::write_u32_raw(victim);
        os_log::write_str_raw(" (score=");
        os_log::write_u32_raw(best_score as u32);
        os_log::write_str_raw(" frames) to free memory\n");
    }

    if let Some(meta) = sched::try_get_task_meta(victim) {
        if let Some(mut m) = meta.try_lock() {
            m.send_signal(signal::SIGKILL, None);
            return true;
        }
    }

    // — IronGhost: Couldn't lock the victim's meta. Probably contended.
    // Return false — caller will propagate OOM. Better than spinning.
    false
}
