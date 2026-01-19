//! Linked-list heap allocator

use core::alloc::Layout;
use core::mem;
use core::ptr;

// Debug disabled for cleaner output
fn alloc_debug(_msg: &str) {}

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
        // Use saturating_add to prevent overflow panic
        self.start_addr().saturating_add(self.size)
    }

    /// Check if this block is valid (sanity check)
    fn is_valid(&self) -> bool {
        let start = self.start_addr();
        // A valid block should have a reasonable size and not overflow
        self.size > 0
            && self.size < usize::MAX / 2
            && start.checked_add(self.size).is_some()
    }
}

/// Linked-list allocator
///
/// Maintains a linked list of free blocks. On allocation, finds the first
/// block that fits. On deallocation, adds the block back to the list and
/// merges adjacent blocks.
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
        unsafe {
            self.add_free_region(heap_start, heap_size);
        }
        self.total_size = heap_size;
        self.used_size = 0;
    }

    /// Add a free region to the allocator
    unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
        alloc_debug("[ADD_FREE] entering\n");
        // Ensure alignment and minimum size
        let aligned_addr = align_up(addr, mem::align_of::<FreeBlock>());
        let aligned_size = size.saturating_sub(aligned_addr - addr);

        if aligned_size < mem::size_of::<FreeBlock>() {
            alloc_debug("[ADD_FREE] too small, returning\n");
            return; // Too small to hold metadata
        }

        // Create a new free block
        alloc_debug("[ADD_FREE] creating block\n");
        let block = unsafe { &mut *(aligned_addr as *mut FreeBlock) };
        block.size = aligned_size;
        block.next = self.head.next.take();
        self.head.next = Some(block);
        alloc_debug("[ADD_FREE] block added\n");

        // TEMPORARILY DISABLED: merging causes infinite loop with corrupted heap
        // Try to merge with adjacent blocks
        // self.merge_free_blocks();
        // alloc_debug("[ADD_FREE] merge done\n");
    }

    /// Merge adjacent free blocks
    ///
    /// NOTE: Currently disabled in add_free_region due to heap corruption issues.
    /// TODO: Investigate root cause and re-enable.
    #[allow(dead_code)]
    fn merge_free_blocks(&mut self) {
        let mut current = &mut self.head;
        let mut loop_count = 0usize;
        const MAX_LOOPS: usize = 10000;

        while let Some(ref mut block) = current.next {
            loop_count += 1;
            if loop_count > MAX_LOOPS {
                return; // Prevent infinite loop
            }

            // Skip invalid blocks (corrupted)
            if !block.is_valid() {
                current = current.next.as_mut().unwrap();
                continue;
            }

            // Get end address before borrowing next
            let block_end = block.end_addr();

            // Check if we can merge with the next block
            let should_merge = block
                .next
                .as_ref()
                .map(|next| next.is_valid() && block_end == next.start_addr())
                .unwrap_or(false);

            if should_merge {
                // Get the next block's info before removing it
                if let Some(ref mut next_block) = block.next {
                    let next_size = next_block.size;
                    let next_next = next_block.next.take();
                    block.size = block.size.saturating_add(next_size);
                    block.next = next_next;
                    continue; // Check again for more merges
                }
            }

            current = current.next.as_mut().unwrap();
        }
    }

    /// Allocate memory
    pub fn allocate(&mut self, layout: Layout) -> *mut u8 {
        let (size, align) = Self::size_align(layout);

        // Find a suitable block
        let mut current = &mut self.head;
        let mut loop_count = 0usize;
        const MAX_LOOPS: usize = 10000;

        while let Some(ref mut block) = current.next {
            loop_count += 1;
            if loop_count > MAX_LOOPS {
                // Print error and return null
                alloc_debug("[ALLOC] ERROR: infinite loop detected!\n");
                return core::ptr::null_mut();
            }

            // Skip invalid blocks
            if !block.is_valid() {
                current = current.next.as_mut().unwrap();
                continue;
            }

            if let Some((alloc_start, alloc_end)) = Self::alloc_from_block(block, size, align) {
                alloc_debug("[ALLOC] found suitable block\n");
                let block_start = block.start_addr();
                let block_end = block.end_addr();

                // Remove block from list
                let next = block.next.take();
                alloc_debug("[ALLOC] removed block from list\n");

                // If there's space before the allocation, add it back
                if alloc_start > block_start {
                    let front_size = alloc_start - block_start;
                    if front_size >= mem::size_of::<FreeBlock>() {
                        alloc_debug("[ALLOC] adding front region\n");
                        unsafe {
                            let front = &mut *(block_start as *mut FreeBlock);
                            front.size = front_size;
                            front.next = next;
                            current.next = Some(front);
                        }
                    } else {
                        current.next = next;
                    }
                } else {
                    current.next = next;
                }
                alloc_debug("[ALLOC] handled front region\n");

                // If there's space after the allocation, add it back
                if block_end > alloc_end {
                    let back_size = block_end - alloc_end;
                    if back_size >= mem::size_of::<FreeBlock>() {
                        alloc_debug("[ALLOC] adding back region\n");
                        unsafe {
                            self.add_free_region(alloc_end, back_size);
                        }
                        alloc_debug("[ALLOC] back region added\n");
                    }
                }

                alloc_debug("[ALLOC] updating used_size\n");
                self.used_size += alloc_end - alloc_start;
                alloc_debug("[ALLOC] returning pointer\n");
                return alloc_start as *mut u8;
            }

            current = current.next.as_mut().unwrap();
        }

        ptr::null_mut()
    }

    /// Check if a block can satisfy an allocation
    fn alloc_from_block(
        block: &FreeBlock,
        size: usize,
        align: usize,
    ) -> Option<(usize, usize)> {
        // Validate block first
        if !block.is_valid() {
            return None;
        }

        let alloc_start = align_up(block.start_addr(), align);
        let alloc_end = alloc_start.checked_add(size)?;
        let block_end = block.end_addr();

        if alloc_end > block_end {
            return None;
        }

        // Ensure remaining space (if any) can hold a FreeBlock
        let remaining = block_end.saturating_sub(alloc_end);
        if remaining > 0 && remaining < mem::size_of::<FreeBlock>() {
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
