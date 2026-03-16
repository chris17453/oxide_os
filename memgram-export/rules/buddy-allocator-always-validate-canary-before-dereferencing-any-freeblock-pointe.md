# Buddy allocator: always validate canary before dereferencing any FreeBlock pointer

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `80f0e2b42078` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T16:01:38.009546+00:00 |
| Keywords | buddy, allocator, canary, FreeBlock, corruption, validation |
| Session | [84e1202df436](../sessions/implement-memory-system-hardening-plan-buddy-allocator-corruption-fixes-fork-exe.md) |

## Details

Every FreeBlock pointer (prev_block, next_block, target_block) must have its magic field checked against FREE_BLOCK_MAGIC before reading any other field. Without this check, a corrupted block causes wild pointer dereferences that cascade into GPFs. The three validation points: (1) pop_free_block validates head canary, (2) pop_free_block validates new-head canary before setting prev=0, (3) remove_from_free_list validates prev_block canary before reading prev_block.next.
