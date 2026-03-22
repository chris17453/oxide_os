//! Environment variable support — Linux-compatible
//!
//! — GraveShift: environ is THE environment. One source of truth.
//! setenv/unsetenv modify it. getenv reads from it. execv passes it.
//! No separate mutex storage. This is how Linux works.

/// Maximum environment entries
const MAX_ENV: usize = 256;
/// Maximum "KEY=value\0" string length
const MAX_ENTRY_LEN: usize = 4096;

/// Static storage for env string data
static mut ENV_STRINGS: [[u8; MAX_ENTRY_LEN]; MAX_ENV] = [[0u8; MAX_ENTRY_LEN]; MAX_ENV];
/// Pointer array (NULL-terminated) — this IS environ
static mut ENV_PTRS: [*mut u8; MAX_ENV + 1] = [core::ptr::null_mut(); MAX_ENV + 1];
/// Current count
static mut ENV_COUNT: usize = 0;

/// Get environ pointer for execv
pub fn get_environ() -> *const *const u8 {
    unsafe { core::ptr::addr_of!(ENV_PTRS) as *const *const u8 }
}

/// Set an environment variable
pub fn setenv(name: &str, value: &str) -> i32 {
    if name.is_empty() { return -1; }
    let nb = name.as_bytes();
    let vb = value.as_bytes();
    if nb.len() + 1 + vb.len() >= MAX_ENTRY_LEN { return -1; }

    unsafe {
        // Update existing
        for i in 0..ENV_COUNT {
            if !ENV_PTRS[i].is_null() && entry_matches(ENV_PTRS[i], nb) {
                write_entry(&mut ENV_STRINGS[i], nb, vb);
                return 0;
            }
        }
        // Add new
        if ENV_COUNT >= MAX_ENV { return -1; }
        let i = ENV_COUNT;
        write_entry(&mut ENV_STRINGS[i], nb, vb);
        ENV_PTRS[i] = ENV_STRINGS[i].as_mut_ptr();
        ENV_COUNT += 1;
        ENV_PTRS[ENV_COUNT] = core::ptr::null_mut();
    }
    0
}

/// Unset an environment variable
pub fn unsetenv(name: &str) -> i32 {
    let nb = name.as_bytes();
    unsafe {
        for i in 0..ENV_COUNT {
            if !ENV_PTRS[i].is_null() && entry_matches(ENV_PTRS[i], nb) {
                // Shift down
                for j in i..ENV_COUNT - 1 {
                    ENV_STRINGS[j] = ENV_STRINGS[j + 1];
                    ENV_PTRS[j] = ENV_STRINGS[j].as_mut_ptr();
                }
                ENV_COUNT -= 1;
                ENV_PTRS[ENV_COUNT] = core::ptr::null_mut();
                return 0;
            }
        }
    }
    -1
}

/// Get an environment variable
pub fn getenv(name: &str) -> Option<&'static str> {
    let nb = name.as_bytes();
    unsafe {
        for i in 0..ENV_COUNT {
            if !ENV_PTRS[i].is_null() && entry_matches(ENV_PTRS[i], nb) {
                let p = ENV_PTRS[i].add(nb.len() + 1);
                let mut len = 0;
                while *p.add(len) != 0 { len += 1; }
                return Some(core::str::from_utf8_unchecked(
                    core::slice::from_raw_parts(p, len)));
            }
        }
    }
    None
}

/// Initialize from envp on stack (called by _start)
pub fn init_from_envp(envp: *const *const u8) {
    if envp.is_null() { return; }
    unsafe {
        ENV_COUNT = 0;
        let mut i = 0;
        while !(*envp.add(i)).is_null() && i < MAX_ENV {
            let ptr = *envp.add(i);
            let mut len = 0;
            while *ptr.add(len) != 0 { len += 1; }
            if len < MAX_ENTRY_LEN {
                core::ptr::copy_nonoverlapping(ptr, ENV_STRINGS[i].as_mut_ptr(), len);
                ENV_STRINGS[i][len] = 0;
                ENV_PTRS[i] = ENV_STRINGS[i].as_mut_ptr();
                ENV_COUNT += 1;
            }
            i += 1;
        }
        ENV_PTRS[ENV_COUNT] = core::ptr::null_mut();
    }
}

/// Set defaults
pub fn init_defaults() {
    setenv("HOME", "/root");
    setenv("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    setenv("TERM", "xterm-256color");
    setenv("SHELL", "/bin/esh");
    setenv("USER", "root");
    setenv("LOGNAME", "root");
    setenv("LANG", "C.UTF-8");
}

/// Iterate all vars
pub fn env_iter<F>(mut callback: F) -> usize
where F: FnMut(&[u8], &[u8]) {
    let mut count = 0;
    unsafe {
        for i in 0..ENV_COUNT {
            if ENV_PTRS[i].is_null() { continue; }
            let p = ENV_PTRS[i];
            let mut len = 0;
            while *p.add(len) != 0 { len += 1; }
            let e = core::slice::from_raw_parts(p, len);
            if let Some(eq) = e.iter().position(|&b| b == b'=') {
                callback(&e[..eq], &e[eq+1..]);
                count += 1;
            }
        }
    }
    count
}

/// Legacy compat
pub fn build_envp() -> *const *const u8 { get_environ() }

// --- helpers ---
fn write_entry(buf: &mut [u8; MAX_ENTRY_LEN], name: &[u8], value: &[u8]) {
    buf[..name.len()].copy_from_slice(name);
    buf[name.len()] = b'=';
    buf[name.len()+1..name.len()+1+value.len()].copy_from_slice(value);
    buf[name.len()+1+value.len()] = 0;
}

unsafe fn entry_matches(entry: *const u8, name: &[u8]) -> bool {
    for (i, &b) in name.iter().enumerate() {
        if *entry.add(i) != b { return false; }
    }
    *entry.add(name.len()) == b'='
}
