//! Process management traits for OXIDE
//!
//! Defines the interfaces for process and address space management.

#![no_std]

use os_core::{PhysAddr, VirtAddr};

/// Process ID type
pub type Pid = u32;

/// Process state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Process is ready to run
    Ready,
    /// Process is currently running
    Running,
    /// Process is blocked waiting for something
    Blocked,
    /// Process has exited but not yet reaped
    Zombie,
}

/// Memory protection flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryFlags {
    bits: u8,
}

impl MemoryFlags {
    pub const NONE: Self = Self { bits: 0 };
    pub const READ: Self = Self { bits: 1 << 0 };
    pub const WRITE: Self = Self { bits: 1 << 1 };
    pub const EXECUTE: Self = Self { bits: 1 << 2 };
    pub const USER: Self = Self { bits: 1 << 3 };

    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub const fn all() -> Self {
        Self { bits: 0x0F }
    }

    pub const fn from_bits(bits: u8) -> Self {
        Self { bits }
    }

    pub const fn bits(&self) -> u8 {
        self.bits
    }

    pub const fn contains(&self, other: Self) -> bool {
        (self.bits & other.bits) == other.bits
    }

    pub const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    pub const fn readable(&self) -> bool {
        self.contains(Self::READ)
    }

    pub const fn writable(&self) -> bool {
        self.contains(Self::WRITE)
    }

    pub const fn executable(&self) -> bool {
        self.contains(Self::EXECUTE)
    }

    pub const fn user(&self) -> bool {
        self.contains(Self::USER)
    }
}

/// Address space trait
///
/// Represents a process's virtual address space.
pub trait AddressSpace {
    /// Create a new empty address space
    fn new() -> Self
    where
        Self: Sized;

    /// Get the physical address of the page table root
    fn page_table_root(&self) -> PhysAddr;

    /// Map a virtual address to a physical address
    ///
    /// # Safety
    /// The physical address must be valid and the mapping must not
    /// conflict with existing mappings in unsafe ways.
    unsafe fn map(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: MemoryFlags,
    ) -> Result<(), MapError>;

    /// Unmap a virtual address
    ///
    /// # Safety
    /// The virtual address must be currently mapped.
    unsafe fn unmap(&mut self, virt: VirtAddr) -> Result<PhysAddr, UnmapError>;

    /// Translate a virtual address to physical
    fn translate(&self, virt: VirtAddr) -> Option<PhysAddr>;

    /// Map a range of pages
    ///
    /// # Safety
    /// All physical addresses must be valid.
    unsafe fn map_range(
        &mut self,
        virt_start: VirtAddr,
        phys_start: PhysAddr,
        size: usize,
        flags: MemoryFlags,
    ) -> Result<(), MapError> {
        let page_size = 4096; // Assume 4KB pages
        let pages = (size + page_size - 1) / page_size;

        for i in 0..pages {
            let offset = (i * page_size) as u64;
            let virt = VirtAddr::new(virt_start.as_u64() + offset);
            let phys = PhysAddr::new(phys_start.as_u64() + offset);
            unsafe { self.map(virt, phys, flags)? };
        }

        Ok(())
    }
}

/// Error mapping a page
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapError {
    /// Page is already mapped
    AlreadyMapped,
    /// Out of memory for page tables
    OutOfMemory,
    /// Invalid address
    InvalidAddress,
}

/// Error unmapping a page
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnmapError {
    /// Page is not mapped
    NotMapped,
    /// Invalid address
    InvalidAddress,
}

/// Process trait
pub trait Process {
    /// The address space type for this process
    type AddressSpace: AddressSpace;

    /// Get the process ID
    fn pid(&self) -> Pid;

    /// Get the process state
    fn state(&self) -> ProcessState;

    /// Get a reference to the address space
    fn address_space(&self) -> &Self::AddressSpace;

    /// Get a mutable reference to the address space
    fn address_space_mut(&mut self) -> &mut Self::AddressSpace;
}
