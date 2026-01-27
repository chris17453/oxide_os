//! Linked-list heap allocator

use core::alloc::Layout;
use core::mem;
use core::ptr;

/// A free block in the heap
struct FreeBlock {
    size: usize,
    next: Option<&'static mut FreeBlock>,
}

impl FreeBlock {
    const fn new(size: usize) -> Self {
        Self { size, next: None }
    }

    fn start_addr(&self) -> usize {
        self as *const Self as usize
    }

    fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
}

/// Linked-list allocator
///
/// Maintains a linked list of free blocks sorted by address.
/// On allocation, finds the first block that fits.
/// On deallocation, adds the block back in sorted order and merges adjacent blocks.
pub struct LinkedListAllocator {
    head: FreeBlock,
    total_size: usize,
    used_size: usize,
}

impl LinkedListAllocator {
    /// Create an empty allocator
    pub const fn empty() -> Self {
        Self {
            head: FreeBlock::new(0),
            total_size: 0,
            used_size: 0,
        }
    }

    /// Initialize the allocator with a memory region
    ///
    /// # Safety
    /// The memory region must be valid and unused.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        // SAFETY: caller guarantees heap region is valid
        unsafe {
            self.add_free_region(heap_start, heap_size);
        }
        self.total_size = heap_size;
        self.used_size = 0;
    }

    /// Add a free region to the allocator in sorted order by address
    ///
    /// # Safety
    /// The memory region must be valid and not overlap with existing free regions.
    unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
        // Ensure alignment and minimum size
        let aligned_addr = align_up(addr, mem::align_of::<FreeBlock>());
        let alignment_loss = aligned_addr - addr;
        if size <= alignment_loss {
            return; // Too small after alignment
        }
        let aligned_size = size - alignment_loss;

        if aligned_size < mem::size_of::<FreeBlock>() {
            return; // Too small to hold metadata
        }

        // Create a new free block
        // SAFETY: aligned_addr points to valid, properly aligned memory
        let new_block = unsafe { &mut *(aligned_addr as *mut FreeBlock) };
        new_block.size = aligned_size;
        new_block.next = None;

        // Insert in sorted order by address
        // Find the right position: after all blocks with address < aligned_addr
        let mut current = &mut self.head;

        // Walk until we find a block at higher address or reach end
        while current
            .next
            .as_ref()
            .map_or(false, |b| b.start_addr() < aligned_addr)
        {
            current = current.next.as_mut().unwrap();
        }

        // Insert new_block between current and current.next
        new_block.next = current.next.take();
        current.next = Some(new_block);

        // Merge adjacent blocks
        self.merge_adjacent_blocks();
    }

    /// Merge adjacent free blocks
    ///
    /// Since blocks are sorted by address, adjacent blocks in memory
    /// will be consecutive in the list.
    fn merge_adjacent_blocks(&mut self) {
        let mut current = &mut self.head;

        while let Some(ref mut block) = current.next {
            let block_end = block.end_addr();

            // Check if we can merge with the next block
            let can_merge = block
                .next
                .as_ref()
                .map(|next| block_end == next.start_addr())
                .unwrap_or(false);

            if can_merge {
                // Merge: absorb next block into current block
                if let Some(next_block) = block.next.take() {
                    block.size += next_block.size;
                    block.next = next_block.next.take();
                }
                // Don't advance - check if we can merge again
            } else {
                // Move to next block
                current = current.next.as_mut().unwrap();
            }
        }
    }

    /// Allocate memory
    pub fn allocate(&mut self, layout: Layout) -> *mut u8 {
        let (size, align) = Self::size_align(layout);

        // Debug: dump heap state on small allocations that might fail
        #[cfg(feature = "debug-heap")]
        {
            use arch_x86_64::serial;
            use core::fmt::Write;
            let mut writer = serial::SerialWriter;
            // Check for corruption
            if self.total_size == 0 || self.total_size > 0x10000000 {
                let _ = writeln!(writer, "[HEAP] CORRUPTION DETECTED!");
                let _ = writeln!(writer, "[HEAP] self={:#x} size={} align={}",
                    self as *mut _ as usize, size, align);
                let _ = writeln!(writer, "[HEAP] total_size={} used_size={} head.size={}",
                    self.total_size, self.used_size, self.head.size);
            } else {
                let _ = writeln!(writer, "[HEAP] alloc size={} align={} used={} total={}",
                    size, align, self.used_size, self.total_size);
            }
        }

        // Find a suitable block
        let mut current = &mut self.head;

        while let Some(ref mut block) = current.next {
            if let Some((alloc_start, alloc_end)) = Self::alloc_from_block(block, size, align) {
                let block_start = block.start_addr();
                let block_end = block.end_addr();

                // Remove block from list
                let next = block.next.take();

                // Calculate front and back regions
                let front_size = alloc_start - block_start;
                let back_size = block_end - alloc_end;

                // Handle the regions
                if front_size >= mem::size_of::<FreeBlock>() {
                    // Keep front region in place
                    let front = unsafe { &mut *(block_start as *mut FreeBlock) };
                    front.size = front_size;

                    if back_size >= mem::size_of::<FreeBlock>() {
                        // Add back region after front
                        let back = unsafe { &mut *(alloc_end as *mut FreeBlock) };
                        back.size = back_size;
                        back.next = next;
                        front.next = Some(back);
                    } else {
                        front.next = next;
                    }
                    current.next = Some(front);
                } else if back_size >= mem::size_of::<FreeBlock>() {
                    // Only back region - put it in place of original block
                    let back = unsafe { &mut *(alloc_end as *mut FreeBlock) };
                    back.size = back_size;
                    back.next = next;
                    current.next = Some(back);
                } else {
                    // No remaining regions
                    current.next = next;
                }

                self.used_size += alloc_end - alloc_start;
                return alloc_start as *mut u8;
            }

            current = current.next.as_mut().unwrap();
        }

        // Allocation failed - dump heap state for debugging
        #[cfg(feature = "debug-heap")]
        {
            use arch_x86_64::serial;
            use core::fmt::Write;
            let mut writer = serial::SerialWriter;
            let _ = writeln!(writer, "[HEAP] ALLOC FAILED! size={} align={}", size, align);
            let _ = writeln!(writer, "[HEAP] used={} total={} free={}",
                self.used_size, self.total_size, self.total_size.saturating_sub(self.used_size));
            // Dump free list
            let mut count = 0;
            let mut curr = &self.head;
            while let Some(ref block) = curr.next {
                if count < 10 {
                    let _ = writeln!(writer, "[HEAP]   block {}: addr={:#x} size={}",
                        count, block.start_addr(), block.size);
                }
                count += 1;
                curr = block;
            }
            let _ = writeln!(writer, "[HEAP]   total {} free blocks", count);
        }

        ptr::null_mut()
    }

    /// Check if a block can satisfy an allocation
    fn alloc_from_block(block: &FreeBlock, size: usize, align: usize) -> Option<(usize, usize)> {
        let alloc_start = align_up(block.start_addr(), align);
        let alloc_end = alloc_start.checked_add(size)?;
        let block_end = block.end_addr();

        if alloc_end > block_end {
            return None;
        }

        // Check if front region (if any) can hold a FreeBlock
        let front_size = alloc_start - block.start_addr();
        if front_size > 0 && front_size < mem::size_of::<FreeBlock>() {
            return None;
        }

        // Check if back region (if any) can hold a FreeBlock
        let back_size = block_end - alloc_end;
        if back_size > 0 && back_size < mem::size_of::<FreeBlock>() {
            return None;
        }

        Some((alloc_start, alloc_end))
    }

    /// Deallocate memory
    pub fn deallocate(&mut self, ptr: *mut u8, layout: Layout) {
        let (size, _) = Self::size_align(layout);
        unsafe {
            self.add_free_region(ptr as usize, size);
        }
        self.used_size = self.used_size.saturating_sub(size);
    }

    /// Get size with proper alignment
    fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(mem::align_of::<FreeBlock>())
            .expect("alignment overflow")
            .pad_to_align();
        let size = layout.size().max(mem::size_of::<FreeBlock>());
        (size, layout.align())
    }

    /// Get free memory
    pub fn free(&self) -> usize {
        self.total_size.saturating_sub(self.used_size)
    }

    /// Get used memory
    pub fn used(&self) -> usize {
        self.used_size
    }
}

/// Align an address up
fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}
