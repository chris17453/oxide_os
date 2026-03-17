//! Poll/Select system calls
//!
//! — GraveShift: NOW WITH REAL WAIT QUEUES. Previous implementation yield-looped
//! at 100Hz timer tick rate — every polling process burned a scheduler pick per
//! tick. With 6 daemons × 4 CPUs, that's 600 spurious wakeups/second.
//!
//! Now uses PollTable + WaitQueue: poll/select registers on each fd's WaitQueue,
//! blocks properly (TASK_INTERRUPTIBLE), and gets woken by the fd driver when
//! data actually arrives. Zero CPU while waiting. Like Linux's poll_wait().

use crate::errno;
use crate::socket;
use crate::time::{self, Timespec};
use crate::with_current_meta;
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use waitqueue::PollTable;

/// Poll event flags (POSIX)
pub mod events {
    /// Data available for reading
    pub const POLLIN: i16 = 0x0001;
    /// Urgent data available
    pub const POLLPRI: i16 = 0x0002;
    /// Writing possible
    pub const POLLOUT: i16 = 0x0004;
    /// Error condition
    pub const POLLERR: i16 = 0x0008;
    /// Hang up
    pub const POLLHUP: i16 = 0x0010;
    /// Invalid request
    pub const POLLNVAL: i16 = 0x0020;
    /// Normal data readable
    pub const POLLRDNORM: i16 = 0x0040;
    /// Priority data readable
    pub const POLLRDBAND: i16 = 0x0080;
    /// Writing normal data possible
    pub const POLLWRNORM: i16 = 0x0100;
    /// Writing priority data possible
    pub const POLLWRBAND: i16 = 0x0200;
}

/// Poll file descriptor structure
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PollFd {
    /// File descriptor
    pub fd: i32,
    /// Requested events
    pub events: i16,
    /// Returned events
    pub revents: i16,
}

/// Timer frequency in Hz (must match time.rs)
const TIMER_HZ: u64 = 100;
const NS_PER_TICK: u64 = 1_000_000_000 / TIMER_HZ;

/// — GraveShift: routed through os_core tick bridge instead of direct arch call.
fn get_ticks() -> u64 {
    os_core::ticks()
}

/// Check if a file descriptor is ready for the requested operations
fn check_fd_ready(fd: i32, events: i16) -> i16 {
    // First check if this is a socket fd
    if socket::is_socket_fd(fd) {
        return check_socket_ready(fd, events);
    }

    // Regular file descriptor through VFS
    let file =
        match with_current_meta(|meta| meta.fd_table.get(fd).map(|fd_entry| fd_entry.file.clone()))
        {
            Some(Ok(f)) => f,
            Some(Err(_)) => return events::POLLNVAL,
            None => return events::POLLNVAL,
        };

    let mut revents: i16 = 0;

    // — GraveShift: Check readability via file.can_read() which delegates to
    // vnode.poll_read_ready(). For TTYs this drains the IRQ ring buffer into
    // the line discipline (processing Ctrl+C along the way). The old FIONREAD
    // ioctl path bypassed this drain — keystrokes rotted in the ring buffer
    // and signals never fired. Never again.
    if events & (events::POLLIN | events::POLLRDNORM) != 0 {
        if file.can_read() {
            revents |= events::POLLIN | events::POLLRDNORM;
        }
    }

    // Check for writability
    if events & (events::POLLOUT | events::POLLWRNORM) != 0 {
        // Regular files and TTYs are always writable if opened for writing
        if file.can_write() {
            revents |= events::POLLOUT | events::POLLWRNORM;
        }
    }

    revents
}

/// Register a file descriptor on a PollTable for event-driven wake.
/// — SableWire: Called during the second pass of poll/select. The file's
/// vnode registers on its WaitQueue via poll_register_wait(). When data
/// arrives, the driver calls wq.wake_all() which wakes our blocked process.
fn register_fd_wait(fd: i32, table: &mut PollTable) {
    // — SableWire: Sockets don't have VnodeOps-based wait queues yet.
    // They still use the fallback HLT path. TODO: socket WaitQueues.
    if socket::is_socket_fd(fd) {
        return;
    }

    let file = match with_current_meta(|meta| {
        meta.fd_table.get(fd).map(|fd_entry| fd_entry.file.clone())
    }) {
        Some(Ok(f)) => f,
        _ => return,
    };

    file.poll_register_wait(table);
}

