# Fork Must Mark ALL Present User Pages as COW

## Rule
During `clone_address_space_cow()`, every present user page at every level (4KB, 2MB huge, 1GB huge) MUST have the COW bit set — not just writable pages.

## Why
Without VMA (Virtual Memory Area) tracking, the kernel cannot distinguish:
- **Genuinely read-only pages** (code/rodata) — should stay read-only forever
- **Temporarily read-only pages** (writable segments mapped RO by exec) — should become writable on write

If only writable pages get the COW bit, read-only-but-should-be-writable pages are shared without COW. When the child writes to such a page, `handle_cow_fault()` finds no COW bit and returns `false` → SIGSEGV.

## Fix
```rust
// For writable pages: strip WRITABLE, add COW
let new_flags = flags & !PageTableFlags::WRITABLE | PageTableFlags::COW;

// For read-only pages: just add COW (leave WRITABLE clear)
let flags = pt_entry.flags() | PageTableFlags::COW;
```

## Why COW on code pages is harmless
COW pages that are never written to never trigger the COW handler. The COW bit is just a software-defined bit (bit 9) in the PTE — it has zero performance impact on reads. The only cost is a slightly larger COW tracker refcount table, which is negligible.

## Affected levels
- 4KB pages (PT entries) — `fork.rs` line ~430
- 2MB huge pages (PD entries) — `fork.rs` line ~367
- 1GB huge pages (PDPT entries) — `fork.rs` line ~305

## Verified
Build 71: oxide-test (pid=9) passed 17 tests without SIGSEGV. Previously crashed on first test with write fault at 0x46ebe0.
