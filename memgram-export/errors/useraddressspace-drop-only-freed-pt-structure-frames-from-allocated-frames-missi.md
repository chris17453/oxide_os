# Error: UserAddressSpace::Drop only freed PT structure frames from allocated_frames, mis

| Field | Value |
|-------|-------|
| ID | `ebc01d7e1586` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T16:19:46.576912+00:00 |
| Keywords | stack-growth, page-table, frame-leak, Drop, allocated_frames, TrackingAllocator |
| Session | [84e1202df436](../sessions/implement-memory-system-hardening-plan-buddy-allocator-corruption-fixes-fork-exe.md) |

## Error

UserAddressSpace::Drop only freed PT structure frames from allocated_frames, missing frames created by handle_stack_growth (direct alloc_frame bypasses TrackingAllocator)

## Cause

handle_stack_growth in fault.rs allocates intermediate PDPT/PD/PT frames via mm().alloc_frame() which does not go through TrackingAllocator. These frames were never added to allocated_frames, so Drop couldn't find and free them.

## Fix

Enhanced Drop to walk the entire PT tree and collect all intermediate PT structure frames during the walk, then free them all. Belt-and-suspenders: also frees any remaining frames from allocated_frames that weren't found during the walk, with dedup to avoid double-free.
