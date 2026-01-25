//! Process management for OXIDE
//!
//! Provides process, thread, and address space management.

#![no_std]
#![allow(unused)]

extern crate alloc;

mod address_space;
mod clone;
mod exec;
mod fork;
mod futex;
mod process;
mod wait;

pub use address_space::UserAddressSpace;
pub use clone::{CloneArgs, CloneError, do_clone};
pub use exec::{ExecError, do_exec};
pub use fork::{ForkError, do_fork, handle_cow_fault};
pub use futex::{FutexError, futex_wait, futex_wake};
pub use proc_traits::{AddressSpace, MapError, MemoryFlags, Pid, ProcessState, UnmapError};
pub use process::{
    Credentials, Process, ProcessContext, ProcessTable, Tid, alloc_pid, clone_flags, process_table,
};
pub use wait::{WaitError, WaitOptions, WaitResult, do_wait, do_waitpid, has_children, is_child};
