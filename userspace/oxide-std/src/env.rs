//! Environment variables and process arguments
//!
//! Provides std::env-like APIs for OXIDE OS.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Returns the arguments which this program was started with
pub fn args() -> Args {
    Args {
        inner: args_os(),
    }
}

/// Returns the arguments which this program was started with, as OsStrings
pub fn args_os() -> ArgsOs {
    // Read from global state set at program start
    // For now, return empty - the actual implementation requires
    // the runtime to parse argc/argv from the stack
    ArgsOs {
        args: Vec::new(),
        index: 0,
    }
}

/// Iterator over command line arguments
pub struct Args {
    inner: ArgsOs,
}

impl Iterator for Args {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for Args {
    fn len(&self) -> usize {
        self.inner.len()
    }
}

/// Iterator over command line arguments as strings
pub struct ArgsOs {
    args: Vec<String>,
    index: usize,
}

impl Iterator for ArgsOs {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        if self.index < self.args.len() {
            let arg = self.args[self.index].clone();
            self.index += 1;
            Some(arg)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.args.len() - self.index;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for ArgsOs {
    fn len(&self) -> usize {
        self.args.len() - self.index
    }
}

/// Fetches the environment variable `key` from the current process
pub fn var(key: &str) -> Result<String, VarError> {
    match libc::getenv(key) {
        Some(val) => Ok(val.to_string()),
        None => Err(VarError::NotPresent),
    }
}

/// Sets the environment variable `key` to `value`
pub fn set_var(key: &str, value: &str) {
    libc::setenv(key, value);
}

/// Removes an environment variable from the environment
pub fn remove_var(key: &str) {
    libc::unsetenv(key);
}

/// Returns an iterator of (variable, value) pairs of strings
pub fn vars() -> Vars {
    Vars {
        inner: vars_os(),
    }
}

/// Returns an iterator of environment variables
pub fn vars_os() -> VarsOs {
    let mut pairs = Vec::new();
    libc::env_iter(|key_bytes, value_bytes| {
        // Convert bytes to str - find null terminator
        let key_len = key_bytes.iter().position(|&c| c == 0).unwrap_or(key_bytes.len());
        let val_len = value_bytes.iter().position(|&c| c == 0).unwrap_or(value_bytes.len());
        if let (Ok(key), Ok(val)) = (
            core::str::from_utf8(&key_bytes[..key_len]),
            core::str::from_utf8(&value_bytes[..val_len])
        ) {
            pairs.push((key.to_string(), val.to_string()));
        }
    });
    VarsOs {
        pairs,
        index: 0,
    }
}

/// Iterator over environment variables
pub struct Vars {
    inner: VarsOs,
}

impl Iterator for Vars {
    type Item = (String, String);

    fn next(&mut self) -> Option<(String, String)> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

/// Iterator over environment variables as strings
pub struct VarsOs {
    pairs: Vec<(String, String)>,
    index: usize,
}

impl Iterator for VarsOs {
    type Item = (String, String);

    fn next(&mut self) -> Option<(String, String)> {
        if self.index < self.pairs.len() {
            let pair = self.pairs[self.index].clone();
            self.index += 1;
            Some(pair)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.pairs.len() - self.index;
        (remaining, Some(remaining))
    }
}

/// Error type for environment variable operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarError {
    /// The specified environment variable was not present
    NotPresent,
    /// The specified environment variable was not valid Unicode
    NotUnicode(String),
}

impl core::fmt::Display for VarError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VarError::NotPresent => write!(f, "environment variable not found"),
            VarError::NotUnicode(s) => write!(f, "environment variable was not valid unicode: {}", s),
        }
    }
}

/// Returns the current working directory as a String
pub fn current_dir() -> crate::io::Result<String> {
    let mut buf = [0u8; 4096];
    let len = libc::getcwd(&mut buf);
    if len < 0 {
        Err(crate::io::Error::new(crate::io::ErrorKind::Other, "failed to get current directory"))
    } else {
        // Find null terminator or use returned length
        let path_len = buf.iter().position(|&c| c == 0).unwrap_or(len as usize);
        match core::str::from_utf8(&buf[..path_len]) {
            Ok(s) => Ok(s.to_string()),
            Err(_) => Err(crate::io::Error::new(crate::io::ErrorKind::InvalidData, "invalid UTF-8 in path")),
        }
    }
}

/// Changes the current working directory
pub fn set_current_dir(path: &str) -> crate::io::Result<()> {
    let result = libc::chdir(path);
    if result < 0 {
        Err(crate::io::Error::from_raw_os_error(result))
    } else {
        Ok(())
    }
}

/// Returns the full filesystem path of the current running executable
///
/// Note: This is not implemented on OXIDE yet
pub fn current_exe() -> crate::io::Result<String> {
    Err(crate::io::Error::new(crate::io::ErrorKind::Other, "current_exe not implemented"))
}

/// Returns the path of a temporary directory
pub fn temp_dir() -> String {
    "/tmp".to_string()
}

/// Returns the path of the current user's home directory
pub fn home_dir() -> Option<String> {
    match var("HOME") {
        Ok(home) => Some(home),
        Err(_) => Some("/".to_string()),
    }
}
