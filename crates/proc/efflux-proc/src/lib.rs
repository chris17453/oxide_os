//! Process management for EFFLUX
//!
//! Provides process and address space management.

#![no_std]

extern crate alloc;

mod address_space;

pub use address_space::UserAddressSpace;
pub use efflux_proc_traits::{AddressSpace, MapError, MemoryFlags, Pid, Process, ProcessState, UnmapError};
