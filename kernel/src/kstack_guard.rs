//! Kernel stack guard page management.
//!
//! — BlackLatch: Every kernel stack gets a silent sentinel below it.
//! No PRESENT bit. No mercy. Touch it and the CPU hands you a page fault
//! that screams "kernel stack overflow" before the system dies cleanly.
//! Better a controlled death than silent heap corruption eating the world.

use mm_paging::{PageTable, PageTableFlags, PHYS_MAP_BASE, phys_to_virt, read_cr3};
use os_core::{PhysAddr, VirtAddr};
use smp;

/// — BlackLatch: Walk the current CR3's page tables and CLEAR the PTE for
/// `guard_phys`'s direct-map virtual address. No PRESENT bit = any access
/// to this page faults immediately. Since the kernel direct map is shared
/// across all PML4s (same physical page table frames in the upper half),
/// clearing it via the current CR3 affects ALL address spaces simultaneously.
///
/// # Safety
/// * `guard_phys` must be a valid physical frame that was just allocated.
/// * Must NOT be called while any CPU is touching the direct-map range
///   containing guard_phys (i.e. call right after allocation, before handing
///   the stack to any thread).
/// * The kernel direct map must be fully set up (post-init only).
pub unsafe fn unmap_guard_page(guard_phys: PhysAddr) {
    // — BlackLatch: The guard's virtual address in the kernel direct map.
    // phys_to_virt(guard_phys) = guard_phys.as_u64() + 0xFFFF_8000_0000_0000
    let guard_virt = phys_to_virt(guard_phys);

    // — BlackLatch: Current CR3 carries the kernel direct map. Because all
    // PML4s share the same physical page tables for the kernel half (entries
    // 256-511), clearing a PTE here clears it everywhere. One write, all CPUs.
    let pml4_phys = unsafe { read_cr3() };
    if !unsafe { clear_pte_for_virt(pml4_phys, guard_virt) } {
        // — BlackLatch: PTE not found — direct map isn't paged at 4KB for
        // this address? That's a 2MB or 1GB huge page. We can't split a huge
        // page here without complex surgery. Log and continue — the stack
        // still works, it just won't have a guard.
        #[cfg(feature = "debug-proc")]
        unsafe {
            arch_x86_64::serial::write_str_unsafe(
                "[GUARD] WARN: guard page PTE not found — huge page mapping?\n",
            );
        }
        return;
    }

    // — BlackLatch: TLB shootdown for the guard page. Every CPU must evict
    // its TLB entry or it can still read/write the guard frame without faulting.
    // We don't want that. Pay the IPI cost once at allocation time.
    let page_start = guard_virt.as_u64();
    let page_end = page_start + 4096;
    smp::tlb_shootdown(page_start, page_end, 0);
}

/// — BlackLatch: Remap the guard page in the kernel direct map BEFORE freeing
/// the frame back to the buddy allocator. If we free without remapping, the
/// buddy allocator writes its free-list canaries into a NOT-PRESENT page and
/// immediately faults. Remap first, THEN free. In that order. Always.
///
/// # Safety
/// * `guard_phys` must be the guard frame address passed to `unmap_guard_page`.
/// * Must be called before `free_contiguous(guard_phys, ...)`.
pub unsafe fn remap_guard_page(guard_phys: PhysAddr) {
    let guard_virt = phys_to_virt(guard_phys);
    let pml4_phys = unsafe { read_cr3() };

    // — BlackLatch: Restore PRESENT | WRITABLE | NO_EXECUTE | GLOBAL.
    // Matches the flags the bootloader used for the direct map.
    // NO_EXECUTE: data pages don't execute. GLOBAL: kernel-wide, not per-process.
    unsafe {
        set_pte_for_virt(
            pml4_phys,
            guard_virt,
            guard_phys,
            PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::NO_EXECUTE
                | PageTableFlags::GLOBAL,
        );
    }

    // — BlackLatch: TLB shootdown — tell all CPUs the page is back.
    let page_start = guard_virt.as_u64();
    let page_end = page_start + 4096;
    smp::tlb_shootdown(page_start, page_end, 0);
}

