//! Process management for OXIDE
//!
//! Provides process metadata, thread, and address space management.
//!
//! The actual process/task management is done by the scheduler (sched crate).
//! This crate provides:
//! - ProcessMeta: Shared process-level state (fd table, credentials, signals, etc.)
//! - Fork/Exec/Clone operations that return result structs
//! - Wait types for wait/waitpid
//! - Futex wait queue management
//! - Address space abstraction

#![no_std]
#![allow(unused)]

extern crate alloc;

mod address_space;
mod clone;
mod exec;
mod fork;
mod futex;
mod meta;
mod process;
mod wait;

pub use address_space::UserAddressSpace;
pub use clone::{CloneArgs, CloneError, CloneResult, do_clone};
pub use exec::{ExecError, ExecResult, do_exec};
pub use fork::{ForkError, ForkResult, do_fork, handle_cow_fault};
pub use futex::{
    FutexError, FutexWaitResult, futex_clear_and_wake, futex_wait_cancel, futex_wait_prepare,
    futex_wake,
};
pub use meta::ProcessMeta;
pub use proc_traits::{AddressSpace, MapError, MemoryFlags, Pid, ProcessState, UnmapError};
pub use process::{Credentials, ProcessContext, Tid, alloc_pid, clone_flags};
pub use wait::{WaitError, WaitOptions, WaitResult};
