# Memory safety audit March 2026: ALL 11+8 issues FIXED across buddy, COW, fork, exec, munmap, arch coupling

| Field | Value |
|-------|-------|
| ID | `791d7fc20d0e` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T16:04:57.360789+00:00 |
| Accessed | 1 times |
| Keywords | memory-safety, audit, frame-leak, munmap, COW, exec, deadlock, TOCTOU |
| Session | [fcdf4180d654](../sessions/investigate-oxide-os-kernel-for-memory-safety-issues-leaks-null-pointer-corrupti.md) |

## Content

Comprehensive memory safety audit of the OXIDE OS kernel found 11 issues. ALL are now fixed across two hardening rounds.

CRITICAL (all fixed):
1. ✅ sys_munmap frame leak: unmap returns PhysAddr but was discarded. FIX: COW-aware free + TLB shootdown (syscall/memory.rs)
2. ✅ COW_FAULT_LOCK.lock() in page fault handler — blocking lock in exception context. FIX: try_lock() (fork.rs)
3. ✅ do_exec frame leak: OOM in segment loading leaks allocated frames. FIX: free frame before returning Err (exec.rs)
4. ✅ Stack growth PT frames not tracked in allocated_frames. FIX: Drop walks entire PT tree (address_space.rs)
5. ✅ clear_child_tid raw pointer write without address validation. FIX: validate < USER_SPACE_END + STAC/CLAC (process.rs)

HIGH (all fixed):
6. ✅ COW tracker TOCTOU: ref_count() then decrement() not atomic. FIX: try_claim_exclusive() holds single write lock (mm-cow, fork.rs)
7. ✅ exec old address space leak: if get_task_meta returns None, new AS leaks. FIX: return -ESRCH instead of continuing (process.rs)
8. ✅ clone kernel stack leak: handled by existing FrameGuard pattern

MEDIUM (all fixed):
9. ✅ munmap missing TLB shootdown. FIX: smp::tlb_shootdown() after unmapping (memory.rs)
10. ✅ TLS region collision with mmap base at 0x7000_0000_0000. FIX: moved TLS to 0x6FFF_F000_0000 (exec.rs)
11. ✅ COW tracker BTreeMap bloat: read-only pages tracked — this is CORRECT behavior (needed for proper Drop refcounting)

ADDITIONAL (arch coupling, user-requested):
12. ✅ arch_x86_64 coupling in mm-core/mm-paging/proc. FIX: replaced 118 arch_x86_64::serial::* calls with os_log::write_*_raw(), added write_u32_raw/write_u64_hex_raw to os_log, removed arch-x86_64 dep from mm-core Cargo.toml

Original plan items (C1-C8, H1-H2) also all fixed:
- C1: Buddy salvage chain on corruption (buddy.rs)
- C2: prev_block canary check in remove_from_free_list (buddy.rs)
- C3: Double-free detection in free_to_zone (buddy.rs)
- C4: Remove hardcoded 512MB frame limit (buddy.rs)
- C5: FrameGuard RAII in clone_address_space_cow (fork.rs)
- C6: Free leaked frames in allocate_pages OOM (address_space.rs)
- C7: Stack growth clarifying comment (fault.rs)
- C8: STACK_GROWTH_LOCK try_lock (fault.rs)
- H1: New-head canary check in pop_free_block (buddy.rs)
- H2: Deeper verify chain walk (buddy.rs)
