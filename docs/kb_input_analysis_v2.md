# OXIDE OS Keyboard Input Pipeline Analysis v2

**Date:** 2026-02-01  
**Status:** Post-refactor analysis after lock-free ring buffer implementation

---

## What Was Fixed ✅

### 1. Lock-Free Ring Buffer - FIXED ✅
**Previous:** `try_lock()` silently dropped keystrokes when locks were contended  
**Now:** SPSC lock-free ring buffer (`crates/tty/vt/src/lockfree_ring.rs`)
- 256-byte capacity
- IRQ handler pushes atomically, no locks
- VT read pops atomically, no contention
- Only drops if buffer genuinely full (256 chars ahead - user typing impossibly fast)

### 2. Duplicate Processing Path - FIXED ✅
**Previous:** Both `ps2/lib.rs` AND `console.rs::process_scancode()` processed the same scancodes  
**Now:** `console.rs` processing is marked as DEPRECATED and disabled
- PS/2 IRQ handler is the ONLY authoritative source
- Modifier state tracked in ONE place (PS/2 driver)
- Code left as "archaeological evidence" with warnings

### 3. Signal Delivery - PARTIALLY FIXED ✅
The lock-free ring buffer prevents the VT lock contention that caused double processing.
Signal check still happens in push_input() for immediate delivery.

---

## Still Broken 🔴

### 1. CAPS LOCK NOT IMPLEMENTED 🔴 CRITICAL

**Location:** `crates/drivers/input/ps2/src/lib.rs`

The PS/2 driver has:
- `numlock: AtomicBool` - ✅ WORKS
- NO `capslock: AtomicBool` - ❌ MISSING

```rust
// Current state in Ps2Keyboard struct (line 162-179):
pub struct Ps2Keyboard {
    shift: AtomicBool,
    ctrl: AtomicBool,
    alt: AtomicBool,
    altgr: AtomicBool,
    numlock: AtomicBool,
    // ⚠️ NO CAPSLOCK!
}
```

NumLock is handled (line 236-251), but Caps Lock scancode (0x3A → KEY_CAPSLOCK) 
is NOT handled. The key is recognized by `keymap.rs` but the driver ignores it.

**Impact:** Users cannot use Caps Lock. Affects all typing.

**Fix Required:**
```rust
// Add to struct:
capslock: AtomicBool,

// Add handling in handle_scancode():
input::KEY_CAPSLOCK => {
    if pressed {
        let new_state = !self.capslock.load(Ordering::SeqCst);
        self.capslock.store(new_state, Ordering::SeqCst);
        let mut leds = self.leds.load(Ordering::SeqCst);
        if new_state { leds |= 0x04; } else { leds &= !0x04; }
        self.leds.store(leds, Ordering::SeqCst);
        self.update_leds();
    }
    return; // Don't forward key
}

// Use in character conversion:
let effective_shift = shift ^ (capslock && is_letter(keycode));
if let Some(ch) = keycode_to_char_current(keycode, effective_shift, altgr) { ... }
```

---

### 2. SCROLL LOCK NOT IMPLEMENTED 🟡 MEDIUM

Same issue as Caps Lock - keycode mapped but not handled. LED never updated.
Traditional behavior: XOFF/XON flow control.

---

### 3. VTIME NOT IMPLEMENTED 🔴 HIGH

**Location:** `crates/tty/tty/src/ldisc.rs` line 551-552

```rust
fn read_raw(&mut self, buf: &mut [u8]) -> usize {
    let vmin = self.termios.c_cc[VMIN] as usize;
    let _vtime = self.termios.c_cc[VTIME];  // ⚠️ COMPLETELY IGNORED!
    // ...
}
```

VTIME controls read timeout in deciseconds. Required modes NOT working:
- `VMIN=0, VTIME>0` - Timed read (return after timeout even if no data)
- `VMIN>0, VTIME>0` - Interbyte timeout (return if gap between bytes)
- `VMIN=0, VTIME=0` - Polling read (return immediately)

**Impact:** Programs using `select()` with timeout on stdin break. Non-blocking 
input patterns fail.

**Fix Required:** Integrate with scheduler's timer/sleep functionality.

---

### 4. O_NONBLOCK NOT SUPPORTED ON TTYs 🔴 HIGH

**Location:** `crates/tty/tty/src/tty.rs` line 293-318

```rust
fn read(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
    loop {
        // ... check ldisc.can_read() ...
        sched::yield_current();  // ⚠️ INFINITE LOOP!
    }
}
```

