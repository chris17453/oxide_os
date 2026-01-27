# V2 Memory/Allocator Audit (x86_64 focus)

## What exists today
- **Physical frames**: `mm-frame::BitmapFrameAllocator` (bitmap, 4 GB cap). Initialized in `kernel/src/init.rs` from boot memory map; global `FRAME_ALLOCATOR` + `mm_frame::frame_allocator()`. Protects frame 0; marks kernel image used.
- **Kernel heap**: `mm-heap::LinkedListAllocator` via global `HEAP_ALLOCATOR` on static 16 MB buffer (`HEAP_STORAGE`). No guard pages or redzones; no canaries; only used/free counters.
- **Paging**: `mm-paging::PageMapper` builds tables using frame allocator; flags in `PageTableFlags`; direct-map helpers `phys_to_virt/virt_to_phys`.
- **User mappings**: `crates/syscall/syscall/src/memory.rs` implements mmap/munmap/mprotect/mremap using `frame_allocator()` and `address_space.allocate_pages`. MAP_SHARED not coherent; COW not wired; brk stub. mremap uses STAC/CLAC for copy; mmap file-backed read uses raw slice into user VA.
- **COW tracker**: `mm-cow` exists (frame refcounts) but not integrated in syscalls or fault path.
- **Other consumers**:
  - V86 (`crates/compat/v86/src/memory.rs`): Vec-backed 1 MB space; independent allocator.
  - Hypervisor (`crates/hypervisor/vmm/src/memory.rs`): guest regions backed by caller-provided HVAs; copies via unsafe ptr; no host-side checks.
  - Cgroups (`crates/container/cgroup/src/memory.rs`): accounting only; not charged from allocators.
  - Kernel memory helpers: `kernel/src/memory.rs` exposes stats for /proc; wraps allocators.

## Gaps vs spec / risks
- Spec mismatch: spec calls for buddy + slab, vmalloc, zones, per-CPU caches, COW; current is bitmap + simple heap, no vmalloc/zones/COW path.
- Frame allocator hard cap at 4 GB; no NUMA/zones; no per-CPU caches.
- Kernel heap lacks guard pages/redzones; single 16 MB static arena; no panic-on-oom path separation.
- User mmap:
  - No COW integration; MAP_SHARED not synced; MAP_PRIVATE behaves as eager copy.
  - MAP_FIXED validation minimal; length/addr overflow not robust.
  - File-backed mmap reads via raw slice into user VA; relies on mapping correctness; no msync.
  - mprotect only adds WRITABLE; cannot remove perms; ignores EXEC/USER changes.
  - brk stubbed.
- Accounting: no linkage to cgroups; no per-process charge/limit; no global pressure feedback.
- Safety: hypervisor guest writes/reads trust HVA; V86 writes unrestricted; no guard rails.

## Duplications / fragmentation
- Multiple alloc sources: FRAME_ALLOCATOR (phys), HEAP_ALLOCATOR (kernel heap), ad-hoc Vec in V86/VMM, raw frame allocations in syscalls. No single choke point or accounting.
- COW tracker exists separately from page mapping and fault handling (unused).
- Memory stats split between kernel helpers and cgroup counters.

## Recommendations (stabilize & unify)
1) **Single authority layering**
   - Make `mm-frame` the sole physical source; add an API surface for “charge + alloc” so higher layers (kernel heap, user pages, vmm) go through a common adapter.
   - Introduce a kernel “allocator facade” (phys + virt) that exposes: frame_alloc/free, page_map/unmap/update, user charge hooks, and passes a cgroup/process handle.
2) **Robust page mapping**
   - Harden mmap inputs (overflow-safe addr+len; enforce USER range; strict MAP_FIXED alignment).
   - Integrate COW tracker: fork marks COW + refcount; faults perform copy and flag update; unmap decrements.
   - Implement mprotect flag replacement (not just add WRITABLE); handle EXEC/USER/NO_EXEC.
   - Add MAP_SHARED semantics placeholder with explicit EOPNOTSUPP until msync/pagecache arrives.
3) **Kernel heap hardening**
   - Add optional guard pages around heap arena using paging; add redzone/canary in allocator (low overhead).
   - Split general heap vs interrupt-safe small allocs (or add pool) to avoid heap lock in IRQ paths.
4) **Allocator visibility / accounting**
   - Wire allocations to cgroup/process accounting hooks (charge before alloc, uncharge on free); expose pressure signals.
   - Extend `get_memory_stats` with allocated-by-type (heap, user, page tables) if available.
5) **VM/v86 safety**
   - Require VMM region creation to validate HVA range and alignment; gate writes through checked copy helpers.
   - For V86, consider bounds-checked interfaces only; reject direct mutable slices for non-conventional ranges.
6) **Spec alignment path**
   - Replace bitmap cap with dynamic-sized buddy (spec) and per-CPU page caches.
   - Add slab caches for kernel types; keep linked-list only as bootstrap.
   - Implement vmalloc region for non-contiguous kernel allocations; maintain direct map integrity.
   - Add memory zones abstraction (DMA/DMA32/Normal) even if single-zone on x86_64 initially.

## Immediate actionable fixes (low risk)
- Harden mmap/mprotect argument validation and overflow checks.
- Hook COW tracker in fork/mmap COW paths and unmap to refcount.
- Add guard pages around heap arena (map/unmap) and a basic redzone check in `LinkedListAllocator`.
- Add accounting hook shims (no-op impl) so cgroups can be connected later without API churn.
- Raise MAX_MEMORY or make bitmap size dynamic, or migrate to buddy to avoid silent >4 GB misaccounting.

## Ownership proposal
- Define a `MemoryManager` facade in kernel that is the only entry for: frame alloc/free, map/unmap/update, user alloc/free, heap init. Route syscalls/vmm/v86 through it. Expose auditing/charge hooks and configurable safety (guards, redzones, assertions).
