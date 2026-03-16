# Complete VT0/VT1 login prompt routing analysis - found the bug!

| Field | Value |
|-------|-------|
| ID | `38e62bb659ac` |
| Type | observation |
| Project | - |
| Branch | - |
| Created | 2026-03-08T02:30:18.394127+00:00 |
| Accessed | 0 times |
| Keywords | vt, getty, login, terminal, routing, activevt, backingbuffer, compositor, tiocsctty, prompt |
| Files | `kernel/src/init.rs`, `kernel/tty/vt/src/lib.rs`, `kernel/src/console.rs`, `userspace/system/getty/src/main.rs`, `userspace/system/login/src/main.rs` |
| Session | [7eaa90cb3c9d](../sessions/debug-why-login-prompt-never-shows-on-vt-after-boot-investigate-vt-initializatio.md) |

## Content

## Complete Boot-to-Login Analysis for OXIDE OS

### The Architecture

**VT Initialization (kernel/src/init.rs lines 1309-1356):**
1. `vt::init()` creates VtManager with 6 virtual terminals (line 1310)
2. `/dev/tty0` registered as Tty0Device (lines 1331-1332) - resolves ACTIVE_VT on every read/write
3. `/dev/tty1` through `/dev/tty6` registered as VtDevice for each VT (lines 1337-1346)
4. ACTIVE_VT defaults to 0 (kernel/tty/vt/src/lib.rs line 508): `static ACTIVE_VT: spin::RwLock<usize> = spin::RwLock::new(0);`

**Terminal Output Routing (kernel/src/console.rs lines 45-56):**
When getty or login calls `write(1, data)` (stdout write):
1. Writes to TTY via VFS
2. TTY line discipline processes output (adds CR, etc.)
3. TTY calls driver.write() 
4. For VT devices: CallbackDriver wraps CONSOLE_WRITE_CALLBACK
5. CONSOLE_WRITE_CALLBACK is set to `console::console_write` (kernel/src/init.rs line 1434)
6. `console::console_write()` calls `terminal::write(data)` if terminal is initialized
7. Terminal writes to compositor's VT0 backing buffer
8. Compositor blits VT0 buffer to hardware framebuffer at ~30Hz (on timer tick)

### The Critical Issue: Getty Opens /dev/ttyN NOT /dev/tty0

**User says:** "VT0 is supposed to point to ACTIVE terminal, console supposed to point to VT1 by default"

**What Actually Happens:**
1. Init spawns getty on `/dev/tty1` (userspace/system/init/src/main.rs lines 147-196):
   - Opens `/dev/tty1` as fd 0 (stdin)
   - dup2 to fd 1 (stdout)
   - dup2 to fd 2 (stderr)
   - Execs getty

2. Getty opens fd 0 which is already set to `/dev/tty1` (lines 99-107)
3. Getty prints banner to stdout → writes to `/dev/tty1`
4. Login runs as getty's child, inherits same fd 1 = `/dev/tty1` (userspace/system/login/src/main.rs line 307)
5. Login prints "OXIDE OS login: " to fd 1 = `/dev/tty1`

**This is CORRECT per Linux standard!**

### Why No Prompt Visible On Boot

The issue is NOT the routing — getty/login writes correctly to their assigned TTY.

**The Root Cause:** ACTIVE_VT starts at 0 but nothing sets it to 1 when getty starts!

**What Should Happen:**
- At boot, user should see VT0 (which initially redirects to VT1 via ACTIVE_VT)
- When getty starts on VT1, ACTIVE_VT should = 1
- Writes to `/dev/tty0` (system messages) should → VT1 (the active one)
- Writes to `/dev/tty1` (getty/login) should → VT1 directly
- Both converge on the same terminal

**What Actually Happens:**
- ACTIVE_VT stays 0 forever
- Writes to `/dev/tty0` → VT0 (which has no getty, no input ring)
- Writes to `/dev/tty1` → VT1 (getty is here)
- VT0 might not even be initialized/rendered to screen!

### VtDevice.write() Implementation (kernel/tty/vt/src/lib.rs lines 670-671)

```rust
fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
    self.manager.write(self.vt_num, buf)  // Writes to fixed VT, not ACTIVE_VT
}
```

VtDevice is bound to a fixed VT number (set at device creation, line 1338):
```rust
let vt_device = vt::VtDevice::new(i, vt_manager.clone(), 1000 + i as u64);
```

So `/dev/tty1` always routes to VT 1, `/dev/tty2` to VT 2, etc. — CORRECT!

### Tty0Device.write() Implementation (kernel/tty/vt/src/lib.rs lines 763-766)

```rust
fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
    let active = *ACTIVE_VT.read();
    self.manager.write(active, buf)  // Resolves ACTIVE_VT at write time
}
```

This is CORRECT — `/dev/tty0` routes to whatever ACTIVE_VT points to.

### The Missing Piece: Who Sets ACTIVE_VT?

Searching the codebase:
- ACTIVE_VT initialized to 0 (kernel/tty/vt/src/lib.rs line 508)
- ACTIVE_VT.try_write() called in switch_to() (kernel/tty/vt/src/lib.rs line 177) — only when user presses Alt+F1, Alt+F2, etc.
- ACTIVE_VT read in Tty0Device.write() and Tty0Device.read() and Tty0Device.ioctl()