There's no check for O_NONBLOCK file flags. VT read (`crates/tty/vt/src/lib.rs`) 
also has no non-blocking path.

**Impact:** 
- `fcntl(fd, F_SETFL, O_NONBLOCK)` has no effect
- Programs expecting EAGAIN get stuck forever
- Polling-based I/O multiplexing breaks

**Fix Required:**
```rust
fn read(&self, _offset: u64, buf: &mut [u8], flags: FileFlags) -> VfsResult<usize> {
    if !ldisc.can_read() {
        if flags.contains(FileFlags::O_NONBLOCK) {
            return Err(VfsError::WouldBlock);  // EAGAIN
        }
        // ... existing blocking loop ...
    }
}
```

---

### 5. SPINLOOP BLOCKING (CPU Waste) 🟠 HIGH

**Location:** `crates/tty/tty/src/tty.rs` line 293-318, `crates/tty/vt/src/lib.rs` line 288-350

Both TTY and VT read paths use busy-wait spinloops:
```rust
loop {
    if can_read() { return data; }
    sched::yield_current();  // Yields, but wakes up immediately to poll again
}
```

This burns CPU cycles continuously while waiting for input. Every process 
waiting for keyboard is constantly waking, checking, and yielding.

**Impact:** High CPU usage, battery drain, performance issues with many processes.

**Fix Required:** Implement proper wait queues:
1. Process calls read() → no data → add to wait queue → SLEEP (not yield)
2. Input arrives → wake ONE process from wait queue
3. Process wakes → read data → return

---

### 6. PIPE READ DOESN'T ACTUALLY BLOCK 🔴 HIGH

**Location:** `crates/vfs/vfs/src/pipe.rs` line 123-140

```rust
fn read(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
    // ...
    let n = buffer.read(buf);
    if n > 0 {
        Ok(n)
    } else {
        Err(VfsError::WouldBlock)  // ⚠️ RETURNS EAGAIN, DOESN'T BLOCK!
    }
}
```

Comment says "In a real implementation, we'd block" but it just returns EAGAIN.

**Impact:**
- `cat file | grep pattern` - grep gets EAGAIN, exits
- Any pipeline with slow producer breaks
- Shell pipes are fundamentally broken

**Fix Required:** Implement blocking with wait queues (same as TTY fix).

---

### 7. KEYBOARD LAYOUT DOESN'T SUPPORT CAPS LOCK 🟡 MEDIUM

**Location:** `crates/input/input/src/layouts.rs` line 27-41

```rust
pub fn get_char(&self, keycode: u16, shift: bool, altgr: bool) -> Option<char> {
    // ⚠️ NO CAPSLOCK PARAMETER!
    let ch = match (shift, altgr) {
        (false, false) => self.normal[idx],
        (true, false) => self.shift[idx],
        // ...
    };
}
```

Even when Caps Lock tracking is implemented, the layout system doesn't use it.
Caps Lock should uppercase letters but NOT affect numbers/symbols.

**Fix Required:**
```rust
pub fn get_char(&self, keycode: u16, shift: bool, altgr: bool, capslock: bool) -> Option<char> {
    // XOR with capslock for letters only
    let effective_shift = shift ^ (capslock && self.is_letter(keycode));
    let ch = match (effective_shift, altgr) { ... };
}
```

---

### 8. PAUSE KEY NOT HANDLED 🟢 LOW

**Location:** `crates/input/input/src/keymap.rs` line 292-294

```rust
if scancode == 0xE1 {
    return None;  // ⚠️ IGNORED
}
```

Pause/Break uses E1 prefix with multi-byte sequence. Completely ignored.

---

### 9. MEDIA/ACPI KEYS NOT MAPPED 🟢 LOW

Many extended E0 scancodes in `SCANCODE_SET1_EXT` are `KEY_RESERVED`:
- Volume Up/Down/Mute
- Play/Pause/Stop
- Browser keys
- etc.

---

### 10. INPUT SUBSYSTEM ONLY TRACKS ONE BLOCKED READER 🟡 MEDIUM

**Location:** `crates/input/input/src/lib.rs` line 66-71

```rust
pub fn set_blocked_reader(device_id: usize, pid: u32) {
    readers[device_id] = pid;  // ⚠️ OVERWRITES PREVIOUS!
}
```

Only ONE PID can wait on `/dev/input/eventX`. Second reader never wakes.

