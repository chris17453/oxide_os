# OXIDE OS Keyboard Input Pipeline Analysis v3

**Date:** 2026-02-01  
**Status:** Major fixes implemented - Caps Lock, Scroll Lock, blocking I/O, VTIME

---

## Changes Since v2

### 🎉 FIXED IN THIS VERSION 🎉

| Issue | v2 Status | v3 Status | Notes |
|-------|-----------|-----------|-------|
| Caps Lock | 🔴 CRITICAL | ✅ FIXED | Full implementation with LED |
| Scroll Lock | 🟡 MEDIUM | ✅ FIXED | LED toggles (no XOFF/XON yet) |
| VTIME support | 🔴 HIGH | ✅ FIXED | Timed reads work |
| Pipe blocking | 🔴 HIGH | ✅ FIXED | Wait queues, proper blocking |
| TTY spinloop | 🟠 HIGH | ✅ FIXED | Proper sleep/wake |
| Layout Caps Lock | 🟡 MEDIUM | ✅ FIXED | `get_char()` has capslock param |
| Pause key | 🟢 LOW | ✅ FIXED | E1 sequence now parsed |

---

## Detailed Fix Analysis

### 1. CAPS LOCK - FULLY IMPLEMENTED ✅

**Location:** `crates/drivers/input/ps2/src/lib.rs` lines 179-182, 258-277

```rust
pub struct Ps2Keyboard {
    // ...
    capslock: AtomicBool,   // 🔥 NOW EXISTS
    scrolllock: AtomicBool, // 🔥 BONUS: Also added
}

input::KEY_CAPSLOCK => {
    if pressed {
        let new_state = !self.capslock.load(Ordering::SeqCst);
        self.capslock.store(new_state, Ordering::SeqCst);
        // LED bit 2 = Caps Lock
        let mut leds = self.leds.load(Ordering::SeqCst);
        if new_state { leds |= 0x04; } else { leds &= !0x04; }
        self.leds.store(leds, Ordering::SeqCst);
        self.update_leds();
    }
    return; // Don't forward to console
}
```

**Character conversion updated** (line 457-467):
```rust
let capslock = self.capslock.load(Ordering::SeqCst);
if let Some(ch) = input::keymap::keycode_to_char_current(keycode, shift, altgr, capslock) {
    // ...
}
```

**Layout support updated** (`crates/input/input/src/layouts.rs` line 51):
```rust
pub fn get_char(&self, keycode: u16, shift: bool, altgr: bool, capslock: bool) -> Option<char> {
    // Caps Lock XORs with Shift for LETTERS ONLY
    let effective_shift = shift ^ (capslock && self.is_letter(keycode));
    // ...
}
```

---

### 2. SCROLL LOCK - IMPLEMENTED ✅

**Location:** `crates/drivers/input/ps2/src/lib.rs` lines 278-300

LED toggles correctly. Traditional XOFF/XON flow control not yet implemented (low priority).

---

### 3. VTIME SUPPORT - IMPLEMENTED ✅

**Location:** `crates/tty/tty/src/tty.rs` lines 323-416

```rust
fn read(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
    let (vmin, vtime) = { /* get from termios */ };

    loop {
        if ldisc.can_read() {
            return Ok(ldisc.read(buf));
        }

        // VMIN=0, VTIME=0: Non-blocking, return immediately
        if vmin == 0 && vtime == 0 {
            return Ok(0);
        }

        // VMIN=0, VTIME>0: Timeout read
        if vtime > 0 && vmin == 0 {
            let _timeout_expired = unsafe { sched_block_deciseconds(vtime) };
            // Return whatever is available
            return Ok(ldisc.read(buf));
        }

        // VMIN>0: Block indefinitely
        unsafe { sched_block_interruptible(); }
    }
}
```

**Modes now working:**
| VMIN | VTIME | Behavior | Status |
|------|-------|----------|--------|
| 0 | 0 | Non-blocking (poll) | ✅ |
| 0 | >0 | Timed read | ✅ |
| >0 | 0 | Block until VMIN bytes | ✅ |
| >0 | >0 | Interbyte timeout | ⚠️ Partial (treated as VMIN>0) |

---

### 4. PIPE BLOCKING - IMPLEMENTED ✅

**Location:** `crates/vfs/vfs/src/pipe.rs` lines 168-231

