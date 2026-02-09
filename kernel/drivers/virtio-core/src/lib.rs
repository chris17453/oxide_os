//! VirtIO Core - Shared VirtIO Infrastructure
//!
//! Eliminates ~1000 LOC of duplication across VirtIO drivers by providing:
//! - Shared virtqueue management
//! - VirtIO PCI transport (re-exported from pci crate)
//! - Common feature negotiation helpers
//! — TorqueJax: one virtqueue implementation to rule them all

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::alloc::{alloc, alloc_zeroed, Layout};
use core::sync::atomic::Ordering;
use mm_manager::mm;
use mm_traits::FrameAllocator;

pub mod status;
pub mod features;
pub mod virtqueue;

pub use virtqueue::Virtqueue;

// Re-export VirtIO PCI transport from pci crate
pub use pci::{VirtioPciTransport, VirtioPciCap, VirtioPciCaps, find_virtio_caps};

/// Physical memory mapping base (same as mm-paging PHYS_MAP_BASE)
pub const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;

/// Convert physical address to virtual address
#[inline]
pub fn phys_to_virt(phys: u64) -> u64 {
    phys + PHYS_MAP_BASE
}

/// Convert virtual address to physical address
#[inline]
pub fn virt_to_phys(virt: u64) -> u64 {
    virt - PHYS_MAP_BASE
}