/// Check if a socket fd is ready for the requested operations
fn check_socket_ready(fd: i32, events: i16) -> i16 {
    let mut revents: i16 = 0;

    // Get the socket state
    let socket_info = socket::get_socket_info(fd);
    if socket_info.is_none() {
        return events::POLLNVAL;
    }

    let (is_connected, has_data, can_send, is_listening, has_pending_connection) =
        socket_info.unwrap();

    // Check for readability
    if events & (events::POLLIN | events::POLLRDNORM) != 0 {
        if has_data || (is_listening && has_pending_connection) {
            revents |= events::POLLIN | events::POLLRDNORM;
        }
    }

    // Check for writability
    if events & (events::POLLOUT | events::POLLWRNORM) != 0 {
        if is_connected && can_send {
            revents |= events::POLLOUT | events::POLLWRNORM;
        }
    }

    // Check for errors/hangup
    if !is_connected && !is_listening {
        revents |= events::POLLHUP;
    }

    revents
}

/// sys_poll - Wait for events on file descriptors
///
/// — GraveShift: Now uses PollTable pattern (like Linux's poll_wait):
/// 1. First pass: check all fds — if any ready, return immediately
/// 2. Second pass: register on each fd's WaitQueue via poll_register_wait
/// 3. Re-check (data may have arrived between step 1 and 2)
/// 4. Block (TASK_INTERRUPTIBLE) — fd driver wakes us on event
/// 5. Unregister all, re-check, return
pub fn sys_poll(fds_ptr: usize, nfds: usize, timeout_ms: i32) -> i64 {
    if fds_ptr == 0 && nfds > 0 {
        return errno::EFAULT;
    }

    if nfds > 1024 {
        return errno::EINVAL;
    }

    // — TorqueJax: Stack-local fast path for the common case. 64 pollfds
    // covers every daemon and shell in existence without touching the heap
    // allocator. The hot path (poll with 1-3 fds) now costs zero alloc cycles.
    // Heap fallback kicks in for the rare beast polling >64 fds at once.
    const STACK_LIMIT: usize = 64;
    let mut stack_buf: [MaybeUninit<PollFd>; STACK_LIMIT] =
        unsafe { MaybeUninit::uninit().assume_init() };
    let mut heap_buf: Vec<PollFd>;

    let fds: &mut [PollFd] = if nfds <= STACK_LIMIT {
        // — TorqueJax: Zero-alloc path. Initialize from userspace directly
        // into stack memory. No heap, no allocator lock, no fragmentation.
        unsafe {
            os_core::user_access_begin();
            let ptr = fds_ptr as *const PollFd;
            for i in 0..nfds {
                stack_buf[i].write(core::ptr::read_volatile(ptr.add(i)));
            }
            os_core::user_access_end();
            // — WireSaint: Safe because we just initialized [0..nfds] above.
            core::slice::from_raw_parts_mut(
                stack_buf.as_mut_ptr() as *mut PollFd,
                nfds,
            )
        }
    } else {
        // — TorqueJax: Heap fallback for the heavy hitters. >64 fds means
        // you're running an event loop server — you can afford one alloc.
        heap_buf = Vec::with_capacity(nfds);
        unsafe {
            os_core::user_access_begin();
            let ptr = fds_ptr as *const PollFd;
            for i in 0..nfds {
                heap_buf.push(core::ptr::read_volatile(ptr.add(i)));
            }
            os_core::user_access_end();
        }
        &mut heap_buf[..]
    };

    // Calculate deadline
    let start_ticks = get_ticks();
    let deadline_ticks = if timeout_ms < 0 {
        u64::MAX // Infinite
    } else if timeout_ms == 0 {
        start_ticks // Return immediately
    } else {
        let timeout_ns = (timeout_ms as u64) * 1_000_000;
        let timeout_ticks = (timeout_ns + NS_PER_TICK - 1) / NS_PER_TICK;
        start_ticks + timeout_ticks
    };

    // Get current PID for PollTable
    let pid = sched::current_pid().unwrap_or(0);

    // — GraveShift: First pass — check all fds without registering.
    // If anything is already ready, skip the registration dance entirely.
    let mut ready_count = check_all_pollfds(fds);
    if ready_count > 0 || timeout_ms == 0 {
        write_pollfds_back(fds_ptr, fds);
        return ready_count;
    }

    // — GraveShift: Second pass — register on WaitQueues via PollTable.
    // After this, any fd event will wake us via sched::try_wake_up.
    let mut poll_table = PollTable::new(pid);
    for pollfd in fds.iter() {
        if pollfd.fd >= 0 {
            register_fd_wait(pollfd.fd, &mut poll_table);
        }
    }

    // — GraveShift: Re-check after registration (lost-wake window).
    // Data may have arrived between the first check and registration.
    ready_count = check_all_pollfds(fds);
    if ready_count > 0 {
        poll_table.unregister_all();
        write_pollfds_back(fds_ptr, fds);
        return ready_count;
    }

    // — GraveShift: Main wait loop. Strategy depends on timeout:
    // - Infinite timeout (-1): block_current (dequeue from CFS) — zero CPU
    //   until WaitQueue wake. This is the Linux poll_schedule_timeout(NULL) path.
    // - Finite timeout: yield_current (stay in CFS) — timer ticks naturally
    //   re-schedule us at 100Hz to check the deadline. block_current with a
    //   finite timeout is BROKEN: nobody wakes a dequeued INTERRUPTIBLE task
    //   when the deadline expires, so the timeout never fires. This was the
    //   "top only refreshes on keypress" bug. — GraveShift
    let use_full_block = timeout_ms < 0; // infinite timeout = safe to fully block

    loop {
        // Check for signals before blocking
        if with_current_meta(|meta| meta.has_pending_signals()).unwrap_or(false) {
            poll_table.unregister_all();
            return errno::EINTR;
        }

        // Check timeout
        if get_ticks() >= deadline_ticks {
            poll_table.unregister_all();
            // Write back zero-revents results
            for pollfd in fds.iter_mut() {
                pollfd.revents = 0;
            }
            write_pollfds_back(fds_ptr, fds);
            return 0;
        }

        // — GraveShift: Wait for next event or timer tick.
        // Infinite: dequeue from CFS, HLT — woken only by WaitQueue wake_all.
        // Finite: stay in CFS, yield + HLT — timer tick re-schedules us at
        // 100Hz to check deadline. ~10μs scheduler overhead per tick, but the
        // timeout actually works. Linux uses hrtimer callbacks for this, but
        // yield-at-100Hz is perfectly fine for decisecond-granularity poll. — GraveShift
        if use_full_block {
            sched::block_current(sched_traits::TaskState::TASK_INTERRUPTIBLE);
        } else {
            sched::yield_current();
        }
        os_core::allow_kernel_preempt();
        os_core::wait_for_interrupt();
        os_core::disallow_kernel_preempt();

        // — GraveShift: Woken up. Check fds again.
        ready_count = check_all_pollfds(fds);
        if ready_count > 0 {
            poll_table.unregister_all();
            write_pollfds_back(fds_ptr, fds);
            return ready_count;
        }

        // — GraveShift: Not ready yet. Re-register on WaitQueues before next wait.
        // For infinite timeout: wake_all() already cleared our slots — we MUST
        // re-register or nobody will ever wake us again.
        // For finite timeout: re-register is still correct — if data arrives
        // between now and the next yield, the WaitQueue wake sets us runnable
        // immediately instead of waiting for the next timer tick. — GraveShift
        poll_table.unregister_all(); // clean slate
        for pollfd in fds.iter() {
            if pollfd.fd >= 0 {
                register_fd_wait(pollfd.fd, &mut poll_table);
            }
        }
    }
}