/// Walk the 4-level page table rooted at `pml4_phys` and clear the PTE for
/// `virt`. Returns true if the PTE was found and cleared, false if the mapping
/// doesn't exist at 4KB granularity (huge page or not mapped).
///
/// — BlackLatch: Only operates on 4KB PTEs. Huge pages are someone else's problem.
///
/// # Safety
/// * `pml4_phys` must point to a valid 4-level page table structure.
/// * `virt` must be a canonical kernel virtual address.
unsafe fn clear_pte_for_virt(pml4_phys: PhysAddr, virt: VirtAddr) -> bool {
    let addr = virt.as_u64();
    let pml4_idx = ((addr >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((addr >> 30) & 0x1FF) as usize;
    let pd_idx = ((addr >> 21) & 0x1FF) as usize;
    let pt_idx = ((addr >> 12) & 0x1FF) as usize;

    let pml4 = unsafe { &mut *phys_to_virt(pml4_phys).as_mut_ptr::<PageTable>() };
    let pml4_entry = &pml4[pml4_idx];
    if !pml4_entry.is_present() {
        return false;
    }

    let pdpt = unsafe { &mut *phys_to_virt(pml4_entry.addr()).as_mut_ptr::<PageTable>() };
    let pdpt_entry = &pdpt[pdpt_idx];
    if !pdpt_entry.is_present() || pdpt_entry.is_huge() {
        return false; // 1GB huge page — can't clear 4KB sub-entry
    }

    let pd = unsafe { &mut *phys_to_virt(pdpt_entry.addr()).as_mut_ptr::<PageTable>() };
    let pd_entry = &pd[pd_idx];
    if !pd_entry.is_present() || pd_entry.is_huge() {
        return false; // 2MB huge page — can't clear 4KB sub-entry
    }

    let pt = unsafe { &mut *phys_to_virt(pd_entry.addr()).as_mut_ptr::<PageTable>() };
    let pt_entry = &mut pt[pt_idx];
    if !pt_entry.is_present() {
        return false; // Already not present
    }

    // — BlackLatch: Pull the trigger. PTE goes dark.
    pt_entry.clear();
    true
}

/// Walk the page tables and set a 4KB PTE for `virt` → `phys` with `flags`.
/// Assumes all intermediate tables already exist (they do for the direct map).
/// Returns true if the PTE was written, false if intermediate tables are missing
/// or the entry is a huge page.
///
/// — BlackLatch: Direct map intermediate tables are pre-allocated at boot.
/// We never need to create them here. If they're missing, something is very wrong.
///
/// # Safety
/// * `pml4_phys` must point to a valid 4-level page table structure.
/// * `virt` must be a canonical kernel virtual address with pre-existing intermediate tables.
unsafe fn set_pte_for_virt(
    pml4_phys: PhysAddr,
    virt: VirtAddr,
    phys: PhysAddr,
    flags: PageTableFlags,
) -> bool {
    let addr = virt.as_u64();
    let pml4_idx = ((addr >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((addr >> 30) & 0x1FF) as usize;
    let pd_idx = ((addr >> 21) & 0x1FF) as usize;
    let pt_idx = ((addr >> 12) & 0x1FF) as usize;

    let pml4 = unsafe { &mut *phys_to_virt(pml4_phys).as_mut_ptr::<PageTable>() };
    let pml4_entry = &pml4[pml4_idx];
    if !pml4_entry.is_present() {
        return false;
    }

    let pdpt = unsafe { &mut *phys_to_virt(pml4_entry.addr()).as_mut_ptr::<PageTable>() };
    let pdpt_entry = &pdpt[pdpt_idx];
    if !pdpt_entry.is_present() || pdpt_entry.is_huge() {
        return false;
    }

    let pd = unsafe { &mut *phys_to_virt(pdpt_entry.addr()).as_mut_ptr::<PageTable>() };
    let pd_entry = &pd[pd_idx];
    if !pd_entry.is_present() || pd_entry.is_huge() {
        return false;
    }

    let pt = unsafe { &mut *phys_to_virt(pd_entry.addr()).as_mut_ptr::<PageTable>() };
    let pt_entry = &mut pt[pt_idx];

    // — BlackLatch: Restore the PTE. Dead page walks again.
    pt_entry.set(phys, flags);
    true
}
