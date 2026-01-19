//! User address space management
//!
//! Creates and manages user-mode virtual address spaces.

use alloc::vec::Vec;
use core::cell::RefCell;
use efflux_core::{PhysAddr, VirtAddr};
use efflux_mm_paging::{PageTableFlags, PageMapper, PageTable, MapError as PagingMapError};
use efflux_mm_paging::{phys_to_virt, write_cr3};
use efflux_mm_traits::FrameAllocator;
use efflux_proc_traits::{AddressSpace, MapError, MemoryFlags, UnmapError};

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
            allocated_frames: alloc::vec![pml4_frame],
        })
    }

    /// Get the physical address of the PML4 table
    pub fn pml4_phys(&self) -> PhysAddr {
        self.pml4_phys
    }

    /// Create from raw PML4 and frame list
    ///
    /// # Safety
    /// The PML4 must be a valid page table and all frames in the list
    /// must be owned by this address space.
    pub unsafe fn from_raw(pml4_phys: PhysAddr, frames: Vec<PhysAddr>) -> Self {
        let mapper = unsafe { PageMapper::new(pml4_phys) };
        Self {
            pml4_phys,
            mapper,
            allocated_frames: frames,
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
        for i in 0..num_pages {
            let offset = (i * 4096) as u64;
            let virt = VirtAddr::new(virt_start.as_u64() + offset);

            // Allocate a frame
            let frame = allocator.alloc_frame().ok_or(MapError::OutOfMemory)?;
            self.allocated_frames.push(frame);

            // Zero the frame
            let frame_virt = phys_to_virt(frame);
            unsafe {
                core::ptr::write_bytes(frame_virt.as_mut_ptr::<u8>(), 0, 4096);
            }

            // Map it
            unsafe { self.map_user_page(virt, frame, flags, allocator)? };
        }

        Ok(())
    }
}

impl AddressSpace for UserAddressSpace {
    fn new() -> Self {
        panic!("Use UserAddressSpace::new_with_kernel() instead");
    }

    fn page_table_root(&self) -> PhysAddr {
        self.pml4_phys
    }

    unsafe fn map(&mut self, _virt: VirtAddr, _phys: PhysAddr, _flags: MemoryFlags) -> Result<(), MapError> {
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

/// Wrapper allocator that tracks allocated frames
struct TrackingAllocator<'a, A: FrameAllocator> {
    inner: &'a A,
    allocated: RefCell<&'a mut Vec<PhysAddr>>,
}

impl<'a, A: FrameAllocator> FrameAllocator for TrackingAllocator<'a, A> {
    fn alloc_frame(&self) -> Option<PhysAddr> {
        let frame = self.inner.alloc_frame()?;
        self.allocated.borrow_mut().push(frame);
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
