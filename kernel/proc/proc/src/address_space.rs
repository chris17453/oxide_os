//! User address space management
//!
//! Creates and manages user-mode virtual address spaces.
//!
//! — GraveShift: Every process takes a PML4 + PDPT/PD/PT tree + user data frames.
//! Used to be: none of it came back. Now it does. You're welcome, buddy allocator.

use alloc::vec::Vec;
use core::cell::RefCell;
use mm_cow::cow_tracker;
use mm_manager::try_mm;
use mm_paging::{MapError as PagingMapError, PageMapper, PageTable, PageTableFlags};
use mm_paging::{phys_to_virt, write_cr3};
use mm_traits::FrameAllocator;
use mm_vma::{VmArea, VmAreaError, VmAreaList};
use os_core::{PhysAddr, VirtAddr};
use proc_traits::{AddressSpace, MapError, MemoryFlags, UnmapError};

/// User virtual address space
///
/// Manages page tables for a user process. The kernel higher-half
/// is shared across all user address spaces.
pub struct UserAddressSpace {
    /// Physical address of the PML4
    pml4_phys: PhysAddr,
    /// Page mapper for this address space
    mapper: PageMapper,
    /// Frames allocated for this address space's page tables
    /// (so we can free them when the address space is destroyed)
    allocated_frames: Vec<PhysAddr>,
    /// — NeonRoot: Virtual memory area metadata — tracks what's mapped where.
    /// The page tables are still the authority for frame ownership; this is
    /// the semantic layer that knows "this is a stack" vs "this is .text".
    pub vmas: VmAreaList,
}

impl UserAddressSpace {
    /// Create a new user address space, copying kernel mappings from the
    /// current address space.
    ///
    /// # Safety
    /// Must be called with a valid frame allocator. The current page tables
    /// must have the kernel properly mapped in the higher half.
    pub unsafe fn new_with_kernel<A: FrameAllocator>(
        allocator: &A,
        kernel_pml4: PhysAddr,
    ) -> Option<Self> {
        // Allocate a new PML4
        let pml4_frame = allocator.alloc_frame()?;

        // Zero the new PML4
        let pml4_virt = phys_to_virt(pml4_frame);
        let new_pml4 = unsafe { &mut *pml4_virt.as_mut_ptr::<PageTable>() };
        new_pml4.clear();

        // Copy kernel mappings (entries 256-511) from the kernel's PML4
        // These entries cover the higher half of virtual address space
        let kernel_pml4_virt = phys_to_virt(kernel_pml4);
        let kernel_pml4_table = unsafe { &*kernel_pml4_virt.as_ptr::<PageTable>() };

        for i in 256..512 {
            new_pml4[i] = kernel_pml4_table[i];
        }

        let mapper = unsafe { PageMapper::new(pml4_frame) };

        Some(Self {
            pml4_phys: pml4_frame,
            mapper,
            // — CrashBloom: PML4 is tracked as self.pml4_phys — NOT in allocated_frames.
            // Having it in both causes double-free: Drop walks PT tree + frees pml4_phys
            // explicitly, then also frees everything in allocated_frames. Two frees of the
            // same frame = buddy corruption = PML4[0] shows up as FREEB0L canary.
            allocated_frames: Vec::new(),
            vmas: VmAreaList::new(),
        })
    }

    /// Get the physical address of the PML4 table
    pub fn pml4_phys(&self) -> PhysAddr {
        self.pml4_phys
    }

    /// — ByteRiot: How many PT structure frames does this address space own?
    /// Used by the OOM killer to score memory hogs. Doesn't count user data
    /// frames (those are in the page tables, not in allocated_frames). But PT
    /// frame count correlates with mapped pages — good enough proxy for RSS.
    pub fn allocated_frames_count(&self) -> usize {
        self.allocated_frames.len()
    }

