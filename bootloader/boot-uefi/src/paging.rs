//! Page table setup for bootloader
//!
//! Sets up 4-level x86_64 page tables with:
//! - Identity mapping for the first 4GB (for bootloader code to continue running)
//! - Direct physical map at 0xFFFF_8000_0000_0000
//! - Kernel mapping at 0xFFFF_FFFF_8000_0000

use core::ptr;
use crate::efi;

use boot_proto::{KERNEL_VIRT_BASE, PHYS_MAP_BASE};

const PAGE_SIZE: u64 = 4096;
const ENTRIES_PER_TABLE: usize = 512;

// Page table entry flags
const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const HUGE_PAGE: u64 = 1 << 7;

/// Set up page tables for the kernel
///
/// Returns the physical address of the PML4 table.
pub fn setup_page_tables(kernel_phys: u64, kernel_size: u64) -> u64 {
    // Allocate PML4
    let pml4_phys = allocate_page_table();
    let pml4 = pml4_phys as *mut [u64; ENTRIES_PER_TABLE];

    // Zero the PML4
    unsafe {
        ptr::write_bytes(pml4, 0, 1);
    }

    // Set up identity mapping for first 4GB using 2MB huge pages
    // This allows bootloader code to continue running after CR3 switch
    setup_identity_mapping(pml4_phys);

    // Set up direct physical memory map at PHYS_MAP_BASE
    // Map first 4GB of physical memory
    setup_physical_map(pml4_phys);

    // Map kernel at KERNEL_VIRT_BASE
    setup_kernel_mapping(pml4_phys, kernel_phys, kernel_size);

    pml4_phys
}

/// Set up identity mapping for first 4GB
fn setup_identity_mapping(pml4_phys: u64) {
    let pml4 = pml4_phys as *mut [u64; ENTRIES_PER_TABLE];

    // PML4 entry 0 -> PDPT for 0x0000_0000_0000_0000
    let pdpt_phys = allocate_page_table();
    unsafe {
        (*pml4)[0] = pdpt_phys | PRESENT | WRITABLE;
    }

    let pdpt = pdpt_phys as *mut [u64; ENTRIES_PER_TABLE];
    unsafe {
        ptr::write_bytes(pdpt, 0, 1);
    }

    // Map first 4GB with 1GB huge pages (PDPT entries)
    for i in 0..4 {
        let phys_addr = (i as u64) * (1024 * 1024 * 1024); // 1GB per entry
        unsafe {
            (*pdpt)[i] = phys_addr | PRESENT | WRITABLE | HUGE_PAGE;
        }
    }
}

/// Set up direct physical memory map at PHYS_MAP_BASE
fn setup_physical_map(pml4_phys: u64) {
    let pml4 = pml4_phys as *mut [u64; ENTRIES_PER_TABLE];

    // PHYS_MAP_BASE = 0xFFFF_8000_0000_0000
    // PML4 index: bits 47:39 = 256
    // Each PML4 entry covers 512GB. We need multiple entries because
    // PCIe 64-bit BARs can be placed above 512GB. Map 4TB total.
    // — TorqueJax: one PDPT per PML4 slot, 1GB huge pages throughout.
    let base_pml4_idx = ((PHYS_MAP_BASE >> 39) & 0x1FF) as usize; // 256
    let num_pml4_entries = 8; // 8 × 512GB = 4TB

    for entry in 0..num_pml4_entries {
        let pml4_idx = base_pml4_idx + entry;
        if pml4_idx >= ENTRIES_PER_TABLE {
            break;
        }

        let pdpt_phys = allocate_page_table();
        unsafe {
            (*pml4)[pml4_idx] = pdpt_phys | PRESENT | WRITABLE;
        }

        let pdpt = pdpt_phys as *mut [u64; ENTRIES_PER_TABLE];
        unsafe {
            ptr::write_bytes(pdpt, 0, 1);
        }

        // Map 512GB per PDPT with 1GB huge pages
        for i in 0..512 {
            let phys_addr = ((entry as u64) * 512 + (i as u64)) * (1024 * 1024 * 1024);
            unsafe {
                (*pdpt)[i] = phys_addr | PRESENT | WRITABLE | HUGE_PAGE;
            }
        }
    }
}

/// Map kernel at KERNEL_VIRT_BASE using 4KB pages
fn setup_kernel_mapping(pml4_phys: u64, kernel_phys: u64, kernel_size: u64) {
    let pml4 = pml4_phys as *mut [u64; ENTRIES_PER_TABLE];

    // KERNEL_VIRT_BASE = 0xFFFF_FFFF_8000_0000
    // PML4 index: bits 47:39 = 511
    let pml4_idx = ((KERNEL_VIRT_BASE >> 39) & 0x1FF) as usize;

    // Allocate PDPT
    let pdpt_phys = allocate_page_table();
    unsafe {
        (*pml4)[pml4_idx] = pdpt_phys | PRESENT | WRITABLE;
    }

    let pdpt = pdpt_phys as *mut [u64; ENTRIES_PER_TABLE];
    unsafe {
        ptr::write_bytes(pdpt, 0, 1);
    }

    // PDPT index for KERNEL_VIRT_BASE: bits 38:30 = 510
    let pdpt_idx = ((KERNEL_VIRT_BASE >> 30) & 0x1FF) as usize;

    // Allocate PD
    let pd_phys = allocate_page_table();
    unsafe {
        (*pdpt)[pdpt_idx] = pd_phys | PRESENT | WRITABLE;
    }

    let pd = pd_phys as *mut [u64; ENTRIES_PER_TABLE];
    unsafe {
        ptr::write_bytes(pd, 0, 1);
    }

    // Map kernel using 4KB pages (kernel may not be 2MB aligned)
    let kernel_pages_4kb = (kernel_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let mut current_virt = KERNEL_VIRT_BASE;
    let mut current_phys = kernel_phys;

    for _ in 0..kernel_pages_4kb {
        // Calculate PD and PT indices for current virtual address
        let pd_idx = ((current_virt >> 21) & 0x1FF) as usize;
        let pt_idx = ((current_virt >> 12) & 0x1FF) as usize;

        // Get or create PT
        let pt_phys = unsafe {
            let pd_entry = (*pd)[pd_idx];
            if pd_entry & PRESENT == 0 {
                let new_pt = allocate_page_table();
                (*pd)[pd_idx] = new_pt | PRESENT | WRITABLE;
                new_pt
            } else {
                pd_entry & !0xFFF
            }
        };

        let pt = pt_phys as *mut [u64; ENTRIES_PER_TABLE];

        // Map the page
        unsafe {
            (*pt)[pt_idx] = current_phys | PRESENT | WRITABLE;
        }

        current_virt += PAGE_SIZE;
        current_phys += PAGE_SIZE;
    }
}

/// Allocate a page table (one 4KB page, zeroed)
/// — SableWire: raw UEFI page allocation — no wrappers, no excuses
fn allocate_page_table() -> u64 {
    let addr = efi::allocate_pages(1).expect("Failed to allocate page table");

    // Zero the page
    unsafe {
        ptr::write_bytes(addr as *mut u8, 0, PAGE_SIZE as usize);
    }

    addr
}
