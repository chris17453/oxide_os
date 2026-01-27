# Buddy Allocator Bug - FIXED!

## Summary

**Root cause**: The UEFI stack overlapped with memory the buddy allocator considered free.

**Fix**: Added UEFI stack protection in `kernel/src/init.rs` by reading RSP at memory manager init time and protecting that region.

## The Fix

In `kernel/src/init.rs`, added:

```rust
// CRITICAL: Protect the UEFI stack region!
let current_rsp: u64;
unsafe { core::arch::asm!("mov {}, rsp", out(reg) current_rsp); }

let stack_bottom = (current_rsp & !0xFFF).saturating_sub(0x100000); // 1MB below current RSP
let stack_top = (current_rsp & !0xFFF).saturating_add(0x1000000);   // 16MB above current RSP
let stack_protection_end = if current_rsp > 0xf000000 { 0x10000000 } else { stack_top };

// Added to is_protected() closure:
if addr >= stack_bottom && addr < stack_protection_end {
    return true;
}
```

## Verification

Serial output shows fix working:

```
[INFO]   UEFI Stack: 0xfdcb000 - 0x10000000 (RSP=0xfecb1a8)
...
[SCHED2] pre-Mutex::new: 0xfec5=0x0000000000000000 0xfec4=0x0000000000000000
[SCHED2] post-Mutex::new: 0xfec5=0x0000000000000000 0xfec4=0x0000000000000000
```

Frames 0xfec4/0xfec5 now show 0x0 (not in free list anymore - protected).

## System Now Boots Through

- ✅ Memory manager initialization
- ✅ Scheduler initialization (ProcessMeta Mutex::new)
- ✅ VFS initialization
- ✅ Network initialization (DHCP works!)
- ✅ Initramfs loading
- ✅ User address space creation
- ✅ User stack setup

## New Issue (Separate Bug)

Page fault during user process creation:
```
PAGE FAULT!
  Address: 0xffffffff812a20c8
  RIP: 0xffffffff80183192
  Error: 0x2 (Write to non-present page)
```

This is a kernel BSS/data access issue, unrelated to the buddy allocator.

## Debug Code to Remove

The following debug code can be cleaned up now:
- `kernel/src/scheduler.rs`: debug_check_both(), debug_check_0xfec5()
- `kernel/src/init.rs`: Post-* debug prints
- `crates/mm/mm-core/src/buddy.rs`: [ADD], [PRE], [BUDDY], [CORRUPT] prints
- `crates/proc/proc/src/meta.rs`: debug_check() function
