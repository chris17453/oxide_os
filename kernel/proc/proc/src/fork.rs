//! Fork implementation with Copy-on-Write
//!
//! Implements the fork() system call, creating a child process
//! with a copy of the parent's address space using COW semantics.

use alloc::sync::Arc;
use alloc::vec::Vec;
use mm_cow::cow_tracker;
use mm_paging::{PageTable, PageTableFlags, phys_to_virt};
use mm_traits::FrameAllocator;
use os_core::{PhysAddr, VirtAddr};
use proc_traits::Pid;
use spin::Mutex;
use smp;

use crate::{ProcessContext, ProcessMeta, UserAddressSpace, alloc_pid};

/// — ColdCipher: C5 — RAII guard for physical frames allocated during fork.
/// If clone_address_space_cow fails partway through, dropping a Vec<PhysAddr>
/// does NOT free physical frames — it just frees the heap allocation for the Vec
/// itself. This guard ensures every allocated PT frame gets returned to the buddy
/// allocator on error. On success, call defuse() to transfer ownership.
struct FrameGuard<'a, A: FrameAllocator> {
    frames: Vec<PhysAddr>,
    allocator: &'a A,
    defused: bool,
}

impl<'a, A: FrameAllocator> FrameGuard<'a, A> {
    fn new(allocator: &'a A) -> Self {
        Self {
            frames: Vec::new(),
            allocator,
            defused: false,
        }
    }

    fn push(&mut self, frame: PhysAddr) {
        self.frames.push(frame);
    }

    /// — SableWire: Transfer ownership — the frames are now someone else's problem.
    /// Returns the collected frames for use in the child address space.
    fn defuse(mut self) -> Vec<PhysAddr> {
        self.defused = true;
        core::mem::take(&mut self.frames)
    }
}

impl<'a, A: FrameAllocator> Drop for FrameGuard<'a, A> {
    fn drop(&mut self) {
        if self.defused {
            return;
        }
        // — GraveShift: Fork failed. Every frame we allocated for the child's
        // page table tree is now orphaned. Free them all or they're lost forever.
        for &frame in &self.frames {
            self.allocator.free_frame(frame);
        }
    }
}

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

/// Result of a successful fork operation
///
/// Contains all the data needed to create a Task for the child process.
pub struct ForkResult {
    /// Child process ID
    pub child_pid: Pid,
    /// Child's ProcessMeta (to be wrapped in Arc<Mutex<>>)
    pub child_meta: ProcessMeta,
    /// Child's initial context (rax=0 for fork return value)
    pub child_context: ProcessContext,
    /// Physical address of child's kernel stack (one page ABOVE the guard frame).
    /// kernel_stack_top = phys_to_virt(kernel_stack_phys) + kernel_stack_size.
    pub kernel_stack_phys: PhysAddr,
    /// Size of kernel stack in bytes (NOT including the guard page).
    pub kernel_stack_size: usize,
    /// Physical address of the guard frame (alloc_base, one page below kernel_stack_phys).
    /// — BlackLatch: Process.rs calls kstack_guard::unmap_guard_page(guard_phys) after fork.
    /// Zero means no guard (should not happen in normal paths).
    pub guard_phys: PhysAddr,
}

