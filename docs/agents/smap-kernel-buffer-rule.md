# SMAP Kernel Buffer Rule — sys_read/sys_write Must Use Kernel Intermediary

## Rule
`sys_read_vfs` and `sys_write_vfs` MUST read/write through a kernel-stack
buffer. User-space pointers MUST NEVER be passed into the VFS/TTY/ldisc
subsystem stack.

## Severity: CRITICAL

## Root Cause

`terminal::write()` does `STAC`/`CLAC` internally because its `data`
parameter might point to user-space memory (from `sys_write`). But when
called from the **echo path** inside `sys_read`:

```
sys_read_vfs(STAC → AC=1)
  → VtManager::read
    → tty.input(&[ch])
      → ldisc.input_char(echo_callback)
        → VtTtyDriver::write()
          → console_write()
            → terminal::write()
              → STAC (redundant)
              → terminal.write(data)
              → CLAC ← THIS NUKES AC TO 0
    → tty.try_read(buf)
      → ldisc.read_canonical(buf)
        → buf[0] = c  ← SMAP FAULT (AC=0, writing user-space)
```

The `CLAC` at `terminal/lib.rs:2004` clears the AC flag that
`sys_read_vfs` set. Any subsequent write to the user-space buffer
triggers a SMAP page fault (#PF) that **silently kills the process**.

## Symptoms

- Login accepts username input (characters echo correctly)
- After pressing Enter, "Password:" prompt appears but typing produces
  no response — the login process is dead
- `[VT-PUSH]` traces appear for password keystrokes (ring buffer works)
  but `[LDISC]` traces never fire (nobody drains the ring)
- No visible crash — the page fault kills the process without trace

## The Fix

```rust
// sys_read_vfs — kernel buffer intermediary
const KBUF_SIZE: usize = 2048;
let mut kbuf = [0u8; KBUF_SIZE];
let chunk = count.min(KBUF_SIZE);

let result = match file.read(&mut kbuf[..chunk]) {
    Ok(n) => {
        // Tight STAC/CLAC — just memcpy, no locks, no yields
        unsafe { core::arch::asm!("stac", options(nomem, nostack)); }
        let user_buf = unsafe {
            core::slice::from_raw_parts_mut(buf as *mut u8, n)
        };
        user_buf.copy_from_slice(&kbuf[..n]);
        unsafe { core::arch::asm!("clac", options(nomem, nostack)); }
        n as i64
    }
    Err(e) => vfs_error_to_errno(e),
};
```

Same pattern for `sys_write_vfs` — copy FROM user space first, then
write from kernel buffer.

## General Rule

**Never hold STAC (AC=1) across function calls into subsystems.** Any
function in the call chain might do its own STAC/CLAC, and the final
CLAC will nuke your AC flag. Use `copy_from_user()` / `copy_to_user()`
or a kernel buffer intermediary — keep the STAC/CLAC window as narrow
as a raw memcpy.

— ColdCipher: If you pass a user-space pointer four function calls
deep and wonder why it faults — you earned that triple-fault, friend.
