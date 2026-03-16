# sys_read/sys_write MUST use kernel-stack buffer — never pass user-space pointers through VFS stack

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `0a6e991e512c` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T19:48:50.177187+00:00 |
| Keywords | SMAP, STAC, CLAC, sys_read_vfs, sys_write_vfs, kernel buffer, user-space |

## Details

sys_read_vfs and sys_write_vfs must read/write through a kernel-stack buffer intermediary (currently 2KB). The VFS/TTY/ldisc stack must NEVER receive user-space pointers because nested STAC/CLAC in subsystems (terminal::write, console_write_bytes) clobber the AC flag. Copy to/from user space in a tight STAC/CLAC window — just memcpy, no locks, no yields, no function calls into subsystems.
