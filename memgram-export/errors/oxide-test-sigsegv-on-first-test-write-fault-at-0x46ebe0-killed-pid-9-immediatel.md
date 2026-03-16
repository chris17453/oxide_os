# Error: oxide-test SIGSEGV on first test — write fault at 0x46ebe0 killed pid=9 immediat

| Field | Value |
|-------|-------|
| ID | `152bda11c4bb` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-04T17:58:56.212133+00:00 |
| Keywords | COW, fork, SIGSEGV, page-fault, clone_address_space_cow, VMA, read-only |
| Session | [381cc655b1f4](../sessions/fix-boot-bugs-cow-fork-marking-page-fault-diagnostics-pml4-corruption-investigat.md) |

## Error

oxide-test SIGSEGV on first test — write fault at 0x46ebe0 killed pid=9 immediately after fork

## Cause

clone_address_space_cow() only marked writable pages as COW. Read-only pages (present+user but not writable) were shared without the COW bit. Without VMAs, the kernel can't distinguish genuinely read-only pages (code) from writable-segment pages that happen to be RO. When a child process wrote to such a page, handle_cow_fault() found no COW bit and returned false → SIGSEGV.

## Fix

Mark ALL present user pages as COW during fork, not just writable ones. For writable pages: strip WRITABLE, add COW. For read-only pages: add COW (leave WRITABLE clear). COW is harmless for genuine code pages (nobody writes, so COW handler never fires). For data pages, COW gives the child a private copy on write instead of SIGSEGV. Applied to 4KB pages, 2MB huge pages, and 1GB huge pages.
