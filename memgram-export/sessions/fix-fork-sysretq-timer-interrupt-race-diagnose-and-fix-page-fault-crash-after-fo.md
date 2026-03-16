# Session: Fix fork sysretq timer interrupt race — diagnose and fix page fault crash after fork+exec of servicemgr

| Field | Value |
|-------|-------|
| ID | `cb4b0c71822b` |
| Agent | claude-code |
| Model | claude-opus-4-6 |
| Project | oxide-os |
| Branch | - |
| Status | completed |
| Started | 2026-03-13T13:24:22.706837+00:00 |
| Ended | 2026-03-13T13:25:23.412437+00:00 |
| Compactions | 0 |

## Summary

Diagnosed and fixed kernel page fault crash after fork+exec of servicemgr. Root cause: timer interrupt race in fork's manual sysretq path — interrupts were enabled while RSP pointed to user memory, causing ISR to corrupt scheduler state. Fix: added CLI as first instruction in the fork inline asm block. Verified fix with build 619 — clean boot through all fork+exec calls.

## Session Summary

**Outcome:** Bug fixed and verified. Rule documented in docs/agents/ and memgram. CLAUDE.md retrieval index updated.

**Decisions:**

- Added cli before fork sysretq inline asm to prevent timer interrupt race
- Audited all other sysretq paths — normal syscall return already has cli, user_exit has no manual sysretq

**Files Modified:**

- kernel/src/process.rs
- docs/agents/fork-sysretq-cli-requirement.md
- CLAUDE.md
