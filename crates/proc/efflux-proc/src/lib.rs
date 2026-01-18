//! Process management for EFFLUX
//!
//! Provides process and address space management.

#![no_std]

extern crate alloc;

mod address_space;
mod process;
mod fork;
mod exec;
mod wait;

pub use address_space::UserAddressSpace;
pub use process::{
    Credentials, Process, ProcessContext, ProcessTable,
    alloc_pid, process_table,
};
pub use fork::{do_fork, handle_cow_fault, ForkError};
pub use exec::{do_exec, ExecError};
pub use wait::{do_wait, do_waitpid, WaitError, WaitOptions, WaitResult, has_children, is_child};
pub use efflux_proc_traits::{AddressSpace, MapError, MemoryFlags, Pid, ProcessState, UnmapError};
