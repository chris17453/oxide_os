# Phase 1: Memory Management

**Stage:** 1 - Foundation
**Status:** In Progress
**Target:** x86_64 only
**Dependencies:** Phase 0 (Boot + Serial)

---

## Goal

Physical and virtual memory management with kernel heap.

---

## Deliverables

| Item | Status |
|------|--------|
| Memory map from bootloader | [ ] |
| Physical frame allocator | [ ] |
| Kernel page tables (4-level) | [ ] |
| Direct physical map | [ ] |
| Kernel heap allocator | [ ] |
| `Box::new()` works | [ ] |

---

## Implementation Plan

### 1. Boot Memory Map
- Bootloader passes UEFI memory map to kernel
- Parse usable RAM regions
- Reserve kernel code/data regions

### 2. Frame Allocator
- Bitmap allocator for frame tracking
- Frame size: 4KB (PAGE_SIZE)
- Track total/free/used frames

### 3. Page Tables (x86_64 4-level)
- PML4 → PDPT → PD → PT
- Identity map first 1GB for early boot
- Direct map all physical memory at 0xFFFF_8000_0000_0000
- Map kernel at higher half

### 4. Kernel Heap
- Linked-list allocator initially
- Heap region: 16MB initial

---

## Memory Layout (x86_64)

```
Virtual Address Space (48-bit canonical):

0x0000_0000_0000_0000 - 0x0000_7FFF_FFFF_FFFF  User space (future)
0xFFFF_8000_0000_0000 - 0xFFFF_8FFF_FFFF_FFFF  Direct physical map
0xFFFF_F000_0000_0000 - 0xFFFF_F000_00FF_FFFF  Kernel heap (16MB)
0xFFFF_FFFF_8000_0000 - 0xFFFF_FFFF_FFFF_FFFF  Kernel code/data
```

---

## Key Structures

```rust
// Physical frame
pub struct PhysFrame {
    addr: PhysAddr,
}

// Page table (512 entries × 8 bytes = 4KB)
pub struct PageTable {
    entries: [PageTableEntry; 512],
}

// Page table entry flags
// Bits: Present, Writable, User, WriteThrough, CacheDisable,
//       Accessed, Dirty, HugePage, Global, NX, Address[51:12]

// Frame allocator trait
pub trait FrameAllocator {
    fn allocate(&mut self) -> Option<PhysFrame>;
    fn deallocate(&mut self, frame: PhysFrame);
}
```

---

## Files to Create

```
crates/mm/
├── efflux-mm-frame/              # Frame allocator
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       └── bitmap.rs             # Bitmap allocator
├── efflux-mm-paging/             # Page tables
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── entry.rs              # PageTableEntry
│       ├── table.rs              # PageTable
│       └── mapper.rs             # Map/unmap
└── efflux-mm-heap/               # Kernel heap
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        └── linked_list.rs

crates/arch/efflux-arch-x86_64/src/
├── paging.rs                     # CR3, TLB flush
└── mod.rs                        # Export paging
```

---

## Exit Criteria

- [ ] Frame allocator can allocate/free frames
- [ ] Page tables set up with direct map
- [ ] Kernel heap functional
- [ ] `Box::new(42)` compiles and runs
- [ ] Memory stats printed on boot

---

## Notes

*(Add implementation notes as work progresses)*

---

*Phase 1 of EFFLUX Implementation*
