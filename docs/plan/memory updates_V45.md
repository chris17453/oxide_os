# OXIDE Memory Updates Plan (V45)

## Problem Statement
OXIDE already has core VM plumbing (buddy allocator, PageDB, VMAs, COW, basic demand paging), but it is still missing Linux-grade memory behavior for sustained workloads: reclaim, page-cache/writeback integration, swap, and real VM policy knobs.

This plan prioritizes doing the work in the right order: **measure first, then reclaim mechanics, then policy tuning**.

## What Is Already Done (Do Not Rebuild)
- Buddy allocator + PageDB hardening (canary checks, double-free detection, event history).
- Per-CPU slab path for kernel allocations; linked-list heap fallback.
- VMA model (`mm-vma`) with region classification used by page-fault handling.
- COW fork/fault path for 4K pages; TLB shootdown safety work.
- Demand paging for anonymous private mappings + stack growth path.
- OOM callback plumbing from `mm-manager` into kernel OOM killer.

## Current Gaps (High Impact)
- No reclaim engine (`kswapd`/direct reclaim, no LRU aging/eviction loop).
- No full page-cache + writeback integration for VM semantics.
- No swap in/out path.
- `sys_madvise` is currently advisory no-op.
- VM knobs (like swappiness) are not yet meaningful because reclaim/swap are incomplete.

---

## Priority Roadmap (Correct Dependency Order)

### P0 - Observability Foundation (must come first)
**Goal:** Make memory behavior measurable before changing policy.

**Deliverables**
- Expand `/proc/meminfo` with active counters backed by live MM state.
- Add `/proc/vmstat` counters (faults, alloc/free, reclaim scan/reclaim, dirty/writeback, OOM events).
- Add `/proc/zoneinfo` from buddy zone/watermark state.
- Add pressure signal export (`/proc/pressure/memory` or equivalent kernel metric endpoint).
- Add low-overhead internal counters in fault, alloc, and VFS mmap paths.

**Exit Criteria**
- Boot-time and steady-state counters are non-zero and internally consistent.
- `make build` and `make test-kernel` remain green.
- Headless run logs show stable counter progression under stress tests.

### P1 - Page Cache + Writeback Data Model
**Goal:** Build the file-page lifecycle model reclaim will operate on.

**Deliverables**
- Unified page-cache objects (file index -> page frame/state).
- Page states: clean/dirty/writeback/evictable; reference/age bits for reclaim.
- MAP_SHARED dirty tracking and writeback hooks.
- Background flusher worker + basic writeback throttling points.

**Exit Criteria**
- File-backed mappings resolve through page cache.
- Dirty pages are flushed and return to clean state.
- No silent data loss across mmap/write/unmap cycles.

### P2 - Reclaim Core (kswapd + direct reclaim)
**Goal:** Keep system alive under pressure without immediate OOM kills.

**Deliverables**
- Zone watermarks (`min/low/high`) and reclaim triggers.
- File/anon LRU lists (inactive/active baseline is acceptable for first cut).
- `kswapd` background reclaim loop.
- Direct reclaim on allocation slow path.
- Shrinker interface for reclaimable kernel caches.

**Exit Criteria**
- Under stress, system reclaims before OOM in normal cases.
- Reclaim counters match observed memory pressure behavior.
- No livelock/deadlock in reclaim path.

### P3 - Swap Subsystem + Swappiness Activation
**Goal:** Add anon memory backpressure relief and make swappiness meaningful.

**Deliverables**
- Swap area abstraction (`/proc/swaps`, slot map, swap cache).
- Anon page-out and fault-time page-in pipeline.
- Basic replacement policy integration with reclaim.
- `vm.swappiness` influences anon-vs-file reclaim balance.

**Exit Criteria**
- Anonymous pages can be swapped out and faulted back in correctly.
- `swappiness=0` and high swappiness show measurable behavior differences.
- No corruption across swap-out/swap-in cycles.

### P4 - VM Policy/Control Surface
**Goal:** Expose tuning knobs only after mechanisms are real.

**Deliverables**
- `/proc/sys/vm/*` core knobs (`swappiness`, watermark tuning, dirty ratios/timeouts).
- Stronger OOM scoring inputs from live counters.
- Wire memory cgroup enforcement to active accounting/reclaim events.

**Exit Criteria**
- Knob changes have predictable measurable effects.
- OOM behavior is explainable from vmstat + policy settings.

### P5 - Performance and Advanced Features
**Goal:** Throughput/latency improvements after correctness baseline.

**Candidates**
- THP/hugepage policy.
- NUMA awareness.
- Smarter readahead and writeback balancing.

---

## Execution Rules
- No feature knob is exposed without working mechanism behind it.
- Each phase must ship with counters first, behavior second, tuning last.
- Keep compatibility with existing hardened MM paths (buddy/PageDB/COW invariants).
- Validate every phase with existing build/test flow (`make build`, `make test-kernel`) plus targeted headless stress runs.

## Immediate Next Step
Start **P0** by defining a canonical vmstat counter schema and wiring minimal counters into:
1) page-fault handler, 2) buddy alloc/free paths, 3) mmap/munmap/brk syscalls, 4) file-backed mapping path.
