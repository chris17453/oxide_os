# Error: Pagedb array sized by buddy total_bytes instead of max physical address — frames

| Field | Value |
|-------|-------|
| ID | `ca2c1ae51a9a` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-05T14:41:18.130213+00:00 |
| Keywords | pagedb, buddy, max_pfn, total_bytes, DUP-ALLOC, OOM, memory-map |
| Session | [3f5f34350788](../sessions/fix-root-cause-of-hundreds-of-buddy-dup-alloc-stale-free-list-entries-causing-oo.md) |

## Error

Pagedb array sized by buddy total_bytes instead of max physical address — frames at high PFNs fell off the array, DUP-ALLOC guard leaked them as stale

## Cause

total_bytes excludes reserved regions so it's smaller than the actual physical address space. max_pfn = total_bytes/4096 made the array too short.

## Fix

Use max_phys_addr from memory map (Usable+BootServices regions only) to size the pagedb array. Also exclude MMIO regions to avoid absurdly large arrays.
