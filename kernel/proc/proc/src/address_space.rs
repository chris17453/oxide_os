//! User address space management
//!
//! Creates and manages user-mode virtual address spaces.

use alloc::vec::Vec;
use core::cell::RefCell;
use mm_paging::{MapError as PagingMapError, PageMapper, PageTable, PageTableFlags};
use mm_paging::{phys_to_virt, write_cr3};
use mm_traits::FrameAllocator;
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
        // Linux-style allocation — GraveShift: Allocate all frames first (lock released after each alloc),
        // then zero and map them. Each alloc_frame() call locks->allocates->unlocks atomically.
        // Mapping can then safely allocate PT pages without deadlock.

        let mut frames = alloc::vec::Vec::with_capacity(num_pages);

        // Phase 1: Allocate all frames (each alloc is independent, no lock held across calls)
        for i in 0..num_pages {
            // TRACE — GraveShift: Before alloc_frame
            if i % 10 == 0 && i > 0 {
                unsafe {
                    use arch_x86_64 as arch;
                    let msg = b"[ALLOC] Pre-alloc page ";
                    for &byte in msg.iter() {
                        while arch::inb(0x3FD) & 0x20 == 0 {}
                        arch::outb(0x3F8, byte);
                    }
                    let msg2 = if i < 100 { [b'0' + (i / 10) as u8, b'0' + (i % 10) as u8] } else { [b'X', b'X'] };
                    for &byte in &msg2 {
                        while arch::inb(0x3FD) & 0x20 == 0 {}
                        arch::outb(0x3F8, byte);
                    }
                    let msg3 = b"\r\n";
                    for &byte in msg3.iter() {
                        while arch::inb(0x3FD) & 0x20 == 0 {}
                        arch::outb(0x3F8, byte);
                    }
                }
            }

            let frame = match allocator.alloc_frame() {
                Some(f) => {
                    // TRACE — BlackLatch: After successful alloc
                    if i % 10 == 0 && i > 0 {
                        unsafe {
                            use arch_x86_64 as arch;
                            let msg = b"[ALLOC] Post-alloc page ";
                            for &byte in msg.iter() {
                                while arch::inb(0x3FD) & 0x20 == 0 {}
                                arch::outb(0x3F8, byte);
                            }
                            let msg2 = if i < 100 { [b'0' + (i / 10) as u8, b'0' + (i % 10) as u8] } else { [b'X', b'X'] };
                            for &byte in &msg2 {
                                while arch::inb(0x3FD) & 0x20 == 0 {}
                                arch::outb(0x3F8, byte);
                            }
                            let msg3 = b"\r\n";
                            for &byte in msg3.iter() {
                                while arch::inb(0x3FD) & 0x20 == 0 {}
                                arch::outb(0x3F8, byte);
                            }
                        }
                    }
                    f
                }
                None => {
                    // Out of memory - trace it
                    unsafe {
                        use arch_x86_64 as arch;
                        let msg = b"[ALLOC-ERROR] OOM at frame ";
                        for &byte in msg.iter() {
                            while arch::inb(0x3FD) & 0x20 == 0 {}
                            arch::outb(0x3F8, byte);
                        }
                        let mut n = i;
                        let mut buf = [0u8; 10];
                        let mut idx = 0;
                        while n > 0 {
                            buf[idx] = b'0' + (n % 10) as u8;
                            n /= 10;
                            idx += 1;
                        }
                        while idx > 0 {
                            idx -= 1;
                            while arch::inb(0x3FD) & 0x20 == 0 {}
                            arch::outb(0x3F8, buf[idx]);
                        }
                        let msg2 = b"\r\n";
                        for &byte in msg2.iter() {
                            while arch::inb(0x3FD) & 0x20 == 0 {}
                            arch::outb(0x3F8, byte);
                        }
                    }
                    return Err(MapError::OutOfMemory);
                }
            };
            self.allocated_frames.push(frame);
            frames.push(frame);

            // [TRACE] Frame allocated — TorqueJax
            unsafe {
                use arch_x86_64 as arch;
                let msg = b"[FRAME-ALLOC] phys=0x";
                for &byte in msg.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                let mut n = frame.as_u64();
                let mut buf = [0u8; 16];
                let mut idx = 0;
                loop {
                    let digit = (n & 0xF) as u8;
                    buf[idx] = if digit < 10 { b'0' + digit } else { b'a' + (digit - 10) };
                    n >>= 4;
                    idx += 1;
                    if n == 0 {
                        break;
                    }
                }
                while idx > 0 {
                    idx -= 1;
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, buf[idx]);
                }
                let msg2 = b"\r\n";
                for &byte in msg2.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
            }
        }

        // Phase 2: Zero and map (lock not held, mapping can safely allocate PT pages)
        for (i, &frame) in frames.iter().enumerate() {
            let offset = (i * 4096) as u64;
            let virt = VirtAddr::new(virt_start.as_u64() + offset);

            // Zero the frame
            let frame_virt = phys_to_virt(frame);

            // [TRACE] Before zeroing — GraveShift
            unsafe {
                use arch_x86_64 as arch;
                let msg = b"[ZERO-START] phys=0x";
                for &byte in msg.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                let mut n = frame.as_u64();
                let mut buf = [0u8; 16];
                let mut idx = 0;
                loop {
                    let digit = (n & 0xF) as u8;
                    buf[idx] = if digit < 10 { b'0' + digit } else { b'a' + (digit - 10) };
                    n >>= 4;
                    idx += 1;
                    if n == 0 {
                        break;
                    }
                }
                while idx > 0 {
                    idx -= 1;
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, buf[idx]);
                }
                let msg2 = b" virt=0x";
                for &byte in msg2.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                let mut n = frame_virt.as_u64();
                let mut buf = [0u8; 16];
                let mut idx = 0;
                loop {
                    let digit = (n & 0xF) as u8;
                    buf[idx] = if digit < 10 { b'0' + digit } else { b'a' + (digit - 10) };
                    n >>= 4;
                    idx += 1;
                    if n == 0 {
                        break;
                    }
                }
                while idx > 0 {
                    idx -= 1;
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, buf[idx]);
                }
                let msg3 = b"\r\n";
                for &byte in msg3.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
            }

            // [VALIDATE] Check if we're about to zero a free block — ColdCipher: Catch corruption source
            const FREE_BLOCK_MAGIC: u64 = 0x4652454542304C;
            unsafe {
                let first_u64 = core::ptr::read_volatile(frame_virt.as_ptr::<u64>());
                if first_u64 == FREE_BLOCK_MAGIC {
                    use arch_x86_64 as arch;
                    let msg = b"[FATAL] About to zero FREE BLOCK! phys=0x";
                    for &byte in msg.iter() {
                        while arch::inb(0x3FD) & 0x20 == 0 {}
                        arch::outb(0x3F8, byte);
                    }
                    let mut n = frame.as_u64();
                    let mut buf = [0u8; 16];
                    let mut idx = 0;
                    loop {
                        let digit = (n & 0xF) as u8;
                        buf[idx] = if digit < 10 { b'0' + digit } else { b'a' + (digit - 10) };
                        n >>= 4;
                        idx += 1;
                        if n == 0 {
                            break;
                        }
                    }
                    while idx > 0 {
                        idx -= 1;
                        while arch::inb(0x3FD) & 0x20 == 0 {}
                        arch::outb(0x3F8, buf[idx]);
                    }
                    let msg2 = b" - GPF\r\n";
                    for &byte in msg2.iter() {
                        while arch::inb(0x3FD) & 0x20 == 0 {}
                        arch::outb(0x3F8, byte);
                    }
                    core::ptr::write_volatile(0xDEADC0DE as *mut u64, frame.as_u64());
                }
            }

            // [RE-ENABLED] Zeroing (not the corruption source) — GraveShift
            unsafe {
                core::ptr::write_bytes(frame_virt.as_mut_ptr::<u8>(), 0, 4096);
            }

            // [TRACE] After zeroing (SKIPPED) — GraveShift
            unsafe {
                use arch_x86_64 as arch;
                let msg = b"[ZERO-DONE] phys=0x";
                for &byte in msg.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                let mut n = frame.as_u64();
                let mut buf = [0u8; 16];
                let mut idx = 0;
                loop {
                    let digit = (n & 0xF) as u8;
                    buf[idx] = if digit < 10 { b'0' + digit } else { b'a' + (digit - 10) };
                    n >>= 4;
                    idx += 1;
                    if n == 0 {
                        break;
                    }
                }
                while idx > 0 {
                    idx -= 1;
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, buf[idx]);
                }
                let msg2 = b"\r\n";
                for &byte in msg2.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
            }

            // [RE-ENABLED] Mapping (not the corruption source) — BlackLatch
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