/// Fork the current process
///
/// Creates a child process with a copy of the parent's address space.
/// Uses Copy-on-Write to share physical frames until written.
///
/// Returns ForkResult with all data needed to create the child Task.
/// The caller is responsible for creating the Task and adding it to the scheduler.
pub fn do_fork<A: FrameAllocator>(
    parent_pid: Pid,
    parent_meta: &ProcessMeta,
    parent_context: &ProcessContext,
    allocator: &A,
    kernel_stack_size: usize,
) -> Result<ForkResult, ForkError> {
    // Allocate child PID
    let child_pid = alloc_pid();

    // Clone address space with COW
    let child_address_space =
        unsafe { clone_address_space_cow(&parent_meta.address_space, allocator)? };

    // — BlackLatch: Allocate (stack_pages + 1) contiguous frames.
    // Frame 0 = guard page (PTE will be cleared — any write here #PFs).
    // Frames 1..N = actual kernel stack (RSP starts at the TOP of these).
    // Storing the full (pages+1) count in owned_frames ensures free_contiguous()
    // returns the correct buddy-allocator block including the guard frame.
    let kernel_stack_pages = kernel_stack_size / 4096;
    let total_pages = kernel_stack_pages + 1; // +1 for the guard frame at the bottom
    let alloc_base = allocator
        .alloc_frames(total_pages)
        .ok_or(ForkError::OutOfMemory)?;

    // — BlackLatch: Guard frame = alloc_base (lowest physical address).
    // Real stack starts one page above the guard. The caller (process.rs) will
    // call kstack_guard::unmap_guard_page(alloc_base) to clear the guard PTE.
    let guard_phys = alloc_base;
    let kernel_stack_phys = PhysAddr::new(alloc_base.as_u64() + 4096);

    // Create child's ProcessMeta (cloned from parent)
    let mut child_meta = parent_meta.clone_for_fork(child_pid, child_address_space);

    // Track the FULL allocation (guard + stack) for cleanup.
    // — GraveShift: Pass total_pages so Drop calls free_contiguous() with the
    // right buddy order. alloc_frames(33) != 32 × free_frame() + 1 × free_frame().
    child_meta.add_owned_frames(alloc_base, total_pages);

    // — BlackLatch: Register the guard page so Drop can remap it before freeing.
    // The actual PTE clear is done in process.rs (kernel crate) which has access
    // to kstack_guard::unmap_guard_page(). This just tells Drop to remap it.
    child_meta.add_guard_page(guard_phys);

    // Copy parent's context to child (will return 0 to child)
    let mut child_context = parent_context.clone();
    child_context.rax = 0; // fork returns 0 to child

    Ok(ForkResult {
        child_pid,
        child_meta,
        child_context,
        kernel_stack_phys,
        kernel_stack_size,
        guard_phys,
    })
}