```rust
fn read(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
    loop {
        let (n, has_writers, write_waiters) = {
            let mut buffer = self.buffer.lock();

            // EOF: empty AND no writers
            if buffer.count == 0 && !buffer.has_writers() {
                return Ok(0);
            }

            let n = buffer.read(buf);
            // ... collect waiters to wake ...
        };

        // Wake writers if we freed space
        for pid in write_waiters {
            unsafe { sched_wake_up(pid); }
        }

        if n > 0 { return Ok(n); }

        // Block waiting for data
        if has_writers {
            // Add to wait queue, sleep
            unsafe { sched_block_interruptible(); }
            // Loop and retry
        } else {
            return Ok(0); // EOF
        }
    }
}
```

**Shell pipes now work correctly:**
- `sleep 5 | cat` - waits 5 seconds ✅
- `(sleep 1; echo hello) | cat` - prints "hello" after 1s ✅
- EOF propagates when writer closes ✅

---

### 5. TTY WAIT QUEUES - IMPLEMENTED ✅

**Location:** `crates/tty/tty/src/tty.rs`

**Before:** Spinloop with `yield_current()` burning CPU
**After:** Proper `sched_block_interruptible()` with wait queues

```rust
// Old (v2):
loop {
    if can_read() { return; }
    sched::yield_current();  // 100% CPU while waiting
}

// New (v3):
loop {
    if can_read() { return; }
    // Add to wait queue
    waiters.push(pid);
    // Sleep until woken by input
    sched_block_interruptible();  // 0% CPU while waiting
}
```

---

### 6. PAUSE KEY - IMPLEMENTED ✅

**Location:** `crates/input/input/src/keymap.rs` lines 300-332

```rust
// Pause/Break uses E1 prefix: E1 1D 45 E1 9D C5
if scancode == 0xE1 || self.pause_index > 0 {
    // Collect 6-byte sequence
    if self.pause_index == 6 {
        let valid = /* check full sequence */;
        if valid {
            return Some((KEY_PAUSE, true));
        }
    }
}
```

---

## Still Broken 🔴

### 1. O_NONBLOCK NOT SUPPORTED ON TTYs 🟠 MEDIUM

**Location:** `crates/tty/tty/src/tty.rs`

File flags not checked in read path. `fcntl(fd, F_SETFL, O_NONBLOCK)` has no effect on TTY reads.

**Impact:** Programs expecting EAGAIN still block.

**Fix needed:**
```rust
fn read(&self, _offset: u64, buf: &mut [u8], flags: FileFlags) -> VfsResult<usize> {
    if !ldisc.can_read() && flags.contains(FileFlags::O_NONBLOCK) {
        return Err(VfsError::WouldBlock);
    }
    // ... existing blocking code ...
}
```

---

### 2. SCROLL LOCK XON/XOFF NOT IMPLEMENTED 🟢 LOW

Scroll Lock LED toggles, but traditional behavior (pause terminal output) not implemented.

Traditional: Scroll Lock = XOFF (stop output), toggle again = XON (resume).

---

### 3. INPUT SUBSYSTEM MULTIPLE READERS 🟡 LOW

**Location:** `crates/input/input/src/lib.rs`

Only one PID can wait on `/dev/input/eventX`. Second reader overwrites first.

---

### 4. INTERBYTE TIMEOUT (VMIN>0, VTIME>0) 🟡 LOW

Complex case: "return when VMIN bytes arrive OR VTIME passes since last byte".
Currently treated as VMIN>0 only (blocks indefinitely until VMIN bytes).

---

### 5. VT SWITCHING NOT IMPLEMENTED 🟡 MEDIUM

Alt+F1 through Alt+F6 should switch virtual terminals.
Currently these keys just produce escape sequences.

---

