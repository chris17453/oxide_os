//! OXIDE Runtime — The unholy glue between Rust's std and bare metal syscalls.
//!
//! This crate provides the runtime primitives that Rust's standard library
//! needs to function on OXIDE OS. It's a `rustc-dep-of-std` crate, meaning
//! it gets linked into std itself during `-Zbuild-std`.
//!
//! No alloc, no collections, no mercy. Just raw syscalls and prayer.
//!
//! — IronGhost: Every function here is a thin wrapper around a syscall.
//! If you're looking for abstractions, you're in the wrong crate.

#![no_std]
#![allow(unused)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(non_camel_case_types)]

pub mod syscall;
pub mod nr;
pub mod types;
pub mod alloc;
pub mod args;
pub mod env;
pub mod io;
pub mod fs;
pub mod os;
pub mod process;
pub mod thread;
pub mod time;
pub mod random;
pub mod pipe;
pub mod net;
pub mod signal;
pub mod start;
pub mod error;
pub mod libc_compat;
pub mod futex;
pub mod poll;
