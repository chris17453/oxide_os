//! Page fault handler for the OXIDE kernel.

use mm_paging::{PageTable, phys_to_virt};
use os_core::{PhysAddr, VirtAddr};
use proc::handle_cow_fault;

use crate::memory::FrameAllocatorWrapper;

/// Page fault handler callback (for COW and other page faults)
pub fn page_fault_handler(fault_addr: u64, error_code: u64, _rip: u64) -> bool {
    // Check if this is a write fault on a present page (potential COW)
    let is_present = error_code & 1 != 0;
    let is_write = error_code & 2 != 0;
    let is_user = error_code & 4 != 0;

    // Get actual CR3 to compare with what process_table says
    let actual_cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) actual_cr3);
    }

    debug_cow!(
        "[PF] fault_addr={:#x} error={:#x} rip={:#x}",
        fault_addr,
        error_code,
        _rip
    );
    debug_cow!(
        "[PF] present={} write={} user={} actual_cr3={:#x}",
        is_present,
        is_write,
        is_user,
        actual_cr3
    );

    // COW faults are: present + write
    // Can occur from user mode OR kernel mode (e.g., copy_to_user)
    let is_userspace_addr = fault_addr < 0x0000_8000_0000_0000;

    if is_present && is_write {
        let pml4 = if is_user || is_userspace_addr {
            // For user-mode faults or kernel-mode faults to userspace:
            // Use current CR3 directly to avoid lock acquisition in exception context
            PhysAddr::new(actual_cr3 & !0xFFF) // Mask off flags
        } else {
            // Not a userspace access
            debug_cow!("[PF] Not userspace access, skipping COW");
            return false;
        };

        debug_cow!(
            "[PF] COW check: fault_addr={:#x} pml4={:#x}",
            fault_addr,
            pml4.as_u64()
        );

        let alloc = FrameAllocatorWrapper;

        // Try to handle as COW fault
        // This is safe from exception context because handle_cow_fault doesn't acquire locks
        if handle_cow_fault(VirtAddr::new(fault_addr), pml4, &alloc) {
            debug_cow!("[PF] COW handled OK");
            return true; // Fault handled
        } else {
            debug_cow!("[PF] COW handler failed - not a COW page");
        }
    }

    debug_cow!("[PF] Fault NOT handled - will panic");

    // For instruction fetch faults (bit 4 set), dump page table flags for debugging
    let is_instruction_fetch = error_code & 0x10 != 0;
    if is_instruction_fetch {
        dump_page_table_flags(fault_addr, actual_cr3);
    }

    false // Fault not handled - will panic
}

/// Dump page table flags for debugging NX issues
fn dump_page_table_flags(fault_addr: u64, cr3: u64) {
    use arch_x86_64::serial::SerialWriter;
    use core::fmt::Write;

    let mut writer = SerialWriter;
    let _ = writeln!(writer, "[DEBUG] Page table walk for {:#x}:", fault_addr);

    let pml4_phys = PhysAddr::new(cr3 & !0xFFF);
    let pml4_virt = phys_to_virt(pml4_phys);
    let pml4 = unsafe { &*pml4_virt.as_ptr::<PageTable>() };

    let pml4_idx = ((fault_addr >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((fault_addr >> 30) & 0x1FF) as usize;
    let pd_idx = ((fault_addr >> 21) & 0x1FF) as usize;
    let pt_idx = ((fault_addr >> 12) & 0x1FF) as usize;

    let pml4_entry = &pml4[pml4_idx];
    let _ = writeln!(
        writer,
        "  PML4[{}] = {:#018x} (present={}, nx={})",
        pml4_idx,
        pml4_entry.raw(),
        pml4_entry.is_present(),
        pml4_entry.raw() & (1 << 63) != 0
    );

    if !pml4_entry.is_present() {
        let _ = writeln!(writer, "  PML4 entry not present!");
        return;
    }

    let pdpt_virt = phys_to_virt(pml4_entry.addr());
    let pdpt = unsafe { &*pdpt_virt.as_ptr::<PageTable>() };
    let pdpt_entry = &pdpt[pdpt_idx];
    let _ = writeln!(
        writer,
        "  PDPT[{}] = {:#018x} (present={}, nx={})",
        pdpt_idx,
        pdpt_entry.raw(),
        pdpt_entry.is_present(),
        pdpt_entry.raw() & (1 << 63) != 0
    );

    if !pdpt_entry.is_present() {
        let _ = writeln!(writer, "  PDPT entry not present!");
        return;
    }

    if pdpt_entry.is_huge() {
        let _ = writeln!(writer, "  PDPT is 1GB huge page");
        return;
    }

    let pd_virt = phys_to_virt(pdpt_entry.addr());
    let pd = unsafe { &*pd_virt.as_ptr::<PageTable>() };
    let pd_entry = &pd[pd_idx];
    let _ = writeln!(
        writer,
        "  PD[{}] = {:#018x} (present={}, nx={})",
        pd_idx,
        pd_entry.raw(),
        pd_entry.is_present(),
        pd_entry.raw() & (1 << 63) != 0
    );

    if !pd_entry.is_present() {
        let _ = writeln!(writer, "  PD entry not present!");
        return;
    }

    if pd_entry.is_huge() {
        let _ = writeln!(writer, "  PD is 2MB huge page");
        return;
    }

    let pt_virt = phys_to_virt(pd_entry.addr());
    let pt = unsafe { &*pt_virt.as_ptr::<PageTable>() };
    let pt_entry = &pt[pt_idx];
    let _ = writeln!(
        writer,
        "  PT[{}] = {:#018x} (present={}, nx={})",
        pt_idx,
        pt_entry.raw(),
        pt_entry.is_present(),
        pt_entry.raw() & (1 << 63) != 0
    );
}
