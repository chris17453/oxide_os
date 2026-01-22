//! Thread management

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::ffi::{c_int, c_void};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::{pthread_attr_t, EAGAIN, EINVAL, ENOMEM, ESUCCESS, PTHREAD_CREATE_JOINABLE};

/// Thread handle
pub type pthread_t = u64;

/// Thread state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThreadState {
    Running,
    Finished,
    Detached,
}

/// Internal thread structure
struct ThreadData {
    /// Thread ID
    id: pthread_t,
    /// Thread state
    state: ThreadState,
    /// Return value
    retval: *mut c_void,
    /// Is joinable
    joinable: bool,
    /// Join waiter
    join_waiter: Option<pthread_t>,
}

// ThreadData contains raw pointers but we manage access via mutex
unsafe impl Send for ThreadData {}
unsafe impl Sync for ThreadData {}

/// Global thread registry
static THREADS: Mutex<Option<BTreeMap<pthread_t, Arc<Mutex<ThreadData>>>>> = Mutex::new(None);

/// Next thread ID
static NEXT_TID: AtomicU64 = AtomicU64::new(1);

/// Current thread ID (per-CPU, simplified to global for now)
static CURRENT_THREAD: AtomicU64 = AtomicU64::new(0);

fn get_threads() -> spin::MutexGuard<'static, Option<BTreeMap<pthread_t, Arc<Mutex<ThreadData>>>>> {
    let mut threads = THREADS.lock();
    if threads.is_none() {
        *threads = Some(BTreeMap::new());
    }
    threads
}

/// Start routine type
pub type StartRoutine = extern "C" fn(*mut c_void) -> *mut c_void;

/// Thread trampoline data
struct ThreadTrampoline {
    start: StartRoutine,
    arg: *mut c_void,
    tid: pthread_t,
}

unsafe impl Send for ThreadTrampoline {}

/// Create a new thread
///
/// # Safety
/// This function is unsafe because it deals with raw function pointers and thread management.
#[no_mangle]
pub unsafe extern "C" fn pthread_create(
    thread: *mut pthread_t,
    attr: *const pthread_attr_t,
    start_routine: StartRoutine,
    arg: *mut c_void,
) -> c_int {
    if thread.is_null() {
        return EINVAL;
    }

    // Get attributes
    let detached = if !attr.is_null() {
        (*attr).detachstate != PTHREAD_CREATE_JOINABLE
    } else {
        false
    };

    // Allocate thread ID
    let tid = NEXT_TID.fetch_add(1, Ordering::SeqCst);

    // Create thread data
    let thread_data = Arc::new(Mutex::new(ThreadData {
        id: tid,
        state: ThreadState::Running,
        retval: core::ptr::null_mut(),
        joinable: !detached,
        join_waiter: None,
    }));

    // Register thread
    {
        let mut threads = get_threads();
        if let Some(ref mut map) = *threads {
            map.insert(tid, thread_data.clone());
        }
    }

    // In a real implementation, we would create an actual kernel thread here
    // For now, we set up the data structures and rely on kernel thread creation
    // This would typically involve a syscall like:
    // syscall!(SYS_CLONE, flags, stack, parent_tid, child_tid, tls)

    *thread = tid;

    // Note: Actual thread execution would be handled by the kernel
    // The trampoline would call start_routine(arg) and then pthread_exit()

    ESUCCESS
}

/// Wait for thread termination
#[no_mangle]
pub unsafe extern "C" fn pthread_join(thread: pthread_t, retval: *mut *mut c_void) -> c_int {
    let thread_data = {
        let threads = get_threads();
        if let Some(ref map) = *threads {
            map.get(&thread).cloned()
        } else {
            None
        }
    };

    let data = match thread_data {
        Some(d) => d,
        None => return EINVAL,
    };

    // Check if joinable
    {
        let d = data.lock();
        if !d.joinable {
            return EINVAL;
        }
        if d.state == ThreadState::Detached {
            return EINVAL;
        }
    }

    // Wait for thread to finish
    // In a real implementation, this would block using futex or similar
    loop {
        let d = data.lock();
        if d.state == ThreadState::Finished {
            if !retval.is_null() {
                *retval = d.retval;
            }
            break;
        }
        drop(d);
        // Yield to other threads
        // In real implementation: syscall!(SYS_SCHED_YIELD)
        core::hint::spin_loop();
    }

    // Remove thread from registry
    {
        let mut threads = get_threads();
        if let Some(ref mut map) = *threads {
            map.remove(&thread);
        }
    }

    ESUCCESS
}

/// Detach a thread
#[no_mangle]
pub unsafe extern "C" fn pthread_detach(thread: pthread_t) -> c_int {
    let thread_data = {
        let threads = get_threads();
        if let Some(ref map) = *threads {
            map.get(&thread).cloned()
        } else {
            None
        }
    };

    let data = match thread_data {
        Some(d) => d,
        None => return EINVAL,
    };

    let mut d = data.lock();
    if !d.joinable {
        return EINVAL;
    }

    d.joinable = false;
    d.state = ThreadState::Detached;

    // If already finished, clean up
    if d.state == ThreadState::Finished {
        drop(d);
        let mut threads = get_threads();
        if let Some(ref mut map) = *threads {
            map.remove(&thread);
        }
    }

    ESUCCESS
}

/// Terminate calling thread
#[no_mangle]
pub unsafe extern "C" fn pthread_exit(retval: *mut c_void) -> ! {
    let tid = CURRENT_THREAD.load(Ordering::SeqCst);

    let thread_data = {
        let threads = get_threads();
        if let Some(ref map) = *threads {
            map.get(&tid).cloned()
        } else {
            None
        }
    };

    if let Some(data) = thread_data {
        let mut d = data.lock();
        d.retval = retval;
        d.state = ThreadState::Finished;

        // Wake up joiner if any
        // In real implementation: futex wake
    }

    // In a real implementation, this would be a syscall to terminate the thread
    // syscall!(SYS_EXIT, 0)
    loop {
        core::hint::spin_loop();
    }
}

/// Get calling thread ID
#[no_mangle]
pub extern "C" fn pthread_self() -> pthread_t {
    let tid = CURRENT_THREAD.load(Ordering::SeqCst);
    if tid == 0 {
        // Main thread
        1
    } else {
        tid
    }
}

/// Compare thread IDs
#[no_mangle]
pub extern "C" fn pthread_equal(t1: pthread_t, t2: pthread_t) -> c_int {
    if t1 == t2 {
        1
    } else {
        0
    }
}

/// Yield execution
#[no_mangle]
pub extern "C" fn sched_yield() -> c_int {
    // In real implementation: syscall!(SYS_SCHED_YIELD)
    core::hint::spin_loop();
    ESUCCESS
}
