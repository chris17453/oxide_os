//! OXIDE Shell (esh) — library crate
//!
//! — ByteRiot: the testable guts of esh. Pure logic modules (tokenizer, parser,
//! AST) live here so `cargo test -p esh --lib` works on the host without needing
//! OXIDE's libc or a running kernel. The binary crate (main.rs) pulls these in
//! via `mod` declarations for the actual no_std build.

#![cfg_attr(not(test), no_std)]
#![allow(unused)]

extern crate alloc;

/// — ByteRiot: debug tracing for the shell parser/evaluator.
/// Compiles to nothing unless `debug-shell` feature is enabled.
#[cfg(feature = "debug-shell")]
macro_rules! debug_shell {
    ($($arg:tt)*) => {
        eprints("[esh-dbg] ");
        eprintlns(core::concat!($($arg)*));
    };
}

#[cfg(not(feature = "debug-shell"))]
macro_rules! debug_shell {
    ($($arg:tt)*) => {};
}

// — ByteRiot: pure logic modules — no libc, no syscalls, fully testable on host
pub mod token;
pub mod ast;
pub mod parser;
