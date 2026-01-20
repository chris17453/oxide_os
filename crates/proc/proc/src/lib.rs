//! Process management for EFFLUX
//!
//! Provides process, thread, and address space management.

#![no_std]

extern crate alloc;

mod address_space;
mod process;
mod fork;
mod clone;
mod exec;
mod wait;
mod futex;

pub use address_space::UserAddressSpace;
pub use process::{
    Credentials, Process, ProcessContext, ProcessTable, Tid,
    alloc_pid, process_table, clone_flags,
};
pub use fork::{do_fork, handle_cow_fault, ForkError};
pub use clone::{do_clone, CloneError, CloneArgs};
pub use exec::{do_exec, ExecError};
pub use wait::{do_wait, do_waitpid, WaitError, WaitOptions, WaitResult, has_children, is_child};
pub use futex::{futex_wait, futex_wake, FutexError};
pub use proc_traits::{AddressSpace, MapError, MemoryFlags, Pid, ProcessState, UnmapError};