    /// — GraveShift: Hollow out this address space for early exit cleanup.
    /// Returns a new UserAddressSpace owning all the frames, leaving self
    /// as an empty husk (pml4=0, no frames). The caller drops the returned
    /// value after switching CR3 away. Self's Drop becomes a no-op.
    pub fn take_for_exit(&mut self) -> Self {
        let taken = Self {
            pml4_phys: self.pml4_phys,
            mapper: unsafe { PageMapper::new(self.pml4_phys) },
            allocated_frames: core::mem::take(&mut self.allocated_frames),
            vmas: core::mem::take(&mut self.vmas),
        };
        self.pml4_phys = PhysAddr::new(0);
        taken
    }

    /// Create from raw PML4 and frame list
    ///
    /// # Safety
    /// The PML4 must be a valid page table and all frames in the list
    /// must be owned by this address space.
    pub unsafe fn from_raw(pml4_phys: PhysAddr, frames: Vec<PhysAddr>, vmas: VmAreaList) -> Self {
        // — CrashBloom: PML4 must NEVER appear in allocated_frames. It's tracked
        // separately as self.pml4_phys and freed explicitly in Drop. Duplicates = double-free.
        debug_assert!(
            !frames.contains(&pml4_phys),
            "PML4 must not be in allocated_frames — double-free guaranteed"
        );
        let mapper = unsafe { PageMapper::new(pml4_phys) };
        Self {
            pml4_phys,
            mapper,
            allocated_frames: frames,
            vmas,
        }
    }

    /// Map a page in user space
    ///
    /// # Safety
    /// The physical address must be valid memory.
    pub unsafe fn map_user_page<A: FrameAllocator>(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: MemoryFlags,
        allocator: &A,
    ) -> Result<(), MapError> {
        // Verify this is a user-space address (lower half)
        if virt.as_u64() >= 0x0000_8000_0000_0000 {
            return Err(MapError::InvalidAddress);
        }

        // Convert MemoryFlags to PageTableFlags
        let mut pt_flags = PageTableFlags::PRESENT | PageTableFlags::USER;

        if flags.writable() {
            pt_flags |= PageTableFlags::WRITABLE;
        }

        if !flags.executable() {
            pt_flags |= PageTableFlags::NO_EXECUTE;
        }

        // Use a wrapper allocator that tracks allocated frames
        let tracking_allocator = TrackingAllocator {
            inner: allocator,
            allocated: RefCell::new(&mut self.allocated_frames),
        };

        self.mapper
            .map(virt, phys, pt_flags, &tracking_allocator)
            .map_err(|e| match e {
                PagingMapError::AlreadyMapped => MapError::AlreadyMapped,
                PagingMapError::FrameAllocationFailed => MapError::OutOfMemory,
                PagingMapError::ParentIsHugePage => MapError::InvalidAddress,
            })
    }

    /// Unmap a page from user space
    pub fn unmap_user_page(&mut self, virt: VirtAddr) -> Result<PhysAddr, UnmapError> {
        // Verify this is a user-space address
        if virt.as_u64() >= 0x0000_8000_0000_0000 {
            return Err(UnmapError::InvalidAddress);
        }

        self.mapper.unmap(virt).ok_or(UnmapError::NotMapped)
    }

    /// Translate a virtual address to physical
    pub fn translate(&self, virt: VirtAddr) -> Option<PhysAddr> {
        self.mapper.translate(virt)
    }

    /// Update flags for an already-mapped user page
    ///
    /// Adds additional permissions (union with existing flags).
    /// Returns true if successful, false if page is not mapped.
    pub fn update_user_page_flags(&mut self, virt: VirtAddr, add_flags: MemoryFlags) -> bool {
        // Verify this is a user-space address
        if virt.as_u64() >= 0x0000_8000_0000_0000 {
            return false;
        }

        // Convert MemoryFlags to PageTableFlags
        let mut pt_flags = PageTableFlags::empty();

        if add_flags.writable() {
            pt_flags |= PageTableFlags::WRITABLE;
        }

        // Note: We don't need to add PRESENT or USER since page is already mapped
        // For NO_EXECUTE, we'd need to handle it specially (it's a "remove" operation)
        // For now we only handle adding WRITABLE permission

        self.mapper.update_flags(virt, pt_flags)
    }

    /// Switch to this address space
    ///
    /// # Safety
    /// Must only be called when it's safe to switch page tables.
    pub unsafe fn activate(&self) {
        unsafe { write_cr3(self.pml4_phys) };
    }

