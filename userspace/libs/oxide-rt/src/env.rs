//! Environment variable storage — no HashMap, no alloc, just vibes.
//!
//! — NeonRoot: Array-backed env storage because we can't use alloc
//! (we ARE the allocator's dependency chain). Fixed-size buffers.
//! If you need more than 128 env vars, reconsider your life choices.

use core::sync::atomic::{AtomicUsize, Ordering};

const MAX_ENVS: usize = 128;
const MAX_KEY_LEN: usize = 128;
const MAX_VAL_LEN: usize = 512;

#[repr(C)]
struct EnvEntry {
    key: [u8; MAX_KEY_LEN],
    key_len: usize,
    val: [u8; MAX_VAL_LEN],
    val_len: usize,
    used: bool,
}

impl EnvEntry {
    const fn empty() -> Self {
        Self {
            key: [0; MAX_KEY_LEN],
            key_len: 0,
            val: [0; MAX_VAL_LEN],
            val_len: 0,
            used: false,
        }
    }
}

// — NeonRoot: UnsafeCell because we're in a no_std runtime crate.
// Single-threaded init, and env access is inherently racy in POSIX anyway.
static mut ENV_STORE: [EnvEntry; MAX_ENVS] = {
    const EMPTY: EnvEntry = EnvEntry::empty();
    [EMPTY; MAX_ENVS]
};
static ENV_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Initialize environment from the envp pointer on the stack.
/// Called by _start after argc/argv parsing.
pub unsafe fn init_from_envp(envp: *const *const u8) {
    if envp.is_null() {
        return;
    }
    let mut i = 0;
    unsafe {
        while !(*envp.add(i)).is_null() {
            let ptr = *envp.add(i);
            // Find the length (null-terminated)
            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let entry = core::slice::from_raw_parts(ptr, len);
            // Split on '='
            if let Some(eq_pos) = entry.iter().position(|&b| b == b'=') {
                let key = &entry[..eq_pos];
                let val = &entry[eq_pos + 1..];
                setenv_bytes(key, val);
            }
            i += 1;
        }
    }
}

/// Set an environment variable (key=value as byte slices)
pub fn setenv_bytes(key: &[u8], val: &[u8]) {
    if key.len() > MAX_KEY_LEN || val.len() > MAX_VAL_LEN {
        return;
    }

    unsafe {
        // Check if key already exists
        let count = ENV_COUNT.load(Ordering::Relaxed);
        for i in 0..count {
            if ENV_STORE[i].used && ENV_STORE[i].key_len == key.len()
                && &ENV_STORE[i].key[..key.len()] == key
            {
                ENV_STORE[i].val[..val.len()].copy_from_slice(val);
                ENV_STORE[i].val_len = val.len();
                return;
            }
        }

        // Find an empty slot
        for i in 0..MAX_ENVS {
            if !ENV_STORE[i].used {
                ENV_STORE[i].key[..key.len()].copy_from_slice(key);
                ENV_STORE[i].key_len = key.len();
                ENV_STORE[i].val[..val.len()].copy_from_slice(val);
                ENV_STORE[i].val_len = val.len();
                ENV_STORE[i].used = true;
                let _ = ENV_COUNT.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
    }
}

/// Get an environment variable by key
pub fn getenv_bytes(key: &[u8]) -> Option<&'static [u8]> {
    unsafe {
        let count = ENV_COUNT.load(Ordering::Relaxed);
        for i in 0..MAX_ENVS {
            if ENV_STORE[i].used && ENV_STORE[i].key_len == key.len()
                && &ENV_STORE[i].key[..key.len()] == key
            {
                return Some(&ENV_STORE[i].val[..ENV_STORE[i].val_len]);
            }
        }
    }
    None
}

/// Unset an environment variable
pub fn unsetenv_bytes(key: &[u8]) {
    unsafe {
        for i in 0..MAX_ENVS {
            if ENV_STORE[i].used && ENV_STORE[i].key_len == key.len()
                && &ENV_STORE[i].key[..key.len()] == key
            {
                ENV_STORE[i].used = false;
                ENV_STORE[i].key_len = 0;
                ENV_STORE[i].val_len = 0;
                return;
            }
        }
    }
}

/// Iterate over all set environment variables (key, value) as byte slices
pub fn env_iter(mut f: impl FnMut(&[u8], &[u8])) {
    unsafe {
        for i in 0..MAX_ENVS {
            if ENV_STORE[i].used {
                f(
                    &ENV_STORE[i].key[..ENV_STORE[i].key_len],
                    &ENV_STORE[i].val[..ENV_STORE[i].val_len],
                );
            }
        }
    }
}

/// Get the number of set environment variables
pub fn env_count() -> usize {
    ENV_COUNT.load(Ordering::Relaxed)
}

/// Get the nth set environment variable (key, value) as byte slices
/// Returns None if index is out of range or slot is unused
pub fn env_nth(n: usize) -> Option<(&'static [u8], &'static [u8])> {
    unsafe {
        let mut count = 0;
        for i in 0..MAX_ENVS {
            if ENV_STORE[i].used {
                if count == n {
                    return Some((
                        &ENV_STORE[i].key[..ENV_STORE[i].key_len],
                        &ENV_STORE[i].val[..ENV_STORE[i].val_len],
                    ));
                }
                count += 1;
            }
        }
    }
    None
}