/// Check all pollfds and return count of ready fds.
/// — GraveShift: Factored out because we call this 3 times in the poll path:
/// first pass (optimistic), after registration (lost-wake check), after wake.
fn check_all_pollfds(fds: &mut [PollFd]) -> i64 {
    let mut ready_count = 0i64;
    for pollfd in fds.iter_mut() {
        pollfd.revents = 0;
        if pollfd.fd < 0 {
            continue;
        }
        let revents = check_fd_ready(pollfd.fd, pollfd.events);
        pollfd.revents = revents;
        if revents != 0 {
            ready_count += 1;
        }
    }
    ready_count
}

/// Write pollfd results back to userspace.
fn write_pollfds_back(fds_ptr: usize, fds: &[PollFd]) {
    unsafe {
        os_core::user_access_begin();
        let ptr = fds_ptr as *mut PollFd;
        for (i, pollfd) in fds.iter().enumerate() {
            core::ptr::write_volatile(ptr.add(i), *pollfd);
        }
        os_core::user_access_end();
    }
}

/// sys_ppoll - Poll with nanosecond timeout and signal mask
pub fn sys_ppoll(fds_ptr: usize, nfds: usize, timeout_ptr: usize, sigmask_ptr: usize) -> i64 {
    let timeout_ms = if timeout_ptr == 0 {
        -1
    } else {
        let ts: Timespec = unsafe {
            os_core::user_access_begin();
            let tp = timeout_ptr as *const Timespec;
            let val = core::ptr::read_volatile(tp);
            os_core::user_access_end();
            val
        };

        if ts.tv_sec < 0 || ts.tv_nsec < 0 {
            return errno::EINVAL;
        }

        let ms = (ts.tv_sec as i64)
            .saturating_mul(1000)
            .saturating_add(ts.tv_nsec / 1_000_000);

        if ms > i32::MAX as i64 {
            i32::MAX
        } else {
            ms as i32
        }
    };

    // Apply signal mask if provided
    let mut old_mask: Option<signal::SigSet> = None;
    if sigmask_ptr != 0 {
        if let Some(sigset) = crate::signal::read_sigset(sigmask_ptr) {
            old_mask = Some(crate::signal::swap_signal_mask(sigset));
        }
    }

    let ret = sys_poll(fds_ptr, nfds, timeout_ms);

    if let Some(mask) = old_mask {
        crate::signal::set_signal_mask(mask);
    }

    ret
}