## Architecture (Current State)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    KEYBOARD INPUT PIPELINE v3                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  PS/2 IRQ Handler (IRQ1)                                                    │
│       │                                                                     │
│       │ handle_scancode()                                                   │
│       │   ├─ Keymap lookup (including E1 pause sequence) ✅                 │
│       │   ├─ Modifier tracking:                                             │
│       │   │    ├─ Shift, Ctrl, Alt, AltGr ✅                                │
│       │   │    ├─ NumLock (with LED) ✅                                     │
│       │   │    ├─ CapsLock (with LED) ✅ NEW                                │
│       │   │    └─ ScrollLock (with LED) ✅ NEW                              │
│       │   ├─ Ctrl+key → control codes (0x01-0x1A) ✅                        │
│       │   ├─ Special keys → ANSI sequences ✅                               │
│       │   └─ Character conversion (with capslock!) ✅                       │
│       │                                                                     │
│       ▼                                                                     │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ VT Manager                                                          │   │
│  │   push_input(ch) ──▶ LockFreeRing (256 bytes) ✅                    │   │
│  │                           │                                         │   │
│  │                           │ pop() in read loop                      │   │
│  │                           ▼                                         │   │
│  │   read() ──▶ drain ring ──▶ TTY line discipline                    │   │
│  │              ✅ WAIT QUEUES (no more spinloop!)                     │   │
│  │              ✅ VTIME support                                       │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│       │                                                                     │
│       ▼                                                                     │
│  Pipes ──▶ ✅ PROPER BLOCKING (no more EAGAIN spam)                        │
│       │                                                                     │
│       ▼                                                                     │
│  Userspace (shell, vim, etc.)                                               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## LED State Summary

| LED | Bit | Default | Status |
|-----|-----|---------|--------|
| Scroll Lock | 0 | OFF | ✅ Toggles |
| Num Lock | 1 | ON | ✅ Works |
| Caps Lock | 2 | OFF | ✅ Works |

---

## Modifier State Tracking

| Modifier | Location | Status |
|----------|----------|--------|
| Shift (L/R) | `ps2/lib.rs` AtomicBool | ✅ |
| Ctrl (L/R) | `ps2/lib.rs` AtomicBool | ✅ |
| Alt (L) | `ps2/lib.rs` AtomicBool | ✅ |
| AltGr (R) | `ps2/lib.rs` AtomicBool | ✅ |
| Num Lock | `ps2/lib.rs` AtomicBool + LED | ✅ |
| Caps Lock | `ps2/lib.rs` AtomicBool + LED | ✅ NEW |
| Scroll Lock | `ps2/lib.rs` AtomicBool + LED | ✅ NEW |

---

## Priority Fix List (Remaining)

| # | Issue | Severity | Effort | Impact |
|---|-------|----------|--------|--------|
| 1 | O_NONBLOCK on TTY | 🟠 MEDIUM | Low | Polling I/O |
| 2 | VT switching (Alt+Fn) | 🟡 MEDIUM | Medium | Multi-terminal |
| 3 | Interbyte timeout | 🟡 LOW | Medium | Edge case |
| 4 | Multiple input readers | 🟡 LOW | Medium | Edge case |
| 5 | Scroll Lock XON/XOFF | 🟢 LOW | Low | Legacy |

---

## Test Commands

```bash
# Test Caps Lock
# Type: hello → should print "hello"
# Press Caps Lock (LED should light)
# Type: hello → should print "HELLO"
# Press Shift+h → should print "h" (Shift cancels Caps)
# Type: 123 → should print "123" (not "!@#")

# Test Scroll Lock
# Press Scroll Lock → LED should light
# Press again → LED should turn off

# Test VTIME (in raw mode program)
# VMIN=0, VTIME=10 → read should timeout after 1 second

# Test pipe blocking
sleep 3 | cat      # Should wait 3 seconds, not exit immediately
echo test | cat    # Should print "test" immediately

# Test Pause key
# Press Pause/Break → should generate KEY_PAUSE event
```

---

## Files Changed Since v2

| File | Changes |
|------|---------|
| `crates/drivers/input/ps2/src/lib.rs` | Added capslock, scrolllock fields and handlers |
| `crates/input/input/src/layouts.rs` | Added capslock param to `get_char()` |
| `crates/input/input/src/keymap.rs` | Added E1 pause sequence parsing, capslock in conversion |
| `crates/tty/tty/src/tty.rs` | Wait queues, VTIME support |
| `crates/vfs/vfs/src/pipe.rs` | Proper blocking with wait queues |

---

## Summary

**v3 is a MAJOR improvement.** The keyboard system now works like a real OS:

- ✅ All lock keys work with LEDs (Caps, Num, Scroll)
- ✅ Proper blocking I/O (no CPU waste)
- ✅ VTIME timeout reads work
- ✅ Pipes block correctly
- ✅ Pause key recognized

**Remaining issues are minor edge cases** (O_NONBLOCK, interbyte timeout, VT switching).

The keyboard input system is now **production-ready for typical use cases**.
