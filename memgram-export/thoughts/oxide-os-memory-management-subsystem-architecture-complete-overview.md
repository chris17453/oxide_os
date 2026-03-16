# OXIDE OS memory management subsystem architecture - complete overview

📌 Pinned

| Field | Value |
|-------|-------|
| ID | `c6cd9d95c63a` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-05T03:23:10.506823+00:00 |
| Accessed | 0 times |
| Keywords | memory-management, buddy-allocator, COW, page-tables, address-space, frame-allocator, VMA |
| Session | [6d914257327f](../sessions/thoroughly-explore-oxide-os-memory-management-subsystem-buddy-allocator-cow-trac.md) |

## Content

## Physical Frame Tracking (Buddy Allocator)

### File: `/home/nd/repos/Projects/oxide_os/kernel/mm/mm-core/src/buddy.rs`

**FreeBlock Structure (lines 50-54):**
- `magic: u64` = 0x4652454542304C ("FREEBL0C") — corruption canary
- `next: u64` = frame number of next free block (or 0 if last)
- `prev: u64` = frame number of previous free block (or 0 if head)
- **Doubly-linked structure** with O(1) removal via prev/next pointers

**Key Methods:**
- `pop_free_block(zone, order)` (lines ~140): Validates canary, handles corruption by severing chains
- `add_free_block(zone, order, addr)` (lines ~550): Writes magic + pointers to the block itself
- `remove_from_free_list(zone, order, addr)` (lines ~730): O(1) removal using doubly-linked pointers, validates both prev/next canaries
- `free_to_zone(zone, addr, order)` (lines ~680): Coalesces buddy blocks up the orders
- Double-free detection via magic canary check before adding to free list
- Frame watchdog (`WATCHED_FRAME`) blocks frees of critical frames during fork

**BuddyAllocator Struct (lines 76-89):**
```rust
pub struct BuddyAllocator {
    zones: [Mutex<MemoryZone>; 3],  // DMA, Normal, High
    stats: MemoryStats,
    initialized: bool,
}
```

### Memory Zones Structure (kernel/mm/mm-core/src/zone.rs, lines 81-102)

**MemoryZone:**
- `zone_type: ZoneType` — Dma/Normal/High
- `base: PhysAddr` — zone start
- `size: u64` — zone size in bytes
- `free_lists: [FreeListHead; 11]` — orders 0-10 (4KB to 4MB blocks)
- `stats: ZoneStats` — per-zone statistics

**FreeListHead (lines 96-102):**
- `head: u64` — physical frame number of first free block
- `count: u64` — number of free blocks at this order

**Zone Boundaries:**
- DMA: 0 to 16MB
- Normal: 16MB to 4GB
- High: 4GB to 2^64

## COW Reference Counting

### File: `/home/nd/repos/Projects/oxide_os/kernel/mm/mm-cow/src/lib.rs`

**CowTracker Struct (lines 22-27):**
```rust
pub struct CowTracker {
    counts: RwLock<BTreeMap<usize, u32>>,  // frame_num -> ref_count
}
```

**Key Methods:**
- `ref_count(phys: PhysAddr) -> u32` (lines 40-43): Returns 0 if not tracked
- `increment(phys)` (lines 49-54): Inserts with count=1 if new, increments existing
- `decrement(phys) -> u32` (lines 61-77): Returns new count, removes if 0
- **`try_claim_exclusive(phys) -> bool`** (lines 130-153): **CRITICAL FIX** for TOCTOU race
  - Holds single write lock for entire check-and-act
  - Returns true if count was 0 or 1 (exclusive), removes entry
  - Returns false if count > 1 (shared), decrements and caller must copy
  - Prevents race where concurrent fork increments between read and write locks

**Global Access:**
- `static COW_TRACKER: CowTracker` (line 162)
- Helper functions: `cow_inc()`, `cow_dec()`, `cow_is_shared()`, `cow_count()`

## Address Space Management

### File: `/home/nd/repos/Projects/oxide_os/kernel/proc/proc/src/address_space.rs`

**UserAddressSpace Struct (lines 23-35):**
```rust
pub struct UserAddressSpace {
    pml4_phys: PhysAddr,                  // PML4 physical address
    mapper: PageMapper,                   // Page mapping interface
    allocated_frames: Vec<PhysAddr>,      // PT structure frames ONLY (not PML4!)
    pub vmas: VmAreaList,                 // Virtual memory area metadata
}
```

**Critical Design:**
- PML4 is tracked separately (pml4_phys), NOT in allocated_frames
- allocated_frames contains ONLY page table structure frames (PDPT/PD/PT)
- User data frames are freed via page table walk in Drop (not tracked here)
- TrackingAllocator wrapper (lines 627-669) tracks frames during map_user_page

**Key Methods:**
- `new_with_kernel(allocator, kernel_pml4)` (lines 44-77): Creates new user space, copies kernel mappings
- `from_raw(pml4, frames, vmas)` (lines 112-126): Validates PML4 not in allocated_frames
- `allocate_pages(virt, num_pages, flags, allocator)` (lines 245-368):
  - Phase 1: Allocate all frames (each alloc releases lock to prevent deadlock)
  - Phase 2: Zero and map frames (locks already released)
  - On OOM: Frees already-collected frames before returning error
- `add_vma(vma)` (line 373): Non-fatal — page tables are authority
- `translate(virt)` (line 182): VA to PA translation

