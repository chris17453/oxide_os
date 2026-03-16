# OXIDE OS Syscall Audit -- Performance & Correctness

Audit date: 2026-03-15
Auditor: Claude Opus 4.6 (1M context)
Source: `kernel/syscall/syscall/src/` (14 source files)

## Summary

| Metric | Count |
|--------|-------|
| Total syscall numbers defined | 142 |
| Fully implemented | ~85 |
| Stubs returning ENOSYS | 14 |
| Advisory no-ops (correct) | 3 |
| Critical performance issues | 6 |
| Correctness bugs found | 12 |
| Safety concerns | 8 |

Syscall numbering follows Linux x86_64 ABI (`asm/unistd_64.h`). OXIDE-specific
extensions live at 500+ to avoid collision with future Linux numbers.

---

## Critical Issues (fix immediately)

### C1. `sys_sigaction` / `sys_sigprocmask` -- Missing SMAP brackets on user pointer access

**File:** `signal.rs:178-191`, `signal.rs:218-229`

`sys_sigaction` reads/writes SigAction structs directly from user pointers via
raw dereference (`*(act_ptr as *const SigAction)`) without `user_access_begin()`
/ `user_access_end()` STAC/CLAC brackets. Same issue in `sys_sigprocmask` for
the SigSet pointer. While pointer validation rejects kernel-space addresses,
the missing STAC means SMAP enforcement will #PF on every call if SMAP is
enabled. This is a **functional correctness bug on SMAP-enabled hardware**.

