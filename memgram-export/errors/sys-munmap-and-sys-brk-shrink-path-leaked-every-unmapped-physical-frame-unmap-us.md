# Error: sys_munmap and sys_brk shrink path leaked every unmapped physical frame - unmap_

| Field | Value |
|-------|-------|
| ID | `bbaefeb161bc` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T16:19:36.343167+00:00 |
| Keywords | munmap, brk, frame-leak, COW, TLB-shootdown, memory-leak |
| Session | [84e1202df436](../sessions/implement-memory-system-hardening-plan-buddy-allocator-corruption-fixes-fork-exe.md) |

## Error

sys_munmap and sys_brk shrink path leaked every unmapped physical frame - unmap_user_page returns PhysAddr but it was discarded with let _ = ...

## Cause

The result of unmap_user_page (the PhysAddr of the unmapped frame) was discarded without freeing the frame back to the buddy allocator. COW-aware freeing was also missing.

## Fix

Free frames after unmapping using COW-aware decrement (cow.decrement → free if remaining==0). Also add TLB shootdown after unmapping to prevent use-after-free from stale TLB entries on other CPUs.