    /// Map a range of pages in user space
    ///
    /// # Safety
    /// All physical addresses in the range must be valid.
    pub unsafe fn map_user_range<A: FrameAllocator>(
        &mut self,
        virt_start: VirtAddr,
        phys_start: PhysAddr,
        size: usize,
        flags: MemoryFlags,
        allocator: &A,
    ) -> Result<(), MapError> {
        let page_size = 4096;
        let pages = (size + page_size - 1) / page_size;

        for i in 0..pages {
            let offset = (i * page_size) as u64;
            let virt = VirtAddr::new(virt_start.as_u64() + offset);
            let phys = PhysAddr::new(phys_start.as_u64() + offset);
            unsafe { self.map_user_page(virt, phys, flags, allocator)? };
        }

        Ok(())
    }

    /// Allocate and map pages at the given virtual address
    ///
    /// Allocates physical frames and maps them to the given virtual address range.
    pub fn allocate_pages<A: FrameAllocator>(
        &mut self,
        virt_start: VirtAddr,
        num_pages: usize,
        flags: MemoryFlags,
        allocator: &A,
    ) -> Result<(), MapError> {
        // Linux-style allocation — GraveShift: Allocate all frames first (lock released after each alloc),
        // then zero and map them. Each alloc_frame() call locks->allocates->unlocks atomically.
        // Mapping can then safely allocate PT pages without deadlock.

        let mut frames = alloc::vec::Vec::with_capacity(num_pages);

        // Phase 1: Allocate all frames (each alloc is independent, no lock held across calls)
        //
        // — GraveShift: User data frames are NOT tracked in allocated_frames.
        // They live in the page tables — Drop walks the PT to find and free them.
        // allocated_frames is reserved for page table frames (PML4/PDPT/PD/PT)
        // so the Drop impl can free them unconditionally without COW checks.
        for i in 0..num_pages {
            // — GraveShift: Alloc progress trace, gated (fires per-page = insane serial load)
            #[cfg(feature = "debug-paging")]
            if i % 10 == 0 && i > 0 {
                unsafe {
                    os_log::write_str_raw("[ALLOC] page ");
                    os_log::write_u32_raw(i as u32);
                    os_log::write_str_raw("\n");
                }
            }

            let frame = match allocator.alloc_frame() {
                Some(f) => f,
                None => {
                    // — BlackLatch: OOM is FATAL — always report, bounded serial
                    unsafe {
                        os_log::write_str_raw("[ALLOC-ERROR] OOM at frame ");
                        os_log::write_u32_raw(i as u32);
                        os_log::write_str_raw("\n");
                    }
                    // — ColdCipher: C6 — Free all frames we already allocated before
                    // returning. The old code just returned Err and dropped the Vec,
                    // which leaks every physical frame collected so far. These frames
                    // aren't in allocated_frames (by design — they're user data frames
                    // tracked via PT walk), so Drop can't find them. We must free them
                    // explicitly here or they're gone forever. One OOM per fork/exec
                    // cycle = slow memory death.
                    for &leaked in &frames {
                        allocator.free_frame(leaked);
                    }
                    return Err(MapError::OutOfMemory);
                }
            };
            // — GraveShift: Do NOT push to allocated_frames here. User data frames
            // are freed via page table walk in Drop. Only PT-structure frames belong here.
            frames.push(frame);

            // — TorqueJax: Per-frame alloc trace, gated (100 pages = 3KB serial)
            #[cfg(feature = "debug-paging")]
            unsafe {
                os_log::write_str_raw("[FRAME-ALLOC] phys=");
                os_log::write_u64_hex_raw(frame.as_u64());
                os_log::write_str_raw("\n");
            }
        }

        // Phase 2: Zero and map (lock not held, mapping can safely allocate PT pages)
        for (i, &frame) in frames.iter().enumerate() {
            let offset = (i * 4096) as u64;
            let virt = VirtAddr::new(virt_start.as_u64() + offset);

            // Zero the frame
            let frame_virt = phys_to_virt(frame);

            // — GraveShift: Zero-start trace, gated (100 pages × 60 bytes = 6KB serial per exec)
            #[cfg(feature = "debug-paging")]
            unsafe {
                os_log::write_str_raw("[ZERO-START] phys=");
                os_log::write_u64_hex_raw(frame.as_u64());
                os_log::write_str_raw(" virt=");
                os_log::write_u64_hex_raw(frame_virt.as_u64());
                os_log::write_str_raw("\n");
            }

            // — ColdCipher: Free block canary check — FATAL, always on, bounded serial.
            // If the buddy allocator handed us a frame that still has the free-list
            // magic canary, the free list is corrupted (frame was double-allocated).
            // Skip this frame entirely — zeroing it would corrupt the free list further.
            const FREE_BLOCK_MAGIC: u64 = 0x4652454542304C;
            let has_canary = unsafe {
                core::ptr::read_volatile(frame_virt.as_ptr::<u64>()) == FREE_BLOCK_MAGIC
            };
            if has_canary {
                unsafe {
                    os_log::write_str_raw("[FATAL] allocate_pages: frame still has FREE canary! phys=0x");
                    os_log::write_u64_hex_raw(frame.as_u64());
                    os_log::write_str_raw(" — skipping (buddy corruption)\n");
                }
                continue;
            }

            // — GraveShift: Zero the frame
            unsafe {
                core::ptr::write_bytes(frame_virt.as_mut_ptr::<u8>(), 0, 4096);
            }

            // — GraveShift: Zero-done trace, gated
            #[cfg(feature = "debug-paging")]
            unsafe {
                os_log::write_str_raw("[ZERO-DONE] phys=");
                os_log::write_u64_hex_raw(frame.as_u64());
                os_log::write_str_raw("\n");
            }

            // — GraveShift: Mark user data frame in page frame database
            if let Some(db) = mm_pagedb::try_pagedb() {
                if let Some(pf) = db.get(frame) {
                    pf.set_flags(mm_pagedb::PF_ALLOCATED | mm_pagedb::PF_MAPPED);
                }
            }

            // — BlackLatch: Map the page. If mapping fails (PT allocation OOM),
            // free all remaining unmapped frames. Already-mapped frames are tracked
            // in the page tables and will be freed by Drop's PT walk.
            if let Err(e) = unsafe { self.map_user_page(virt, frame, flags, allocator) } {
                // — ColdCipher: C6 Phase 2 — Free this frame and all subsequent
                // unmapped frames. frames[0..i] are already mapped and owned by PTs.
                allocator.free_frame(frame);
                for &remaining in &frames[i + 1..] {
                    allocator.free_frame(remaining);
                }
                return Err(e);
            }
        }

        Ok(())
    }