/// FD set for select() - bitmap of file descriptors
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FdSet {
    /// Bitmap of file descriptors (supports up to 1024 fds)
    pub fds_bits: [u64; 16],
}

impl FdSet {
    pub const fn new() -> Self {
        FdSet { fds_bits: [0; 16] }
    }

    pub fn is_set(&self, fd: i32) -> bool {
        if fd < 0 || fd >= 1024 {
            return false;
        }
        let idx = (fd / 64) as usize;
        let bit = (fd % 64) as u64;
        (self.fds_bits[idx] & (1 << bit)) != 0
    }

    pub fn set(&mut self, fd: i32) {
        if fd >= 0 && fd < 1024 {
            let idx = (fd / 64) as usize;
            let bit = (fd % 64) as u64;
            self.fds_bits[idx] |= 1 << bit;
        }
    }

    pub fn clear(&mut self, fd: i32) {
        if fd >= 0 && fd < 1024 {
            let idx = (fd / 64) as usize;
            let bit = (fd % 64) as u64;
            self.fds_bits[idx] &= !(1 << bit);
        }
    }

    pub fn zero(&mut self) {
        self.fds_bits = [0; 16];
    }
}

impl Default for FdSet {
    fn default() -> Self {
        Self::new()
    }
}

/// sys_select - Synchronous I/O multiplexing (legacy interface)
///
/// — GraveShift: Now uses PollTable + WaitQueue pattern. Same as sys_poll
/// but with the FdSet bitmap interface. No more yield+HLT spinning.
pub fn sys_select(
    nfds: i32,
    readfds_ptr: usize,
    writefds_ptr: usize,
    exceptfds_ptr: usize,
    timeout_ptr: usize,
) -> i64 {
    if nfds < 0 || nfds > 1024 {
        return errno::EINVAL;
    }

    // Read fd sets from userspace
    let mut readfds = FdSet::new();
    let mut writefds = FdSet::new();
    let mut exceptfds = FdSet::new();

    unsafe {
        os_core::user_access_begin();
        if readfds_ptr != 0 {
            readfds = core::ptr::read_volatile(readfds_ptr as *const FdSet);
        }
        if writefds_ptr != 0 {
            writefds = core::ptr::read_volatile(writefds_ptr as *const FdSet);
        }
        if exceptfds_ptr != 0 {
            exceptfds = core::ptr::read_volatile(exceptfds_ptr as *const FdSet);
        }
        os_core::user_access_end();
    }

    // Read timeout
    let timeout_ms = if timeout_ptr == 0 {
        -1i32
    } else {
        unsafe {
            os_core::user_access_begin();
            let tv = core::ptr::read_volatile(timeout_ptr as *const time::Timeval);
            os_core::user_access_end();

            if tv.tv_sec < 0 || tv.tv_usec < 0 {
                return errno::EINVAL;
            }

            let ms = (tv.tv_sec as i64)
                .saturating_mul(1000)
                .saturating_add(tv.tv_usec / 1000);

            if ms > i32::MAX as i64 {
                i32::MAX
            } else {
                ms as i32
            }
        }
    };

    // Calculate deadline
    let start_ticks = get_ticks();
    let deadline_ticks = if timeout_ms < 0 {
        u64::MAX
    } else if timeout_ms == 0 {
        start_ticks
    } else {
        let timeout_ns = (timeout_ms as u64) * 1_000_000;
        let timeout_ticks = (timeout_ns + NS_PER_TICK - 1) / NS_PER_TICK;
        start_ticks + timeout_ticks
    };

    let pid = sched::current_pid().unwrap_or(0);

    // — GraveShift: First pass — check all fds
    let (ready_count, result_read, result_write, result_except) =
        check_select_fds(nfds, &readfds, &writefds, &exceptfds);

    if ready_count > 0 || timeout_ms == 0 {
        write_select_back(readfds_ptr, writefds_ptr, exceptfds_ptr,
                         &result_read, &result_write, &result_except);
        return ready_count;
    }

    // — GraveShift: Register on WaitQueues
    let mut poll_table = PollTable::new(pid);
    for fd in 0..nfds {
        if readfds.is_set(fd) || writefds.is_set(fd) || exceptfds.is_set(fd) {
            register_fd_wait(fd, &mut poll_table);
        }
    }

    // Re-check after registration
    let (ready_count, result_read, result_write, result_except) =
        check_select_fds(nfds, &readfds, &writefds, &exceptfds);
    if ready_count > 0 {
        poll_table.unregister_all();
        write_select_back(readfds_ptr, writefds_ptr, exceptfds_ptr,
                         &result_read, &result_write, &result_except);
        return ready_count;
    }

    // — GraveShift: Same timeout strategy as sys_poll — infinite timeout uses
    // block_current (zero CPU), finite uses yield_current (timer-driven wakeup).
    let use_full_block = timeout_ms < 0;

    // Main wait loop
    loop {
        if with_current_meta(|meta| meta.has_pending_signals()).unwrap_or(false) {
            poll_table.unregister_all();
            return errno::EINTR;
        }

        if get_ticks() >= deadline_ticks {
            poll_table.unregister_all();
            let empty = FdSet::new();
            write_select_back(readfds_ptr, writefds_ptr, exceptfds_ptr,
                             &empty, &empty, &empty);
            return 0;
        }

        if use_full_block {
            sched::block_current(sched_traits::TaskState::TASK_INTERRUPTIBLE);
        } else {
            sched::yield_current();
        }
        os_core::allow_kernel_preempt();
        os_core::wait_for_interrupt();
        os_core::disallow_kernel_preempt();

        let (ready_count, result_read, result_write, result_except) =
            check_select_fds(nfds, &readfds, &writefds, &exceptfds);
        if ready_count > 0 || get_ticks() >= deadline_ticks {
            poll_table.unregister_all();
            write_select_back(readfds_ptr, writefds_ptr, exceptfds_ptr,
                             &result_read, &result_write, &result_except);
            return ready_count;
        }

        // — GraveShift: Re-register. wake_all() cleared our slots.
        poll_table.unregister_all();
        for fd in 0..nfds {
            if readfds.is_set(fd) || writefds.is_set(fd) || exceptfds.is_set(fd) {
                register_fd_wait(fd, &mut poll_table);
            }
        }
    }
}