/// Clone an address space with Copy-on-Write semantics
///
/// Creates a new address space that shares physical frames with the parent.
/// All writable user pages are marked read-only in both parent and child,
/// with the COW bit set.
///
/// — BlackLatch: Every page we demote to read-only here is a TLB time bomb on every
/// remote CPU. flush_tlb(virt) only nukes the LOCAL entry — other CPUs keep their
/// stale writable mapping and gleefully scribble over shared frames without ever
/// triggering a COW fault. We collect all demoted addresses and issue ONE batched
/// shootdown after the walk. One IPI, all CPUs, no silent corruption.
unsafe fn clone_address_space_cow<A: FrameAllocator>(
    parent: &UserAddressSpace,
    allocator: &A,
) -> Result<UserAddressSpace, ForkError> {
    // Get parent's PML4 physical address
    let parent_pml4_phys = parent.pml4_phys();

    // — ColdCipher: C5 — Wrap all PT frame allocations in a FrameGuard.
    // If ANY alloc_frame() call below fails, the guard's Drop frees everything
    // we allocated so far. No more phantom page table frames haunting the buddy
    // allocator after a failed fork.
    let mut guard = FrameGuard::new(allocator);

    // Allocate new PML4 for child
    let child_pml4_phys = allocator.alloc_frame().ok_or(ForkError::OutOfMemory)?;
    guard.push(child_pml4_phys);

    // Get virtual addresses for both PML4s
    let parent_pml4_virt = phys_to_virt(parent_pml4_phys);
    let child_pml4_virt = phys_to_virt(child_pml4_phys);

    let parent_pml4 = unsafe { &mut *parent_pml4_virt.as_mut_ptr::<PageTable>() };
    let child_pml4 = unsafe { &mut *child_pml4_virt.as_mut_ptr::<PageTable>() };

    // — GraveShift: Check if the frame is still on the buddy free list (magic
    // canary present = genuine double allocation). The PML4[256] check was removed —
    // it was a false positive. ALL recycled PML4 frames have stale kernel entries at
    // PML4[256] because pop_free_block() only zeros the first 24 bytes (magic/next/prev).
    // The rest of the 4KB frame retains whatever was there before. Since every process
    // shares the same kernel mappings at PML4[256..512], any recycled PML4 frame will
    // match. This is normal and harmless — we clear() the frame two lines below.
    {
        let child_first_u64 = unsafe {
            core::ptr::read_volatile(child_pml4_virt.as_ptr::<u64>())
        };
        if child_first_u64 == 0x4652454542304C {
            // — CrashBloom: Frame is STILL on the free list! Double allocation!
            unsafe {
                os_log::write_str_raw("[FORK-DOUBLE-ALLOC] PML4 frame still FREE! phys=0x");
                os_log::write_u64_hex_raw(child_pml4_phys.as_u64());
                os_log::write_str_raw("\n");
            }
        }
    }

    // Clear child PML4
    child_pml4.clear();

    // Copy kernel mappings (entries 256-511) directly
    for i in 256..512 {
        child_pml4[i] = parent_pml4[i];
    }

    // — CrashBloom: Snapshot the golden PML4[256] value right after kernel copy.
    // If anything corrupts it during the PT walk below, we'll catch it.
    let golden_256 = unsafe {
        core::ptr::read_volatile(child_pml4_virt.as_ptr::<u64>().add(256))
    };

    // — BlackLatch: Accumulate every virt addr we demote to read-only.
    // We'll fire ONE cross-CPU TLB shootdown after the full walk instead of
    // hammering the IPI bus once per page. Batch the pain, pay it once.
    let mut cow_demoted_min: u64 = u64::MAX;
    let mut cow_demoted_max: u64 = 0;
    let mut any_cow_demoted = false;

    for pml4_idx in 0..256 {
        let pml4_entry = &mut parent_pml4[pml4_idx];
        if !pml4_entry.is_present() {
            continue;
        }

        // Allocate PDPT for child
        let child_pdpt_phys = allocator.alloc_frame().ok_or(ForkError::OutOfMemory)?;
        guard.push(child_pdpt_phys);

        // — CrashBloom: Detect buddy double-alloc — if any PT frame is the same
        // as the PML4 frame, clearing it will destroy the PML4.
        if child_pdpt_phys == child_pml4_phys {
            unsafe {
                os_log::write_str_raw("[FORK-BUG] PDPT alloc returned PML4 frame! phys=0x");
                os_log::write_u64_hex_raw(child_pdpt_phys.as_u64());
                os_log::write_str_raw("\n");
            }
        }

        let parent_pdpt_virt = phys_to_virt(pml4_entry.addr());
        let child_pdpt_virt = phys_to_virt(child_pdpt_phys);

        let parent_pdpt = unsafe { &mut *parent_pdpt_virt.as_mut_ptr::<PageTable>() };
        let child_pdpt = unsafe { &mut *child_pdpt_virt.as_mut_ptr::<PageTable>() };
        child_pdpt.clear();

        // — CrashBloom: Catch corruption at the exact point
        let check_256 = unsafe { core::ptr::read_volatile(child_pml4_virt.as_ptr::<u64>().add(256)) };
        if check_256 != golden_256 {
            unsafe {
                os_log::write_str_raw("[FORK-CORRUPT-PDPT] PML4[256] destroyed by PDPT clear! pdpt_phys=0x");
                os_log::write_u64_hex_raw(child_pdpt_phys.as_u64());
                os_log::write_str_raw(" pml4=0x");
                os_log::write_u64_hex_raw(child_pml4_phys.as_u64());
                os_log::write_str_raw("\n");
            }
        }

        // Set child PML4 entry
        // IMPORTANT: Do NOT propagate NO_EXECUTE from parent - intermediate entries
        // should never have NO_EXECUTE set (only leaf PTEs should control it)
        child_pml4[pml4_idx].set(
            child_pdpt_phys,
            (pml4_entry.flags() & !PageTableFlags::NO_EXECUTE)
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
                if pdpt_entry.is_writable() {
                    // — BlackLatch: 1GB writable huge page → COW demote.
                    let flags = pdpt_entry.flags();
                    pdpt_entry.set_flags(flags & !PageTableFlags::WRITABLE | PageTableFlags::COW);
                    child_pdpt[pdpt_idx].set(
                        pdpt_entry.addr(),
                        flags & !PageTableFlags::WRITABLE | PageTableFlags::COW,
                    );
                    cow_tracker().increment(pdpt_entry.addr());
                    let virt = compute_virt_addr(pml4_idx, pdpt_idx, 0, 0);
                    let v = virt.as_u64();
                    cow_demoted_min = cow_demoted_min.min(v);
                    cow_demoted_max = cow_demoted_max.max(v + (1u64 << 30));
                    any_cow_demoted = true;
                } else {
                    // — BlackLatch: Read-only 1GB huge page — still COW-tag it.
                    let flags = pdpt_entry.flags() | PageTableFlags::COW;
                    pdpt_entry.set_flags(flags);
                    child_pdpt[pdpt_idx].set(pdpt_entry.addr(), flags);
                    cow_tracker().increment(pdpt_entry.addr());
                }
                continue;
            }

            // Allocate PD for child
            let child_pd_phys = allocator.alloc_frame().ok_or(ForkError::OutOfMemory)?;
            guard.push(child_pd_phys);

            let parent_pd_virt = phys_to_virt(pdpt_entry.addr());
            let child_pd_virt = phys_to_virt(child_pd_phys);

            let parent_pd = unsafe { &mut *parent_pd_virt.as_mut_ptr::<PageTable>() };
            let child_pd = unsafe { &mut *child_pd_virt.as_mut_ptr::<PageTable>() };
            child_pd.clear();

            // — CrashBloom: Catch corruption at the exact point it happens
            let check_256 = unsafe { core::ptr::read_volatile(child_pml4_virt.as_ptr::<u64>().add(256)) };
            if check_256 != golden_256 {
                unsafe {
                    os_log::write_str_raw("[FORK-CORRUPT-PD] PML4[256] destroyed by PD clear! pd_phys=0x");
                    os_log::write_u64_hex_raw(child_pd_phys.as_u64());
                    os_log::write_str_raw(" pml4=0x");
                    os_log::write_u64_hex_raw(child_pml4_phys.as_u64());
                    os_log::write_str_raw("\n");
                }
            }

            // Set child PDPT entry
            // IMPORTANT: Do NOT propagate NO_EXECUTE from parent - intermediate entries
            // should never have NO_EXECUTE set (only leaf PTEs should control it)
            child_pdpt[pdpt_idx].set(
                child_pd_phys,
                (pdpt_entry.flags() & !PageTableFlags::NO_EXECUTE)
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
                    if pd_entry.is_writable() {
                        // — BlackLatch: 2MB writable huge page → COW demote.
                        let flags = pd_entry.flags();
                        pd_entry.set_flags(flags & !PageTableFlags::WRITABLE | PageTableFlags::COW);
                        child_pd[pd_idx].set(
                            pd_entry.addr(),
                            flags & !PageTableFlags::WRITABLE | PageTableFlags::COW,
                        );
                        cow_tracker().increment(pd_entry.addr());
                        let virt = compute_virt_addr(pml4_idx, pdpt_idx, pd_idx, 0);
                        let v = virt.as_u64();
                        cow_demoted_min = cow_demoted_min.min(v);
                        cow_demoted_max = cow_demoted_max.max(v + (1u64 << 21));
                        any_cow_demoted = true;
                    } else {
                        // — BlackLatch: Read-only 2MB huge page — still COW-tag it.
                        let flags = pd_entry.flags() | PageTableFlags::COW;
                        pd_entry.set_flags(flags);
                        child_pd[pd_idx].set(pd_entry.addr(), flags);
                        cow_tracker().increment(pd_entry.addr());
                    }
                    continue;
                }

                // Allocate PT for child
                let child_pt_phys = allocator.alloc_frame().ok_or(ForkError::OutOfMemory)?;
                guard.push(child_pt_phys);

                let parent_pt_virt = phys_to_virt(pd_entry.addr());
                let child_pt_virt = phys_to_virt(child_pt_phys);

                let parent_pt = unsafe { &mut *parent_pt_virt.as_mut_ptr::<PageTable>() };
                let child_pt = unsafe { &mut *child_pt_virt.as_mut_ptr::<PageTable>() };
                child_pt.clear();

                // — CrashBloom: Catch corruption at the exact point
                let check_256 = unsafe { core::ptr::read_volatile(child_pml4_virt.as_ptr::<u64>().add(256)) };
                if check_256 != golden_256 {
                    unsafe {
                        os_log::write_str_raw("[FORK-CORRUPT-PT] PML4[256] destroyed by PT clear! pt_phys=0x");
                        os_log::write_u64_hex_raw(child_pt_phys.as_u64());
                        os_log::write_str_raw(" pml4=0x");
                        os_log::write_u64_hex_raw(child_pml4_phys.as_u64());
                        os_log::write_str_raw("\n");
                    }
                }

                // Set child PD entry
                // IMPORTANT: Do NOT propagate NO_EXECUTE from parent - intermediate entries
                // should never have NO_EXECUTE set (only leaf PTEs should control it)
                child_pd[pd_idx].set(
                    child_pt_phys,
                    (pd_entry.flags() & !PageTableFlags::NO_EXECUTE)
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

                    if pt_entry.is_writable() {
                        // — BlackLatch: Writable page → must mark COW in both
                        // parent and child. Strip write, add COW. Standard Linux
                        // fork behavior — nobody writes for free after fork().
                        let flags = pt_entry.flags();
                        let new_flags = flags & !PageTableFlags::WRITABLE | PageTableFlags::COW;
                        pt_entry.set_flags(new_flags);
                        child_pt[pt_idx].set(pt_entry.addr(), new_flags);
                        cow_tracker().increment(pt_entry.addr());

                        // — BlackLatch: Track demoted page for batch shootdown.
                        // We do NOT call flush_tlb(virt) here — per-page local INVLPG
                        // is worthless; other CPUs still see writable. Defer to the
                        // batched cross-CPU shootdown below.
                        let virt = compute_virt_addr(pml4_idx, pdpt_idx, pd_idx, pt_idx);
                        let v = virt.as_u64();
                        cow_demoted_min = cow_demoted_min.min(v);
                        cow_demoted_max = cow_demoted_max.max(v + 0x1000);
                        any_cow_demoted = true;
                    } else {
                        // — BlackLatch: Read-only page — still mark COW. Without
                        // VMAs we can't tell if this is genuinely read-only (code)
                        // or a writable-segment page that's temporarily RO. COW is
                        // harmless for real code pages (nobody writes, so the COW
                        // handler is never invoked). For data pages, COW gives the
                        // child a private copy on write instead of a SIGSEGV.
                        // Defense in depth — one bit to rule them all.
                        let flags = pt_entry.flags() | PageTableFlags::COW;
                        pt_entry.set_flags(flags);
                        child_pt[pt_idx].set(pt_entry.addr(), flags);
                        cow_tracker().increment(pt_entry.addr());
                    }
                }
            }
        }
    }

    // — BlackLatch: The page table walk is done. ALL demoted-to-read-only pages
    // live in [cow_demoted_min, cow_demoted_max). Fire ONE cross-CPU TLB shootdown
    // to evict every stale writable entry from every CPU's TLB simultaneously.
    // Without this, a parent thread on another core has a valid writable TLB entry
    // and writes past the COW guard straight into shared memory. That's not a race —
    // that's corruption wearing a cape.
    //
    // invalidate_range() auto-promotes to a full CR3 reload if the range covers
    // more than 32 pages, so large address spaces pay a flat cost, not O(pages).
    if any_cow_demoted {
        smp::tlb_shootdown(cow_demoted_min, cow_demoted_max, 0);
    }

    // — CrashBloom: Post-fork PML4[256] validation. If any PT frame allocation
    // during the walk stomped on the child PML4 (buddy double-alloc), PML4[256]
    // would be zeroed or corrupted. Catch it before the child is ever scheduled.
    {
        let final_256 = unsafe {
            core::ptr::read_volatile(child_pml4_virt.as_ptr::<u64>().add(256))
        };
        let parent_256 = unsafe {
            core::ptr::read_volatile(parent_pml4_virt.as_ptr::<u64>().add(256))
        };
        if final_256 != parent_256 {
            unsafe {
                os_log::write_str_raw("[FORK-CORRUPT] Child PML4[256] CHANGED during fork! child=0x");
                os_log::write_u64_hex_raw(final_256);
                os_log::write_str_raw(" parent=0x");
                os_log::write_u64_hex_raw(parent_256);
                os_log::write_str_raw(" pml4=0x");
                os_log::write_u64_hex_raw(child_pml4_phys.as_u64());
                os_log::write_str_raw("\n");
            }
        }
    }

    // — SableWire: Success path — defuse the guard and transfer ownership of all
    // PT frames to the child address space. From here, UserAddressSpace::Drop
    // owns them and will free them when the child process exits.
    // — NeonRoot: Clone the parent's VMA metadata for the child. O(n) Vec clone.
    // The PT walk above already handled the actual COW marking on physical frames;
    // this is just the semantic overlay that says "these pages are stack/heap/text".
    let child_vmas = parent.vmas.clone_for_fork();
    let child_frames = guard.defuse();
    let child_as = unsafe { UserAddressSpace::from_raw(child_pml4_phys, child_frames, child_vmas) };

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
    // — BlackLatch: try_lock, not lock. This runs from the page fault handler
    // (exception context). If another CPU holds this lock and WE take a page
    // fault while spinning, we deadlock the CPU forever. try_lock returns None
    // immediately if contended — the fault returns false, the process gets
    // SIGSEGV, which is infinitely better than a permanent deadlock.
    //
    // The old comment in fault.rs ("handle_cow_fault doesn't acquire locks")
    // was a lie. It does. Now it does so safely.
    let _guard = match COW_FAULT_LOCK.try_lock() {
        Some(g) => g,
        None => return false,
    };

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
            return true; // Success - already fixed
        }
        return false; // Not COW and not writable = error
    }

    let old_phys = pt_entry.addr();
    let cow = cow_tracker();

    // — ColdCipher: TOCTOU fix. The old code did ref_count() [read lock] then
    // remove()/decrement() [write lock]. Between the two, a concurrent fork()
    // on another CPU can increment the count. Result: both parent and child
    // think they own the frame exclusively, both make it writable, both scribble
    // on the same physical memory. Silent corruption, no signals, no mercy.
    //
    // try_claim_exclusive() holds a SINGLE write lock for the entire operation:
    // check the count, decide, and act — all atomic. If we're the last owner,
    // it removes the entry and returns true. If shared, it decrements and
    // returns false (we must copy). No window for fork() to sneak in.
    if cow.try_claim_exclusive(old_phys) {
        // — ColdCipher: Sole owner confirmed under lock. Make writable in-place.
        let mut flags = pt_entry.flags();
        flags |= PageTableFlags::WRITABLE;
        flags &= !PageTableFlags::COW;
        pt_entry.set_flags(flags);
    } else {
        // — ColdCipher: Still shared. Copy the frame — don't touch the original.
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

        // Update page table entry to point to our private copy
        let mut flags = pt_entry.flags();
        flags |= PageTableFlags::WRITABLE;
        flags &= !PageTableFlags::COW;
        pt_entry.set(new_phys, flags);

        // — ColdCipher: Decrement already happened inside try_claim_exclusive().
        // Do NOT call decrement() again or we'd double-decrement.
    }

    // Flush TLB for this page on ALL CPUs
    // This is critical for multi-CPU correctness - other CPUs may have stale
    // TLB entries marking the page as read-only
    let page_start = fault_addr.as_u64() & !0xFFF;
    let page_end = page_start + 0x1000;
    smp::tlb_shootdown(page_start, page_end, 0);

    true
}
