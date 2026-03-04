//! Thread-Local Storage — the foundation std builds its thread_local! on.
//!
//! — ThreadRogue: std's thread_local dispatch needs exactly four things from us:
//! a Key type, create(), destroy(), get(), and set(). The racy::LazyKey wrapper
//! handles concurrent initialization races via atomics — we just manage the
//! actual storage slots.
//!
//! For now, OXIDE processes are single-address-space (fork = new process, not
//! shared-memory threads). So TLS is just a global slot array per process.
//! When we add real CLONE_VM threads, we'll key by TID or use %fs-based
//! thread control blocks. The interface won't change.

use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

/// — ThreadRogue: TLS key — an index into the slot array.
/// std's racy::LazyKey stores this in an AtomicUsize, so it must fit.
pub type Key = usize;

/// — ThreadRogue: Maximum number of TLS keys per process. 128 is generous
/// for a kernel integration test suite. Linux's PTHREAD_KEYS_MAX is 1024,
/// but we're not pretending to be Linux. Yet.
const MAX_KEYS: usize = 128;

/// — ThreadRogue: Per-slot metadata. Tracks whether a key is live and
/// stores the optional destructor that runs on thread exit.
struct SlotMeta {
    in_use: bool,
    dtor: Option<unsafe extern "C" fn(*mut u8)>,
}

/// — ThreadRogue: The slot array. Each key maps to a pointer value.
/// Since we're single-threaded-per-process right now, one array is enough.
/// When real threads land, this becomes per-thread via TLS base register.
static mut SLOTS: [*mut u8; MAX_KEYS] = [core::ptr::null_mut(); MAX_KEYS];

/// — ThreadRogue: Metadata for each slot (in_use flag + destructor).
/// Protected by the atomic key counter — only create/destroy mutate this.
static mut SLOT_META: [SlotMeta; MAX_KEYS] = {
    const EMPTY: SlotMeta = SlotMeta { in_use: false, dtor: None };
    [EMPTY; MAX_KEYS]
};

/// — ThreadRogue: Next key to allocate. Monotonically increasing.
/// We don't recycle keys — 128 slots is plenty and recycling adds complexity
/// that buys nothing for our use case.
static NEXT_KEY: AtomicU32 = AtomicU32::new(0);

/// Create a new TLS key with an optional destructor.
/// Called by racy::LazyKey::lazy_init() — may race with other create() calls,
/// but the atomic counter ensures each gets a unique key.
pub fn create(dtor: Option<unsafe extern "C" fn(*mut u8)>) -> Key {
    let key = NEXT_KEY.fetch_add(1, Ordering::Relaxed) as usize;
    if key >= MAX_KEYS {
        // — ThreadRogue: out of TLS keys. This would be a panic in a real
        // runtime, but we can't panic here (we're inside std's guts).
        // Return a sentinel that get/set will bounds-check.
        return usize::MAX;
    }
    unsafe {
        SLOT_META[key] = SlotMeta { in_use: true, dtor };
        SLOTS[key] = core::ptr::null_mut();
    }
    key
}

/// Destroy a TLS key. Frees the slot for conceptual reuse (though we don't
/// actually recycle keys — the counter is monotonic).
pub unsafe fn destroy(key: Key) {
    if key < MAX_KEYS {
        unsafe {
            SLOT_META[key] = SlotMeta { in_use: false, dtor: None };
            SLOTS[key] = core::ptr::null_mut();
        }
    }
}

/// Get the value associated with a TLS key for the current thread.
/// Returns null if the key was never set or is invalid.
#[inline]
pub unsafe fn get(key: Key) -> *mut u8 {
    if key < MAX_KEYS {
        unsafe { SLOTS[key] }
    } else {
        core::ptr::null_mut()
    }
}

/// Set the value associated with a TLS key for the current thread.
#[inline]
pub unsafe fn set(key: Key, value: *mut u8) {
    if key < MAX_KEYS {
        unsafe {
            SLOTS[key] = value;
        }
    }
}