**Drop Implementation (lines 438-623):** **THREE-STEP CLEANUP**
1. **Step 1:** Walk user-space page tables (PML4[0-255]), free leaf frames with COW decrement
2. **Step 2:** Free all PT structure frames discovered during walk (catches both TrackingAllocator AND direct alloc_frame calls)
3. **Step 3:** Free remaining frames in allocated_frames not found during walk (belt and suspenders)

Critical detail: Walks PT tree to discover frames because handle_stack_growth() allocates PT frames via direct alloc_frame() calls that bypass TrackingAllocator.

## Page Table Types

### File: `/home/nd/repos/Projects/oxide_os/kernel/mm/mm-paging/src/entry.rs`

**PageTableFlags (lines 9-32):**
```rust
bitflags! {
    pub struct PageTableFlags: u64 {
        const PRESENT = 1 << 0;
        const WRITABLE = 1 << 1;
        const USER = 1 << 2;
        const WRITE_THROUGH = 1 << 3;
        const NO_CACHE = 1 << 4;
        const ACCESSED = 1 << 5;
        const DIRTY = 1 << 6;
        const HUGE_PAGE = 1 << 7;
        const GLOBAL = 1 << 8;
        const NO_EXECUTE = 1 << 63;
        const COW = 1 << 9;  // Software bit for COW
    }
}
```

**PageTableEntry (lines 38-120):**
```rust
#[repr(transparent)]
pub struct PageTableEntry(u64);
```
- `new(addr, flags)` — Creates entry with address and flags
- `addr()` — Extracts physical address via ADDR_MASK (0x000F_FFFF_FFFF_F000)
- `is_present()` — Checks PRESENT flag
- `is_cow()` — Checks COW flag (bit 9)
- `is_huge()` — Checks HUGE_PAGE flag (2MB at PD, 1GB at PDPT)
- `is_writable()` — Checks WRITABLE flag

**PageTable (lines 6-72 in table.rs):**
```rust
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; 512],
}
```
- 512 entries × 8 bytes = 4KB, cache-aligned
- Indexing via PML4/PDPT/PD/PT level (each level is 9 bits of vaddr)

## Address Types (os_core)

### File: `/home/nd/repos/Projects/oxide_os/kernel/core/os_core/src/addr.rs`

**PhysAddr (lines 6-125):**
```rust
#[repr(transparent)]
pub struct PhysAddr(u64);
```
- Methods: `new()`, `as_u64()`, `as_usize()`, `is_null()`, `align_up()`, `align_down()`, `is_aligned()`
- Operators: Add/Sub with u64, Sub with PhysAddr
- Page alignment: `page_align_up()`, `page_align_down()`

**VirtAddr (lines 127-262):**
```rust
#[repr(transparent)]
pub struct VirtAddr(u64);
```
- Same interface as PhysAddr
- Additional: `as_ptr()`, `as_mut_ptr()` for pointer conversion
- `from_ptr()` constructor for pointer to VirtAddr

## Memory Manager Facade

### File: `/home/nd/repos/Projects/oxide_os/kernel/mm/mm-manager/src/lib.rs`

**MemoryManager (lines 83-86):**
```rust
pub struct MemoryManager {
    buddy: BuddyAllocator,
}
```

Global access:
- `init_global(mm: &'static MemoryManager)` — Initialize global reference
- `mm() -> &'static MemoryManager` — Panics if not initialized
- `try_mm() -> Option<&'static MemoryManager>` — Safe accessor

Methods delegate to buddy allocator:
- `alloc_frame()` — Single frame
- `alloc_frames(count)` — Contiguous frames (rounds up to power of 2)
- `alloc_contiguous(count)` — Same as alloc_frames
- `free_frame(addr)` — Single frame
- `free_frames(addr, count)` — Contiguous frames

## FrameAllocator Trait

### File: `/home/nd/repos/Projects/oxide_os/kernel/mm/mm-traits/src/lib.rs`

```rust
pub trait FrameAllocator {
    fn alloc_frame(&self) -> Option<PhysAddr>;
    fn free_frame(&self, addr: PhysAddr);
    fn alloc_frames(&self, count: usize) -> Option<PhysAddr>;
    fn free_frames(&self, addr: PhysAddr, count: usize);
}
```

**Frame Watchdog:**
- `WATCHED_FRAME: AtomicU64` — Critical frame address being protected (0 = none)
- `set_frame_watch(phys)` — Set watchdog during fork
- `clear_frame_watch()` — Clear after fork or on error
- `is_frame_watched(phys)` — Check if address matches (used in buddy.free())

Used for fork PML4 protection: If anyone tries to free the child PML4 during fork, the buddy allocator skips the free and logs violation.

## VMA System

### File: `/home/nd/repos/Projects/oxide_os/kernel/mm/mm-vma/src/lib.rs`

**VmFlags (lines 18-26):**
- READ, WRITE, EXEC, SHARED, GROWSDOWN, STACK, DONTCOPY

**VmType (lines 32-42):**
- Text, Data, Bss, Stack, Heap, Anon, FileBacked, Tls, Guard

**VmArea (lines 48-61):**
```rust
pub struct VmArea {
    pub start: u64,           // Page-aligned, inclusive
    pub end: u64,             // Page-aligned, exclusive
    pub flags: VmFlags,
    pub vm_type: VmType,
    name: [u8; 32],           // Fixed-size, no heap alloc
    name_len: u8,
}
```

Design philosophy: Page tables are authoritative. VMA metadata is informational — helps with /proc, debugging, and fault handling classification. Non-fatal errors in VMA operations.
