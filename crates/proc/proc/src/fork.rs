//! Fork implementation with Copy-on-Write
//!
//! Implements the fork() system call, creating a child process
//! with a copy of the parent's address space using COW semantics.

use mm_cow::cow_tracker;
use mm_paging::{PageTable, PageTableFlags, flush_tlb, phys_to_virt};
use mm_traits::FrameAllocator;
use os_core::{PhysAddr, VirtAddr};
use proc_traits::Pid;
use spin::Mutex;

use crate::{Process, ProcessContext, UserAddressSpace, alloc_pid, process_table};

/// Global lock for COW fault handling
///
/// This lock protects the critical section in handle_cow_fault() where we:
/// 1. Check if page is COW
/// 2. Check reference count
/// 3. Either make writable or allocate+copy
/// 4. Update PTE
///
/// This prevents races where multiple CPUs handle COW faults on the same page.
/// It's a global lock (not ideal for performance) but ensures correctness.
static COW_FAULT_LOCK: Mutex<()> = Mutex::new(());

/// Error during fork
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkError {
    /// Out of memory
    OutOfMemory,
    /// Parent process not found
    ParentNotFound,
    /// Internal error
    Internal,
}

/// Fork the current process
///
/// Creates a child process with a copy of the parent's address space.
/// Uses Copy-on-Write to share physical frames until written.
///
/// Returns the child PID to the parent, or 0 to the child.
pub fn do_fork<A: FrameAllocator>(
    parent_pid: Pid,
    parent_context: &ProcessContext,
    allocator: &A,
) -> Result<Pid, ForkError> {
    let table = process_table();

    // Get parent process
    let parent_arc = table.get(parent_pid).ok_or(ForkError::ParentNotFound)?;
    let mut parent = parent_arc.lock();

    // Allocate child PID
    let child_pid = alloc_pid();

    // Clone address space with COW
    let child_address_space =
        unsafe { clone_address_space_cow(parent.address_space(), allocator)? };

    // Allocate kernel stack for child - inherit size from parent
    let kernel_stack_size = parent.kernel_stack_size();
    let kernel_stack_pages = kernel_stack_size / 4096;
    let kernel_stack_phys = allocator
        .alloc_frames(kernel_stack_pages)
        .ok_or(ForkError::OutOfMemory)?;

    // Create child process
    let mut child = Process::new(
        child_pid,
        parent_pid,
        child_address_space,
        kernel_stack_phys,
        kernel_stack_size,
        parent.entry_point(),
        parent.user_stack_top(),
    );

    // Copy parent's context to child (will return 0 to child)
    let mut child_context = parent_context.clone();
    child_context.rax = 0; // fork returns 0 to child
    *child.context_mut() = child_context;

    // Copy credentials and process group info
    child.set_credentials(*parent.credentials());
    child.set_pgid(parent.pgid());
    child.set_sid(parent.sid());

    // Clone file descriptor table
    let fd_table = parent.clone_fd_table();
    child.set_fd_table(fd_table);

    // Clone cmdline and environ
    child.set_cmdline(parent.clone_cmdline());
    child.set_environ(parent.clone_environ());

    // Clone cwd
    child.set_cwd(parent.clone_cwd());

    // Add child to parent's children list
    parent.add_child(child_pid);

    // Track kernel stack frame
    child.add_owned_frame(kernel_stack_phys);

    // Add child to process table
    drop(parent); // Release parent lock before adding child
    table.add(child);

    Ok(child_pid)
}

