# Page fault handler locks MUST use try_lock — never blocking lock in exception context

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `51489c14e4e6` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T16:01:49.455561+00:00 |
| Keywords | page-fault, ISR, deadlock, try_lock, STACK_GROWTH_LOCK, exception |
| Session | [84e1202df436](../sessions/implement-memory-system-hardening-plan-buddy-allocator-corruption-fixes-fork-exe.md) |

## Details

handle_stack_growth uses STACK_GROWTH_LOCK which then calls mm().alloc_frame() (takes zone lock). If the page fault fires inside an ISR and the interrupted code holds the zone lock, .lock() deadlocks permanently. try_lock returns None immediately — the process gets SIGSEGV which is infinitely better than a frozen CPU. This is the same pattern used in ISR code throughout OXIDE.
