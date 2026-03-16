# Buddy allocator: mark_free in pagedb BEFORE free_to_zone — never after

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `c62c54981574` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-05T14:14:21.428586+00:00 |
| Keywords | buddy, allocator, pagedb, mark_free, free_to_zone, race, DoubleFree, ordering |

## Details

In buddy.free(), the pagedb must be updated to PF_FREE BEFORE free_to_zone adds the frame to the free list. The old order (free_to_zone first, mark_free second) created a race: free_to_zone makes the frame allocatable, another CPU pops it, mark_free then stomps the new owner's pagedb to PF_FREE. This caused cascading DoubleFree, RefcountUnderflow, and PML4 corruption (PML4[256]=0x0). Fix: mark_free() → free_to_zone(). Pagedb and free list stay in sync.
