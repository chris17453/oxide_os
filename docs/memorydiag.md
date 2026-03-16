# OXIDE Kernel Memory Management Map

This captures who owns which part of memory management, how components call each other, and the main flows to target in tests.

## Layered View
```
User syscalls (mmap/mprotect/brk/fork/exec) ──┐
                                             ▼
                                 Page fault handler (kernel/src/fault.rs)
                      ┌──────────────┬───────────────┴──────────────┬─────────────┐
                      │ COW handler  │ Demand paging                │ Stack grow  │
                      │ (proc/fork)  │ (fault.rs)                   │ (fault.rs)  │
                      └───────┬──────┴───────────────┬──────────────┴───────┬─────┘
                              │                      │                      │
                        Page tables (mm-paging)      │                      │
                              │                      │                      │
                 Frame allocator (mm-core buddy) ◄───┴─────► PageDB (mm-pagedb)
                              │
                     Zones: DMA / Normal / High
                              │
                        Kernel heap (mm-heap)
```

## Physical Memory
- **Buddy allocator (`kernel/mm/mm-core`)**
  - Files: `lib.rs`, `buddy.rs`, `zone.rs`, `stats.rs`.
  - Zones: DMA (<16MB), Normal (16MB–4GB), High (>4GB). Free lists per order (0–10 ⇒ 4KB–4MB).
  - APIs: `alloc_frame/free_frame`, `alloc_frames/free_frames`, `alloc_contiguous` (prefers DMA), `free_contiguous`.
  - Guards: canary header per free block, double-free detection, `mm_traits::set_frame_watch()` around fork.
- **Page frame DB (`kernel/mm/mm-pagedb`)**
  - Per-frame flags: `PF_FREE`, `PF_PAGETABLE`, `PF_COW`, etc.; refcounts + lock-free event ring for last 4096 alloc/free events.
- **Init path**
  - Bootinfo regions parsed in `kernel/src/init.rs` → `mm_manager::init()` → `buddy.init_regions()` populates zones and PageDB.

## Kernel Heap
- **Linked-list heap (`kernel/mm/mm-heap`)**
  - Files: `lib.rs`, `linked_list.rs`, `hardened.rs`.
  - Starts from static 16MB chunk; grow callback pulls contiguous pages from buddy.
  - Serves `GlobalAlloc` for kernel Rust allocations; no slab caches.

## Virtual Memory & Paging
- **Page tables (`kernel/mm/mm-paging`)**
  - Files: `mapper.rs`, `entry.rs`, `table.rs`, `demand.rs`.
  - 4-level x86_64, 4KB pages; flags include software `COW` bit.
  - APIs: `map/unmap/translate`, `update_flags/remove_flags`, `flush_tlb(_all)`.
  - Direct map: `PHYS_MAP_BASE = 0xFFFF_8000_0000_0000` used by allocators and copy paths.
- **Virtual memory areas (`kernel/mm/mm-vma`)**
  - Tracks semantic regions (text/data/heap/anon/file/stack/tls/guard) with flags (READ/WRITE/EXEC/SHARED/GROWSDOWN/DONTCOPY).
  - Sorted `VmAreaList` drives fault classification and `/proc/maps`.

## Copy-on-Write & Fork
- **COW tracker (`kernel/mm/mm-cow`)**: RwLock BTreeMap of frame → refcount; sets `PF_COW` in PageDB.
- **Fork flow (`proc/proc/src/fork.rs`)**
  1. Clone PML4, copy kernel half.
  2. Walk user pages: demote writable to RO + set COW, increment refcounts.
  3. Batch TLB shootdown.
  4. Watchdog prevents freeing parent PML4 during walk.
- **COW fault (`handle_cow_fault` in fork.rs)**: try exclusive claim; else allocate new frame, copy page, update PTE, INVLPG.

## Page Fault Handling
- Entry: `arch/arch-x86_64/src/exceptions.rs` → `page_fault_handler` (`kernel/src/fault.rs`).
- Flow:
  1. Decode error code (present/write/user/fetch).
  2. If present+write+COW flag → `handle_cow_fault`.
  3. Else classify via VMA: anonymous/data/bss/heap → `handle_demand_page`; stack with GROWSDOWN → `handle_stack_growth`.
  4. Unhandled/forbidden → return false (user SIGSEGV or kernel panic).