**NOWHERE is ACTIVE_VT set to 1 when getty starts!**

### What Should Happen (Linux Behavior)

When getty(1) spawns on a tty, it should:
1. Open its tty device (`/dev/tty1`)
2. Call `ioctl(fd, TIOCSCTTY, 0)` to set it as the controlling terminal
3. This should also set the console to this TTY

In OXIDE, getty doesn't call TIOCSCTTY — it just opens the tty.

### The Missing Feature: ioctl(TIOCSCTTY) in getty

getty (userspace/system/getty/src/main.rs) does NOT call `ioctl(fd, TIOCSCTTY, 0)` to claim the TTY as the controlling terminal.

This means:
1. getty opens `/dev/tty1`
2. getty doesn't call TIOCSCTTY
3. Kernel never knows this is the foreground session's tty
4. ACTIVE_VT stays 0
5. Console output tries to go to VT0, not VT1
6. VT0 might be uninitialized/not composited yet

### Compositor Status

Looking at init.rs lines 720-726:
- Compositor initialized with hardware framebuffer
- Terminal initialized with VT0's backing buffer (line 725)
- Only VT0 gets backing buffer at init (line 718 comment)

This means:
- VT0 gets a backing buffer and is composited
- But terminal renders to VT0's buffer
- getty writes to VT1's TTY
- VT1 doesn't have a backing buffer (lazy allocation)
- Console writes go to... where exactly?

### The Real Problem

1. Getty writes to `/dev/tty1` → VT1's TTY → line discipline → calls CONSOLE_WRITE_CALLBACK → terminal::write()
2. `terminal::write()` writes to the terminal emulator
3. Terminal emulator was initialized with VT0's backing buffer (line 725)
4. So getty's output goes to VT0's buffer, not VT1's buffer!
5. VT0 is composited and visible
6. But ACTIVE_VT is still 0, so getty is invisible in the VT0 context

Wait, let me re-read the console routing...

### Tracing console_write() Path (kernel/src/console.rs lines 45-56)

```rust
pub fn console_write(data: &[u8]) {
    if terminal::is_initialized() {
        terminal::write(data);
        compositor::mark_dirty(compositor::focused_vt());
    } else if fb::is_initialized() {
        for &byte in data {
            fb::putchar(byte as char);
        }
    }
}
```

This is called DIRECTLY by the TTY layer when VT device write() calls it.
- It writes to `terminal::write()` — which is the global terminal emulator
- That emulator is initialized with VT0's backing buffer
- So all TTY output (from any VT) → terminal::write() → VT0's buffer

This is WRONG! Each VT should have its own terminal emulator or backing buffer!

### The Actual Bug

**Per-VT Output Routing Is Missing!**

The architecture should be:
1. VT0 has terminal emulator A rendering to backing buffer 0
2. VT1 has terminal emulator B rendering to backing buffer 1
3. When getty writes to `/dev/tty1`, it goes to emulator B, buffer 1
4. Compositor composites all active VT buffers to hardware fb

**What OXIDE has:**
1. Single global `terminal::TERMINAL` emulator instance
2. Single backing buffer (VT0's)
3. All TTY writes → CONSOLE_WRITE_CALLBACK → terminal::write() → VT0's buffer
4. VT1's getty output goes to VT0's buffer
5. But ACTIVE_VT=0 so user sees VT0
6. VT0 now has getty's prompt (correct!) but VT0 is not supposed to run getty (wrong!)

### Why Prompt Might Not Show

Possible reasons:
1. Terminal not fully initialized when getty starts printing (race condition)
2. ACTIVE_VT=0 but compositor not compositing VT0 because it's not marked "active"
3. Line buffering issue — getty's banner printed but not flushed
4. getty calls fflush_stdout() on line 308 of login.rs but getty doesn't call it

Actually checking getty (lines 109-111):
```rust
clear_screen();
print_banner();
prints("\n");
```

It calls `prints()` which goes to libc which uses buffered I/O. The prompt might be buffered!

Line 307 in login.rs:
```rust
prints("\nOXIDE OS login: ");
fflush_stdout();  // — SoftGlyph: Flush prompt before blocking on read
```

Login DOES call fflush_stdout()! But getty doesn't flush after printing banner.

### Conclusion

**Multiple Issues:**

1. **Per-VT Terminal Emulation Missing:** All VT output routes to single global terminal emulator + VT0's backing buffer. Should be per-VT emulators or separate backing buffers per VT.

2. **No TIOCSCTTY in getty:** getty doesn't call ioctl(TIOCSCTTY) so kernel doesn't know which VT is "active"

3. **ACTIVE_VT never updated:** Only Alt+Fn keypresses update ACTIVE_VT. Should also be set when a TTY becomes controlling terminal or when getty starts.

4. **getty Buffer Flush:** getty prints banner but may not flush stdout before returning from setup_terminal()

5. **Race Condition:** getty starts printing before terminal/compositor fully initialized, or before its backing buffer created (if using lazy allocation for VT1)


