# Session: Design implementation plan: fix pipe test syscall bug, commit strategy for ~1300 line diff, plan flat array task storage (audit 2.5)

| Field | Value |
|-------|-------|
| ID | `478017f5a5d1` |
| Agent | claude-code |
| Model | claude-opus-4-6 |
| Project | oxide-os |
| Branch | - |
| Status | completed |
| Started | 2026-03-04T22:07:20.307403+00:00 |
| Ended | 2026-03-04T22:09:34.910263+00:00 |
| Compactions | 0 |

## Summary

Designed implementation plan for next OXIDE OS work session: (1) pipe test syscall bug fix (3 bugs found: syscall1(3) = FORK not CLOSE, syscall3(0) = EXIT not READ, missing SYS_CLOSE/SYS_READ/SYS_WRITE constants), (2) commit strategy for ~1300 line diff across 34 files, (3) flat array task storage audit item 2.5 analysis

## Session Summary

**Outcome:** Plan designed successfully. Found the bug is worse than initially described: not just close() calling fork (syscall 3), but also read() calling exit (syscall 0). 14 close calls and 3 read calls are wrong.

**Decisions:**

- Pipe test has THREE distinct syscall bugs not just one: close(3)→fork, read(0)→exit, missing constants
- Commit strategy: single commit for the existing diff, then separate commits for new work
- Flat array task storage: replace BTreeMap with [Option<Task>; MAX_TASKS_PER_CPU] per run queue, not a global array

**Next Session Hints:** Execute the 3-step plan: fix pipe test syscall numbers, commit existing diff, implement flat array task storage. The pipe test fix is critical because it also affects socket tests (syscall1(3, fd) for close at lines 1688, 1725).
