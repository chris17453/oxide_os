//! Environment variable support
//!
//! Simple environment variable storage for userspace programs.

use spin::Mutex;

/// Maximum number of environment variables
const MAX_ENVVARS: usize = 64;

/// Maximum length of a variable name or value
const MAX_VAR_LEN: usize = 256;

/// Environment variable entry
struct EnvVar {
    name: [u8; MAX_VAR_LEN],
    value: [u8; MAX_VAR_LEN],
    used: bool,
}

impl EnvVar {
    const fn new() -> Self {
        EnvVar {
            name: [0u8; MAX_VAR_LEN],
            value: [0u8; MAX_VAR_LEN],
            used: false,
        }
    }
}

/// Global environment storage (thread-safe with Mutex)
/// ⚡ GraveShift: Fixed - now thread-safe with spin::Mutex
static ENV: Mutex<[EnvVar; MAX_ENVVARS]> = Mutex::new({
    const INIT: EnvVar = EnvVar::new();
    [INIT; MAX_ENVVARS]
});

/// Copy string to buffer
fn copy_str(dst: &mut [u8], src: &str) {
    let bytes = src.as_bytes();
    let len = bytes.len().min(dst.len() - 1);
    dst[..len].copy_from_slice(&bytes[..len]);
    dst[len] = 0;
}

/// Compare string with buffer
fn str_eq_buf(buf: &[u8], s: &str) -> bool {
    let bytes = s.as_bytes();
    let buf_len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());

    if buf_len != bytes.len() {
        return false;
    }

    for i in 0..buf_len {
        if buf[i] != bytes[i] {
            return false;
        }
    }
    true
}

/// Set an environment variable
///
/// Returns 0 on success, -1 on error.
pub fn setenv(name: &str, value: &str) -> i32 {
    if name.is_empty() {
        return -1;
    }

    let mut env = ENV.lock();

    // Look for existing variable
    for var in env.iter_mut() {
        if var.used && str_eq_buf(&var.name, name) {
            copy_str(&mut var.value, value);
            return 0;
        }
    }

    // Find empty slot
    for var in env.iter_mut() {
        if !var.used {
            copy_str(&mut var.name, name);
            copy_str(&mut var.value, value);
            var.used = true;
            return 0;
        }
    }

    // No space
    -1
}

/// Unset an environment variable
///
/// Returns 0 on success, -1 if not found.
pub fn unsetenv(name: &str) -> i32 {
    let mut env = ENV.lock();

    for var in env.iter_mut() {
        if var.used && str_eq_buf(&var.name, name) {
            var.used = false;
            var.name[0] = 0;
            var.value[0] = 0;
            return 0;
        }
    }
    -1
}

/// Thread-local buffer for getenv return value
static mut GETENV_BUF: [u8; MAX_VAR_LEN] = [0; MAX_VAR_LEN];

/// Get an environment variable
///
/// Returns the value or None if not found.
/// ⚡ Note: Uses static buffer, not safe for concurrent getenv calls from multiple threads
pub fn getenv(name: &str) -> Option<&'static str> {
    let env = ENV.lock();

    for var in env.iter() {
        if var.used && str_eq_buf(&var.name, name) {
            let len = var
                .value
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(var.value.len());

            // Copy to static buffer to avoid holding lock
            unsafe {
                GETENV_BUF[..len].copy_from_slice(&var.value[..len]);
                GETENV_BUF[len] = 0;
                return Some(core::str::from_utf8_unchecked(&GETENV_BUF[..len]));
            }
        }
    }
    None
}

/// Initialize default environment variables
pub fn init_env() {
    setenv("PATH", "/bin");
    setenv("HOME", "/");
    setenv("TERM", "vt100");
    setenv("SHELL", "/bin/esh");
    setenv("USER", "root");
    setenv("PWD", "/");
}

/// Iterate over all environment variables
///
/// Calls the callback with (name, value) for each set variable.
/// Returns the number of variables iterated.
pub fn env_iter<F>(mut callback: F) -> usize
where
    F: FnMut(&[u8], &[u8]),
{
    let mut count = 0;
    let env = ENV.lock();

    for var in env.iter() {
        if var.used {
            callback(&var.name, &var.value);
            count += 1;
        }
    }
    count
}
