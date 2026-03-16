# Buddy allocator split code and canary handling - potential stale free list issue

| Field | Value |
|-------|-------|
| ID | `faeb96451df1` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T15:30:31.393523+00:00 |
| Accessed | 0 times |
| Keywords | buddy-allocator, split, canary, race-condition, free-list, corruption |
| Files | `kernel/mm/mm-core/src/buddy.rs` |

## Content

## Buddy Allocator Splitting (kernel/mm/mm-core/src/buddy.rs:545-577)

In alloc_from_zone, when splitting order N to order M:
```rust
for split_order in (order..current_order).rev() {
    let buddy_addr = addr + ((1u64 << split_order) << FRAME_SHIFT);
    unsafe { self.add_free_block(zone, split_order, buddy_addr) };
    zone.stats.free_pages[split_order].fetch_add(1, Ordering::Relaxed);
}
```

This loop adds buddy blocks to the free list. For example, splitting order 3 down to order 0:
- Iteration split_order=2: buddy_addr = addr + 4KB*4 = 0x4000 offset
- Iteration split_order=1: buddy_addr = addr + 4KB*2 = 0x2000 offset
- Iteration split_order=0: buddy_addr = addr + 4KB*1 = 0x1000 offset

Each buddy_addr is written to the free list via add_free_block.

## canary handling (lines 247-248, 385-397):
- On add_free_block: Sets block.magic = FREE_BLOCK_MAGIC (0x4652454542304C)
- On pop_free_block: Clears canary with block.magic = 0 (line 395)
- On list removal via remove_from_free_list: Clears magic (line 709)

## Potential Race Window:

If two allocations request the same buddy address:
1. Thread A splits order 5 → creates order 4 buddy at 0x1ff13000, writes magic
2. Thread B simultaneously splits same parent, tries to re-split 0x1ff13000
3. Thread B reads magic as 0x4652454542304C (valid)
4. Thread A pops this buddy from free list, clears magic to 0
5. Thread B continues with same address, unaware magic was cleared
6. Next pop of that address reads magic=0, triggers BUDDY-WARN, skips block, zeros free_list.head

This would explain why two corrupted blocks have magic=0 — they were popped by different allocations.