    /// — NeonRoot: Add a VMA to the address space metadata. Errors are
    /// non-fatal — the page tables are the real authority. VMA overlap just
    /// means our bookkeeping is slightly off, not that memory is corrupted.
    pub fn add_vma(&mut self, vma: VmArea) -> Result<(), VmAreaError> {
        self.vmas.insert(vma)
    }

    /// — NeonRoot: Remove VMAs covering [start, end). Returns the affected VMAs.
    pub fn remove_vma_range(&mut self, start: u64, end: u64) -> Result<Vec<VmArea>, VmAreaError> {
        self.vmas.remove(start, end)
    }
}

impl AddressSpace for UserAddressSpace {
    fn new() -> Self {
        panic!("Use UserAddressSpace::new_with_kernel() instead");
    }

    fn page_table_root(&self) -> PhysAddr {
        self.pml4_phys
    }

    unsafe fn map(
        &mut self,
        _virt: VirtAddr,
        _phys: PhysAddr,
        _flags: MemoryFlags,
    ) -> Result<(), MapError> {
        // This trait method doesn't have an allocator, so we can't implement it properly
        // Use map_user_page instead
        Err(MapError::OutOfMemory)
    }

    unsafe fn unmap(&mut self, virt: VirtAddr) -> Result<PhysAddr, UnmapError> {
        self.unmap_user_page(virt)
    }

    fn translate(&self, virt: VirtAddr) -> Option<PhysAddr> {
        self.mapper.translate(virt)
    }
}

