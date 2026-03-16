# Error: PML4 canary check false-positive on idle task (PID 0, cr3=0) — killed idle loop,

| Field | Value |
|-------|-------|
| ID | `d9d998435ee6` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-05T02:21:11.372941+00:00 |
| Keywords | PML4, canary, idle-task, cr3, false-positive, scheduler-freeze, PID-0 |

## Error

PML4 canary check false-positive on idle task (PID 0, cr3=0) — killed idle loop, froze system

## Cause

The PML4[256] corruption detector read physical address 0x0 + 256*8 for idle tasks with cr3=0, got 0, which didn't match the expected kernel entry. It then killed PID 0 on every timer tick, preventing any context switches.

## Fix

Added switch_info.new_cr3 != 0 guard to skip canary check for idle/kernel tasks. They run on the kernel PML4 directly and don't need per-task validation.