/// Clone an address space with Copy-on-Write semantics
///
/// Creates a new address space that shares physical frames with the parent.
/// All writable user pages are marked read-only in both parent and child,
/// with the COW bit set.
unsafe fn clone_address_space_cow<A: FrameAllocator>(
    parent: &UserAddressSpace,
    allocator: &A,
) -> Result<UserAddressSpace, ForkError> {
    // Get parent's PML4 physical address
    let parent_pml4_phys = parent.pml4_phys();

    // Allocate new PML4 for child
    let child_pml4_phys = allocator.alloc_frame().ok_or(ForkError::OutOfMemory)?;

    // Get virtual addresses for both PML4s
    let parent_pml4_virt = phys_to_virt(parent_pml4_phys);
    let child_pml4_virt = phys_to_virt(child_pml4_phys);

    let parent_pml4 = unsafe { &mut *parent_pml4_virt.as_mut_ptr::<PageTable>() };
    let child_pml4 = unsafe { &mut *child_pml4_virt.as_mut_ptr::<PageTable>() };

    // Clear child PML4
    child_pml4.clear();

    // Copy kernel mappings (entries 256-511) directly
    for i in 256..512 {
        child_pml4[i] = parent_pml4[i];
    }

    // Clone user space entries (0-255) with COW
    let mut child_frames = alloc::vec![child_pml4_phys];

    for pml4_idx in 0..256 {
        let pml4_entry = &mut parent_pml4[pml4_idx];
        if !pml4_entry.is_present() {
            continue;
        }

        // Allocate PDPT for child
        let child_pdpt_phys = allocator.alloc_frame().ok_or(ForkError::OutOfMemory)?;
        child_frames.push(child_pdpt_phys);

        let parent_pdpt_virt = phys_to_virt(pml4_entry.addr());
        let child_pdpt_virt = phys_to_virt(child_pdpt_phys);

        let parent_pdpt = unsafe { &mut *parent_pdpt_virt.as_mut_ptr::<PageTable>() };
        let child_pdpt = unsafe { &mut *child_pdpt_virt.as_mut_ptr::<PageTable>() };
        child_pdpt.clear();

        // Set child PML4 entry
        child_pml4[pml4_idx].set(
            child_pdpt_phys,
            pml4_entry.flags()
                | PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER,
        );

        for pdpt_idx in 0..512 {
            let pdpt_entry = &mut parent_pdpt[pdpt_idx];
            if !pdpt_entry.is_present() {
                continue;
            }

            if pdpt_entry.is_huge() {
                // 1GB huge page - mark as COW if writable
                if pdpt_entry.is_writable() {
                    let flags = pdpt_entry.flags();
                    pdpt_entry.set_flags(flags & !PageTableFlags::WRITABLE | PageTableFlags::COW);
                    child_pdpt[pdpt_idx].set(
                        pdpt_entry.addr(),
                        flags & !PageTableFlags::WRITABLE | PageTableFlags::COW,
                    );
                    cow_tracker().increment(pdpt_entry.addr());
                } else {
                    child_pdpt[pdpt_idx] = *pdpt_entry;
                }
                continue;
            }

            // Allocate PD for child
            let child_pd_phys = allocator.alloc_frame().ok_or(ForkError::OutOfMemory)?;
            child_frames.push(child_pd_phys);

            let parent_pd_virt = phys_to_virt(pdpt_entry.addr());
            let child_pd_virt = phys_to_virt(child_pd_phys);

            let parent_pd = unsafe { &mut *parent_pd_virt.as_mut_ptr::<PageTable>() };
            let child_pd = unsafe { &mut *child_pd_virt.as_mut_ptr::<PageTable>() };
            child_pd.clear();

            // Set child PDPT entry
            child_pdpt[pdpt_idx].set(
                child_pd_phys,
                pdpt_entry.flags()
                    | PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::USER,
            );

            for pd_idx in 0..512 {
                let pd_entry = &mut parent_pd[pd_idx];
                if !pd_entry.is_present() {
                    continue;
                }

                if pd_entry.is_huge() {
                    // 2MB huge page - mark as COW if writable
                    if pd_entry.is_writable() {
                        let flags = pd_entry.flags();
                        pd_entry.set_flags(flags & !PageTableFlags::WRITABLE | PageTableFlags::COW);
                        child_pd[pd_idx].set(
                            pd_entry.addr(),
                            flags & !PageTableFlags::WRITABLE | PageTableFlags::COW,
                        );
                        cow_tracker().increment(pd_entry.addr());
                    } else {
                        child_pd[pd_idx] = *pd_entry;
                    }
                    continue;
                }

                // Allocate PT for child
                let child_pt_phys = allocator.alloc_frame().ok_or(ForkError::OutOfMemory)?;
                child_frames.push(child_pt_phys);

                let parent_pt_virt = phys_to_virt(pd_entry.addr());
                let child_pt_virt = phys_to_virt(child_pt_phys);

                let parent_pt = unsafe { &mut *parent_pt_virt.as_mut_ptr::<PageTable>() };
                let child_pt = unsafe { &mut *child_pt_virt.as_mut_ptr::<PageTable>() };
                child_pt.clear();

                // Set child PD entry
                child_pd[pd_idx].set(
                    child_pt_phys,
                    pd_entry.flags()
                        | PageTableFlags::PRESENT
                        | PageTableFlags::WRITABLE
                        | PageTableFlags::USER,
                );

                // Clone individual pages
                for pt_idx in 0..512 {
                    let pt_entry = &mut parent_pt[pt_idx];
                    if !pt_entry.is_present() {
                        continue;
                    }

                    // Mark as COW if writable
                    if pt_entry.is_writable() {
                        let flags = pt_entry.flags();
                        let new_flags = flags & !PageTableFlags::WRITABLE | PageTableFlags::COW;
                        pt_entry.set_flags(new_flags);
                        child_pt[pt_idx].set(pt_entry.addr(), new_flags);
                        cow_tracker().increment(pt_entry.addr());

                        // Flush TLB for this page in parent
                        let virt = compute_virt_addr(pml4_idx, pdpt_idx, pd_idx, pt_idx);
                        flush_tlb(virt);
                    } else {
                        // Read-only or non-writable page - just share
                        child_pt[pt_idx] = *pt_entry;
                        // Still increment ref count for proper cleanup
                        cow_tracker().increment(pt_entry.addr());
                    }
                }
            }
        }
    }

    // Create child address space
    let child_as = unsafe { UserAddressSpace::from_raw(child_pml4_phys, child_frames) };

    Ok(child_as)
}

