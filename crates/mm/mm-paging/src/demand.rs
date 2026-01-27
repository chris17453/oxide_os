//! Demand paging and page fault handling support
//!
//! This module provides infrastructure for handling page faults:
//! - COW (Copy-on-Write) fault handling
//! - Demand allocation (lazy page allocation)
//! - Guard page detection
//!
//! The actual COW implementation is in `proc::fork`, but this module
//! provides the classification logic and integration points.
//!
//! # Debug Output
//!
//! Enable the `debug-demand` feature to get serial output for page fault handling:
//! ```toml
//! mm-paging = { workspace = true, features = ["debug-demand"] }
//! ```

use crate::{PageTable, phys_to_virt};
use os_core::{PhysAddr, VirtAddr};

/// Debug macro for demand paging - outputs to serial when debug-demand feature is enabled
#[cfg(feature = "debug-demand")]
macro_rules! debug_demand {
    ($($arg:tt)*) => {{
        #[cfg(target_arch = "x86_64")]
        {
            use arch_x86_64::serial::SerialWriter;
            use core::fmt::Write;
            let mut writer = SerialWriter;
            let _ = writeln!(writer, $($arg)*);
        }
    }};
}

#[cfg(not(feature = "debug-demand"))]
macro_rules! debug_demand {
    ($($arg:tt)*) => {};
}

/// Page fault error code bits (x86_64)
pub mod error_code {
    /// Page was present when fault occurred
    pub const PRESENT: u64 = 1 << 0;
    /// Fault was caused by a write access
    pub const WRITE: u64 = 1 << 1;
    /// Fault occurred in user mode
    pub const USER: u64 = 1 << 2;
    /// Fault was caused by reserved bit violation
    pub const RESERVED: u64 = 1 << 3;
    /// Fault was caused by instruction fetch
    pub const INSTRUCTION_FETCH: u64 = 1 << 4;
    /// Fault was caused by protection key violation
    pub const PROTECTION_KEY: u64 = 1 << 5;
    /// Fault was caused by shadow stack access
    pub const SHADOW_STACK: u64 = 1 << 6;
    /// Fault was caused by SGX violation
    pub const SGX: u64 = 1 << 15;
}

/// Classification of a page fault
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultType {
    /// Write to a COW page - needs copy
    CowFault,
    /// Access to unmapped page that should be demand-allocated
    DemandFault,
    /// Access to a guard page (stack overflow detection)
    GuardPageFault,
    /// Invalid access - should result in SIGSEGV for user mode
    InvalidAccess,
    /// Kernel bug - should panic
    KernelFault,
}

/// Result of page fault analysis
#[derive(Debug)]
pub struct FaultInfo {
    /// Type of fault
    pub fault_type: FaultType,
    /// Virtual address that caused the fault
    pub fault_addr: VirtAddr,
    /// Error code from CPU
    pub error_code: u64,
    /// Instruction pointer when fault occurred
    pub rip: u64,
    /// Whether fault was from user mode
    pub user_mode: bool,
    /// Whether it was a write access
    pub write_access: bool,
    /// Whether page was present
    pub page_present: bool,
}

impl FaultInfo {
    /// Create fault info from fault parameters
    pub fn new(fault_addr: u64, error_code: u64, rip: u64) -> Self {
        let user_mode = error_code & error_code::USER != 0;
        let write_access = error_code & error_code::WRITE != 0;
        let page_present = error_code & error_code::PRESENT != 0;

        // Classify the fault
        let fault_type = Self::classify(fault_addr, error_code, user_mode);

        debug_demand!(
            "[DEMAND] Fault: addr={:#x} err={:#x} rip={:#x} type={:?}",
            fault_addr,
            error_code,
            rip,
            fault_type
        );
        debug_demand!(
            "[DEMAND] user={} write={} present={}",
            user_mode,
            write_access,
            page_present
        );

        Self {
            fault_type,
            fault_addr: VirtAddr::new(fault_addr),
            error_code,
            rip,
            user_mode,
            write_access,
            page_present,
        }
    }

    /// Classify the fault type
    fn classify(fault_addr: u64, error_code: u64, user_mode: bool) -> FaultType {
        let is_present = error_code & error_code::PRESENT != 0;
        let is_write = error_code & error_code::WRITE != 0;
        let is_userspace_addr = fault_addr < 0x0000_8000_0000_0000;

        // COW fault: present page + write access
        if is_present && is_write {
            return FaultType::CowFault;
        }

        // Demand fault: not present, in valid user range
        if !is_present && user_mode && is_userspace_addr {
            // Could check VMA here to distinguish demand from invalid
            // For now, treat all non-present user faults as potential demand
            return FaultType::DemandFault;
        }

        // Kernel fault on user address
        if !user_mode && is_userspace_addr {
            // Kernel accessing user space (e.g., copy_to_user)
            if is_present && is_write {
                return FaultType::CowFault;
            }
            return FaultType::InvalidAccess;
        }

        // Kernel fault on kernel address
        if !user_mode && !is_userspace_addr {
            return FaultType::KernelFault;
        }

        FaultType::InvalidAccess
    }

