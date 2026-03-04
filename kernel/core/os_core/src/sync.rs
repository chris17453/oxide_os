//! Synchronization primitives
//!
//! — GraveShift: Two flavors of mutex for two different worlds.
//! spin::Mutex — raw spinlock. No preemption awareness. Fine for ISR-reachable
//! code where we already know the scheduler won't touch us.
//! KernelMutex — Linux-model spinlock. Disables preemption on lock, re-enables
//! on unlock. Prevents the classic "preempted while holding lock → next task
//! deadlocks on the same lock" pattern. Use this for anything the scheduler's
//! timer ISR might interrupt (heap, VFS, block I/O paths).

pub use spin::{Mutex, MutexGuard};

use core::sync::atomic::{AtomicUsize, Ordering};
use core::ops::{Deref, DerefMut};

// ============================================================================
// Preemption hook registration
//
// — GraveShift: os_core can't depend on arch-x86_64 (circular dep). So we use
// function pointer callbacks registered at boot. Before registration, the hooks
// are no-ops — safe for early boot before the arch crate is initialized.
// After init.rs calls register_preempt_hooks(), every KernelMutex lock/unlock
// routes through arch::preempt_disable/enable.
// ============================================================================

static PREEMPT_DISABLE_FN: AtomicUsize = AtomicUsize::new(0);
static PREEMPT_ENABLE_FN: AtomicUsize = AtomicUsize::new(0);

/// Register preemption control hooks. Called once from kernel init.
/// — GraveShift: After this call, KernelMutex actually disables preemption.
/// Before this call, it's just a regular spinlock. Which is fine — during early
/// boot there's no scheduler to preempt us anyway.
pub fn register_preempt_hooks(disable: fn(), enable: fn()) {
    PREEMPT_DISABLE_FN.store(disable as usize, Ordering::Release);
    PREEMPT_ENABLE_FN.store(enable as usize, Ordering::Release);
}

#[inline]
fn call_preempt_disable() {
    let f = PREEMPT_DISABLE_FN.load(Ordering::Acquire);
    if f != 0 {
        // SAFETY: f was stored from a valid fn() pointer in register_preempt_hooks
        let func: fn() = unsafe { core::mem::transmute(f) };
        func();
    }
}

#[inline]
fn call_preempt_enable() {
    let f = PREEMPT_ENABLE_FN.load(Ordering::Acquire);
    if f != 0 {
        // SAFETY: f was stored from a valid fn() pointer in register_preempt_hooks
        let func: fn() = unsafe { core::mem::transmute(f) };
        func();
    }
}

// ============================================================================
// KernelMutex — preemption-aware spinlock
//
// — GraveShift: This is the Linux model. spin_lock() disables preemption,
// spin_unlock() re-enables it. The scheduler timer ISR checks preempt_count
// before yanking a task — if count > 0, something holds a lock, and preempting
// would deadlock the next task that tries the same lock. Simple. Effective.
// Took us 67 builds to figure out we needed it.
// ============================================================================

/// Preemption-aware spinlock. Disables preemption on lock, re-enables on unlock.
///
/// — GraveShift: Use this instead of raw spin::Mutex for any lock that could be
/// held when the timer ISR fires. The heap allocator is the poster child — every
/// Vec::push, Box::new, and format!() goes through it. Without preemption
/// protection, the scheduler can yank us mid-alloc, hand the CPU to another task
/// that also needs the heap, and boom — permanent deadlock. KernelMutex makes
/// that impossible by telling the scheduler "not now" while the lock is held.
pub struct KernelMutex<T: ?Sized> {
    inner: spin::Mutex<T>,
}

// SAFETY: KernelMutex has the same Send/Sync bounds as spin::Mutex
unsafe impl<T: ?Sized + Send> Send for KernelMutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for KernelMutex<T> {}

/// Guard returned by KernelMutex::lock(). Dropping re-enables preemption.
///
/// — GraveShift: The Drop impl is critical. We MUST drop the inner guard
/// (releasing the spinlock) BEFORE calling preempt_enable(). Otherwise there's
/// a window where preemption is enabled but the lock is still held — exactly
/// the bug we're trying to prevent.
pub struct KernelMutexGuard<'a, T: ?Sized> {
    // Order matters: Rust drops fields in declaration order.
    // inner guard drops first (releases spinlock), then _preempt_token
    // triggers preempt_enable in its Drop.
    guard: spin::MutexGuard<'a, T>,
    _preempt_token: PreemptToken,
}

/// Token that calls preempt_enable on drop. Ensures correct ordering.
struct PreemptToken;

impl Drop for PreemptToken {
    #[inline]
    fn drop(&mut self) {
        call_preempt_enable();
    }
}

impl<T> KernelMutex<T> {
    /// Create a new KernelMutex wrapping the given value.
    pub const fn new(val: T) -> Self {
        Self {
            inner: spin::Mutex::new(val),
        }
    }

    /// Lock the mutex, disabling preemption until the guard is dropped.
    /// — GraveShift: preempt_disable BEFORE spin. If we spin first and get
    /// preempted mid-spin, we waste the whole timeslice spinning. Worse: the
    /// lock holder might be on the same CPU and can't make progress because
    /// we stole its timeslice to spin. Classic priority inversion.
    #[inline]
    pub fn lock(&self) -> KernelMutexGuard<'_, T> {
        call_preempt_disable();
        KernelMutexGuard {
            guard: self.inner.lock(),
            _preempt_token: PreemptToken,
        }
    }

    /// Try to lock the mutex without blocking.
    /// Disables preemption on success, leaves it unchanged on failure.
    #[inline]
    pub fn try_lock(&self) -> Option<KernelMutexGuard<'_, T>> {
        call_preempt_disable();
        match self.inner.try_lock() {
            Some(guard) => Some(KernelMutexGuard {
                guard,
                _preempt_token: PreemptToken,
            }),
            None => {
                // — GraveShift: Lock contended. Undo the preempt_disable since
                // we're not holding anything. Caller can retry or bail.
                call_preempt_enable();
                None
            }
        }
    }
}

impl<T: ?Sized> Deref for KernelMutexGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &*self.guard
    }
}

impl<T: ?Sized> DerefMut for KernelMutexGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.guard
    }
}
