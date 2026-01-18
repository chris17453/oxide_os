//! Process management for EFFLUX
//!
//! Provides process and address space management.

#![no_std]

extern crate alloc;

mod address_space;
mod process;

pub use address_space::UserAddressSpace;
pub use process::{
    Credentials, Process, ProcessContext, ProcessTable,
    alloc_pid, process_table,
};
pub use efflux_proc_traits::{AddressSpace, MapError, MemoryFlags, Pid, ProcessState, UnmapError};