/// Check all select fds and return results.
fn check_select_fds(
    nfds: i32,
    readfds: &FdSet,
    writefds: &FdSet,
    exceptfds: &FdSet,
) -> (i64, FdSet, FdSet, FdSet) {
    let mut ready_count = 0i64;
    let mut result_read = FdSet::new();
    let mut result_write = FdSet::new();
    let mut result_except = FdSet::new();

    for fd in 0..nfds {
        let in_read = readfds.is_set(fd);
        let in_write = writefds.is_set(fd);
        let in_except = exceptfds.is_set(fd);

        if !in_read && !in_write && !in_except {
            continue;
        }

        let mut poll_events: i16 = 0;
        if in_read { poll_events |= events::POLLIN; }
        if in_write { poll_events |= events::POLLOUT; }
        if in_except { poll_events |= events::POLLPRI; }

        let revents = check_fd_ready(fd, poll_events);

        if revents & events::POLLNVAL != 0 {
            if in_except {
                result_except.set(fd);
                ready_count += 1;
            }
        } else {
            if in_read && (revents & (events::POLLIN | events::POLLHUP | events::POLLERR) != 0) {
                result_read.set(fd);
                ready_count += 1;
            }
            if in_write && (revents & (events::POLLOUT | events::POLLERR) != 0) {
                result_write.set(fd);
                ready_count += 1;
            }
            if in_except && (revents & events::POLLPRI != 0) {
                result_except.set(fd);
                ready_count += 1;
            }
        }
    }

    (ready_count, result_read, result_write, result_except)
}

