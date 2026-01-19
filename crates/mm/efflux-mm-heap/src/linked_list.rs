//! Linked-list heap allocator

use core::alloc::Layout;
use core::mem;
use core::ptr;

// Serial port debug output
fn heap_debug(s: &str) {
    const SERIAL: u16 = 0x3F8;
    for b in s.bytes() {
        unsafe {
            loop {
                let status: u8;
                core::arch::asm!("in al, dx", out("al") status, in("dx") SERIAL + 5, options(nomem, nostack));
                if status & 0x20 != 0 { break; }
            }
            core::arch::asm!("out dx, al", in("al") b, in("dx") SERIAL, options(nomem, nostack));
        }
    }
}

fn print_num(n: usize) {
    const SERIAL: u16 = 0x3F8;
    const HEX: &[u8] = b"0123456789abcdef";
    heap_debug("0x");
    for i in (0..16).rev() {
        let nibble = ((n >> (i * 4)) & 0xF) as usize;
        let c = HEX[nibble];
        unsafe {
            loop {
                let status: u8;
                core::arch::asm!("in al, dx", out("al") status, in("dx") SERIAL + 5, options(nomem, nostack));
                if status & 0x20 != 0 { break; }
            }
            core::arch::asm!("out dx, al", in("al") c, in("dx") SERIAL, options(nomem, nostack));
        }
    }
}

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
        heap_debug("[HEAP:init] start=");
        print_num(heap_start);
        heap_debug(" size=");
        print_num(heap_size);
        heap_debug("\n");
        // SAFETY: caller guarantees heap region is valid
        unsafe { self.add_free_region(heap_start, heap_size); }
        self.total_size = heap_size;
        self.used_size = 0;

        // Debug: show the block we just created
        if let Some(ref b) = self.head.next {
            heap_debug("[HEAP:init] block size=");
            print_num(b.size);
            heap_debug("\n");
        } else {
            heap_debug("[HEAP:init] ERROR: no block created!\n");
        }
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
            heap_debug("[HEAP:add] too small after align\n");
            return; // Too small after alignment
        }
        let aligned_size = size - alignment_loss;

        if aligned_size < mem::size_of::<FreeBlock>() {
            heap_debug("[HEAP:add] too small for metadata\n");
            return; // Too small to hold metadata
        }

        heap_debug("[HEAP:add] addr=");
        print_num(aligned_addr);
        heap_debug(" size=");
        print_num(aligned_size);
        heap_debug("\n");

        // Create a new free block
        // SAFETY: aligned_addr points to valid, properly aligned memory
        let new_block = unsafe { &mut *(aligned_addr as *mut FreeBlock) };
        new_block.size = aligned_size;
        new_block.next = None;

        // Insert in sorted order by address
        let mut current = &mut self.head;
        let mut loop_count = 0usize;

        loop {
            loop_count += 1;
            if loop_count > 100000 {
                heap_debug("[HEAP] add_free_region: infinite loop!\n");
                return;
            }
            match current.next {
                Some(ref mut next_block) if next_block.start_addr() < aligned_addr => {
                    // Keep searching - next block is before our new block
                    current = current.next.as_mut().unwrap();
                }
                _ => {
                    // Insert here - either no next block or next block is after our new block
                    new_block.next = current.next.take();
                    current.next = Some(new_block);
                    break;
                }
            }
        }

        // Merge with adjacent blocks
        self.merge_adjacent_blocks();
    }

    /// Merge adjacent free blocks
    ///
    /// Since blocks are sorted by address, adjacent blocks in memory
    /// will be consecutive in the list.
    fn merge_adjacent_blocks(&mut self) {
        let mut current = &mut self.head;
        let mut loop_count = 0usize;
        let mut merged_count = 0usize;

        while let Some(ref mut block) = current.next {
            loop_count += 1;
            if loop_count > 100000 {
                heap_debug("[HEAP] merge: infinite loop!\n");
                return;
            }

            let block_end = block.end_addr();

            // Check if we can merge with the next block
            let can_merge = block
                .next
                .as_ref()
                .map(|next| block_end == next.start_addr())
                .unwrap_or(false);

            if can_merge {
                merged_count += 1;
                // Merge: absorb next block into current block
                if let Some(mut next_block) = block.next.take() {
                    block.size += next_block.size;
                    block.next = next_block.next.take();
                }
                // Don't advance - check if we can merge again
            } else {
                // Move to next block
                current = current.next.as_mut().unwrap();
            }
        }

        if merged_count > 0 {
            heap_debug("[HEAP:merge] merged ");
            print_num(merged_count);
            heap_debug(" blocks\n");
        }
    }

    /// Allocate memory
    pub fn allocate(&mut self, layout: Layout) -> *mut u8 {
        heap_debug("[HEAP:alloc] start\n");
        let (size, align) = Self::size_align(layout);

        // Debug: count free blocks
        let mut block_count = 0usize;
        let mut largest_block = 0usize;
        {
            let mut c = &self.head;
            while let Some(ref b) = c.next {
                block_count += 1;
                if b.size > largest_block {
                    largest_block = b.size;
                }
                c = c.next.as_ref().unwrap();
            }
        }
        if size > 0x1000 {
            heap_debug("[HEAP:alloc] need=");
            print_num(size);
            heap_debug(" blocks=");
            print_num(block_count);
            heap_debug(" largest=");
            print_num(largest_block);
            heap_debug("\n");
        }

        // Find a suitable block
        let mut current = &mut self.head;
        let mut loop_count = 0usize;

        while let Some(ref mut block) = current.next {
            loop_count += 1;
            if loop_count > 100000 {
                heap_debug("[HEAP] allocate: infinite loop!\n");
                return ptr::null_mut();
            }
            if let Some((alloc_start, alloc_end)) = Self::alloc_from_block(block, size, align) {
                let block_start = block.start_addr();
                let block_end = block.end_addr();

                // Debug: show allocation details for large blocks
                if block.size > 0x100000 {
                    heap_debug("[HEAP:alloc] from size=");
                    print_num(block.size);
                    heap_debug(" alloc=");
                    print_num(size);
                    heap_debug(" at=");
                    print_num(alloc_start);
                    heap_debug("\n");
                }

                // Remove block from list
                let next = block.next.take();

                // Calculate front and back regions
                let front_size = alloc_start - block_start;
                let back_size = block_end - alloc_end;

                // Debug: show split for large blocks
                if back_size > 0x100000 {
                    heap_debug("[HEAP:split] back at=");
                    print_num(alloc_end);
                    heap_debug(" size=");
                    print_num(back_size);
                    heap_debug("\n");
                }

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

        ptr::null_mut()
    }

    /// Check if a block can satisfy an allocation
    fn alloc_from_block(
        block: &FreeBlock,
        size: usize,
        align: usize,
    ) -> Option<(usize, usize)> {
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