/// Drop implementation for UserAddressSpace — the anti-OOM guarantee.
///
/// — GraveShift: Six months of process churn, six months of frame leaks.
/// Every process exit used to vaporize its entire address space into the void.
/// PML4, PDPTs, PDs, PTs, user data — all of it just gone. The buddy
/// allocator's free list shrank by a few hundred frames on every exec.
/// This is the fix. This is the floor that catches what used to fall forever.
///
/// Strategy:
///   1. Walk the user-space portion of the PML4 (entries 0-255) and free
///      every leaf page frame via COW-aware logic:
///        - decrement the COW tracker
///        - free to buddy only if we're the last owner (count == 0 after dec)
///        - frames not in the COW tracker (count == 0 initially) are exclusively
///          owned and always freed
///   2. Free all frames in `allocated_frames` — these are page-table structure
///      frames (PML4, PDPTs, PDs, PTs) that are always exclusively owned.
///
/// Kernel mappings (PML4 indices 256-511) are shared across all address spaces
/// and must NEVER be freed here — they live for the entire kernel lifetime.
///
/// # Safety invariants
/// - try_mm() is used so this is safe to call even if mm isn't initialized
///   (kernel tasks with empty address spaces hit this path on shutdown)
/// - The PML4 frame itself is in allocated_frames[0]; it is freed last in step 2
/// - We never touch CR3 here — the process is already dead by the time Drop runs
impl Drop for UserAddressSpace {
    fn drop(&mut self) {
        // — GraveShift: No mm = early boot task or mm went away. Either way,
        // we can't free. Frames are gone. That's acceptable for boot-time tasks.
        let mm = match try_mm() {
            Some(m) => m,
            None => return,
        };

        // — GraveShift: Kernel pseudo-tasks (idle, etc.) have pml4_phys == 0
        // and an empty allocated_frames. Nothing to walk, nothing to free.
        if self.pml4_phys.as_u64() == 0 {
            return;
        }

        let cow = cow_tracker();

        // ----------------------------------------------------------------
        // Step 1: Walk user-space page tables (PML4 indices 0-255) and
        // free all leaf user data frames with COW reference counting.
        // — GraveShift: The walk is O(mapped pages). For typical userspace
        // processes that's a few hundred frames — fast enough for exit path.
        // ----------------------------------------------------------------

        // Safety: pml4_phys is valid — we checked above. phys_to_virt maps
        // physical addresses into the kernel's direct-map region.
        let pml4_virt = phys_to_virt(self.pml4_phys);
        let pml4 = unsafe { &*pml4_virt.as_ptr::<PageTable>() };

        #[cfg(feature = "debug-proc")]
        let mut freed_leaf: u32 = 0;
        #[cfg(feature = "debug-proc")]
        let mut skipped_cow: u32 = 0;

        // — TorqueJax: Collect ALL PT structure frames discovered during the walk,
        // not just the ones in allocated_frames. handle_stack_growth() creates
        // intermediate PDPT/PD/PT frames via direct alloc_frame() calls that bypass
        // the TrackingAllocator, so they're invisible to allocated_frames. By
        // collecting them during the walk we catch every frame regardless of how it
        // was allocated. The PML4 itself is freed separately (it's always index 0
        // in allocated_frames or we handle it explicitly).
        let mut walked_pt_frames: Vec<PhysAddr> = Vec::new();

        // Only walk user-space (indices 0-255). Kernel half (256-511) is shared.
        for pml4_idx in 0..256usize {
            let pml4_entry = &pml4[pml4_idx];
            if !pml4_entry.is_present() {
                continue;
            }

            let pdpt_phys = pml4_entry.addr();
            walked_pt_frames.push(pdpt_phys);
            let pdpt_virt = phys_to_virt(pdpt_phys);
            let pdpt = unsafe { &*pdpt_virt.as_ptr::<PageTable>() };

            for pdpt_idx in 0..512usize {
                let pdpt_entry = &pdpt[pdpt_idx];
                if !pdpt_entry.is_present() {
                    continue;
                }

                if pdpt_entry.is_huge() {
                    let phys = pdpt_entry.addr();
                    if let Some(db) = mm_pagedb::try_pagedb() {
                        if let Some(pf) = db.get(phys) {
                            if pf.has_flag(mm_pagedb::PF_RESERVED) {
                                continue;
                            }
                        }
                    }
                    let remaining = cow.decrement(phys);
                    if remaining == 0 {
                        mm_pagedb::set_free_context(mm_pagedb::CTX_DROP_LEAF);
                        let _ = mm.free_frames(phys, 18);
                        #[cfg(feature = "debug-proc")]
                        { freed_leaf += 1; }
                    } else {
                        #[cfg(feature = "debug-proc")]
                        { skipped_cow += 1; }
                    }
                    continue;
                }

                let pd_phys = pdpt_entry.addr();
                walked_pt_frames.push(pd_phys);
                let pd_virt = phys_to_virt(pd_phys);
                let pd = unsafe { &*pd_virt.as_ptr::<PageTable>() };

                for pd_idx in 0..512usize {
                    let pd_entry = &pd[pd_idx];
                    if !pd_entry.is_present() {
                        continue;
                    }

                    if pd_entry.is_huge() {
                        let phys = pd_entry.addr();
                        if let Some(db) = mm_pagedb::try_pagedb() {
                            if let Some(pf) = db.get(phys) {
                                if pf.has_flag(mm_pagedb::PF_RESERVED) {
                                    continue;
                                }
                            }
                        }
                        let remaining = cow.decrement(phys);
                        if remaining == 0 {
                            mm_pagedb::set_free_context(mm_pagedb::CTX_DROP_LEAF);
                            let _ = mm.free_frames(phys, 9);
                            #[cfg(feature = "debug-proc")]
                            { freed_leaf += 1; }
                        } else {
                            #[cfg(feature = "debug-proc")]
                            { skipped_cow += 1; }
                        }
                        continue;
                    }

                    let pt_phys = pd_entry.addr();
                    walked_pt_frames.push(pt_phys);
                    let pt_virt = phys_to_virt(pt_phys);
                    let pt = unsafe { &*pt_virt.as_ptr::<PageTable>() };

                    for pt_idx in 0..512usize {
                        let pt_entry = &pt[pt_idx];
                        if !pt_entry.is_present() {
                            continue;
                        }

                        let phys = pt_entry.addr();

                        // — ColdCipher: Guard against corrupted/stale PT entries.
                        // Skip frames that are: reserved, already free, or page
                        // table structures. The pagedb tells us the truth about
                        // each frame's state — trust it over the PT entry.
                        //
                        // — GraveShift: The PF_PAGETABLE guard is CRITICAL. When
                        // buddy free-list corruption causes a frame to be double-
                        // allocated (once as user data, once as PT structure), a
                        // leaf PTE can point to a physical frame that's actually a
                        // PT structure belonging to another process. Without this
                        // guard, the leaf walk frees the PT frame (cow.decrement +
                        // free_frame), then Step 2's PT walk tries to free it again
                        // → DoubleFree. The ring buffer confirmed this: frame freed
                        // as Drop-leaf with flags=ALLOC|PT, then DoubleFree from
                        // Drop-pt-walk. This is the root cause of ALL remaining
                        // DoubleFree errors in fork stress tests.
                        // — ColdCipher: Full state dump for every leaf frame we
                        // touch. This is the ONLY way to trace DoubleFree root causes.
                        // Log: phys, pagedb flags, pagedb rc, cow count, PTE flags,
                        // and the VA (pml4_idx/pdpt_idx/pd_idx/pt_idx).
                        let cow_count = cow.ref_count(phys);
                        let mut skip = false;
                        if let Some(db) = mm_pagedb::try_pagedb() {
                            if let Some(pf) = db.get(phys) {
                                let flags = pf.flags();
                                let rc = pf.refcount();
                                if flags & mm_pagedb::PF_RESERVED != 0 {
                                    skip = true;
                                } else if flags == mm_pagedb::PF_FREE && rc == 0 {
                                    // — WireSaint: Frame already freed. Log it.
                                    unsafe {
                                        os_log::write_str_raw("[DROP-LEAF-STALE] phys=0x");
                                        os_log::write_u64_hex_raw(phys.as_u64());
                                        os_log::write_str_raw(" pml4=0x");
                                        os_log::write_u64_hex_raw(self.pml4_phys.as_u64());
                                        os_log::write_str_raw(" idx=");
                                        os_log::write_u32_raw(pml4_idx as u32);
                                        os_log::write_str_raw("/");
                                        os_log::write_u32_raw(pdpt_idx as u32);
                                        os_log::write_str_raw("/");
                                        os_log::write_u32_raw(pd_idx as u32);
                                        os_log::write_str_raw("/");
                                        os_log::write_u32_raw(pt_idx as u32);
                                        os_log::write_str_raw(" cow=");
                                        os_log::write_u32_raw(cow_count);
                                        os_log::write_str_raw(" pte=0x");
                                        os_log::write_u64_hex_raw(pt_entry.raw());
                                        os_log::write_str_raw("\n");
                                    }
                                    skip = true;
                                } else if flags & mm_pagedb::PF_PAGETABLE != 0 {
                                    skip = true;
                                }
                            }
                        }
                        if skip {
                            continue;
                        }

                        let remaining = cow.decrement(phys);
                        if remaining == 0 {
                            mm_pagedb::set_free_context(mm_pagedb::CTX_DROP_LEAF);
                            let result = mm.free_frame(phys);
                            // — ColdCipher: If free_frame failed, something freed
                            // this frame between our guard check and now. Log it.
                            if result.is_err() {
                                unsafe {
                                    os_log::write_str_raw("[DROP-LEAF-DFREE] phys=0x");
                                    os_log::write_u64_hex_raw(phys.as_u64());
                                    os_log::write_str_raw(" cow_was=");
                                    os_log::write_u32_raw(cow_count);
                                    os_log::write_str_raw(" pml4=0x");
                                    os_log::write_u64_hex_raw(self.pml4_phys.as_u64());
                                    os_log::write_str_raw(" idx=");
                                    os_log::write_u32_raw(pml4_idx as u32);
                                    os_log::write_str_raw("/");
                                    os_log::write_u32_raw(pdpt_idx as u32);
                                    os_log::write_str_raw("/");
                                    os_log::write_u32_raw(pd_idx as u32);
                                    os_log::write_str_raw("/");
                                    os_log::write_u32_raw(pt_idx as u32);
                                    os_log::write_str_raw("\n");
                                }
                            }
                            #[cfg(feature = "debug-proc")]
                            { freed_leaf += 1; }
                        } else {
                            #[cfg(feature = "debug-proc")]
                            { skipped_cow += 1; }
                        }
                    }
                }
            }
        }

        // ----------------------------------------------------------------
        // Step 2: Free ALL page-table structure frames.
        // — TorqueJax: We now free the walked PT frames (discovered during the
        // tree walk) PLUS the PML4 frame itself. This catches frames from BOTH
        // TrackingAllocator (map_user_page) AND direct alloc_frame calls
        // (handle_stack_growth). The old code only freed allocated_frames which
        // missed anything created outside TrackingAllocator.
        //
        // Dedup: some frames appear in BOTH walked_pt_frames AND allocated_frames
        // (e.g., PT frames from map_user_page go into both). We use the allocated_frames
        // set for the PML4 and any frames not discovered during the walk (shouldn't
        // happen, but belt and suspenders).
        // ----------------------------------------------------------------

        #[cfg(feature = "debug-proc")]
        let pt_frame_count = (walked_pt_frames.len() + 1) as u32; // +1 for PML4

        // — GraveShift: Free all PT structure frames found during the walk.
        // These are PDPT, PD, and PT frames. Never COW-shared — always ours.
        // — ColdCipher: Dedup first. If two PT entries at the same level point to
        // the same physical frame (shouldn't happen, but does under buddy corruption
        // or stale mappings), freeing the same frame twice is a DoubleFree.
        let pre_dedup = walked_pt_frames.len();
        walked_pt_frames.sort_unstable_by_key(|f| f.as_u64());
        walked_pt_frames.dedup_by_key(|f| f.as_u64());
        if walked_pt_frames.len() < pre_dedup {
            unsafe {
                os_log::write_str_raw("[DROP-DEDUP] Removed ");
                os_log::write_u32_raw((pre_dedup - walked_pt_frames.len()) as u32);
                os_log::write_str_raw(" duplicate PT frames from walk (pml4=0x");
                os_log::write_u64_hex_raw(self.pml4_phys.as_u64());
                os_log::write_str_raw(")\n");
            }
        }
        mm_pagedb::set_free_context(mm_pagedb::CTX_DROP_PT_WALK);
        for &pt_frame in &walked_pt_frames {
            if pt_frame.as_u64() == 0 {
                continue;
            }
            let _ = mm.free_frame(pt_frame);
        }

        // — GraveShift: Free the PML4 frame itself (always exclusively owned).
        // The PML4 was not visited during the walk (it's the root, not a child).
        if self.pml4_phys.as_u64() != 0 {
            mm_pagedb::set_free_context(mm_pagedb::CTX_DROP_PML4);
            let _ = mm.free_frame(self.pml4_phys);
        }

        // — TorqueJax: Free any remaining frames in allocated_frames that weren't
        // found during the walk. In theory this should be empty (all PT frames
        // should be reachable from the PML4), but belt and suspenders beats
        // silent leaks. Skip frames we already freed above to avoid double-free.
        mm_pagedb::set_free_context(mm_pagedb::CTX_DROP_ALLOC);
        for &pt_frame in &self.allocated_frames {
            if pt_frame.as_u64() == 0 || pt_frame == self.pml4_phys {
                continue;
            }
            // — SableWire: Only free if NOT already freed via the walk.
            // Linear scan is fine — allocated_frames is typically <20 entries.
            if !walked_pt_frames.contains(&pt_frame) {
                let _ = mm.free_frame(pt_frame);
            }
        }

        #[cfg(feature = "debug-proc")]
        unsafe {
            os_log::write_str_raw("[ADDR-DROP] pml4=");
            os_log::write_u64_hex_raw(self.pml4_phys.as_u64());
            os_log::write_str_raw(" freed_leaf=");
            os_log::write_u32_raw(freed_leaf);
            os_log::write_str_raw(" skipped_cow=");
            os_log::write_u32_raw(skipped_cow);
            os_log::write_str_raw(" pt_frames=");
            os_log::write_u32_raw(pt_frame_count);
            os_log::write_str_raw("\n");
        }
    }
}

