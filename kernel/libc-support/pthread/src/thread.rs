//! Thread management

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::ffi::{c_int, c_void};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::{pthread_attr_t, EAGAIN, EINVAL, ENOMEM, ESUCCESS, PTHREAD_CREATE_JOINABLE};

// — ThreadRogue: Raw syscall wrappers for thread creation.
// Can't use libc syscall wrappers because this is a staticlib linked
// into user binaries — we need to talk to the kernel directly.
unsafe fn syscall1(nr: u64, a1: usize) -> isize {
    let ret: isize;
    core::arch::asm!("syscall", in("rax") nr, in("rdi") a1,
        lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

unsafe fn syscall2(nr: u64, a1: usize, a2: usize) -> isize {
    let ret: isize;
    core::arch::asm!("syscall", in("rax") nr, in("rdi") a1, in("rsi") a2,
        lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

unsafe fn syscall5(nr: u64, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> isize {
    let ret: isize;
    core::arch::asm!("syscall", in("rax") nr, in("rdi") a1, in("rsi") a2,
        in("rdx") a3, in("r10") a4, in("r8") a5,
        lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

unsafe fn syscall6(nr: u64, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize, a6: usize) -> isize {
    let ret: isize;
    core::arch::asm!("syscall", in("rax") nr, in("rdi") a1, in("rsi") a2,
        in("rdx") a3, in("r10") a4, in("r8") a5, in("r9") a6,
        lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

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
    _start_routine: StartRoutine,
    _arg: *mut c_void,
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

    // — ThreadRogue: REAL thread creation via sys_clone.
    // 1. Allocate a stack (8MB default, mmap'd)
    // 2. Set up trampoline data at the top of the stack
    // 3. Call clone(CLONE_VM|CLONE_FS|CLONE_FILES|CLONE_SIGHAND|CLONE_THREAD, stack_top)
    // 4. Trampoline calls start_routine(arg), then pthread_exit(retval)

    let stack_size = if !attr.is_null() && (*attr).stacksize > 0 {
        (*attr).stacksize
    } else {
        8 * 1024 * 1024 // 8MB default
    };

    // mmap anonymous stack (syscall 9: mmap)
    // PROT_READ|PROT_WRITE = 3, MAP_PRIVATE|MAP_ANONYMOUS|MAP_STACK = 0x20022
    let stack_base_raw = syscall6(9, 0, stack_size, 3, 0x20022, usize::MAX, 0);
    let stack_base = stack_base_raw as usize;
    if stack_base_raw < 0 || stack_base == 0 {
        // mmap failed — remove thread data and return EAGAIN
        let mut threads = get_threads();
        if let Some(ref mut map) = *threads { map.remove(&tid); }
        return EAGAIN;
    }

    // Stack grows downward — top is base + size
    // Place ThreadTrampoline at top of stack (below stack pointer)
    let stack_top = stack_base + stack_size;
    let trampoline_size = core::mem::size_of::<ThreadTrampoline>();
    let trampoline_ptr = (stack_top - trampoline_size - 16) as *mut ThreadTrampoline; // 16-byte aligned
    (*trampoline_ptr).start = _start_routine;
    (*trampoline_ptr).arg = _arg;
    (*trampoline_ptr).tid = tid;

    // The child's stack pointer — below the trampoline data, 16-byte aligned
    let child_sp = (trampoline_ptr as usize - 8) & !0xF;

    // clone flags: CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD | CLONE_SYSVSEM
    // CLONE_VM=0x100, CLONE_FS=0x200, CLONE_FILES=0x400, CLONE_SIGHAND=0x800
    // CLONE_THREAD=0x10000, CLONE_SYSVSEM=0x40000
    let clone_flags: usize = 0x100 | 0x200 | 0x400 | 0x800 | 0x10000 | 0x40000;

    // sys_clone(flags, stack, parent_tid, child_tid, tls)
    // syscall 56: clone
    let child_pid = syscall5(56, clone_flags, child_sp, 0, 0, 0) as i64;

    if child_pid < 0 {
        // clone failed
        let _ = syscall2(11, stack_base, stack_size); // munmap
        let mut threads = get_threads();
        if let Some(ref mut map) = *threads { map.remove(&tid); }
        return EAGAIN;
    }

    if child_pid == 0 {
        // — ThreadRogue: We are the child thread. Run the user's function.
        CURRENT_THREAD.store(tid, Ordering::SeqCst);
        let trampoline = &*trampoline_ptr;
        let retval = (trampoline.start)(trampoline.arg);

        // Mark thread as finished
        {
            let threads = get_threads();
            if let Some(ref map) = *threads {
                if let Some(data) = map.get(&tid) {
                    let mut d = data.lock();
                    d.state = ThreadState::Finished;
                    d.retval = retval;
                }
            }
        }

        // Exit thread (not process) — syscall 60 with just this thread
        let _ = syscall1(60, 0);
        loop { core::hint::spin_loop(); }
    }

    // Parent: child_pid > 0
    *thread = tid;
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

/// Yield execution — real syscall to scheduler
/// — ThreadRogue: syscall 24 = sched_yield. Gives up the CPU to other threads.
#[no_mangle]
pub extern "C" fn sched_yield() -> c_int {
    unsafe { syscall1(24, 0); }
    ESUCCESS
}