    /// Check if this is a COW fault
    pub fn is_cow(&self) -> bool {
        self.fault_type == FaultType::CowFault
    }

    /// Check if this is a demand fault
    pub fn is_demand(&self) -> bool {
        self.fault_type == FaultType::DemandFault
    }
}

/// Check if a page table entry has the COW flag set
pub fn is_cow_page(pml4_phys: PhysAddr, virt: VirtAddr) -> bool {
    let pml4_virt = phys_to_virt(pml4_phys);
    let pml4 = unsafe { &*pml4_virt.as_ptr::<PageTable>() };

    let pml4_idx = ((virt.as_u64() >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((virt.as_u64() >> 30) & 0x1FF) as usize;
    let pd_idx = ((virt.as_u64() >> 21) & 0x1FF) as usize;
    let pt_idx = ((virt.as_u64() >> 12) & 0x1FF) as usize;

    let pml4_entry = &pml4[pml4_idx];
    if !pml4_entry.is_present() {
        return false;
    }

    let pdpt_virt = phys_to_virt(pml4_entry.addr());
    let pdpt = unsafe { &*pdpt_virt.as_ptr::<PageTable>() };
    let pdpt_entry = &pdpt[pdpt_idx];
    if !pdpt_entry.is_present() {
        return false;
    }

    if pdpt_entry.is_huge() {
        return pdpt_entry.is_cow();
    }

    let pd_virt = phys_to_virt(pdpt_entry.addr());
    let pd = unsafe { &*pd_virt.as_ptr::<PageTable>() };
    let pd_entry = &pd[pd_idx];
    if !pd_entry.is_present() {
        return false;
    }

    if pd_entry.is_huge() {
        return pd_entry.is_cow();
    }

    let pt_virt = phys_to_virt(pd_entry.addr());
    let pt = unsafe { &*pt_virt.as_ptr::<PageTable>() };
    let pt_entry = &pt[pt_idx];

    pt_entry.is_present() && pt_entry.is_cow()
}

/// Get the physical address mapped at a virtual address
pub fn get_phys_addr(pml4_phys: PhysAddr, virt: VirtAddr) -> Option<PhysAddr> {
    let pml4_virt = phys_to_virt(pml4_phys);
    let pml4 = unsafe { &*pml4_virt.as_ptr::<PageTable>() };

    let pml4_idx = ((virt.as_u64() >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((virt.as_u64() >> 30) & 0x1FF) as usize;
    let pd_idx = ((virt.as_u64() >> 21) & 0x1FF) as usize;
    let pt_idx = ((virt.as_u64() >> 12) & 0x1FF) as usize;

    let pml4_entry = &pml4[pml4_idx];
    if !pml4_entry.is_present() {
        return None;
    }

    let pdpt_virt = phys_to_virt(pml4_entry.addr());
    let pdpt = unsafe { &*pdpt_virt.as_ptr::<PageTable>() };
    let pdpt_entry = &pdpt[pdpt_idx];
    if !pdpt_entry.is_present() {
        return None;
    }

    if pdpt_entry.is_huge() {
        // 1GB page
        let offset = virt.as_u64() & 0x3FFF_FFFF;
        return Some(PhysAddr::new(pdpt_entry.addr().as_u64() + offset));
    }

    let pd_virt = phys_to_virt(pdpt_entry.addr());
    let pd = unsafe { &*pd_virt.as_ptr::<PageTable>() };
    let pd_entry = &pd[pd_idx];
    if !pd_entry.is_present() {
        return None;
    }

    if pd_entry.is_huge() {
        // 2MB page
        let offset = virt.as_u64() & 0x1F_FFFF;
        return Some(PhysAddr::new(pd_entry.addr().as_u64() + offset));
    }

    let pt_virt = phys_to_virt(pd_entry.addr());
    let pt = unsafe { &*pt_virt.as_ptr::<PageTable>() };
    let pt_entry = &pt[pt_idx];
    if !pt_entry.is_present() {
        return None;
    }

    // 4KB page
    let offset = virt.as_u64() & 0xFFF;
    Some(PhysAddr::new(pt_entry.addr().as_u64() + offset))
}
