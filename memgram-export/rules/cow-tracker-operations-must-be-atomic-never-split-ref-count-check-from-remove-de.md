# COW tracker operations MUST be atomic — never split ref_count check from remove/decrement

🔴 critical | ❌ dont

| Field | Value |
|-------|-------|
| ID | `9287259bc658` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T16:35:41.897005+00:00 |
| Keywords | COW, TOCTOU, race-condition, ref_count, try_claim_exclusive, fork, memory-corruption |
| Session | [b27c9dc86d34](../sessions/continue-memory-hardening-fix-remaining-audit-issues-cow-toctou-race-exec-as-lea.md) |

## Details

The old pattern: cow.ref_count(phys) [read lock] → if count ≤ 1 { cow.remove(phys) } else { cow.decrement(phys) } [write lock] is a TOCTOU race. Between releasing the read lock and acquiring the write lock, a concurrent fork() can increment the count. Result: two processes both think they own the frame exclusively, both make it writable, both scribble on the same physical memory — silent corruption.

Fix: use try_claim_exclusive() which holds a SINGLE write lock for the entire check-and-act operation. Returns true if exclusive (count was ≤1, entry removed), false if shared (decremented). No window for concurrent increment.
