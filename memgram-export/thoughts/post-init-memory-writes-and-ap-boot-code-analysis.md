# Post-init memory writes and AP boot code analysis

| Field | Value |
|-------|-------|
| ID | `ddf1819f39e7` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T15:30:38.071245+00:00 |
| Accessed | 0 times |
| Keywords | init, memory-writes, AP-boot, trampoline, corruption |
| Files | `kernel/src/init.rs`, `kernel/arch/arch-x86_64/src/ap_boot.rs` |

## Content

## Post-Buddy-Init Memory Writes (kernel/src/init.rs:489+)

After buddy allocator init (line 465), the following writes to physical memory occur:

### 1. Framebuffer init (line 504: fb::init_from_boot)
- Initializes GOP framebuffer (read-only query of existing fb)
- Does NOT allocate or write to buddy-allocated frames

### 2. Page table setup (line 169, BEFORE buddy init)
- paging::setup_page_tables already executed
- Not a post-init write

### 3. Trampoline setup (line 701: arch::ap_boot::setup_trampoline)
- Copies trampoline to TRAMPOLINE_PHYS = 0x8000 (low memory, not in buddy allocator)
- Writes CR3, stack, entry values to 0x8000+data_offset (lines 52-67 in ap_boot.rs)
- These are fixed addresses, NOT dynamically allocated

### 4. AP kernel stacks (line 676-677: mm_manager::mm().alloc_contiguous(4))
- Allocates 4 pages for AP stack from buddy allocator
- But these are allocated AFTER init and BEFORE they're used
- No reason to zero them explicitly

## AP Boot Code Writes (kernel/arch/arch-x86_64/src/ap_boot.rs:26-68)

The setup_trampoline function (lines 31-68):
1. Copies trampoline assembly code to 0x8000 (lines 42-44)
2. Writes CR3 at offset data_offset (lines 52-55)
3. Writes stack pointer at offset data_offset + 8 (lines 58-61)
4. Writes entry point at offset data_offset + 16 (lines 64-67)

All writes are to the TRAMPOLINE_PHYS (0x8000) region, which is:
- Low memory (typically reserved by BIOS)
- NOT part of the buddy allocator's free list
- Fixed physical address, not from alloc()

## Critical Finding:
NO explicit zeroing of buddy-allocated frames post-init. The corruption pattern
(magic=0x0 in frames 0x1ff13000 and 0x1feea000) suggests these frames had
their canaries cleared by pop_free_block, not overwritten by some other code.