**Impact:** SMAP violation (#PF) on every sigaction/sigprocmask call.
**Fix:** Wrap the reads/writes in `os_core::user_access_begin()` / `user_access_end()`.

### C2. `sys_sigpending` -- Missing SMAP brackets

**File:** `signal.rs:269-271`

Writes SigSet to user pointer without STAC/CLAC. Same SMAP violation.

### C3. `sys_mprotect` -- Only handles WRITABLE flag, ignores READ/EXEC removal

**File:** `memory.rs:340-355`

The implementation only updates page flags when `PROT_WRITE` is set. It cannot:
- Remove write permission (downgrade to read-only)
- Remove execute permission
- Set PROT_NONE (fully inaccessible)

This breaks security features like W^X enforcement and guard pages. Programs
that call `mprotect(addr, len, PROT_NONE)` to create guard pages get a silent
success with no actual protection change.

**Impact:** Security -- guard pages don't work, W^X not enforceable.

### C4. `sys_mremap` -- Hardcodes PROT_READ|PROT_WRITE on regrown mapping

**File:** `memory.rs:401-413, 422-429`

When mremap needs to move a mapping, it creates the new mapping with hardcoded
`PROT_READ | PROT_WRITE` instead of preserving the original protection flags.
An executable mapping remapped via mremap loses PROT_EXEC. A read-only mapping
becomes writable.

**Impact:** Correctness -- protection flags not preserved across remap.

### C5. `sys_fw_add_rule` / `sys_fw_get_conntrack` -- Missing SMAP for user pointer

**File:** `firewall.rs:210-216, 358-364`

`sys_fw_add_rule` dereferences `rule_ptr` as a raw pointer without STAC/CLAC.
`sys_fw_get_conntrack` writes to `stats_ptr` without STAC/CLAC. Both will SMAP
fault. Additionally, `sys_fw_add_rule` only checks for null, not for
kernel-space addresses -- a crafted pointer >= 0x8000_0000_0000 would let
userspace read/write kernel memory (if SMAP were disabled).

**Impact:** SMAP violation + potential privilege escalation without SMAP.

### C6. Sleep queue fixed-size limit (64 entries)

**File:** `time.rs:26-27`

`MAX_SLEEPERS = 64` is a hard limit. If more than 64 tasks call `nanosleep`
simultaneously, `sleep_queue_add` returns false and the task falls back to
busy-polling with HLT. With 64+ concurrent sleeping processes (plausible with
daemons), the 65th sleeper burns a timer tick per wakeup instead of sleeping
properly.

**Impact:** CPU waste under moderate load; 100Hz wakeup rate per overflow sleeper.
**Fix:** Use a dynamically-sized structure or a timer wheel.

---

## Performance Optimization Opportunities

### P1. `sys_poll` / `sys_select` -- Vec heap allocation on every call

**File:** `poll.rs:177`

`sys_poll` allocates `Vec::with_capacity(nfds)` for the pollfd array on every
invocation. For hot-path daemons calling poll() at 100Hz with small fd sets
(1-4 fds), this is an unnecessary heap alloc/dealloc pair per call. The max
nfds is 1024, so a stack-allocated array (8KB) would eliminate this entirely
for the common case.

**Severity:** Medium. Measurable overhead for poll-heavy workloads.
**Fix:** Use a stack buffer for nfds <= 64, fall back to Vec for larger sets.

### P2. `sys_poll` / `sys_select` re-register on every wake cycle

**File:** `poll.rs:270-278, 536-542`

After each spurious wakeup, the code calls `poll_table.unregister_all()` then
re-registers every fd. If the WaitQueue wake clears registrations, this is
necessary. But it means each wakeup cycle does O(nfds) work even if only one fd
woke us. Consider a design where registrations persist across wakeups.

**Severity:** Low-Medium. Affects latency on spurious wakeups.

### P3. `sys_munmap` -- Per-page COW check with pagedb lookup

**File:** `memory.rs:272-296`

For each unmapped page, sys_munmap does: `unmap_user_page` + `try_pagedb()` +
`pagedb.get(phys)` + `lru_remove` + `cow.decrement` + `free_frame`. That's
5-6 operations per page. For a 1MB munmap (256 pages), that's ~1500 function
calls. Consider batching: collect all physical frames first, then batch-process
COW decrements and frees.

**Severity:** Medium. Large munmap operations are slow.

### P4. `sys_brk` -- Removes and re-adds heap VMA on every call

**File:** `memory.rs:530-539`

Every brk() call removes the entire heap VMA and creates a new one, even for
small expansions. This is O(n) in the VMA list size. For programs that use brk
heavily (e.g., Python's memory allocator), this adds unnecessary overhead.

**Severity:** Low. brk() is infrequent in practice (mmap dominates).

### P5. `sys_pread64` / `sys_pwrite64` -- Save/seek/read/restore pattern

**File:** `vfs_ext.rs:315-334`

Positional I/O is implemented as save-position, seek, read/write, restore.
This is not atomic -- a concurrent read on the same fd between seek and restore
would see the wrong position. Should use a direct offset-based read/write path
or hold the file position lock across the operation.

**Severity:** Medium. Race condition on concurrent pread + read on same fd.

### P6. `sys_sendfile` / `sys_copy_file_range` -- Heap-allocated bounce buffer

**File:** `vfs_ext.rs:486, 612`

`sys_sendfile` allocates an 8KB bounce buffer on the heap. `sys_copy_file_range`
uses a 4KB stack buffer (better). Both could use a stack buffer since the chunk
sizes are bounded. The sendfile heap allocation is particularly wasteful for
small transfers.

**Severity:** Low. One alloc per sendfile call.

---

## Per-Syscall Analysis

### Core I/O

#### sys_read (NR_READ = 0)
- **Status:** Complete
- **Correctness:** Proper user buffer validation (null, overflow, kernel-space check). Delegates to VFS path which uses kernel bounce buffer to avoid nested STAC/CLAC. O_NONBLOCK correctly checked before blocking.
- **Performance:** Good. Preemption enabled for blocking reads. Kernel bounce buffer adds one memcpy but prevents SMAP issues.
- **Signal handling:** VFS read path should check for pending signals in blocking loops -- depends on TTY/pipe implementation. The `signal-delivery-blocking-reads.md` rule is documented.
- **Recommendation:** Solid implementation. No changes needed.

#### sys_write (NR_WRITE = 1)
- **Status:** Complete
- **Correctness:** Proper validation. Falls back to console_write callback for stdout/stderr when no fd table entry exists (early boot). Preemption enabled to prevent deadlock with TERMINAL lock holder.
- **Performance:** Good. Kernel bounce buffer used to avoid SMAP nesting.
- **Recommendation:** The fallback path creates user-space slice directly (`core::slice::from_raw_parts`) -- this is safe because STAC is active, but would be cleaner using the bounce buffer pattern.

#### sys_open (NR_OPEN = 2)
- **Status:** Complete
- **Correctness:** Path copied to kernel-owned String (TOCTOU closed). O_CREAT, O_EXCL, O_TRUNC, O_APPEND, O_NONBLOCK handled. Preemption enabled for VFS lookups that may trigger block I/O.
- **Performance:** One heap allocation for the kernel path String. Unavoidable.
- **Recommendation:** Good implementation.

#### sys_close (NR_CLOSE = 3)
- **Status:** Complete
- **Correctness:** Delegates to VFS fd_table.close(). Socket fds handled separately via is_socket_fd check. Flock advisory locks auto-released on close.
- **Recommendation:** Good.

#### sys_stat / sys_fstat / sys_lstat (NR 4, 5, 6)
- **Status:** Complete
- **Correctness:** Proper SMAP brackets, user buffer validation, path resolution. lstat follows symlinks correctly (via VFS).
- **Recommendation:** Good.

#### sys_poll (NR_POLL = 7)
- **Status:** Complete
- **Correctness:** Proper PollTable+WaitQueue pattern (Linux-like poll_wait). Three-phase check: optimistic, post-registration, post-wake. Signal checks in blocking loop return EINTR. Timeout handling correct with tick-based deadline.
- **Performance:** Vec heap allocation per call (see P1). Re-registration after each wake (see P2). write_pollfds_back uses volatile writes per-element instead of memcpy.
- **Safety:** Proper SMAP brackets for userspace pollfd read/write.
- **Missing:** nfds > 1024 rejected (matches Linux RLIMIT_NOFILE behavior).
- **Recommendation:** Add stack-local fast path for small nfds.

#### sys_lseek (NR_LSEEK = 8)
- **Status:** Complete
- **Correctness:** Delegates to VFS File::seek(). SEEK_SET, SEEK_CUR, SEEK_END supported.
- **Recommendation:** Good.

### Memory Mapping

#### sys_mmap (NR_MMAP = 9)
- **Status:** Complete
- **Correctness:** Demand paging for MAP_ANONYMOUS+MAP_PRIVATE (good -- saves physical memory). MAP_FIXED properly unmaps existing VMAs. VMA-aware gap finder with bump-allocator fallback. File-backed mmap reads file data eagerly.
- **Performance:** Good. Demand paging avoids eager allocation. Per-process mmap hint address avoids global lock contention.
- **Missing:** MAP_SHARED semantics incomplete -- writes to shared mappings don't sync back to file (no msync). Comment acknowledges this.
- **Correctness issue:** File-backed mmap creates a user-space slice directly for the read buffer (`core::slice::from_raw_parts_mut(map_addr as *mut u8, ...)`). This is inside STAC brackets, so it works, but if the VFS read path internally does CLAC, the subsequent writes will SMAP-fault.
- **Recommendation:** Consider using kernel bounce buffer for file-backed mmap population, consistent with sys_read pattern.

#### sys_munmap (NR_MUNMAP = 11)
- **Status:** Complete
- **Correctness:** Proper COW-aware unmapping. Checks pagedb before freeing to avoid double-free. LRU removal before buddy free. TLB shootdown after unmapping (critical for SMP).
- **Performance:** Per-page processing is O(n) with 5-6 ops per page (see P3).
- **Recommendation:** Consider batch processing for large unmaps.

#### sys_mprotect (NR_MPROTECT = 10)
- **Status:** Partial (see C3)
- **Correctness:** Only handles adding WRITABLE. Cannot remove permissions or set PROT_NONE. **Bug.**
- **Recommendation:** Implement full flag update including permission removal.

#### sys_mremap (NR_MREMAP = 25)
- **Status:** Partial (see C4)
- **Correctness:** Shrink case works (delegates to munmap). Grow-in-place attempts MAP_FIXED extension. Move case copies data but loses protection flags. **Bug.**
- **Missing:** MREMAP_FIXED not implemented (new_addr parameter is `_new_addr`).
- **Recommendation:** Preserve original protection flags, implement MREMAP_FIXED.

#### sys_brk (NR_BRK = 12)
- **Status:** Complete
- **Correctness:** Properly handles expand (allocate pages) and shrink (unmap + free + TLB shootdown). COW-aware shrink. VMA tracking updated.
- **Performance:** VMA remove+add on every call (see P4).
- **Edge case:** HEAP_START hardcoded to 0x600000 -- should match actual ELF data segment end.
- **Recommendation:** Minor -- acceptable for brk's infrequent use.

#### sys_madvise (NR_MADVISE = 28)
- **Status:** Stub (advisory no-op)
- **Correctness:** Returns 0 for all advice values. This is technically correct per POSIX (advice is optional), but MADV_DONTNEED should free pages.
- **Recommendation:** Implement MADV_DONTNEED to free pages (important for malloc arenas).

### Signals

#### sys_kill (NR_KILL = 62)
- **Status:** Complete
- **Correctness:** Signal 0 (null signal) properly handled. Permission checks via `can_signal()`. Group kill (pid=0), broadcast (pid=-1), process group (pid<-1) all implemented. Target process woken after signal delivery.
- **Missing:** POSIX says broadcast should skip init (pid 1) AND self. Code skips both -- correct.
- **Recommendation:** Good implementation.

#### sys_sigaction (NR_SIGACTION = 13)
- **Status:** Complete but **SMAP-broken** (see C1)
- **Correctness:** SIGKILL/SIGSTOP correctly rejected. Old action returned. New action stored.
- **Safety:** Raw pointer dereference without STAC brackets = SMAP fault.
- **Recommendation:** Add STAC/CLAC brackets. Use `uaccess::get_user` / `uaccess::put_user`.

#### sys_sigprocmask (NR_SIGPROCMASK = 14)
- **Status:** Complete but **SMAP-broken** (see C1)
- **Correctness:** SIG_BLOCK, SIG_UNBLOCK, SIG_SETMASK all handled. SIGKILL/SIGSTOP never blocked.
- **Safety:** Same SMAP issue as sigaction.

#### sys_sigreturn (NR_SIGRETURN = 15)
- **Status:** Complete
- **Correctness:** Reads SignalFrame from user stack with SMAP brackets. Uses deferred restoration via `set_sigreturn_frame()` to avoid race with asm resave. Validates frame pointer is in userspace.
- **Recommendation:** Good -- the deferred approach is correct.

#### sys_sigsuspend (NR_SIGSUSPEND = 130)
- **Status:** Complete
- **Correctness:** Atomically swaps signal mask, blocks until deliverable signal, restores mask before returning EINTR. SIGKILL/SIGSTOP sanitized from temp mask. Uses `should_interrupt_for_signal()` to check for actually-deliverable signals (not just any pending).
- **Recommendation:** Good implementation.

#### sys_pause (NR_PAUSE = 34)
- **Status:** Complete
- **Correctness:** HLT-loops until deliverable signal. Returns EINTR.
- **Recommendation:** Good.

#### sys_sigaltstack (NR_SIGALTSTACK = 131)
- **Status:** Partial
- **Correctness:** Accepts and validates stack parameters but does not actually save them in ProcessMeta. Signal delivery does not use alternate stack. Returns success misleadingly.
- **Recommendation:** Either implement fully or return ENOSYS to avoid silent misbehavior.

#### sys_sigpending (NR_SIGPENDING = 127)
- **Status:** Complete but **SMAP-broken** (see C2)

### Time

#### sys_clock_gettime (NR_CLOCK_GETTIME = 228)
- **Status:** Complete
- **Correctness:** All clock IDs handled: REALTIME, MONOTONIC, MONOTONIC_RAW, PROCESS_CPUTIME_ID, THREAD_CPUTIME_ID, BOOTTIME, REALTIME_COARSE, MONOTONIC_COARSE. Uses TSC-based hires provider for nanosecond precision. PROCESS_CPUTIME returns actual per-process cpu_time_ns from ProcessMeta.
- **Performance:** TSC read is a single RDTSC instruction -- fast path.
- **Recommendation:** Good implementation.

#### sys_clock_getres (NR_CLOCK_GETRES = 229)
- **Status:** Complete
- **Correctness:** Reports 1ns resolution for all clocks (TSC-based). NULL res_ptr just validates clock_id.
- **Recommendation:** Good.

#### sys_gettimeofday (NR_GETTIMEOFDAY = 96)
- **Status:** Complete
- **Correctness:** Converts nanoseconds to microseconds for tv_usec. Timezone returns UTC (0 offset). Both tv_ptr and tz_ptr can be NULL independently.
- **Recommendation:** Good.

#### sys_nanosleep (NR_NANOSLEEP = 35)
- **Status:** Complete
- **Correctness:** Validates tv_nsec < 1,000,000,000. Sleep queue registration with timer ISR wakeup. Signal interruption returns EINTR with remaining time written to rem_ptr. Fatal signal delivery via `deliver_fatal_signal_now()` safety net.
- **Performance:** Uses sleep queue + block_current + HLT (zero CPU burn). Fallback to polling if queue is full (see C6).
- **Edge case:** Serial debug trace truncates PID to u8 (`pid_b = current_pid as u8`). PIDs > 255 display incorrectly in debug output. Not a functional bug.
- **Recommendation:** Increase MAX_SLEEPERS or switch to timer wheel.

#### sys_clock_nanosleep (NR_CLOCK_NANOSLEEP = 230)
- **Status:** Complete
- **Correctness:** TIMER_ABSTIME computes relative duration from current time. Relative mode delegates to sys_nanosleep. Signal check in absolute mode loop.
- **Correctness issue:** In TIMER_ABSTIME mode, signal check happens AFTER blocking+waking, not before. If a signal is pending when the syscall starts, it blocks once before checking. Minor -- one extra HLT at worst.
- **Recommendation:** Move signal check before first HLT in TIMER_ABSTIME loop.

### Poll/Select

#### sys_select (NR_SELECT = 23)
- **Status:** Complete
- **Correctness:** Full FdSet bitmap support (1024 fds). Three-phase PollTable pattern matching sys_poll. Signal check in blocking loop. Timeout handling via ticks.
- **Performance:** Iterates 0..nfds linearly even for sparse fd sets. Linux has the same behavior. FdSet::is_set is O(1) per fd (bitwise).
- **Recommendation:** Good.

#### sys_pselect6 (NR_PSELECT6 = 270)
- **Status:** Complete
- **Correctness:** Atomically swaps signal mask, does full select with PollTable pattern, restores mask on return. Properly handles all exit paths (signal, timeout, ready fds).
- **Recommendation:** Good implementation.

#### sys_ppoll (NR_PPOLL = 271)
- **Status:** Complete
- **Correctness:** Converts timespec to millisecond timeout, swaps signal mask, delegates to sys_poll, restores mask.
- **Precision loss:** Converts nanosecond timeout to millisecond. Loses sub-ms precision. Linux ppoll preserves nanosecond precision. Minor.
- **Recommendation:** Consider preserving nanosecond precision in the tick calculation.

### VFS Operations

#### sys_ioctl (NR_IOCTL = 16)
- **Status:** Complete (delegates to VFS)
- **Recommendation:** Check VFS ioctl for SMAP compliance.

#### sys_fcntl (NR_FCNTL = 72)
- **Status:** Complete (delegates to VFS)

#### sys_flock (NR_FLOCK = 73)
- **Status:** Complete (BSD advisory locking via FlockRegistry)

#### sys_pipe / sys_pipe2 (NR 22, 293)
- **Status:** Complete. pipe2 delegates to pipe (flags not meaningful yet).

#### sys_dup / sys_dup2 / sys_dup3 (NR 32, 33, 292)
- **Status:** Complete. dup3 rejects old_fd == new_fd (EINVAL per POSIX). O_CLOEXEC flag accepted but not enforced (no exec close-on-exec support).

#### sys_readv / sys_writev (NR 19, 20)
- **Status:** Complete
- **Correctness:** IoVec array copied from userspace to kernel first (avoids STAC/CLAC nesting). Short reads break loop. Error on first vec returns error; error after partial success returns partial count.
- **Recommendation:** Good.

#### sys_pread64 / sys_pwrite64 (NR 17, 18)
- **Status:** Complete
- **Correctness issue:** Not atomic -- save/seek/read/restore pattern races with concurrent operations on same fd (see P5).
- **Recommendation:** Needs atomic offset-based read/write.

#### sys_sendfile (NR_SENDFILE = 40)
- **Status:** Complete
- **Correctness:** Handles offset pointer, partial writes, error recovery. Updates offset after transfer.
- **Performance:** 8KB heap-allocated bounce buffer (see P6).
- **Recommendation:** Use stack buffer.

#### sys_getdents / sys_getdents64 (NR 78, 217)
- **Status:** Complete
- **Correctness:** linux_dirent64 format. Proper d_type mapping. Entry alignment to 8 bytes. Kernel preemption enabled during directory iteration (prevents starvation during large /proc reads).
- **Recommendation:** Good.

#### sys_fsync / sys_fdatasync (NR 74, 75)
- **Status:** Stub (verifies fd valid, returns 0)
- **Correctness:** No actual sync to disk. Acceptable for in-memory VFS.

#### sys_truncate / sys_ftruncate (NR 76, 77)
- **Status:** Complete

#### sys_statfs / sys_fstatfs (NR 137, 138)
- **Status:** Complete (returns hardcoded filesystem info)

#### sys_splice (NR_SPLICE = 275)
- **Status:** Complete
- **Correctness:** Handles WouldBlock (EAGAIN for non-blocking). Proper short read/write handling. 64KB max buffer.
- **Recommendation:** Good.

#### sys_copy_file_range (NR = 326)
- **Status:** Complete
- **Correctness:** 4KB stack buffer (good). Updates offset pointers. Handles partial reads.
- **Safety issue:** Reads offset from user pointer without validate_user_buffer for the offset pointer itself (only checks >= 0x8000_0000_0000 but not for null).
- **Recommendation:** Add full validation for offset pointers.

#### sys_close_range (NR = 436)
- **Status:** Complete
- **Correctness:** Validates first <= last. Iterates and closes.

### Directory Operations

#### sys_mkdir / sys_rmdir / sys_unlink / sys_rename (NR 83, 84, 87, 82)
- **Status:** Complete
- **Correctness:** Path copied to kernel String (TOCTOU closed). Resolved against CWD.

#### sys_link / sys_symlink / sys_readlink (NR 86, 88, 89)
- **Status:** Complete
- **Correctness:** symlink target stored as-is (not resolved). readlink validates symlink vtype. Copy length capped to bufsize.

#### sys_chdir / sys_getcwd (NR 80, 79)
- **Status:** Complete
- **Correctness:** chdir validates target is directory. getcwd copies inside meta lock (zero heap allocs).

#### sys_utimes / sys_utimensat / sys_futimens (NR 235, 280, 261)
- **Status:** Complete
- **Correctness:** UTIME_OMIT handled. Delegates to vnode.set_times().

#### *at variants (mkdirat, unlinkat, renameat, readlinkat, symlinkat, linkat)
- **Status:** Partial -- only AT_FDCWD supported. Non-AT_FDCWD returns ENOSYS.
- **Recommendation:** Implement dirfd-relative lookups for container/chroot support.

### Process Management

#### sys_fork (NR_FORK = 57)
- **Status:** Complete (delegates to callback)

#### sys_exec / sys_execve (NR 58, 59)
- **Status:** Complete
- **Correctness:** User pointer validation for path, argv, envp. SMAP enabled. FS_BASE restored on failed exec.
- **Safety:** argv and envp are passed as raw pointers from userspace to the exec callback without deep validation of the pointer arrays. Each pointer in argv[] could be kernel-space.
- **Recommendation:** Validate and copy argv/envp arrays to kernel memory before passing to exec.

#### sys_clone (NR_CLONE = 56)
- **Status:** Complete (delegates to callback)

#### sys_exit / sys_exit_group (NR 60, 231)
- **Status:** Complete
- **Correctness:** exit_group calls exit for current task. No thread group killing yet.

#### sys_wait4 / sys_waitpid (NR_WAIT4 = 61)
- **Status:** Complete
- **Correctness:** Result encoding: `(pid << 32) | status`. WNOHANG flag support. Unconditional serial trace in non-WNOHANG path (with CR3 dump) -- **this is debug code that should be gated behind a feature flag**.
- **Performance issue:** Unconditional serial writes in sys_waitpid for non-WNOHANG calls. This happens on every blocking waitpid, producing serial output that can saturate UART.
- **Recommendation:** Gate serial traces behind `#[cfg(feature = "debug-proc")]`.

#### sys_waitid (NR_WAITID = 247)
- **Status:** Implemented (converts to wait4 semantics)

#### sys_getpid / sys_getppid / sys_gettid (NR 39, 110, 186)
- **Status:** Complete
- **Correctness:** getpid returns TGID (thread group ID) per Linux semantics. gettid returns actual PID.

#### sys_setpgid / sys_getpgid / sys_setsid / sys_getsid (NR 109, 121, 112, 124)
- **Status:** Complete
- **Correctness:** setsid properly rejects if already group/session leader. Detaches controlling terminal (tty_nr = 0).

#### sys_setuid / sys_setgid / sys_seteuid / sys_setegid (NR 105, 106, 113, 114)
- **Status:** Complete
- **Correctness:** Root can set any UID/GID. Non-root restricted to real or current effective ID. POSIX-compliant permission checks.

#### sys_setreuid / sys_setregid / sys_setresuid / sys_setresgid (NR 113, 114, 117, 119)
- **Status:** Complete

#### sys_getresuid / sys_getresgid (NR 118, 120)
- **Status:** Complete (writes to user pointers)

#### sys_getgroups / sys_setgroups (NR 115, 116)
- **Status:** Implemented (supplementary groups)

### Credential and Resource

#### sys_umask (NR_UMASK = 95)
- **Status:** Complete

#### sys_prlimit (NR_PRLIMIT = 302)
- **Status:** Implemented (resource limits)

#### sys_getrusage (NR_GETRUSAGE = 98)
- **Status:** Implemented (returns zeroed rusage with cpu_time_ns populated)

### Scheduler

#### sys_sched_yield (NR_SCHED_YIELD = 24)
- **Status:** Complete

#### sys_sched_setscheduler / sys_sched_getscheduler (NR 144, 145)
- **Status:** Stub-like (accept but mostly no-op)

#### sys_sched_setaffinity / sys_sched_getaffinity (NR 203, 204)
- **Status:** Implemented

#### sys_sched_get_priority_max / min (NR 146, 147)
- **Status:** Complete (returns 99 / 1)

#### sys_nice (NR_NICE = 502) -- OXIDE-specific
- **Status:** Implemented

#### sys_getpriority / sys_setpriority (NR 140, 141)
- **Status:** Implemented

### Timer/Alarm

#### sys_alarm (NR_ALARM = 37)
- **Status:** Implemented

#### sys_setitimer / sys_getitimer (NR 38, 36)
- **Status:** Implemented

### Sockets

#### sys_socket / sys_bind / sys_listen / sys_accept / sys_connect (NR 41-43, 49-50)
- **Status:** Complete
- **Correctness:** Loopback networking with in-kernel data routing. Ephemeral port assignment. Unified socket registry. Socket FDs use separate numbering (SOCKET_FD_BASE = 1000).

#### sys_sendto / sys_recvfrom (NR 44, 45)
- **Status:** Complete
- **Correctness:** NULL addr means send/recv (aliases). TCP/IP stack integration for non-loopback.

#### sys_shutdown / sys_getsockname / sys_getpeername (NR 48, 51, 52)
- **Status:** Complete

#### sys_setsockopt / sys_getsockopt (NR 54, 55)
- **Status:** Implemented (common options: SO_REUSEADDR, SO_KEEPALIVE, TCP_NODELAY, etc.)

#### sys_socketpair (NR_SOCKETPAIR = 53)
- **Status:** Complete

#### sys_accept4 (NR_ACCEPT4 = 288)
- **Status:** Complete (flags accepted but not enforced)

### Epoll

#### sys_epoll_create1 / sys_epoll_ctl / sys_epoll_wait (NR 291, 233, 232)
- **Status:** Complete
- **Correctness:** Proper EpollNode downcast. CTL_ADD, CTL_DEL, CTL_MOD implemented. Wait returns ready events.
- **Missing:** epoll_wait timeout parameter is accepted but blocking is not implemented (returns immediately). This breaks event-driven applications expecting blocking epoll_wait.
- **Recommendation:** Implement blocking epoll_wait with WaitQueue pattern.

#### sys_epoll_pwait / sys_epoll_pwait2 (NR 281, 441)
- **Status:** Stub (ENOSYS)

### Event I/O

#### sys_eventfd2 (NR_EVENTFD2 = 290)
- **Status:** Complete

#### sys_memfd_create (NR_MEMFD_CREATE = 319)
- **Status:** Complete

#### sys_timerfd_create / settime / gettime (NR 283, 286, 287)
- **Status:** Stub (ENOSYS)

#### sys_signalfd / sys_signalfd4 (NR 282, 289)
- **Status:** Stub (ENOSYS)

#### sys_recvmmsg / sys_sendmmsg (NR 299, 307)
- **Status:** Stub (ENOSYS)

#### sys_preadv2 / sys_pwritev2 (NR 327, 328)
- **Status:** Stub (ENOSYS)

### Security

#### sys_prctl (NR_PRCTL = 157)
- **Status:** Partial
- **Correctness:** PR_SET_NAME silently succeeds without storing. PR_GET_NAME returns ENOSYS. PR_SET_NO_NEW_PRIVS accepted but not enforced.
- **Recommendation:** Implement PR_SET_NAME / PR_GET_NAME properly.

#### sys_capget / sys_capset (NR 125, 126)
- **Status:** Partial
- **Correctness:** capget returns all-capabilities (0xFFFFFFFF) for every process. capset accepts without enforcing. This means no capability restriction is possible.
- **Security:** **All processes effectively run with all capabilities.** Any process can do anything root can do capability-wise.
- **Recommendation:** Implement actual capability tracking in ProcessMeta.

### Container Primitives

#### sys_unshare / sys_setns / sys_clone3 / pidfd_* (NR 272, 308, 435, 424, 434, 438)
- **Status:** All stub (ENOSYS)
- **Recommendation:** These are prerequisites for containers. Implement when namespace infrastructure is ready.

### Filesystem Mount

#### sys_mount / sys_umount / sys_pivot_root (NR 165, 166, 155)
- **Status:** Complete (delegates to callbacks)

### Misc

#### sys_uname (NR_UNAME = 63)
- **Status:** Complete

#### sys_sysinfo (NR_SYSINFO = 99)
- **Status:** Complete

#### sys_getrandom (NR_GETRANDOM = 318)
- **Status:** Complete

#### sys_arch_prctl (NR_ARCH_PRCTL = 158)
- **Status:** Complete (ARCH_SET_FS, ARCH_GET_FS for TLS)

#### sys_futex (NR_FUTEX = 202)
- **Status:** Implemented (FUTEX_WAIT, FUTEX_WAKE)

#### sys_set_tid_address (NR = 218)
- **Status:** Complete

### Firewall (OXIDE-specific, NR 510-515)

#### sys_fw_add_rule / del / list / set_policy / flush / get_conntrack
- **Status:** Complete but SMAP-broken for add_rule and get_conntrack (see C5)
- **Correctness:** Root-only permission check. Filter rule conversion between user struct and internal representation.

### OXIDE-specific (NR 500+)

#### sys_setkeymap / sys_getkeymap (NR 500, 501)
- **Status:** Complete

#### sys_net_control (NR 520)
- **Status:** Complete (userspace DHCP trigger)

---

## User Access Safety (SMAP/SMEP) Summary

The codebase has a well-designed `uaccess.rs` module with `validate_user_buffer`,
`copy_from_user`, `copy_to_user`, `get_user<T>`, and `put_user<T>`. However,
several syscalls bypass this module and do raw pointer dereferences:

| Syscall | File | Issue |
|---------|------|-------|
| sys_sigaction | signal.rs:178-191 | Raw `*(ptr as *const SigAction)` without STAC |
| sys_sigprocmask | signal.rs:218-229 | Raw `*(ptr as *const SigSet)` without STAC |
| sys_sigpending | signal.rs:269-271 | Raw `*out = pending` without STAC |
| sys_fw_add_rule | firewall.rs:210-216 | Raw `*ptr` without STAC |
| sys_fw_get_conntrack | firewall.rs:358-364 | Raw `&mut *ptr` without STAC |
| sys_copy_file_range | vfs_ext.rs:592 | Offset ptr checked for >= 0x8000... but not for null |

**Recommendation:** Audit all raw pointer accesses and route through `uaccess` helpers.

---

## Lock Ordering and Deadlock Risk

The syscall subsystem interacts with several lock domains:

1. **ProcessMeta lock** (`meta.lock()`) -- per-process, held briefly
2. **TERMINAL lock** -- global, held during write/echo
3. **VFS locks** -- per-vnode, held during I/O
4. **Scheduler run queue locks** -- per-CPU, held during task state changes
5. **SOCKET_TABLE** / **LISTENING_SOCKETS** -- global, held during socket ops

Known safe patterns:
- `with_current_meta()` uses scheduler's `this_cpu` fast path
- `allow_kernel_preempt()` enabled before acquiring TERMINAL lock
- Timer ISR uses `try_wake_up()` (non-blocking) for sleep queue

Potential risk:
- `sys_sendfile` / `sys_splice` hold two file references simultaneously. If both
  are pipes to the same process, and that process is blocked in read, the write
  could deadlock if the pipe buffer is full. Linux handles this with
  pipe_lock ordering. OXIDE does not appear to have explicit pipe lock ordering.

---

## errno Correctness

All defined errno values match Linux x86_64 values. The module defines 38 errno
constants covering core errors (EPERM through ERANGE), device errors (ENODEV,
ENETDOWN), and socket errors (ENOTSOCK through EINPROGRESS). All are negative
as required by the syscall return convention.

**Missing errno values that may be needed:**
- ENAMETOOLONG (-36) -- for path length validation
- ELOOP (-40) -- for symlink loop detection
- EDEADLK (-35) -- for lock deadlock detection
- EWOULDBLOCK (alias for EAGAIN) -- for consistency in socket code

---

## Implementation Checklist

### Immediate (security/correctness)
- [x] C1+C2: Add STAC/CLAC brackets to sys_sigaction, sys_sigprocmask, sys_sigpending
- [x] C5: Add STAC/CLAC + address validation to firewall syscalls
- [x] C3: Implement full protection flag handling in sys_mprotect (remove perms, PROT_NONE)

### Short-term (correctness)
- [x] C4: Preserve protection flags in sys_mremap
- [x] P5: Make sys_pread64/pwrite64 atomic (direct offset read, no seek)
- [x] Gate sys_waitpid serial traces behind debug-proc feature
- [x] C6: Increase MAX_SLEEPERS from 64 to 256

### Medium-term (performance)
- [x] P1: Stack-local buffer for small poll/select fd sets (nfds <= 64)
- [x] P6: Stack buffer for sendfile (replace heap vec with [u8; 8192])
- [x] Implement MADV_DONTNEED in sys_madvise (free pages, keep VMA)

### Long-term (completeness) — deferred
- [ ] Implement blocking epoll_wait with WaitQueue pattern
- [ ] Implement *at syscall dirfd support (beyond AT_FDCWD)
- [ ] Implement actual capability tracking (capget/capset)
- [ ] Implement timerfd, signalfd
- [ ] Container syscalls (unshare, setns, clone3, pidfd)
