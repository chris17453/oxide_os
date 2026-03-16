# Session: Debug why login prompt never shows on VT after boot - investigate VT initialization, console routing, and getty spawning

| Field | Value |
|-------|-------|
| ID | `7eaa90cb3c9d` |
| Agent | claude-code |
| Model | claude-haiku-4-5-20251001 |
| Project | oxide-os |
| Branch | - |
| Status | completed |
| Started | 2026-03-08T02:29:12.864830+00:00 |
| Ended | 2026-03-08T02:30:36.897002+00:00 |
| Compactions | 0 |

## Summary

Comprehensive investigation of login prompt not showing on VT after OXIDE OS boot. Traced entire VT/TTY/terminal/console initialization from kernel to userspace.

## Session Summary

**Outcome:** Found 5 architectural issues: (1) Single global terminal emulator shared by all VTs, (2) All TTY output routed to VT0 backing buffer, (3) ACTIVE_VT stays 0 forever, (4) getty missing ioctl(TIOCSCTTY), (5) getty missing stdout flush

**Decisions:**

- Traced data flow: getty /dev/tty1 → VtDevice → TTY ldisc → CONSOLE_WRITE_CALLBACK → terminal → VT0 buffer
- Confirmed /dev/tty1 correctly routes to VT1 via VtDevice.write(self.vt_num)
- Identified terminal emulator is single-instance with VT0 backing buffer only

**Unresolved:**

- Architectural mismatch: compositor needs per-VT emulators but has one shared instance
- ACTIVE_VT never set to 1 when getty starts
- Getty output goes to VT0 instead of VT1 due to CONSOLE_WRITE_CALLBACK design
