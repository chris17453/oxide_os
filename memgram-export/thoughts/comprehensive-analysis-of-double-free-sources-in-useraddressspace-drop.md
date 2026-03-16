# Comprehensive analysis of double-free sources in UserAddressSpace::Drop

| Field | Value |
|-------|-------|
| ID | `5039c8194ccf` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-05T11:56:41.801470+00:00 |
| Accessed | 0 times |
| Keywords | double-free, UserAddressSpace, Drop, COW, pagedb, validate_free, handle_stack_growth |
| Files | `/home/nd/repos/Projects/oxide_os/kernel/proc/proc/src/address_space.rs`, `/home/nd/repos/Projects/oxide_os/kernel/src/fault.rs`, `/home/nd/repos/Projects/oxide_os/kernel/proc/proc/src/fork.rs`, `/home/nd/repos/Projects/oxide_os/kernel/mm/mm-cow/src/lib.rs`, `/home/nd/repos/Projects/oxide_os/kernel/mm/mm-pagedb/src/lib.rs` |
| Session | [2a0c56e2b291](../sessions/analyze-double-free-sources-in-useraddressspace-drop-implementation.md) |

## Content

## Double-Free Sources in UserAddressSpace::Drop

### Drop Implementation (3-step cleanup):
1. Walk user PT tree (PML4[0-255]), free leaf data frames with COW decrement, collect PT structure frames
2. Free all walked_pt_frames (PDPT/PD/PT), then free PML4
3. Free any allocated_frames NOT in walked_pt_frames (belt-and-suspenders dedup)

### Root Cause Analysis of 32 DoubleFree detections:

**Primary Source: handle_stack_growth PT frames not tracked in pagedb as PAGETABLE**
handle_stack_growth (fault.rs) allocates PDPT/PD/PT frames via direct mm().alloc_frame() but only marks DATA frames in pagedb. The intermediate PT frames get `mark_allocated(flags=PF_ALLOCATED)` from buddy.alloc() but never get `mark_pagetable()`. This is cosmetic but reveals poor tracking.

**The REAL root cause: the pagedb validates_free can produce false DoubleFree when `cow.decrement()` calls `ref_dec()` before `free_frame()` calls `mark_free()`**

Sequence for COW last-owner path:
1. cow.decrement(phys) enters if-let-Some branch, count 1→0, removes entry
2. db.ref_dec(phys) sets refcount 1→0 
3. cow.decrement returns 0
4. Drop calls mm.free_frame(phys)
5. validate_free sees: flags=PF_ALLOCATED|PF_MAPPED, refcount=0 → passes
6. free_to_zone adds to free list
7. mark_free sets flags=0, refcount=0

This is CORRECT but reveals that refcount=0 with PF_ALLOCATED is a transiently valid state.

**False DoubleFree source: exec dropping old address space while new one is being built**
When kernel_exec replaces m.address_space, the old UserAddressSpace::Drop fires. If the old address space was forked (COW), the frames are properly ref-counted. No actual double-free.

**Actual likely source: frames freed by sys_munmap/sys_brk during process lifetime**
If unmap_user_page removes a PT entry and frees the data frame, but the PT STRUCTURE frames remain in the tree (empty but present PTs), those PT structure frames get freed during Drop's walk. If those same frames are ALSO in allocated_frames (because TrackingAllocator tracked them), the dedup check at line 613 should catch them. Unless there's a race.

**Most likely actual source: 32 stack pages from contiguous allocation**
A kernel stack is ~32 pages allocated via alloc_frames(33). The buddy allocator's mark_allocated only marks the BASE address. But kernel stacks are in kernel space and shouldn't appear in user-space PT walks. So this isn't it either.

**Conclusion: Need to build and examine serial output to see the actual phys addresses triggering DoubleFree, then trace them back to their allocation point.**
