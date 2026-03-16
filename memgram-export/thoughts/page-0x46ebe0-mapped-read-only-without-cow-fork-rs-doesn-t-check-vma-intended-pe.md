# Page 0x46ebe0 mapped read-only without COW: fork.rs doesn't check VMA-intended permissions

| Field | Value |
|-------|-------|
| ID | `c4b11dc2f692` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-04T16:37:16.078327+00:00 |
| Accessed | 0 times |
| Keywords | fork, COW, BSS, VMA, read-only, page-permissions, address-space |
| Files | `kernel/proc/proc/src/fork.rs`, `kernel/proc/proc/src/exec.rs`, `kernel/proc/proc/src/address_space.rs`, `kernel/src/fault.rs` |

## Content

## ROOT CAUSE: Fork Clone Without VMA Metadata

**Problem:** Page 0x46ebe0 (in BSS/data segment) is mapped read-only without COW bit in child process. Child write faults with SIGSEGV instead of COW copying.

**Root Cause Chain:**
1. exec.rs loads ELF with correct segment.flags (WRITE bit for BSS/data)
2. At some point, the page becomes read-only in parent (demand-paging, lazy allocation, or other optimization)
3. fork.rs::clone_address_space_cow (line 431) checks ONLY current PTE flags via `pt_entry.is_writable()`
4. Since PTE says read-only, fork treats page as intentionally read-only
5. Line 447-449: shares page without COW (no COW bit set)
6. Child write fault → present=1, write=1, COW=0 → not a COW fault → SIGSEGV

**The Architectural Problem:**
OXIDE OS has **NO VMA (Virtual Memory Area) structure**. Page permissions are stored ONLY in PTEs, not in segment metadata. This creates a gap:
- Fork cannot distinguish "intentionally read-only" from "temporarily read-only"
- The intended permissions from exec (WRITE bit in segment.flags) are **not persisted** after exec completes
- When fork runs, it has ONLY the current PTE state to work with

**Affected Code Locations:**
- fork.rs line 431: `if pt_entry.is_writable()` — checks only PTE, not intended permissions
- fork.rs lines 447-449: shares read-only pages without COW
- address_space.rs: No VMA tracking structure
- exec.rs: No metadata written about segment permissions

**Why This Breaks COW:**
In a system with VMAs, fork would check:
```
if vma.writable OR pte.writable {
    mark_cow = true
}
```
But in OXIDE, it checks only PTE:
```
if pte.is_writable() {
    mark_cow = true
} else {
    share_without_cow()  // ← BUG: assumes read-only is intentional
}
```

**Verification Steps:**
1. Check if page 0x46ebe0 is in a PT_LOAD segment with PF_W flag (readelf -l)
2. Check parent's PTE before fork (is it marked writable or read-only?)
3. Trace fork.rs line 431 to see if page is reaching the "else" branch
4. Search for code that temporarily marks pages read-only (demand-paging, lazy allocation)
