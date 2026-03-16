# Implemented mm-pagedb crate: Linux-style page frame database (struct page)

| Field | Value |
|-------|-------|
| ID | `899be3ffb0d7` |
| Type | decision |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-05T11:34:08.721016+00:00 |
| Accessed | 0 times |
| Keywords | mm-pagedb, page-frame-database, struct-page, frame-metadata, corruption-detection, buddy-allocator, COW, fork |
| Files | `kernel/mm/mm-pagedb/src/lib.rs`, `kernel/mm/mm-core/src/buddy.rs`, `kernel/mm/mm-cow/src/lib.rs`, `kernel/proc/proc/src/fork.rs`, `kernel/proc/proc/src/address_space.rs`, `kernel/src/init.rs`, `kernel/src/fault.rs` |
| Session | [52a1622fd0a6](../sessions/implement-linux-style-page-frame-database-struct-page-mm-pagedb-crate-with-pagef.md) |

## Content

## mm-pagedb Implementation

Created `kernel/mm/mm-pagedb/` crate providing per-frame metadata (16 bytes/frame):
- PageFrame: flags (AtomicU32), refcount (AtomicU32), owner PID (AtomicU32), order_and_reserved (AtomicU32)
- PageDatabase: flat array indexed by PFN, global singleton (try_pagedb()/pagedb())
- Flags: PF_FREE, PF_ALLOCATED, PF_MAPPED, PF_PAGETABLE, PF_RESERVED, PF_COW, PF_KERNEL, PF_SLAB, PF_DIRTY

Integration points:
1. Boot init (init.rs): Allocated after buddy init, pre-populates reserved frames (low mem, kernel, boot PTs, UEFI guard, array itself)
2. Buddy allocator (buddy.rs): mark_allocated on alloc, validate_free + mark_free on free
3. COW tracker (mm-cow): ref_inc/ref_dec mirrored, PF_COW flag managed
4. Fork (fork.rs): All child PT frames marked PF_PAGETABLE
5. Address space (address_space.rs): TrackingAllocator marks PT frames, allocate_pages marks user data as PF_MAPPED
6. Stack growth (fault.rs): Data frames marked PF_ALLOCATED|PF_MAPPED

Cost: 256MB RAM = 65536 frames × 16 bytes = 1MB (0.4% overhead)
