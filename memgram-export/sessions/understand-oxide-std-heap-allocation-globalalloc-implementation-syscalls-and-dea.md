# Session: Understand oxide-std heap allocation: GlobalAlloc implementation, syscalls, and deadlock risks

| Field | Value |
|-------|-------|
| ID | `c526588aeb52` |
| Agent | claude-code |
| Model | claude-opus-4-6 |
| Project | oxide-os |
| Branch | - |
| Status | completed |
| Started | 2026-03-04T13:28:24.519068+00:00 |
| Ended | 2026-03-04T13:29:10.774959+00:00 |
| Compactions | 0 |

## Summary

Completed comprehensive analysis of oxide-std heap allocation architecture. Traced the allocation chain from Vec::with_capacity() through Rust std's GlobalAlloc, to oxide_rt's lock-free mmap-backed bump allocator, and identified the underlying syscalls and kernel handlers.

## Session Summary

**Outcome:** Successfully identified all components: (1) GlobalAlloc implementation using oxide_rt::alloc::mmap/munmap, (2) Two-tier allocator with 256KB bootstrap heap + 2MB mmap arenas, (3) Syscalls used (#90 MMAP, #91 MUNMAP), (4) Lock-free CAS implementation with no deallocation, (5) Deadlock risk assessment showing ProcessMeta lock contention as real risk but not in allocator itself. Pinned comprehensive observation to memory for future reference.

**Decisions:**

- Store findings in memory as pinned observation for cross-session reference
- Focus deadlock analysis on kernel-side ProcessMeta lock contention rather than allocator-side locking

**Unresolved:**

- Heap fragmentation risk for long-running processes (64MB cap with bump allocator)
