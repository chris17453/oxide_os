# EFFLUX Memory Management Specification

**Version:** 1.0
**Status:** Draft
**License:** MIT

---

## 0) Overview

EFFLUX implements a modern memory management system:

- **Physical Memory**: Buddy allocator for scalability
- **Kernel Heap**: Slab allocator for efficiency
- **Virtual Memory**: Multi-level paging with CoW support
- **User Memory**: Demand paging, mmap, shared memory

---

## Architecture-Specific Documentation

See `docs/arch/<arch>/MEMORY.md` for page table details:

- [x86_64](arch/x86_64/MEMORY.md) - 4/5-level paging, CR3, PCID
- [i686](arch/i686/MEMORY.md) - 2-level or PAE paging
- [AArch64](arch/aarch64/MEMORY.md) - TTBR0/TTBR1, MAIR, granules
- [ARM32](arch/arm/MEMORY.md) - 2-level, sections, domains
- [MIPS64](arch/mips64/MEMORY.md) - Software TLB, xkphys
- [MIPS32](arch/mips32/MEMORY.md) - Software TLB, kseg0/1
- [RISC-V 64](arch/riscv64/MEMORY.md) - Sv39/Sv48/Sv57
- [RISC-V 32](arch/riscv32/MEMORY.md) - Sv32

---

## 1) Architecture Abstraction

### Traits

| Trait | Purpose |
|-------|---------|
| `Mmu` | Page table creation, activation, cloning |
| `PageTableOps` | Map, unmap, protect, translate |
| `Tlb` | Invalidate page/range, shootdown |

### Generic MapFlags

| Flag | Description |
|------|-------------|
| READ | Readable |
| WRITE | Writable |
| EXEC | Executable |
| USER | User-accessible |
| NOCACHE | Uncached (device memory) |
| GLOBAL | Global mapping |
| HUGE | Use huge page |

Each arch converts to its PTE format.

---

## 2) Physical Memory Management

### Memory Map

| Type | Description |
|------|-------------|
| Usable | Free RAM |
| Reserved | Firmware reserved |
| AcpiReclaimable | Reclaimable after ACPI init |
| Kernel | Kernel code/data |
| FrameBuffer | GPU framebuffer |

### Boot-Time Bump Allocator

Simple allocator for early boot (before buddy is ready):
- Allocates from a contiguous region
- No free support
- Replaced by buddy allocator after init

### Buddy Allocator

Primary physical allocator:

| Property | Value |
|----------|-------|
| Order range | 0-10 (4KB - 4MB) |
| Free lists | One per order |
| Bitmap | Tracks allocated/split status |

**Operations:**
- `alloc(order)` - Find smallest block, split if needed
- `free(addr, order)` - Merge with buddy if free

### Per-CPU Page Cache

- Cache of order-0 pages per CPU
- Reduces lock contention
- Batch refill from buddy allocator

---

## 3) Kernel Heap (Slab Allocator)

For kernel allocations (kmalloc):

| Component | Purpose |
|-----------|---------|
| Slab cache | Per-type cache (e.g., task_struct) |
| Slab | Group of same-size objects |
| Generic caches | 8, 16, 32...4096 byte sizes |

**Object lifecycle:**
1. Allocate from partial slab
2. If none, allocate from empty slab
3. If none, create new slab (buddy alloc)
4. On free, return to slab

---

## 4) Virtual Memory

### VMA (Virtual Memory Area)

| Field | Description |
|-------|-------------|
| start, end | Address range |
| prot | Protection flags |
| flags | MAP_PRIVATE, MAP_SHARED, etc. |
| file | Backing file (or None) |
| offset | File offset |

### Page Fault Handler

| Fault type | Action |
|------------|--------|
| Not mapped | Check VMA, allocate page, map |
| CoW | Copy page, make writable |
| Permission | Send SIGSEGV |

### Copy-on-Write (CoW)

On fork():
1. Clone page table, mark all pages read-only
2. On write fault, copy page, update mapping
3. Reference count tracks shared pages

---

## 5) User Memory Operations

### mmap

| Type | Description |
|------|-------------|
| Anonymous | Zero-filled on demand |
| File-backed | Read from file on fault |
| Shared | Changes visible to others |
| Private | CoW semantics |

### Syscalls

| Syscall | Description |
|---------|-------------|
| mmap | Map memory region |
| munmap | Unmap region |
| mprotect | Change protection |
| brk | Adjust heap |
| mremap | Resize mapping |
| mlock | Lock in RAM |
| madvise | Hint to kernel |

---

## 6) Kernel Virtual Memory

### Layout (64-bit)

| Region | Purpose |
|--------|---------|
| Direct map | All physical memory |
| vmalloc | Non-contiguous allocations |
| Module space | Kernel modules |
| Fixmap | Fixed compile-time mappings |

### vmalloc

For large kernel allocations that don't need physical contiguity:
- Allocates individual pages
- Maps contiguously in virtual space

---

## 7) Memory Zones

| Zone | Purpose |
|------|---------|
| DMA | Legacy devices (< 16MB) |
| DMA32 | 32-bit devices (< 4GB) |
| Normal | Regular memory |
| HighMem | > 896MB on 32-bit (mapped on demand) |

Zone selection based on allocation constraints.

---

## 8) Special Memory Modes

### No-MMU Mode

For systems without MMU:
- No virtual addressing
- Physical addresses only
- Position-independent code required

### Tiny Memory Mode (< 64MB)

Optimizations for constrained systems:
- Smaller slab sizes
- Aggressive page reclaim
- No vmalloc

### Large Memory Mode (> 128GB)

Optimizations for servers:
- Larger buddy orders
- NUMA awareness
- Larger per-CPU caches

---

## 9) TLB Management

### Operations

| Operation | Description |
|-----------|-------------|
| invalidate_page | Single page |
| invalidate_range | Range of pages |
| invalidate_all | Full flush |
| shootdown | Cross-CPU invalidation via IPI |

MIPS note: Software TLB requires manual refill on miss.

---

## 10) Page Cache

Caches file data in memory:
- Read-ahead for sequential access
- Write-back with dirty page tracking
- Reclaimed under memory pressure

---

## 11) OOM Killer

When out of memory:
1. Try to reclaim pages (cache, inactive)
2. If still OOM, select process to kill
3. Score based on memory usage, nice value
4. Kill, wait for pages to be freed

---

## 12) Exit Criteria

- [ ] Buddy allocator working
- [ ] Slab allocator working
- [ ] Per-CPU page cache reduces contention
- [ ] Page tables work on all architectures
- [ ] mmap/munmap/mprotect working
- [ ] CoW fork working
- [ ] Demand paging working
- [ ] Page fault handler correct
- [ ] TLB shootdown working on SMP
- [ ] Page cache functional
- [ ] OOM killer functional
- [ ] Memory zones respected
- [ ] vmalloc working

---

*End of EFFLUX Memory Specification*
