# Buddy Allocator: Doubly-Linked Lists and Corruption Detection

**Rule**: The buddy allocator MUST use doubly-linked free lists with magic canary for O(1) removal and corruption detection.

## Why This Matters

Singly-linked free lists have two fatal flaws:
1. **O(n) removal** - Must traverse entire list to find predecessor when removing a node
2. **Infinite loop vulnerability** - If list becomes cyclic due to corruption, traversal hangs WHILE HOLDING LOCK

Original bug: After ~40 allocations, the free list became corrupted with a cycle, causing infinite loop in `remove_from_free_list()`. This hung the kernel during ELF segment loading.

## The Fix: Doubly-Linked Lists (The Linux Way)

### FreeBlock Structure
```rust
#[repr(C)]
struct FreeBlock {
    magic: u64, // Canary 0x4652454542304C - corruption detector
    next: u64,  // Frame number of next free block, or 0 if none
    prev: u64,  // Frame number of previous free block, or 0 if head
}

const FREE_BLOCK_MAGIC: u64 = 0x4652454542304C; // "FREEBL0C"
```

### Key Operations

**Add to list (O(1)):**
```rust
unsafe fn add_free_block(&self, zone: &mut MemoryZone, order: usize, addr: u64) {
    let block = &mut *(virt as *mut FreeBlock);

    // Set canary
    block.magic = FREE_BLOCK_MAGIC;
    block.next = old_head;
    block.prev = 0; // New head has no predecessor

    // Update old head's prev to point back to us
    if old_head != 0 {
        old_head_block.prev = frame_num;
    }

    zone.free_lists[order].head = frame_num;
    zone.free_lists[order].count += 1;
}
```

**Remove from list (O(1)):**
```rust
unsafe fn remove_from_free_list(&self, zone: &mut MemoryZone, order: usize, addr: u64) -> bool {
    // Check canary first
    if block.magic != FREE_BLOCK_MAGIC {
        trigger_gpf("Corrupted magic");
    }

    // O(1) removal - update prev's next and next's prev
    if block.prev != 0 {
        prev_block.next = block.next;
    } else {
        zone.free_lists[order].head = block.next; // Was head
    }

    if block.next != 0 {
        next_block.prev = block.prev;
    }

    // Invalidate the block
    block.magic = 0;
    block.next = 0;
    block.prev = 0;

    zone.free_lists[order].count -= 1;
    true
}
```

**Pop from head (O(1)):**
```rust
unsafe fn pop_free_block(&self, zone: &mut MemoryZone, order: usize) -> Option<u64> {
    // Check canary
    if block.magic != FREE_BLOCK_MAGIC {
        trigger_gpf("Corrupted magic at pop");
    }

    let next_frame = block.next;

    // Clear the block
    block.magic = 0;
    block.next = 0;
    block.prev = 0;

    // Update head and new head's prev pointer
    zone.free_lists[order].head = next_frame;
    if next_frame != 0 {
        next_block.prev = 0; // New head has no predecessor
    }
    zone.free_lists[order].count -= 1;

    Some(addr)
}
```

## Corruption Detection

The magic canary (`0x4652454542304C`) detects:
1. **Use-after-free** - Writing to freed memory corrupts the canary
2. **Buffer overflow** - Writing past allocation end corrupts adjacent free blocks
3. **Wild pointers** - Random writes to memory hit free blocks

When corruption is detected:
```rust
if block.magic != FREE_BLOCK_MAGIC {
    serial_trace("Corrupted magic: 0x%016x at addr 0x%016x", block.magic, addr);
    trigger_gpf(); // Die screaming - fail loud not silent
}
```

## Benefits

1. **O(1) removal** - No traversal needed, direct pointer manipulation
2. **No infinite loops** - Can't happen with direct access
3. **Corruption detection** - Canary catches use-after-free immediately
4. **Fail loud** - GPF on corruption instead of silent hang
5. **Linux-style** - Proven design from Linux kernel's buddy allocator

## Implementation Notes

- FreeBlock is 24 bytes (magic + next + prev), fits comfortably in 4KB minimum allocation
- Magic canary is checked on EVERY access to free blocks
- Blocks are invalidated (magic = 0) when popped to detect use-after-free
- Count is tracked separately and validated to prevent underflow
- All operations are O(1) - no loops over free list

## Common Bugs Prevented

1. **Cyclic list** - Can't happen, direct access via prev/next pointers
2. **Use-after-free** - Canary detects writes to freed memory
3. **Count underflow** - Defensive checks prevent subtract-with-overflow
4. **Stale pointers** - Cleared on pop, canary invalid on next access

## References

- Linux kernel: `mm/page_alloc.c` - Buddy allocator with doubly-linked free lists
- This implementation: `kernel/mm/mm-core/src/buddy.rs`

---
**Author**: GraveShift, BlackLatch, TorqueJax, SableWire
**Status**: Implemented and tested
**Impact**: Critical - prevents kernel hangs during memory allocation