/// Compute virtual address from page table indices
fn compute_virt_addr(pml4_idx: usize, pdpt_idx: usize, pd_idx: usize, pt_idx: usize) -> VirtAddr {
    let mut addr: u64 = 0;
    addr |= (pt_idx as u64) << 12;
    addr |= (pd_idx as u64) << 21;
    addr |= (pdpt_idx as u64) << 30;
    addr |= (pml4_idx as u64) << 39;

    // Sign extend for canonical address
    if pml4_idx >= 256 {
        addr |= 0xFFFF_0000_0000_0000;
    }

    VirtAddr::new(addr)
}

/// Handle a Copy-on-Write page fault
///
/// Called when a write fault occurs on a COW page.
/// Allocates a new frame, copies the contents, and makes it writable.
///
/// Returns true if the fault was handled, false otherwise.
pub fn handle_cow_fault<A: FrameAllocator>(
    fault_addr: VirtAddr,
    pml4_phys: PhysAddr,
    allocator: &A,
) -> bool {
    // Acquire global COW fault lock to prevent races
    // This ensures only one CPU handles COW faults at a time
    let _guard = COW_FAULT_LOCK.lock();

    // Walk page tables to find the faulting entry
    let pml4_virt = phys_to_virt(pml4_phys);
    let pml4 = unsafe { &mut *pml4_virt.as_mut_ptr::<PageTable>() };

    let pml4_idx = (fault_addr.as_u64() >> 39) as usize & 0x1FF;
    let pdpt_idx = (fault_addr.as_u64() >> 30) as usize & 0x1FF;
    let pd_idx = (fault_addr.as_u64() >> 21) as usize & 0x1FF;
    let pt_idx = (fault_addr.as_u64() >> 12) as usize & 0x1FF;

    // Navigate to PT entry
    let pml4_entry = &pml4[pml4_idx];
    if !pml4_entry.is_present() {
        return false;
    }

    let pdpt_virt = phys_to_virt(pml4_entry.addr());
    let pdpt = unsafe { &mut *pdpt_virt.as_mut_ptr::<PageTable>() };
    let pdpt_entry = &mut pdpt[pdpt_idx];
    if !pdpt_entry.is_present() {
        return false;
    }

    if pdpt_entry.is_huge() {
        // 1GB COW page - too complex for now, just fail
        return false;
    }

    let pd_virt = phys_to_virt(pdpt_entry.addr());
    let pd = unsafe { &mut *pd_virt.as_mut_ptr::<PageTable>() };
    let pd_entry = &mut pd[pd_idx];
    if !pd_entry.is_present() {
        return false;
    }

    if pd_entry.is_huge() {
        // 2MB COW page - too complex for now, just fail
        return false;
    }

    let pt_virt = phys_to_virt(pd_entry.addr());
    let pt = unsafe { &mut *pt_virt.as_mut_ptr::<PageTable>() };
    let pt_entry = &mut pt[pt_idx];

    // Double-check after acquiring lock: another CPU might have handled this
    if !pt_entry.is_present() {
        return false;
    }

    if !pt_entry.is_cow() {
        // Already writable - another CPU handled it
        // Check if it's actually writable now
        if pt_entry.is_writable() {
            return true;  // Success - already fixed
        }
        return false;  // Not COW and not writable = error
    }

    let old_phys = pt_entry.addr();
    let cow = cow_tracker();
    let ref_count = cow.ref_count(old_phys);

    if ref_count <= 1 {
        // We're the only reference - just make it writable
        let mut flags = pt_entry.flags();
        flags |= PageTableFlags::WRITABLE;
        flags &= !PageTableFlags::COW;
        pt_entry.set_flags(flags);
        cow.remove(old_phys);
    } else {
        // Shared page - need to copy
        let new_phys = match allocator.alloc_frame() {
            Some(f) => f,
            None => return false,
        };

        // Copy contents
        let old_virt = phys_to_virt(old_phys);
        let new_virt = phys_to_virt(new_phys);
        unsafe {
            core::ptr::copy_nonoverlapping(
                old_virt.as_ptr::<u8>(),
                new_virt.as_mut_ptr::<u8>(),
                4096,
            );
        }

        // Update page table entry
        let mut flags = pt_entry.flags();
        flags |= PageTableFlags::WRITABLE;
        flags &= !PageTableFlags::COW;
        pt_entry.set(new_phys, flags);

        // Decrement old frame's ref count
        cow.decrement(old_phys);
    }

    // Flush TLB for this page on ALL CPUs
    // This is critical for multi-CPU correctness - other CPUs may have stale
    // TLB entries marking the page as read-only
    let page_start = fault_addr.as_u64() & !0xFFF;
    let page_end = page_start + 0x1000;
    smp::tlb_shootdown(page_start, page_end, 0);

    true
}
