//! Page fault handler for the OXIDE kernel.
//!
//! -- GraveShift: The gate between alive and dead processes --
//! Handles COW resolution and dynamic stack growth. One wrong
//! move here and the whole userland flatlines. No pressure.

use mm_paging::{PageTable, PageTableFlags, phys_to_virt};
use mm_vma::{VmFlags, VmType};
use mm_vmstat::Counter as VmC;
use os_core::{PhysAddr, VirtAddr};
use proc::handle_cow_fault;
use spin::Mutex;

use mm_manager::mm;

/// — SableWire: no hardcoded stack geometry. The VMA system is the single source of
/// truth for what addresses are valid. Exec creates VMAs, fork clones them, and the
/// page fault handler consults VMAs to decide what to map. Guessing address ranges
/// led to faults at 0x7FFFFFFFD000 (argv/envp area) falling through all handlers
/// because it was above the old hardcoded USER_STACK_TOP. Never again. — SableWire

/// -- BlackLatch: Spinlock for stack growth serialization --
/// Separate from COW lock to avoid contention between the two paths.
static STACK_GROWTH_LOCK: Mutex<()> = Mutex::new(());

/// Page fault handler callback (for COW and other page faults)
pub fn page_fault_handler(fault_addr: u64, error_code: u64, _rip: u64) -> bool {
    // — TorqueJax: Every page fault counted. Zero exceptions.
    mm_vmstat::inc(VmC::PgFault);

    // Check if this is a write fault on a present page (potential COW)
    let is_present = error_code & 1 != 0;
    let is_write = error_code & 2 != 0;
    let is_user = error_code & 4 != 0;

    // — BlackLatch: Get actual CR3 to compare with what process_table says
    let actual_cr3: u64 = crate::arch::read_page_table_root().as_u64();

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

        // — BlackLatch: Try to handle as COW fault. handle_cow_fault DOES acquire
        // COW_FAULT_LOCK, but uses try_lock so it's safe from exception context.
        // If contended, it returns false and we fall through to stack growth.
        if handle_cow_fault(VirtAddr::new(fault_addr), pml4, mm()) {
            // — TorqueJax: COW resolution succeeded — count it.
            mm_vmstat::inc(VmC::PgFaultCow);
            debug_cow!("[PF] COW handled OK");
            return true; // Fault handled
        } else {
            debug_cow!("[PF] COW handler failed - not a COW page");
        }
    }

    // — IronGhost: Swap page-in check. If the PTE is not-present but has the
    // swap marker bit (bit 1), this page was evicted to swap by the reclaim engine.
    // Decode the swap entry, page in from swap cache or disk, remap the frame.
    if !is_present && is_userspace_addr {
        let pml4_phys_swap = PhysAddr::new(actual_cr3 & !0xFFF);
        let page_addr_swap = fault_addr & !0xFFF;
        if let Some(pte_val) = read_pte_value(page_addr_swap, pml4_phys_swap) {
            if mm_swap::SwapEntry::is_swap_entry(pte_val) {
                if let Some(entry) = mm_swap::SwapEntry::decode(pte_val) {
                    if let Some(phys) = mm_swap::page_in(&entry) {
                        // — IronGhost: Frame recovered from swap. Remap it.
                        if let Some(pt_entry) = get_pt_entry_mut(page_addr_swap, pml4_phys_swap) {
                            let flags = PageTableFlags::PRESENT | PageTableFlags::USER
                                | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
                            pt_entry.set(phys, flags);
                            mm_reclaim::lru_add(phys, mm_reclaim::LruList::InactiveAnon);
                            os_core::tlb_flush(page_addr_swap);
                            mm_vmstat::inc(VmC::PgFaultAnon);
                            debug_cow!("[PF] Swap page-in OK for {:#x}", fault_addr);
                            return true;
                        }
                    }
                }
            }
        }
    }

    // — SableWire: VMA-based fault resolution. The VMA system knows every valid
    // mapping — stack, heap, mmap, bss, data. No hardcoded address ranges, no
    // guessing. If classify_fault_by_vma returns None (lock contended), return
    // true to retry — the lock will be free on the next fault. This replaces the
    // old "legacy fallback" that guessed stack boundaries and missed the argv/envp
    // area entirely, causing infinite fault loops. — SableWire
    if is_userspace_addr {
        let pml4_phys = PhysAddr::new(actual_cr3 & !0xFFF);
        let page_addr = fault_addr & !0xFFF;

        match classify_fault_by_vma(fault_addr) {
            VmaLookup::Mapped(vm_type, vm_flags) => {
                let access_ok = if is_write {
                    vm_flags.contains(VmFlags::WRITE)
                } else {
                    vm_flags.contains(VmFlags::READ) || vm_flags.contains(VmFlags::EXEC)
                };

                // — NeonRoot: demand paging for anon/heap/bss/data VMAs
                if !is_present && access_ok
                    && (vm_type == VmType::Anon || vm_type == VmType::Heap
                        || vm_type == VmType::Bss || vm_type == VmType::Data)
                {
                    if handle_demand_page(page_addr, pml4_phys, vm_flags) {
                        // — TorqueJax: Anonymous demand page — zeroed frame mapped.
                        mm_vmstat::inc(VmC::PgFaultAnon);
                        debug_cow!("[PF] Demand page handled OK for {:#x}", fault_addr);
                        return true;
                    }
                }

                // — NeonRoot: stack growth (GROWSDOWN) — handles both present
                // (COW fallback) and not-present (demand growth) cases.
                if is_write && (vm_type == VmType::Stack && vm_flags.contains(VmFlags::GROWSDOWN)) {
                    if handle_stack_growth(page_addr, pml4_phys) {
                        // — TorqueJax: Stack grew downward — new page mapped.
                        mm_vmstat::inc(VmC::PgFaultStackGrowth);
                        debug_cow!("[PF] VMA stack growth handled OK for {:#x}", fault_addr);
                        return true;
                    }
                }
            }
            VmaLookup::Contended => {
                // — SableWire: VMA metadata lock was contended in exception context.
                // Treat as transient and retry the faulting instruction.
                debug_cow!("[PF] VMA lock contended for {:#x}, retrying", fault_addr);
                return true;
            }
            VmaLookup::Unmapped => {
                debug_cow!("[PF] No VMA for {:#x}", fault_addr);
            }
        }
    }

    // — BlackLatch: Kernel stack overflow detection.
    // Guard pages are in the physical direct map (0xFFFF_8000 + phys_ram_range).
    // Only flag as guard page if the faulting address is plausibly within
    // physical RAM (< 4GB for typical QEMU configs). Wild pointers to 103TB
    // physical offsets are NOT guard pages — they're corruption and should fall
    // through to the generic page fault handler.
    const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;
    const MAX_PLAUSIBLE_PHYS: u64 = 0x2_0000_0000; // 8GB — generous ceiling
    if !is_present && !is_user && fault_addr >= PHYS_MAP_BASE {
        let phys_equiv = fault_addr - PHYS_MAP_BASE;
        // — BlackLatch: Only treat this as a guard page hit if the physical
        // address is within plausible RAM range. Otherwise it's a wild pointer.
        if phys_equiv >= MAX_PLAUSIBLE_PHYS {
            return false; // Not a guard page — let generic handler deal with it
        }
        let rsp: u64 = crate::arch::read_stack_pointer();

        // — BlackLatch: Log the guard hit with enough context to reconstruct the crime.
        // This fires in the #PF handler, so serial output is our only lifeline.
        unsafe {
            os_log::write_str_raw("
[GUARD] *** KERNEL STACK OVERFLOW ***
");
            os_log::write_str_raw("[GUARD] fault_addr=");
        }
        // Use the ConsoleWriter for formatted output
        {
            use core::fmt::Write;
            struct RawWriter;
            impl Write for RawWriter {
                fn write_str(&mut self, s: &str) -> core::fmt::Result {
                    unsafe { os_log::write_str_raw(s); }
                    Ok(())
                }
            }
            let mut w = RawWriter;
            let _ = writeln!(w, "
[KSTACK OVERFLOW DETECTED]");
            let _ = writeln!(w, "  fault_addr = {:#018x}", fault_addr);
            let _ = writeln!(w, "  phys_equiv = {:#018x}", phys_equiv);
            let _ = writeln!(w, "  rip        = {:#018x}", _rip);
            let _ = writeln!(w, "  rsp        = {:#018x}", rsp);
            let _ = writeln!(w, "  error_code = {:#x}", error_code);
            let _ = writeln!(w, "Kernel stack has been exhausted — guard page hit.");
            let _ = writeln!(w, "System must halt.");
        }

        // — BlackLatch: Return false → the exception handler will panic/halt.
        // We don't try to recover. A blown kernel stack has undefined state.
        return false;
    }

    debug_cow!("[PF] Fault NOT handled - will panic");

    // For instruction fetch faults (bit 4 set), dump page table flags for debugging
    let is_instruction_fetch = error_code & 0x10 != 0;
    if is_instruction_fetch {
        dump_page_table_flags(fault_addr, actual_cr3);
    }

    false // Fault not handled - will panic
}

/// -- GraveShift: Demand-page a single stack frame into the process address space --
///
/// Walks the 4-level page table hierarchy, creating intermediate tables as
/// needed, then maps a zeroed data frame at `page_addr`.
///
/// # Safety
/// Caller must ensure `page_addr` is page-aligned and within the valid stack region.
/// The page table pointed to by `pml4_phys` must be the current process's PML4.
fn handle_stack_growth(page_addr: u64, pml4_phys: PhysAddr) -> bool {
    // — BlackLatch: C8 — Use try_lock instead of lock. If a page fault fires
    // inside an ISR (e.g., kernel stack overflow during interrupt handling) and
    // the interrupted code already holds the stack growth lock or a buddy zone
    // lock, .lock() would deadlock forever — the interrupted code can never
    // release its lock because we're in its exception handler. try_lock returns
    // None immediately if contended. The process gets killed (SIGSEGV) which is
    // infinitely better than a permanent deadlock that freezes the entire CPU.
    let _guard = match STACK_GROWTH_LOCK.try_lock() {
        Some(g) => g,
        None => {
            debug_cow!("[PF] Stack growth lock contended — bailing to prevent deadlock");
            return false;
        }
    };

    let allocator = mm();

    // -- GraveShift: Extract page table indices from faulting address --
    let pml4_idx = ((page_addr >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((page_addr >> 30) & 0x1FF) as usize;
    let pd_idx = ((page_addr >> 21) & 0x1FF) as usize;
    let pt_idx = ((page_addr >> 12) & 0x1FF) as usize;

    let table_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER;

    // -- GraveShift: Walk PML4 -> PDPT --
    let pml4_virt = phys_to_virt(pml4_phys);
    let pml4 = unsafe { &mut *pml4_virt.as_mut_ptr::<PageTable>() };
    let pml4_entry = &mut pml4[pml4_idx];

    let pdpt_phys = if pml4_entry.is_present() {
        pml4_entry.addr()
    } else {
        let frame = match allocator.alloc_frame() {
            Ok(f) => f,
            Err(_) => return false,
        };
        // -- BlackLatch: Zero before linking -- stale data = security hole --
        let virt = phys_to_virt(frame);
        let table = unsafe { &mut *virt.as_mut_ptr::<PageTable>() };
        table.clear();
        pml4_entry.set(frame, table_flags);
        frame
    };

    // -- GraveShift: Walk PDPT -> PD --
    let pdpt_virt = phys_to_virt(pdpt_phys);
    let pdpt = unsafe { &mut *pdpt_virt.as_mut_ptr::<PageTable>() };
    let pdpt_entry = &mut pdpt[pdpt_idx];

    if pdpt_entry.is_huge() {
        debug_cow!("[PF] Stack growth blocked: PDPT is huge page");
        return false;
    }

    let pd_phys = if pdpt_entry.is_present() {
        pdpt_entry.addr()
    } else {
        let frame = match allocator.alloc_frame() {
            Ok(f) => f,
            Err(_) => return false,
        };
        let virt = phys_to_virt(frame);
        let table = unsafe { &mut *virt.as_mut_ptr::<PageTable>() };
        table.clear();
        pdpt_entry.set(frame, table_flags);
        frame
    };

    // -- GraveShift: Walk PD -> PT --
    let pd_virt = phys_to_virt(pd_phys);
    let pd = unsafe { &mut *pd_virt.as_mut_ptr::<PageTable>() };
    let pd_entry = &mut pd[pd_idx];

    if pd_entry.is_huge() {
        debug_cow!("[PF] Stack growth blocked: PD is huge page");
        return false;
    }

    let pt_phys = if pd_entry.is_present() {
        pd_entry.addr()
    } else {
        let frame = match allocator.alloc_frame() {
            Ok(f) => f,
            Err(_) => return false,
        };
        let virt = phys_to_virt(frame);
        let table = unsafe { &mut *virt.as_mut_ptr::<PageTable>() };
        table.clear();
        pd_entry.set(frame, table_flags);
        frame
    };

    // -- GraveShift: Check PT entry -- race protection --
    let pt_virt = phys_to_virt(pt_phys);
    let pt = unsafe { &mut *pt_virt.as_mut_ptr::<PageTable>() };
    let pt_entry = &mut pt[pt_idx];

    if pt_entry.is_present() {
        // — BlackLatch: Page exists. If it's writable, another CPU beat us —
        // but our local TLB still has the stale non-present entry that triggered
        // this fault. We MUST flush the local TLB or we'll infinite-loop:
        // CPU retries → stale TLB → same fault → "already present" → return true
        // → CPU retries → same stale TLB → forever. This was the bug that caused
        // hundreds of [PF-DIAG] HANDLED OK for the same address. — SableWire
        if pt_entry.is_writable() {
            debug_cow!("[PF] Stack page already present+writable (race), flushing TLB");
            os_core::tlb_flush(page_addr);
            return true;
        }
        debug_cow!("[PF] Stack page present but NOT writable (COW?) — not a stack growth issue");
        return false;
    }

    // — GraveShift: Allocate the actual data frame for the stack page.
    // If THIS alloc fails, intermediate tables (PDPT/PD/PT) created above
    // are still linked into the page table tree. This is safe because:
    //   - The process gets false → SIGSEGV → process killed
    //   - On process exit, UserAddressSpace::Drop walks the ENTIRE PT tree
    //     and frees all intermediate frames it discovers during the walk.
    //     This catches frames from ANY allocation path — TrackingAllocator
    //     or direct alloc_frame() like we use here.
    //   - On retry (if we don't kill), the tables already exist so the next
    //     stack growth attempt only allocates the missing data frame
    let data_frame = match allocator.alloc_frame() {
        Ok(f) => f,
        Err(_) => return false,
    };

    // -- BlackLatch: Zero the data frame -- stack pages must start clean --
    let data_virt = phys_to_virt(data_frame);
    unsafe {
        core::ptr::write_bytes(data_virt.as_mut_ptr::<u8>(), 0, 4096);
    }

    // — GraveShift: Mark data frame in page frame database as mapped user page
    if let Some(db) = mm_pagedb::try_pagedb() {
        if let Some(pf) = db.get(data_frame) {
            pf.set_flags(mm_pagedb::PF_ALLOCATED | mm_pagedb::PF_MAPPED);
        }
    }

    // — IronGhost: Stack pages go on InactiveAnon LRU — same as demand pages.
    mm_reclaim::lru_add(data_frame, mm_reclaim::LruList::InactiveAnon);

    // -- GraveShift: Map with PRESENT | WRITABLE | USER | NO_EXECUTE --
    // Stack data is never executable. NX bit keeps us honest.
    let data_flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::USER
        | PageTableFlags::NO_EXECUTE;
    pt_entry.set(data_frame, data_flags);

    // — BlackLatch: LOCAL TLB flush only. Page fault handlers run with IF=0 —
    // cross-CPU TLB shootdown IPIs deadlock if the target is also in a fault handler.
    // New page mappings only affect THIS process on THIS CPU. Other CPUs will fault
    // if they access the page, and the fault handler will see it's now present.
    os_core::tlb_flush(page_addr);

    true
}

/// — NeonRoot: Demand-page a single zeroed frame into the process address space.
///
/// Handles MAP_ANONYMOUS + MAP_PRIVATE pages that were not eagerly allocated.
/// Walks the 4-level page table, creating intermediate entries as needed,
/// then maps a zeroed frame with appropriate permissions from the VMA.
fn handle_demand_page(page_addr: u64, pml4_phys: PhysAddr, vm_flags: VmFlags) -> bool {
    let allocator = mm();

    let pml4_idx = ((page_addr >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((page_addr >> 30) & 0x1FF) as usize;
    let pd_idx = ((page_addr >> 21) & 0x1FF) as usize;
    let pt_idx = ((page_addr >> 12) & 0x1FF) as usize;

    let table_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER;

    // — NeonRoot: Walk PML4 → PDPT → PD → PT, creating entries as needed.
    // Same pattern as handle_stack_growth — factored differently because
    // the data page flags differ (stack is always RW+NX, demand pages follow VMA).
    let pml4_virt = phys_to_virt(pml4_phys);
    let pml4 = unsafe { &mut *pml4_virt.as_mut_ptr::<PageTable>() };
    let pml4_entry = &mut pml4[pml4_idx];

    let pdpt_phys = if pml4_entry.is_present() {
        pml4_entry.addr()
    } else {
        let frame = match allocator.alloc_frame() { Ok(f) => f, Err(_) => return false };
        let virt = phys_to_virt(frame);
        unsafe { &mut *virt.as_mut_ptr::<PageTable>() }.clear();
        pml4_entry.set(frame, table_flags);
        frame
    };

    let pdpt_virt = phys_to_virt(pdpt_phys);
    let pdpt = unsafe { &mut *pdpt_virt.as_mut_ptr::<PageTable>() };
    let pdpt_entry = &mut pdpt[pdpt_idx];
    if pdpt_entry.is_huge() { return false; }

    let pd_phys = if pdpt_entry.is_present() {
        pdpt_entry.addr()
    } else {
        let frame = match allocator.alloc_frame() { Ok(f) => f, Err(_) => return false };
        let virt = phys_to_virt(frame);
        unsafe { &mut *virt.as_mut_ptr::<PageTable>() }.clear();
        pdpt_entry.set(frame, table_flags);
        frame
    };

    let pd_virt = phys_to_virt(pd_phys);
    let pd = unsafe { &mut *pd_virt.as_mut_ptr::<PageTable>() };
    let pd_entry = &mut pd[pd_idx];
    if pd_entry.is_huge() { return false; }

    let pt_phys = if pd_entry.is_present() {
        pd_entry.addr()
    } else {
        let frame = match allocator.alloc_frame() { Ok(f) => f, Err(_) => return false };
        let virt = phys_to_virt(frame);
        unsafe { &mut *virt.as_mut_ptr::<PageTable>() }.clear();
        pd_entry.set(frame, table_flags);
        frame
    };

    let pt_virt = phys_to_virt(pt_phys);
    let pt = unsafe { &mut *pt_virt.as_mut_ptr::<PageTable>() };
    let pt_entry = &mut pt[pt_idx];

    if pt_entry.is_present() {
        // — NeonRoot: Race — another CPU handled it first. Flush local TLB
        // so our stale non-present entry doesn't cause an infinite fault loop.
        // Without this, the CPU retries the instruction with the stale TLB entry
        // and faults again forever. — SableWire
        os_core::tlb_flush(page_addr);
        return true;
    }

    let data_frame = match allocator.alloc_frame() {
        Ok(f) => f,
        Err(_) => return false,
    };

    // — NeonRoot: Zero the frame — anonymous pages start clean
    let data_virt = phys_to_virt(data_frame);
    unsafe { core::ptr::write_bytes(data_virt.as_mut_ptr::<u8>(), 0, 4096); }

    // — NeonRoot: Mark in pagedb
    if let Some(db) = mm_pagedb::try_pagedb() {
        if let Some(pf) = db.get(data_frame) {
            pf.set_flags(mm_pagedb::PF_ALLOCATED | mm_pagedb::PF_MAPPED);
        }
    }

    // — IronGhost: Add to LRU — anonymous demand pages go on InactiveAnon.
    // Reclaim scanner will promote to Active if accessed again.
    mm_reclaim::lru_add(data_frame, mm_reclaim::LruList::InactiveAnon);

    // — NeonRoot: Build page table flags from VMA permissions
    let mut data_flags = PageTableFlags::PRESENT | PageTableFlags::USER;
    if vm_flags.contains(VmFlags::WRITE) {
        data_flags |= PageTableFlags::WRITABLE;
    }
    if !vm_flags.contains(VmFlags::EXEC) {
        data_flags |= PageTableFlags::NO_EXECUTE;
    }
    pt_entry.set(data_frame, data_flags);

    // — BlackLatch: LOCAL TLB flush only — same reasoning as handle_stack_growth.
    // Page fault handlers run with IF=0, cross-CPU IPIs deadlock.
    os_core::tlb_flush(page_addr);

    true
}

/// — NeonRoot: Try to classify a fault address using VMA metadata.
/// Uses try_lock on the scheduler's ProcessMeta to avoid deadlocking
/// in exception context. Returns None if the lock is contended or
/// no VMA covers the faulting address.
enum VmaLookup {
    Mapped(VmType, VmFlags),
    Contended,
    Unmapped,
}

fn classify_fault_by_vma(fault_addr: u64) -> VmaLookup {
    let pid = match sched::current_pid() {
        Some(pid) => pid,
        None => return VmaLookup::Unmapped,
    };

    let meta_arc = match sched::try_get_task_meta(pid) {
        Some(meta) => meta,
        None => return VmaLookup::Contended,
    };

    let meta = match meta_arc.try_lock() {
        Some(meta) => meta,
        None => return VmaLookup::Contended,
    };

    match meta.address_space.vmas.find(fault_addr) {
        Some(vma) => VmaLookup::Mapped(vma.vm_type, vma.flags),
        None => VmaLookup::Unmapped,
    }
}

/// — IronGhost: Read the raw PTE value for a virtual address by walking the page table.
/// Returns None if any intermediate table is not present. Used by swap page-in to
/// check for swap marker bits in non-present PTEs.
fn read_pte_value(page_addr: u64, pml4_phys: PhysAddr) -> Option<u64> {
    let pml4_idx = ((page_addr >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((page_addr >> 30) & 0x1FF) as usize;
    let pd_idx = ((page_addr >> 21) & 0x1FF) as usize;
    let pt_idx = ((page_addr >> 12) & 0x1FF) as usize;

    let pml4 = unsafe { &*phys_to_virt(pml4_phys).as_ptr::<PageTable>() };
    let pml4e = &pml4[pml4_idx];
    if !pml4e.is_present() { return None; }

    let pdpt = unsafe { &*phys_to_virt(pml4e.addr()).as_ptr::<PageTable>() };
    let pdpte = &pdpt[pdpt_idx];
    if !pdpte.is_present() { return None; }
    if pdpte.is_huge() { return None; }

    let pd = unsafe { &*phys_to_virt(pdpte.addr()).as_ptr::<PageTable>() };
    let pde = &pd[pd_idx];
    if !pde.is_present() { return None; }
    if pde.is_huge() { return None; }

    let pt = unsafe { &*phys_to_virt(pde.addr()).as_ptr::<PageTable>() };
    // — IronGhost: Return the RAW PTE value — including non-present swap entries.
    Some(pt[pt_idx].raw())
}

/// — IronGhost: Get a mutable reference to a PTE for updating during swap page-in.
/// Returns None if any intermediate table is not present.
fn get_pt_entry_mut(page_addr: u64, pml4_phys: PhysAddr) -> Option<&'static mut mm_paging::PageTableEntry> {
    let pml4_idx = ((page_addr >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((page_addr >> 30) & 0x1FF) as usize;
    let pd_idx = ((page_addr >> 21) & 0x1FF) as usize;
    let pt_idx = ((page_addr >> 12) & 0x1FF) as usize;

    let pml4 = unsafe { &*phys_to_virt(pml4_phys).as_ptr::<PageTable>() };
    let pml4e = &pml4[pml4_idx];
    if !pml4e.is_present() { return None; }

    let pdpt = unsafe { &*phys_to_virt(pml4e.addr()).as_ptr::<PageTable>() };
    let pdpte = &pdpt[pdpt_idx];
    if !pdpte.is_present() { return None; }
    if pdpte.is_huge() { return None; }

    let pd = unsafe { &*phys_to_virt(pdpte.addr()).as_ptr::<PageTable>() };
    let pde = &pd[pd_idx];
    if !pde.is_present() { return None; }
    if pde.is_huge() { return None; }

    let pt = unsafe { &mut *phys_to_virt(pde.addr()).as_mut_ptr::<PageTable>() };
    Some(&mut pt[pt_idx])
}

/// Dump page table flags for debugging NX issues
/// — PatchBay: Outputs to os_log → console, not serial
fn dump_page_table_flags(fault_addr: u64, cr3: u64) {
    use core::fmt::Write;

    struct OsLogWriter;
    impl Write for OsLogWriter {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            unsafe {
                os_log::write_str_raw(s);
            }
            Ok(())
        }
    }

    let mut writer = OsLogWriter;
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
