# Error: oxide-test used Linux syscall numbers (READ=0, WRITE=1, CLOSE=3) instead of OXID

| Field | Value |
|-------|-------|
| ID | `925922e1d3ce` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-04T22:50:35.371073+00:00 |
| Keywords | syscall, oxide-test, Linux, READ, WRITE, CLOSE, wrong-number |
| Session | [beaad52bec1e](../sessions/fix-syscall-number-bugs-in-oxide-test-commit-vma-kernelmutex-work-implement-flat.md) |

## Error

oxide-test used Linux syscall numbers (READ=0, WRITE=1, CLOSE=3) instead of OXIDE numbers (READ=2, WRITE=1, CLOSE=21) — caused test_pipe_basic to call EXIT instead of READ and FORK instead of CLOSE, crashing mid-test-suite

## Cause

Hardcoded Linux syscall numbers in raw syscall calls instead of using named SYS_* constants

## Fix

Added SYS_READ=2, SYS_WRITE=1, SYS_CLOSE=21 constants and replaced all 14 hardcoded occurrences
