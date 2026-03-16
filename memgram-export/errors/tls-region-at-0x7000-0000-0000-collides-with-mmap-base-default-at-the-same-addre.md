# Error: TLS region at 0x7000_0000_0000 collides with MMAP_BASE_DEFAULT at the same addre

| Field | Value |
|-------|-------|
| ID | `300bf31affd9` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T16:35:54.012367+00:00 |
| Keywords | TLS, mmap, address-collision, exec, virtual-address-layout |
| Session | [b27c9dc86d34](../sessions/continue-memory-hardening-fix-remaining-audit-issues-cow-toctou-race-exec-as-lea.md) |

## Error

TLS region at 0x7000_0000_0000 collides with MMAP_BASE_DEFAULT at the same address — mmap(NULL) allocations can overwrite TLS pages

## Cause

exec.rs hardcoded TLS vaddr to exactly MMAP_BASE_DEFAULT (0x7000_0000_0000). The mmap allocator grows downward from this same address, so the first large mmap(NULL) or multiple small ones will eventually collide with the TLS region.

## Fix

Moved TLS to 0x6FFF_F000_0000 — safely below the mmap base. The mmap region grows downward from ~0x7000_0000_0000, TLS sits in a separate neighborhood.
