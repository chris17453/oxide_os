//! Thread support for EFFLUX OS
//!
//! Provides std::thread-like APIs using EFFLUX syscalls.

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

/// Duration type for sleep functions
pub struct Duration {
    secs: u64,
    nanos: u32,
}

impl Duration {
    /// Creates a new Duration from seconds and nanoseconds
    pub const fn new(secs: u64, nanos: u32) -> Self {
        Self { secs, nanos }
    }

    /// Creates a Duration from seconds
    pub const fn from_secs(secs: u64) -> Self {
        Self { secs, nanos: 0 }
    }

    /// Creates a Duration from milliseconds
    pub const fn from_millis(millis: u64) -> Self {
        Self {
            secs: millis / 1000,
            nanos: ((millis % 1000) * 1_000_000) as u32,
        }
    }

    /// Returns the number of whole seconds in this Duration
    pub const fn as_secs(&self) -> u64 {
        self.secs
    }

    /// Returns the fractional part of this Duration in nanoseconds
    pub const fn subsec_nanos(&self) -> u32 {
        self.nanos
    }
}

/// Thread ID type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThreadId(u32);

impl ThreadId {
    /// Returns the thread ID as a u64
    pub fn as_u64(&self) -> u64 {
        self.0 as u64
    }
}

/// Handle to a spawned thread
pub struct JoinHandle<T> {
    /// Shared state with the thread
    inner: Arc<ThreadInner<T>>,
}

/// Inner state shared between thread and handle
struct ThreadInner<T> {
    /// Thread ID (set after spawn)
    tid: AtomicU32,
    /// Thread result (set when thread completes)
    result: UnsafeCell<Option<T>>,
    /// Completion flag for futex wait
    completed: AtomicU32,
}

// Safety: ThreadInner is only accessed through atomic operations or after join
unsafe impl<T: Send> Sync for ThreadInner<T> {}
unsafe impl<T: Send> Send for ThreadInner<T> {}

impl<T> JoinHandle<T> {
    /// Wait for the thread to finish and get its result
    pub fn join(self) -> Result<T, ()> {
        // Wait for thread to complete using futex
        while self.inner.completed.load(Ordering::Acquire) == 0 {
            // Futex wait - if still 0, sleep
            unsafe {
                let ptr = &self.inner.completed as *const AtomicU32 as *mut u32;
                libc::sys_futex_wait(ptr, 0, 0); // 0 timeout = infinite
            }
        }

        // Thread completed - get result
        // Safety: Thread is done, we have exclusive access
        unsafe {
            (*self.inner.result.get()).take().ok_or(())
        }
    }

    /// Get the thread's ID
    pub fn thread(&self) -> Thread {
        Thread {
            id: ThreadId(self.inner.tid.load(Ordering::Relaxed)),
        }
    }

    /// Check if the thread has finished
    pub fn is_finished(&self) -> bool {
        self.inner.completed.load(Ordering::Acquire) != 0
    }
}

/// A handle to a thread
#[derive(Clone)]
pub struct Thread {
    id: ThreadId,
}

impl Thread {
    /// Get the thread's unique identifier
    pub fn id(&self) -> ThreadId {
        self.id
    }
}

/// Get the current thread's ID
pub fn current() -> Thread {
    let tid = libc::sys_gettid() as u32;
    Thread {
        id: ThreadId(tid),
    }
}