---

## Architecture Status

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    CURRENT ARCHITECTURE (Post-Fix)                      │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  PS/2 IRQ Handler ────────────────────────────────────────────────────┐ │
│       │                                                               │ │
│       │ handle_scancode()                                             │ │
│       │   - Keymap lookup                                             │ │
│       │   - Modifier tracking (Shift, Ctrl, Alt, AltGr, NumLock)      │ │
│       │   - ❌ NO CAPS LOCK                                           │ │
│       │   - Character conversion                                      │ │
│       │   - push_to_console()                                         │ │
│       │                                                               │ │
│       ▼                                                               │ │
│  ┌─────────────────────────────────────────────────────────────────┐  │ │
│  │ VT Manager                                                      │  │ │
│  │   push_input(ch) ──▶ LockFreeRing (256 bytes) ✅ FIXED         │  │ │
│  │                           │                                     │  │ │
│  │                           │ pop() in read loop                  │  │ │
│  │                           ▼                                     │  │ │
│  │   read() ──▶ drain ring ──▶ TTY line discipline                │  │ │
│  │              ⚠️ SPINLOOP BLOCKS (no wait queue)                 │  │ │
│  └─────────────────────────────────────────────────────────────────┘  │ │
│       │                                                               │ │
│       ▼                                                               │ │
│  Userspace (shell, vim, etc.)                                         │ │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Priority Fix List

| # | Issue | Severity | Effort | Impact |
|---|-------|----------|--------|--------|
| 1 | Caps Lock | 🔴 CRITICAL | Low | Every user notices |
| 2 | Pipe blocking | 🔴 HIGH | Medium | Shell pipes broken |
| 3 | O_NONBLOCK on TTY | 🔴 HIGH | Medium | Polling I/O broken |
| 4 | VTIME support | 🔴 HIGH | Medium | Timed reads broken |
| 5 | Wait queues (replace spinloops) | 🟠 HIGH | High | CPU waste |
| 6 | Scroll Lock | 🟡 MEDIUM | Low | Minor |
| 7 | Layout Caps Lock support | 🟡 MEDIUM | Low | Depends on #1 |
| 8 | Multiple blocked readers | 🟡 MEDIUM | Medium | Edge case |
| 9 | Pause key | 🟢 LOW | Low | Rare use |
| 10 | Media keys | 🟢 LOW | Low | Nice to have |

---

## Recommended Next Steps

### Immediate (1 hour)
1. **Implement Caps Lock in PS/2 driver** - Add `capslock: AtomicBool`, handle KEY_CAPSLOCK, update LEDs, use in character conversion

### Short-term (1 day)
2. **Fix pipe blocking** - Implement wait queue for pipe readers
3. **Add O_NONBLOCK check** - Check file flags in VT/TTY read paths

### Medium-term (1 week)
4. **Implement wait queues** - Replace all spinloops with proper sleep/wake
5. **Implement VTIME** - Timer-based read timeout

---

## Files to Modify

| File | Changes Needed |
|------|----------------|
| `crates/drivers/input/ps2/src/lib.rs` | Add capslock tracking, LED, use in char conversion |
| `crates/input/input/src/layouts.rs` | Add capslock parameter to `get_char()` |
| `crates/input/input/src/keymap.rs` | Add capslock parameter to conversion functions |
| `crates/vfs/vfs/src/pipe.rs` | Implement blocking with wait queue |
| `crates/tty/tty/src/tty.rs` | Add O_NONBLOCK check, replace spinloop |
| `crates/tty/vt/src/lib.rs` | Add O_NONBLOCK check, replace spinloop |
| `crates/tty/tty/src/ldisc.rs` | Implement VTIME support |

---

## Test Cases Needed

1. **Caps Lock:**
   - Type lowercase letters → Caps Lock → type again → verify uppercase
   - Verify LED lights up
   - Verify numbers/symbols unchanged

2. **Pipe blocking:**
   - `sleep 5 | cat` - should wait 5 seconds, not exit immediately
   - `(sleep 1; echo hello) | cat` - should print "hello" after 1s

3. **O_NONBLOCK:**
   - Open TTY with O_NONBLOCK, read immediately → should return EAGAIN
   - `fcntl(STDIN, F_SETFL, O_NONBLOCK)` → read → EAGAIN

4. **VTIME:**
   - Set VMIN=0, VTIME=10 (1 second) → read → should timeout after 1s