- Locks: global `COW_FAULT_LOCK`; `STACK_GROWTH_LOCK`; VMA lookup uses try-lock (ISR safety).

## Memory Syscalls
- `kernel/syscall/syscall/src/memory.rs`: `sys_mmap`, `sys_munmap`, `sys_mprotect`, `sys_brk`.
  - mmap picks gap unless `MAP_FIXED`; sets VMA + maps pages/file-backed regions; honors `MAP_PRIVATE`/`MAP_SHARED`/`MAP_STACK`.
  - munmap removes VMA, frees frames.
  - mprotect updates VMA + PTE flags.
  - brk grows/shrinks heap VMA, alloc/free pages accordingly.
- Process creation: `sys_fork` → `kernel_fork`; `sys_clone` → `kernel_clone`; `sys_execve` (in `proc/exec.rs`) rebuilds address space from ELF.
- IPC shared memory: `sys_shmget/shmat/shmdt` (IPC module) map shared pages into VMAs.

## Boot Memory Sequence (kernel/src/init.rs)
1) Serial/log init.  
2) Validate BootInfo, print regions.  
3) Heap init (16MB) + grow callback to buddy.  
4) Reset PCI/VirtIO before buddy to avoid DMA clobbering free lists.  
5) `mm_manager::init()` → buddy + PageDB.  
6) Arch init (GDT/IDT/TSS, SMEP/SMAP, handlers).  
7) Scheduler/process table; OOM handler registration.  
8) Root FS mount; drivers init.  
9) Launch `/init` (ELF loader creates VMAs: text/data/bss/stack).

## Driver-Facing Allocation
- `mm_traits::FrameAllocator` used by drivers; `mm().alloc_contiguous(count)` prefers DMA zone then falls back.
- DMA buffers pinned (no paging); consult `docs/agents/dma-physical-frame-allocator.md`.
- PageDB flags and event ring aid driver-related corruption debugging.

## Key Interactions
- **Scheduler**: context switch writes CR3 to task PML4; kernel half shared, avoiding TLB flush for kernel mappings.
- **VFS/File-backed mmap**: `mmap_file/vma.rs` handles file VMAs; faults read pages from file; MAP_PRIVATE uses COW, MAP_SHARED shares frames.
- **Signals**: stacks validated against VMAs; altstack allocations come from kernel heap.
- **Debug/safety rules (docs/agents)**: ISR try-locks; no heap alloc in ISR; SMAP buffer rules; TLB shootdown batching; DMA uses contiguous allocator.

## Coverage Targets (what to test)
- Buddy: zone selection (DMA/Normal/High), contiguous alloc/free, canary double-free detection, PageDB flags.
- Heap: grow callback path, hardened redzones (if enabled).
- Paging: map/unmap/update_flags, TLB flush paths, direct-map correctness.
- Faults: COW write faults, anon demand faults, stack growth, illegal access → SIGSEGV.
- Syscalls: MAP_PRIVATE vs MAP_SHARED, MAP_FIXED overlap, mprotect permission flips, brk growth/shrink, fork/exec with shared and private mappings.
- Boot: region filtering (exclude kernel/initramfs/PT), PCI reset before buddy, PageDB sizing for high memory.

## Flowchart: Page Fault Resolution
```
trap (#PF) → page_fault_handler
  ├─ present && write && PTE.COW ? handle_cow_fault
  │     ├─ refcount==1 → make writable in place
  │     └─ refcount>1  → alloc frame, copy, update PTE, INVLPG
  └─ !present
        ├─ VMA lookup ok?
        │     ├─ anon/heap/bss/data → handle_demand_page (alloc+zero+map)
        │     ├─ stack+GROWSDOWN    → handle_stack_growth
        │     └─ file-backed        → page-in via mmap_file fault path
        └─ else → SIGSEGV/panic
```