/// Spawn a new thread
///
/// Creates a new thread and executes the given closure in it.
/// Returns a JoinHandle that can be used to wait for the thread to finish.
pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    // Create shared state
    let inner = Arc::new(ThreadInner {
        tid: AtomicU32::new(0),
        result: UnsafeCell::new(None),
        completed: AtomicU32::new(0),
    });

    // Box the closure and inner reference for the thread
    let inner_clone = Arc::clone(&inner);
    let boxed_fn: Box<dyn FnOnce() + Send + 'static> = Box::new(move || {
        // Run the user's function
        let result = f();

        // Store result
        unsafe {
            *inner_clone.result.get() = Some(result);
        }

        // Mark as completed and wake waiter
        inner_clone.completed.store(1, Ordering::Release);
        unsafe {
            let ptr = &inner_clone.completed as *const AtomicU32 as *mut u32;
            libc::sys_futex_wake(ptr, 1);
        }
    });

    // Leak the box to get a raw pointer (thread will clean up)
    let fn_ptr = Box::into_raw(Box::new(boxed_fn));

    // Allocate a stack for the new thread (64KB)
    const THREAD_STACK_SIZE: usize = 64 * 1024;
    let stack = unsafe {
        let layout = alloc::alloc::Layout::from_size_align(THREAD_STACK_SIZE, 16).unwrap();
        alloc::alloc::alloc(layout)
    };

    if stack.is_null() {
        panic!("Failed to allocate thread stack");
    }

    // Stack grows down, so point to top
    let stack_top = unsafe { stack.add(THREAD_STACK_SIZE) };

    // Set up stack with function pointer at top
    // The thread entry function will read this
    unsafe {
        let stack_ptr = stack_top as *mut *mut (dyn FnOnce() + Send + 'static);
        *stack_ptr.offset(-1) = fn_ptr as *mut _;
    }

    // Clone flags for creating a thread:
    // CLONE_VM - share address space
    // CLONE_FS - share filesystem info
    // CLONE_FILES - share file descriptors
    // CLONE_SIGHAND - share signal handlers
    // CLONE_THREAD - same thread group
    // CLONE_CHILD_CLEARTID - clear tid and wake on exit
    let flags = libc::clone_flags::CLONE_VM
        | libc::clone_flags::CLONE_FS
        | libc::clone_flags::CLONE_FILES
        | libc::clone_flags::CLONE_SIGHAND
        | libc::clone_flags::CLONE_THREAD
        | libc::clone_flags::CLONE_CHILD_CLEARTID;

    // Create the thread
    // Note: In a full implementation, we'd use clone() with thread_entry as the entry point
    // For now, since clone isn't fully wired up, we use a simpler approach
    let mut child_tid: u32 = 0;
    let _result = libc::sys_clone(
        flags,
        unsafe { stack_top.offset(-8) }, // Leave room for function pointer
        core::ptr::null_mut(),
        &mut child_tid as *mut u32,
        0,
    );

    // Store TID in shared state
    inner.tid.store(child_tid, Ordering::Release);

    JoinHandle { inner }
}

/// Put the current thread to sleep for the specified duration
pub fn sleep(dur: Duration) {
    let secs = dur.as_secs();
    let nanos = dur.subsec_nanos() as u64;
    libc::sys_nanosleep(secs, nanos);
}

/// Yield execution to another thread
pub fn yield_now() {
    // On EFFLUX, we don't have a dedicated yield syscall
    // Sleep for 0 time as a hint to the scheduler
    libc::sys_nanosleep(0, 0);
}

/// Get the number of available CPUs
///
/// Returns 1 since EFFLUX is currently single-CPU
pub fn available_parallelism() -> usize {
    1
}

/// Builder for configuring and spawning threads
pub struct Builder {
    name: Option<alloc::string::String>,
    stack_size: Option<usize>,
}

impl Builder {
    /// Create a new thread builder
    pub fn new() -> Self {
        Builder {
            name: None,
            stack_size: None,
        }
    }

    /// Set the thread name
    pub fn name(mut self, name: alloc::string::String) -> Self {
        self.name = Some(name);
        self
    }

    /// Set the stack size for the thread
    pub fn stack_size(mut self, size: usize) -> Self {
        self.stack_size = Some(size);
        self
    }

    /// Spawn a thread with the configured options
    pub fn spawn<F, T>(self, f: F) -> Result<JoinHandle<T>, ()>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        // For now, ignore the configuration and use spawn()
        // A full implementation would use the stack_size
        Ok(spawn(f))
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

/// Cooperatively gives up a time slice to the OS scheduler
pub fn park() {
    // Simple implementation using futex
    let parker = AtomicU32::new(0);
    unsafe {
        let ptr = &parker as *const AtomicU32 as *mut u32;
        libc::sys_futex_wait(ptr, 0, 0);
    }
}

/// Blocks unless or until the current thread's token is made available
pub fn park_timeout(dur: Duration) {
    let nanos = dur.as_secs() * 1_000_000_000 + dur.subsec_nanos() as u64;
    let parker = AtomicU32::new(0);
    unsafe {
        let ptr = &parker as *const AtomicU32 as *mut u32;
        libc::sys_futex_wait(ptr, 0, nanos);
    }
}