/// Write select results back to userspace.
fn write_select_back(
    readfds_ptr: usize,
    writefds_ptr: usize,
    exceptfds_ptr: usize,
    result_read: &FdSet,
    result_write: &FdSet,
    result_except: &FdSet,
) {
    unsafe {
        os_core::user_access_begin();
        if readfds_ptr != 0 {
            core::ptr::write_volatile(readfds_ptr as *mut FdSet, *result_read);
        }
        if writefds_ptr != 0 {
            core::ptr::write_volatile(writefds_ptr as *mut FdSet, *result_write);
        }
        if exceptfds_ptr != 0 {
            core::ptr::write_volatile(exceptfds_ptr as *mut FdSet, *result_except);
        }
        os_core::user_access_end();
    }
}

/// sys_pselect6 - Select with nanosecond timeout and signal mask
pub fn sys_pselect6(
    nfds: i32,
    readfds_ptr: usize,
    writefds_ptr: usize,
    exceptfds_ptr: usize,
    timeout_ptr: usize,
    sigmask_ptr: usize,
) -> i64 {
    // Apply signal mask if provided
    let mut old_mask: Option<signal::SigSet> = None;
    if sigmask_ptr != 0 {
        if let Some(sigset) = crate::signal::read_sigset(sigmask_ptr) {
            old_mask = Some(crate::signal::swap_signal_mask(sigset));
        }
    }

    // Convert timespec to timeval-style timeout for select
    if timeout_ptr == 0 {
        let ret = sys_select(nfds, readfds_ptr, writefds_ptr, exceptfds_ptr, 0);
        if let Some(mask) = old_mask {
            crate::signal::set_signal_mask(mask);
        }
        return ret;
    }

    let ts: Timespec = unsafe {
        os_core::user_access_begin();
        let tp = timeout_ptr as *const Timespec;
        let val = core::ptr::read_volatile(tp);
        os_core::user_access_end();
        val
    };

    let timeout_ms = if ts.tv_sec < 0 {
        -1i32
    } else {
        let ms = (ts.tv_sec as i64)
            .saturating_mul(1000)
            .saturating_add(ts.tv_nsec / 1_000_000);
        if ms > i32::MAX as i64 {
            i32::MAX
        } else {
            ms as i32
        }
    };

    // — GraveShift: pselect6 inline implementation using the same PollTable
    // pattern as sys_select. Signal mask is swapped for the duration.

    if nfds < 0 || nfds > 1024 {
        if let Some(mask) = old_mask {
            crate::signal::set_signal_mask(mask);
        }
        return errno::EINVAL;
    }

    let mut readfds = FdSet::new();
    let mut writefds = FdSet::new();
    let mut exceptfds = FdSet::new();

    unsafe {
        os_core::user_access_begin();
        if readfds_ptr != 0 {
            readfds = core::ptr::read_volatile(readfds_ptr as *const FdSet);
        }
        if writefds_ptr != 0 {
            writefds = core::ptr::read_volatile(writefds_ptr as *const FdSet);
        }
        if exceptfds_ptr != 0 {
            exceptfds = core::ptr::read_volatile(exceptfds_ptr as *const FdSet);
        }
        os_core::user_access_end();
    }

    let start_ticks = get_ticks();
    let deadline_ticks = if timeout_ms < 0 {
        u64::MAX
    } else if timeout_ms == 0 {
        start_ticks
    } else {
        let timeout_ns = (timeout_ms as u64) * 1_000_000;
        let timeout_ticks = (timeout_ns + NS_PER_TICK - 1) / NS_PER_TICK;
        start_ticks + timeout_ticks
    };

    let pid = sched::current_pid().unwrap_or(0);

    // First pass
    let (ready_count, result_read, result_write, result_except) =
        check_select_fds(nfds, &readfds, &writefds, &exceptfds);

    if ready_count > 0 || timeout_ms == 0 {
        write_select_back(readfds_ptr, writefds_ptr, exceptfds_ptr,
                         &result_read, &result_write, &result_except);
        if let Some(mask) = old_mask {
            crate::signal::set_signal_mask(mask);
        }
        return ready_count;
    }

    // Register on WaitQueues
    let mut poll_table = PollTable::new(pid);
    for fd in 0..nfds {
        if readfds.is_set(fd) || writefds.is_set(fd) || exceptfds.is_set(fd) {
            register_fd_wait(fd, &mut poll_table);
        }
    }

    // Re-check after registration
    let (ready_count, result_read, result_write, result_except) =
        check_select_fds(nfds, &readfds, &writefds, &exceptfds);
    if ready_count > 0 {
        poll_table.unregister_all();
        write_select_back(readfds_ptr, writefds_ptr, exceptfds_ptr,
                         &result_read, &result_write, &result_except);
        if let Some(mask) = old_mask {
            crate::signal::set_signal_mask(mask);
        }
        return ready_count;
    }

    // — GraveShift: Same timeout strategy — infinite blocks fully, finite yields.
    let use_full_block_ps = timeout_ms < 0;

    loop {
        if with_current_meta(|meta| meta.has_pending_signals()).unwrap_or(false) {
            poll_table.unregister_all();
            if let Some(mask) = old_mask {
                crate::signal::set_signal_mask(mask);
            }
            return errno::EINTR;
        }

        if get_ticks() >= deadline_ticks {
            poll_table.unregister_all();
            let empty = FdSet::new();
            write_select_back(readfds_ptr, writefds_ptr, exceptfds_ptr,
                             &empty, &empty, &empty);
            if let Some(mask) = old_mask {
                crate::signal::set_signal_mask(mask);
            }
            return 0;
        }

        if use_full_block_ps {
            sched::block_current(sched_traits::TaskState::TASK_INTERRUPTIBLE);
        } else {
            sched::yield_current();
        }
        os_core::allow_kernel_preempt();
        os_core::wait_for_interrupt();
        os_core::disallow_kernel_preempt();

        let (ready_count, result_read, result_write, result_except) =
            check_select_fds(nfds, &readfds, &writefds, &exceptfds);
        if ready_count > 0 || get_ticks() >= deadline_ticks {
            poll_table.unregister_all();
            write_select_back(readfds_ptr, writefds_ptr, exceptfds_ptr,
                             &result_read, &result_write, &result_except);
            if let Some(mask) = old_mask {
                crate::signal::set_signal_mask(mask);
            }
            return ready_count;
        }

        // — GraveShift: Re-register. wake_all() cleared our slots.
        poll_table.unregister_all();
        for fd in 0..nfds {
            if readfds.is_set(fd) || writefds.is_set(fd) || exceptfds.is_set(fd) {
                register_fd_wait(fd, &mut poll_table);
            }
        }
    }
}
