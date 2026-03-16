# Fork must mark ALL present user pages as COW — not just writable ones

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `63e8b454d626` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-04T17:59:02.920121+00:00 |
| Keywords | COW, fork, clone_address_space_cow, VMA, page-table, SIGSEGV |
| Session | [381cc655b1f4](../sessions/fix-boot-bugs-cow-fork-marking-page-fault-diagnostics-pml4-corruption-investigat.md) |

## Details

Without VMAs, fork cannot distinguish genuinely read-only pages (code) from writable-segment pages that are temporarily RO. The safe approach: mark ALL present user pages with the COW bit during fork. For writable pages, also strip WRITABLE. For already-RO pages, just add COW. COW is harmless for code pages (nobody writes to them, so COW handler is never invoked). This applies to 4KB PT entries, 2MB huge PD entries, and 1GB huge PDPT entries.
