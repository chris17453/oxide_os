//! Page fault handler for the OXIDE kernel.

use arch_x86_64::serial;
use core::fmt::Write;
use os_core::{PhysAddr, VirtAddr};
use proc::handle_cow_fault;

use crate::memory::FrameAllocatorWrapper;

/// Page fault handler callback (for COW and other page faults)
pub fn page_fault_handler(fault_addr: u64, error_code: u64, rip: u64) -> bool {
    // Check if this is a write fault on a present page (potential COW)
    let is_present = error_code & 1 != 0;
    let is_write = error_code & 2 != 0;
    let is_user = error_code & 4 != 0;

    // Get actual CR3 to compare with what process_table says
    let actual_cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) actual_cr3);
    }

    {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(
            writer,
            "[PF] fault_addr={:#x} error={:#x} rip={:#x}",
            fault_addr, error_code, rip
        );
        let _ = writeln!(
            writer,
            "[PF] present={} write={} user={} actual_cr3={:#x}",
            is_present, is_write, is_user, actual_cr3
        );
    }

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
            {
                let mut writer = serial::SerialWriter;
                let _ = writeln!(writer, "[PF] Not userspace access, skipping COW");
            }
            return false;
        };

        {
            let mut writer = serial::SerialWriter;
            let _ = writeln!(
                writer,
                "[PF] COW check: fault_addr={:#x} pml4={:#x}",
                fault_addr,
                pml4.as_u64()
            );
        }

        let alloc = FrameAllocatorWrapper;

        // Try to handle as COW fault
        // This is safe from exception context because handle_cow_fault doesn't acquire locks
        if handle_cow_fault(VirtAddr::new(fault_addr), pml4, &alloc) {
            {
                let mut writer = serial::SerialWriter;
                let _ = writeln!(writer, "[PF] COW handled OK");
            }
            return true; // Fault handled
        } else {
            {
                let mut writer = serial::SerialWriter;
                let _ = writeln!(writer, "[PF] COW handler failed - not a COW page");
            }
        }
    }

    {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[PF] Fault NOT handled - will panic");
    }
    false // Fault not handled - will panic
}