/// Wrapper allocator that tracks allocated frames
struct TrackingAllocator<'a, A: FrameAllocator> {
    inner: &'a A,
    allocated: RefCell<&'a mut Vec<PhysAddr>>,
}

impl<'a, A: FrameAllocator> FrameAllocator for TrackingAllocator<'a, A> {
    fn alloc_frame(&self) -> Option<PhysAddr> {
        let frame = self.inner.alloc_frame()?;
        self.allocated.borrow_mut().push(frame);
        // — GraveShift: PT structure frame — mark in page frame database
        if let Some(db) = mm_pagedb::try_pagedb() {
            db.mark_pagetable(frame, 0);
        }
        Some(frame)
    }

    fn free_frame(&self, frame: PhysAddr) {
        self.inner.free_frame(frame);
        // Remove from tracking list
        let mut allocated = self.allocated.borrow_mut();
        if let Some(pos) = allocated.iter().position(|&f| f == frame) {
            allocated.remove(pos);
        }
    }

    fn alloc_frames(&self, count: usize) -> Option<PhysAddr> {
        let frames = self.inner.alloc_frames(count)?;
        // Track all frames in the allocation
        let mut allocated = self.allocated.borrow_mut();
        for i in 0..count {
            allocated.push(PhysAddr::new(frames.as_u64() + (i as u64 * 4096)));
        }
        Some(frames)
    }

    fn free_frames(&self, addr: PhysAddr, count: usize) {
        self.inner.free_frames(addr, count);
        // Remove all frames from tracking
        let mut allocated = self.allocated.borrow_mut();
        for i in 0..count {
            let frame = PhysAddr::new(addr.as_u64() + (i as u64 * 4096));
            if let Some(pos) = allocated.iter().position(|&f| f == frame) {
                allocated.remove(pos);
            }
        }
    }
}
