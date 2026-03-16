# Fork/exec frame allocation MUST use RAII guards for cleanup on OOM

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `00e2b5e10ed8` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T16:01:44.469252+00:00 |
| Keywords | fork, exec, OOM, frame-leak, RAII, FrameGuard, allocate_pages |
| Session | [84e1202df436](../sessions/implement-memory-system-hardening-plan-buddy-allocator-corruption-fixes-fork-exe.md) |

## Details

When allocating physical frames in a loop (clone_address_space_cow, allocate_pages), a Vec<PhysAddr> that gets dropped does NOT free the physical frames — it only frees the Vec's heap allocation. Use either (1) an RAII FrameGuard struct with Drop that frees all collected frames, or (2) explicit cleanup loops before returning Err. In fork.rs, FrameGuard wraps all PT frame allocations and calls defuse() on success. In address_space.rs, the OOM error path explicitly frees all frames collected so far.
