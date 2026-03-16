# VmAreaList::insert() must silently accept zero-size VMAs (start >= end)

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `67748578f4eb` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-04T20:17:28.815314+00:00 |
| Keywords | VMA, heap, exec, zero-size, debug_assert, panic |
| Session | [0005e1095767](../sessions/complete-vma-implementation-boot-verification.md) |

## Details

At exec time, the heap VMA has start == end (e.g., 0x600000 == program_break) because the heap hasn't been extended yet. A debug_assert!(start < end) in VmArea::new() causes a kernel panic during every exec. The fix: VmAreaList::insert() returns Ok(()) for zero-size VMAs without inserting them. This is semantically correct — there's nothing to track for a zero-size region.
