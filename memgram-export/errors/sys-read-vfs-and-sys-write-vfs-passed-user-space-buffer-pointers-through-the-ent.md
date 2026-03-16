# Error: sys_read_vfs and sys_write_vfs passed user-space buffer pointers through the ent

| Field | Value |
|-------|-------|
| ID | `ff4c38010a77` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T19:48:42.193146+00:00 |
| Keywords | SMAP, STAC, CLAC, AC flag, sys_read_vfs, sys_write_vfs, terminal::write, kernel buffer, user-space access, page fault, login, ldisc |

## Error

sys_read_vfs and sys_write_vfs passed user-space buffer pointers through the entire VFS/TTY/ldisc stack with STAC (AC=1) active. The echo path in VtManager::read (tty.input → VtTtyDriver::write → console_write → terminal::write) does its own STAC/CLAC, and the CLAC at terminal/lib.rs:2004 cleared AC back to 0. When ldisc.read_canonical then wrote to buf[0] (user-space), SMAP fault killed the process silently. Login could type username (echo worked, chars stored in ldisc), but died on the first actual read return — password prompt never worked.

## Cause

terminal::write() does STAC/CLAC internally because data might be user-space (from sys_write). But when called from the echo path inside sys_read, the CLAC clobbers the AC flag that sys_read_vfs set. Any subsequent write to the user-space buffer triggers a SMAP page fault.

## Fix

Changed sys_read_vfs and sys_write_vfs to use a 2KB kernel-stack buffer as intermediary. VFS/TTY stack only touches kernel memory. User-space copy happens in a tight STAC/CLAC window (just memcpy, no locks/yields/subsystem calls). This eliminates the entire class of SMAP bugs from nested STAC/CLAC in the VFS call stack.
