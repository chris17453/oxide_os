# Error: clear_child_tid, parent_tid_addr, child_tid_addr written as raw pointers without

| Field | Value |
|-------|-------|
| ID | `af1e60ec555f` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T16:19:41.739604+00:00 |
| Keywords | security, arbitrary-write, clone, clear_child_tid, SMAP, userspace-validation |
| Session | [84e1202df436](../sessions/implement-memory-system-hardening-plan-buddy-allocator-corruption-fixes-fork-exe.md) |

## Error

clear_child_tid, parent_tid_addr, child_tid_addr written as raw pointers without userspace address validation - arbitrary kernel memory write from userspace

## Cause

Thread exit and clone wrote to user-provided TID pointers using raw pointer dereference without checking that the address was below USER_SPACE_END (0x0000_8000_0000_0000). A malicious process could pass a kernel address and corrupt kernel memory.

## Fix

Validate address < USER_SPACE_END before writing, use STAC/CLAC for SMAP compliance, use write_volatile to prevent compiler reordering outside the SMAP window.
