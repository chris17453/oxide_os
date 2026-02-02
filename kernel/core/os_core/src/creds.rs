//! Process credentials bridge
//!
//! Provides decoupled access to the current process's UID/GID for
//! subsystems (e.g. ext4) that don't depend on the proc or sched crates.
//! The kernel registers a concrete provider at boot.
//!
//! — EmberLock: identity bridge from process to filesystem

use core::sync::atomic::{AtomicPtr, Ordering};

/// Credentials provider: returns (uid, gid) of the current process
type CredsFn = fn() -> (u32, u32);

/// Fallback: returns root credentials when no provider is registered
fn root_creds() -> (u32, u32) {
    (0, 0)
}

/// Global credentials function pointer
static CREDS_FN: AtomicPtr<()> = AtomicPtr::new(root_creds as *mut ());

/// Register the credentials provider (called once at boot from init.rs)
pub fn register_creds_provider(f: CredsFn) {
    CREDS_FN.store(f as *mut (), Ordering::Release);
}

/// Get the current process's (uid, gid).
///
/// Returns (0, 0) if no provider has been registered yet.
pub fn current_uid_gid() -> (u32, u32) {
    let ptr = CREDS_FN.load(Ordering::Acquire);
    // Safety: we only store valid fn pointers (root_creds or a registered provider)
    let f: CredsFn = unsafe { core::mem::transmute(ptr) };
    f()
}
