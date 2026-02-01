# OXIDE OS Keyboard Input Pipeline Analysis

This document describes the complete data flow of keyboard input from hardware
interrupts to userspace applications.

---

# 🚨 CRITICAL ISSUES & FIXES NEEDED

## Executive Summary

The keyboard input system has **fundamental architectural problems** that prevent
it from working like a normal Linux keyboard. The issues range from missing
features (Caps Lock, Scroll Lock) to dangerous design flaws (duplicate processing
paths, spinloop blocking, dropped keystrokes).

**Severity Ratings:**
- 🔴 **CRITICAL** - Causes data loss, hangs, or security issues
- 🟠 **HIGH** - Major functionality broken
- 🟡 **MEDIUM** - Feature incomplete or degraded UX
- 🟢 **LOW** - Minor issues or enhancements

---

## 1. DUPLICATE INPUT PROCESSING PATHS 🔴 CRITICAL

### Problem
There are **TWO completely separate keyboard processing paths** that don't share state:

**Path A: PS/2 Driver (IRQ-driven)**
```
PS/2 IRQ → Ps2Keyboard::handle_scancode()
         → Keymap::process_scancode()
         → Track SHIFT/CTRL/ALT/NUMLOCK
         → push_to_console() via CONSOLE_CALLBACK
```

**Path B: Console Timer Tick (30 FPS polling)**
```
terminal_tick() → arch::get_scancode()
               → process_scancode() in console.rs
               → Track SHIFT/CTRL/ALT (SEPARATE STATIC VARS!)
               → vt::push_input()
```

### Why This Is Broken
1. **Modifier state is tracked twice** with separate variables:
   - `ps2/lib.rs`: `self.shift`, `self.ctrl`, `self.alt`, `self.altgr`, `self.numlock`
   - `console.rs`: `SHIFT_PRESSED`, `CTRL_PRESSED`, `ALT_PRESSED`, `EXTENDED_SCANCODE`

2. **Both paths process the same scancode** - the IRQ handler processes it AND
   the timer tick polls and processes it again (or misses it due to race).

3. **Escape sequences generated twice** - Arrow keys get `push_to_console()` in
   PS/2 driver AND `push_escape_sequence()` in console.rs.

### Fix Required
- Remove the duplicate `process_scancode()` in `console.rs`
- Have PS/2 IRQ handler be the ONLY place that processes scancodes
- Route all output through a single path to VT subsystem

---

## 2. CAPS LOCK NOT IMPLEMENTED 🟠 HIGH

### Problem
Caps Lock key is **recognized but completely ignored**:

```rust
// ps2/lib.rs line 236-252
input::KEY_NUMLOCK => {
    if pressed {
        let new_state = !self.numlock.load(Ordering::SeqCst);
        self.numlock.store(new_state, Ordering::SeqCst);
        // ... LED update
    }
    return; // Don't forward
}
// NO CAPSLOCK HANDLING AT ALL!
```

The keycode `KEY_CAPSLOCK` (58) is mapped but never processed. NumLock is
handled; Caps Lock and Scroll Lock are not.

### Fix Required
```rust
input::KEY_CAPSLOCK => {
    if pressed {
        let new_state = !self.capslock.load(Ordering::SeqCst);
        self.capslock.store(new_state, Ordering::SeqCst);
        let mut leds = self.leds.load(Ordering::SeqCst);
        if new_state { leds |= 0x04; } else { leds &= !0x04; }
        self.leds.store(leds, Ordering::SeqCst);
        self.update_leds();
    }
    return;
}
```

Then use `capslock` state in character conversion to uppercase letters.

---

## 3. KEYSTROKE DROPPING IN VT MANAGER 🔴 CRITICAL

### Problem
In `VtManager::push_input()`:

```rust
// vt/lib.rs line 170-173
let active = match ACTIVE_VT.try_read() {
    Some(guard) => *guard,
    None => return, // ⚠️ KEYSTROKE SILENTLY DROPPED!
};
```

And:
```rust
// line 181-183
let tty = if let Some(mut vt) = self.vts[active].try_lock() {
    // ...
} else {
    None  // ⚠️ KEYSTROKE SILENTLY DROPPED IF LOCK CONTENDED!
};
```

**When locks are contended, keystrokes are permanently lost.** This can happen
when a process is reading from the TTY while the timer tick is trying to push input.

### Fix Required
Use a **lock-free ring buffer** for the interrupt→process bridge:
```rust
struct LockFreeRing {
    buffer: [AtomicU8; 256],
    head: AtomicUsize,  // Written by interrupt
    tail: AtomicUsize,  // Read by process
}
```

Or use a proper SPSC queue like `heapless::spsc::Queue`.

---

## 4. VTIME NOT IMPLEMENTED (Raw Mode Timeout) 🟠 HIGH

### Problem
```rust
// ldisc.rs line 550-552
fn read_raw(&mut self, buf: &mut [u8]) -> usize {
    let vmin = self.termios.c_cc[VMIN] as usize;
    let _vtime = self.termios.c_cc[VTIME];  // ⚠️ COMPLETELY IGNORED!
```

VTIME controls read timeout in deciseconds. Without it:
- `VMIN=0, VTIME>0` (timeout read) doesn't work
- `VMIN>0, VTIME>0` (interbyte timeout) doesn't work

Many programs depend on these modes (e.g., `select()` with timeout on stdin).

### Fix Required
Implement proper timer-based read with the scheduler's sleep functionality.

---

## 5. O_NONBLOCK NOT SUPPORTED ON TTYs 🟠 HIGH

### Problem
```rust
// Searched: No O_NONBLOCK handling in VT or TTY read paths
```

The VT read path is:
```rust
loop {
    // drain buffer
    // check ldisc.can_read()
    // if not ready: yield_current() and LOOP FOREVER
}
```

There's no way to do a non-blocking read. Programs using `fcntl(F_SETFL, O_NONBLOCK)`
or `open(..., O_NONBLOCK)` will hang.

### Fix Required
Check file flags in syscall layer and return `EAGAIN` for non-blocking FDs when
no data is available.

---

## 6. SPINLOOP BLOCKING IN TTY READ 🟠 HIGH

### Problem
```rust
// tty.rs line 293-317
fn read(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
    loop {
        {
            let mut ldisc = self.ldisc.lock();
            if ldisc.can_read() {
                return Ok(ldisc.read(buf));
            }
        }
        sched::yield_current();  // ⚠️ BUSY-WAIT SPINLOOP!
    }
}
```

This **burns CPU cycles** polling for input. Every process waiting for keyboard
input is constantly waking up, checking, and yielding.

### Fix Required
Implement proper **wait queues**:
1. Process calls read() → no data → add to wait queue → sleep
2. Input arrives → wake processes on wait queue
3. Process wakes → read data → return

---

## 7. PIPE READ DOESN'T BLOCK 🟠 HIGH

### Problem
```rust
// pipe.rs line 123-140
fn read(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
    let mut buffer = self.buffer.lock();
    if buffer.count == 0 && !buffer.has_writers() {
        return Ok(0);  // EOF
    }
    let n = buffer.read(buf);
    if n > 0 {
        Ok(n)
    } else {
        Err(VfsError::WouldBlock)  // ⚠️ DOESN'T ACTUALLY BLOCK!
    }
}
```

The comment says "In a real implementation, we'd block the process" but it just
returns `EAGAIN`. This breaks:
- `cat file | grep pattern` (grep gets EAGAIN instead of blocking)
- Any pipeline with slow producers

### Fix Required
Implement blocking with wait queues, similar to TTY fix above.

---

## 8. SIGNAL DELIVERED TWICE 🟡 MEDIUM

### Problem
Ctrl+C signal is checked in TWO places:

1. **Immediately in push_input()** (interrupt context):
```rust
// vt/lib.rs line 198-207
if let Some((signal, pgid)) = tty.try_check_signal(ch) {
    callback(pgid, signal.to_signo());
}
```

2. **Again in read() loop** (process context):
```rust
// vt/lib.rs line 247-256
if let Some(signal) = tty.input(&[ch]) {
    callback(pgid, signal.to_signo());
}
```

The byte is still in the buffer, so when read() processes it, it sends SIGINT again!

### Fix Required
Either:
- Mark signal bytes as "already signaled" so they're not re-processed
- OR only do signal delivery in one place (prefer the immediate path)

---

## 9. KEYBOARD LAYOUT DOESN'T AFFECT CAPS LOCK 🟡 MEDIUM

### Problem
```rust
// layouts.rs line 29-41
pub fn get_char(&self, keycode: u16, shift: bool, altgr: bool) -> Option<char> {
    let ch = match (shift, altgr) {
        (false, false) => self.normal[idx],
        (true, false) => self.shift[idx],
        // ⚠️ NO CAPS LOCK PARAMETER!
    };
}
```

Even when Caps Lock is implemented, the layout system doesn't support it.
Caps Lock should uppercase letters but NOT affect numbers/symbols (unlike Shift).

### Fix Required
```rust
pub fn get_char(&self, keycode: u16, shift: bool, altgr: bool, capslock: bool) -> Option<char> {
    let effective_shift = shift ^ (capslock && self.is_letter(keycode));
    // ...
}
```

---

## 10. READLINE DISABLES ISIG (No Ctrl+C in Shell) 🟡 MEDIUM

### Problem
```rust
// readline.rs line 243-250
raw.c_lflag &= !(lflag::ICANON
    | lflag::ECHO
    | lflag::ECHOE
    | lflag::ECHOK
    | lflag::ECHOKE
    | lflag::ECHOCTL
    | lflag::ISIG       // ⚠️ DISABLES SIGNAL GENERATION!
    | lflag::IEXTEN);
```

When readline is active, ISIG is disabled, meaning Ctrl+C won't generate SIGINT
through the kernel's normal path. Readline should handle Ctrl+C itself, but
if it doesn't, the user can't interrupt a stuck readline.

### Fix Required
Either:
- Keep ISIG enabled and let kernel deliver signals
- OR handle Ctrl+C explicitly in readline and call `raise(SIGINT)`

---

## 11. NO SCROLL LOCK SUPPORT 🟢 LOW

### Problem
Scroll Lock key is mapped but not handled. Traditional behavior:
- Scroll Lock ON → pause terminal output (like Ctrl+S / XOFF)
- Scroll Lock OFF → resume output (like Ctrl+Q / XON)

### Fix Required
Track Scroll Lock state and implement XOFF/XON flow control, or at minimum
update the LED.

---

## 12. PAUSE KEY NOT HANDLED 🟢 LOW

### Problem
```rust
// keymap.rs line 292-294
if scancode == 0xE1 {
    return None;  // Just ignored
}
```

Pause/Break key uses the E1 prefix and a multi-byte sequence. Currently completely
ignored.

### Fix Required
Implement E1 sequence parsing for Pause key (E1 1D 45 E1 9D C5).

---

## 13. MEDIA/ACPI KEYS NOT SUPPORTED 🟢 LOW

### Problem
Many extended scancodes in the E0 prefix are marked `KEY_RESERVED`:
- Volume Up/Down/Mute (E0 30, E0 2E, E0 20)
- Play/Pause/Stop (E0 22, E0 24, etc.)
- Power/Sleep/Wake (partially supported)

### Fix Required
Add mappings in `SCANCODE_SET1_EXT` and report events through input subsystem.

---

## 14. INPUT SUBSYSTEM WAKE MECHANISM INCOMPLETE 🟡 MEDIUM

### Problem
```rust
// input/lib.rs line 66-71
pub fn set_blocked_reader(device_id: usize, pid: u32) {
    let mut readers = BLOCKED_READERS.lock();
    if device_id < MAX_DEVICES {
        readers[device_id] = pid;  // ⚠️ ONLY ONE PID PER DEVICE!
    }
}
```

Only ONE process can wait on each input device. If two processes try to read
from `/dev/input/event0`, one will never wake up.

### Fix Required
Use a list/set of waiting PIDs per device, or a proper wait queue.

---

## PRIORITY FIX ORDER

1. **Remove duplicate processing path** (console.rs vs ps2 driver) - CRITICAL
2. **Fix keystroke dropping** (lock-free buffer) - CRITICAL  
3. **Implement Caps Lock** - HIGH (users notice immediately)
4. **Fix pipe blocking** - HIGH (breaks pipelines)
5. **Implement wait queues** (replace spinloops) - HIGH (CPU usage)
6. **Fix O_NONBLOCK** - HIGH (many programs need it)
7. **Implement VTIME** - HIGH (select/poll depend on it)
8. **Fix double signal delivery** - MEDIUM
9. **Fix readline ISIG** - MEDIUM
10. Everything else - LOW

---

## ARCHITECTURE RECOMMENDATION

The current design mixes concerns badly. A cleaner architecture:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    SINGLE INPUT PATH                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  PS/2 IRQ Handler                                                   │
│       │                                                             │
│       ▼                                                             │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │ InputManager (lock-free SPSC queue per VT)                  │    │
│  │   - Receives raw keycodes + modifiers from IRQ              │    │
│  │   - No processing, just buffering                           │    │
│  └─────────────────────────────────────────────────────────────┘    │
│       │                                                             │
│       │ process context (syscall read)                              │
│       ▼                                                             │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │ LineDiscipline                                              │    │
│  │   - Character conversion (using layout + capslock + shift)  │    │
│  │   - Echo                                                    │    │
│  │   - Line editing (canonical mode)                           │    │
│  │   - Signal generation                                       │    │
│  └─────────────────────────────────────────────────────────────┘    │
│       │                                                             │
│       ▼                                                             │
│  Application (shell, vim, etc.)                                     │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

Key principles:
1. **IRQ handler does minimal work** - just queue the event
2. **Lock-free queue** between IRQ and process context
3. **All processing in process context** - safer, can block
4. **Wait queues** instead of spinloops
5. **Single source of truth** for modifier state

---

## High-Level Overview

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                           HARDWARE LAYER                                     │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   PS/2 Keyboard         Serial Port (COM1)         USB HID Keyboard          │
│   ┌──────────┐          ┌──────────┐               ┌──────────┐              │
│   │ Scancode │          │  Bytes   │               │ Scancode │              │
│   └────┬─────┘          └────┬─────┘               └────┬─────┘              │
│        │ IRQ 1               │ IRQ 4                    │ USB IRQ            │
│        ▼                     ▼                          ▼                    │
│   ┌──────────────────────────────────────────────────────────┐               │
│   │              Interrupt Handlers (IDT)                    │               │
│   │  crates/arch/arch-x86_64/src/exceptions.rs               │               │
│   └────────────────────────────┬─────────────────────────────┘               │
│                                │                                             │
└────────────────────────────────┼─────────────────────────────────────────────┘
                                 │
┌────────────────────────────────┼─────────────────────────────────────────────┐
│                           KERNEL LAYER                                       │
├────────────────────────────────┼─────────────────────────────────────────────┤
│                                ▼                                             │
│   ┌──────────────────────────────────────────────────────────┐               │
│   │             PS/2 Driver (crates/drivers/input/ps2/)      │               │
│   │                                                          │               │
│   │   Ps2Keyboard::handle_scancode()                         │               │
│   │     ├── Keymap::process_scancode() → keycode             │               │
│   │     ├── Update modifier state (Shift, Ctrl, Alt)         │               │
│   │     ├── input::report_key() → Input Subsystem            │               │
│   │     └── push_to_console() → CONSOLE_CALLBACK             │               │
│   └─────────────────────────────┬────────────────────────────┘               │
│                                 │                                            │
│    ┌────────────────────────────┴──────────────────────────────┐             │
│    │                                                           │             │
│    ▼                                                           ▼             │
│ ┌─────────────────────────────────┐    ┌───────────────────────────────────┐ │
│ │   Input Subsystem               │    │   Console/VT Manager              │ │
│ │   (crates/input/input/)         │    │   (kernel/src/console.rs)         │ │
│ │                                 │    │                                   │ │
│ │   ┌─────────────────────────┐   │    │   terminal_tick() [30 FPS]        │ │
│ │   │ InputEvent Queue (256)  │   │    │     ├── arch::get_scancode()      │ │
│ │   │ per device              │   │    │     ├── process_scancode()        │ │
│ │   └──────────┬──────────────┘   │    │     └── vt::push_input(byte)      │ │
│ │              │                  │    │                                   │ │
│ │              ▼                  │    └──────────────┬────────────────────┘ │
│ │   /dev/input/event0            │                   │                      │
│ │   (for evtest, libinput)       │                   │                      │
│ └────────────────────────────────┘                   │                      │
│                                                      ▼                      │
│                         ┌────────────────────────────────────────────────┐  │
│                         │   VT Manager (crates/tty/vt/)                  │  │
│                         │                                                │  │
│                         │   push_input(ch: u8)                           │  │
│                         │     ├── ACTIVE_VT.try_read() → vt_num          │  │
│                         │     ├── vts[vt_num].input_buffer.push(ch)      │  │
│                         │     └── Signal check (Ctrl+C immediate)        │  │
│                         │                                                │  │
│                         │   ┌──────────────────────────────────────┐     │  │
│                         │   │ VtState per VT (6 VTs: tty1-tty6)    │     │  │
│                         │   │                                      │     │  │
│                         │   │   input_buffer: Vec<u8> [4096]       │     │  │
│                         │   │   tty: Arc<Tty>                      │     │  │
│                         │   └──────────────────────────────────────┘     │  │
│                         │                    │                           │  │
│                         └────────────────────┼───────────────────────────┘  │
│                                              │                              │
│                                              ▼                              │
│                         ┌────────────────────────────────────────────────┐  │
│                         │   TTY Device (crates/tty/tty/)                 │  │
│                         │                                                │  │
│                         │   Tty { ldisc, winsize, fg_pgid, driver }      │  │
│                         │     │                                          │  │
│                         │     └── input() → Line Discipline              │  │
│                         └────────────────────┬───────────────────────────┘  │
│                                              │                              │
│                                              ▼                              │
│                         ┌────────────────────────────────────────────────┐  │
│                         │   Line Discipline (crates/tty/tty/src/ldisc.rs)│  │
│                         │                                                │  │
│                         │   LineDiscipline {                             │  │
│                         │     termios: Termios,                          │  │
│                         │     input_queue: VecDeque<u8> [4096],          │  │
│                         │     edit_buf: Vec<u8> [255 canonical],         │  │
│                         │     output_queue: VecDeque<u8> [4096],         │  │
│                         │   }                                            │  │
│                         │                                                │  │
│                         │   Modes:                                       │  │
│                         │   ┌───────────────────┬───────────────────────┐│  │
│                         │   │ Canonical (ICANON)│ Raw (non-ICANON)      ││  │
│                         │   │ Line editing:     │ Characters go         ││  │
│                         │   │ - ^H backspace    │ directly to           ││  │
│                         │   │ - ^U kill line    │ input_queue           ││  │
│                         │   │ - ^W word erase   │ (based on VMIN/VTIME) ││  │
│                         │   │ Edit in edit_buf  │                       ││  │
│                         │   │ Commit on \n/EOF  │                       ││  │
│                         │   └───────────────────┴───────────────────────┘│  │
│                         │                                                │  │
│                         │   Signal generation (ISIG):                    │  │
│                         │     ^C → SIGINT,  ^\ → SIGQUIT,  ^Z → SIGTSTP  │  │
│                         │                                                │  │
│                         │   Echo (ECHO flag):                            │  │
│                         │     Characters echoed back via TtyDriver       │  │
│                         └────────────────────┬───────────────────────────┘  │
│                                              │                              │
│                                              │ read() syscall               │
│                                              ▼                              │
│                         ┌────────────────────────────────────────────────┐  │
│                         │   /dev/tty1-6, /dev/console (crates/vfs/devfs/)│  │
│                         │                                                │  │
│                         │   ConsoleDevice → delegates to active VT       │  │
│                         │   VtDevice → VtManager.read(vt_num)            │  │
│                         │                                                │  │
│                         │   VnodeOps::read() blocking loop:              │  │
│                         │     1. Drain input_buffer → TTY ldisc          │  │
│                         │     2. Check ldisc.can_read()                  │  │
│                         │     3. If ready: ldisc.read(buf) → return      │  │
│                         │     4. Else: yield_current() → loop            │  │
│                         └────────────────────┬───────────────────────────┘  │
│                                              │                              │
└──────────────────────────────────────────────┼──────────────────────────────┘
                                               │
                                               │ syscall read(fd, buf, len)
                                               ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                           USERSPACE LAYER                                    │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   ┌──────────────────────────────────────────────────────────────────────┐   │
│   │   libc (userspace/libc/)                                             │   │
│   │                                                                      │   │
│   │   Low-level I/O:                                                     │   │
│   │     read(fd, buf, n) → syscall SYS_READ                              │   │
│   │     getchar() → read(STDIN_FILENO, &c, 1)                            │   │
│   │                                                                      │   │
│   │   termios:                                                           │   │
│   │     tcgetattr() / tcsetattr() → ioctl(TCGETS/TCSETS)                 │   │
│   │                                                                      │   │
│   │   readline (userspace/libc/src/readline.rs):                         │   │
│   │     ┌───────────────────────────────────────────────────────────┐    │   │
│   │     │ readline(prompt) → *mut u8                                │    │   │
│   │     │                                                           │    │   │
│   │     │ 1. Save termios, set raw mode (disable ICANON, ECHO)      │    │   │
│   │     │ 2. Print prompt                                           │    │   │
│   │     │ 3. Loop: getchar() → process key                          │    │   │
│   │     │    ├── Printable → insert at cursor                       │    │   │
│   │     │    ├── Backspace → delete char                            │    │   │
│   │     │    ├── Arrow keys → cursor movement                       │    │   │
│   │     │    ├── Tab → completion callback                          │    │   │
│   │     │    ├── Up/Down → history navigation                       │    │   │
│   │     │    └── Enter → return line                                │    │   │
│   │     │ 4. Restore termios                                        │    │   │
│   │     │                                                           │    │   │
│   │     │ Buffers:                                                  │    │   │
│   │     │   LINE_BUF: [u8; 4096]  (current line)                    │    │   │
│   │     │   HISTORY: [[u8; 4096]; 500] (history ring)               │    │   │
│   │     └───────────────────────────────────────────────────────────┘    │   │
│   └──────────────────────────────────────────────────────────────────────┘   │
│                                           │                                  │
│                                           │                                  │
│                ┌──────────────────────────┴───────────────────────┐          │
│                │                                                  │          │
│                ▼                                                  ▼          │
│   ┌────────────────────────────┐              ┌────────────────────────────┐ │
│   │   Shell (userspace/shell/) │              │   Editors/Apps (vim, cat)  │ │
│   │                            │              │                            │ │
│   │   readline(prompt)         │              │   Raw mode apps:           │ │
│   │     └── Returns line       │              │     tcsetattr(~ICANON)     │ │
│   │                            │              │     read(fd, &c, 1)        │ │
│   │   Builtins: cd, export,    │              │     Process ANSI escapes   │ │
│   │   read, history, etc.      │              │                            │ │
│   │                            │              │   Cooked mode apps:        │ │
│   │   Pipes:                   │              │     fgets() / read()       │ │
│   │     cmd1 | cmd2            │              │     Line-buffered input    │ │
│   │     ┌───────────┐          │              │                            │ │
│   │     │ PipeWrite ├──────────┤              └────────────────────────────┘ │
│   │     └───────────┘          │                                             │
│   │          │                 │                                             │
│   │          ▼                 │                                             │
│   │     ┌───────────┐          │                                             │
│   │     │ PipeRead  │──────────┤                                             │
│   │     └───────────┘          │                                             │
│   └────────────────────────────┘                                             │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Detailed Component Breakdown

### 1. Hardware → IRQ Handler

**File:** `crates/arch/arch-x86_64/src/exceptions.rs`, `crates/drivers/input/ps2/src/lib.rs`

```
PS/2 Keyboard (Port 0x60/0x64)
    │
    │ IRQ 1 fires on keypress
    ▼
handle_keyboard_irq()
    ├── inb(0x60) → scancode
    ├── Ps2Keyboard::handle_scancode(scancode)
    │     ├── Keymap::process_scancode() → (keycode, pressed)
    │     ├── Update modifier state (SHIFT, CTRL, ALT, ALTGR, NUMLOCK)
    │     ├── input::report_key(device_id, keycode, KeyValue)
    │     └── push_to_console(bytes) via CONSOLE_CALLBACK
    └── EOI to APIC
```

### 2. Input Subsystem (evdev-style)

**File:** `crates/input/input/src/lib.rs`

```
                InputDeviceHandle
                ┌────────────────────────────────────┐
                │ info: InputDeviceInfo              │
                │ events: Mutex<VecDeque<InputEvent>>│ ← 256 events max
                │ device: Arc<dyn InputDevice>       │
                └────────────────────────────────────┘
                          │
                          │ report_key(), report_rel(), report_sync()
                          ▼
                    Global DEVICES registry
                          │
                          │ /dev/input/event0, event1, ...
                          ▼
                    evtest, libinput consumers
```

### 3. Console Timer Tick (Keyboard Polling)

**File:** `kernel/src/console.rs`

```
terminal_tick() [called at 30 FPS from timer interrupt]
    │
    ├── Poll arch::get_scancode() ring buffer
    │     └── process_scancode() → ASCII/escape sequence
    │           └── vt::push_input(byte)
    │
    ├── Poll arch::poll_keyboard() [fallback for QEMU sendkey]
    │     └── Same as above
    │
    ├── Poll arch::serial_read_unsafe() [COM1 input]
    │     └── vt::push_input(byte)
    │
    └── Mouse event processing → fb::mouse_move()
```

### 4. VT Manager

**File:** `crates/tty/vt/src/lib.rs`

```
VtManager
├── vts: [Mutex<VtState>; 6]  (tty1-tty6)
├── ACTIVE_VT: RwLock<usize>
│
│ push_input(ch) [from interrupt context]
│   ├── ACTIVE_VT.try_read() → non-blocking
│   ├── vts[active].try_lock() → non-blocking
│   ├── input_buffer.push(ch)
│   └── Signal check (Ctrl+C) → immediate SIGINT
│
│ read(vt_num, buf) [blocking syscall]
│   ├── Loop:
│   │     ├── Drain input_buffer → tty.input(&[ch])
│   │     ├── tty.try_read(buf) → returns if data ready
│   │     └── yield_current() if not ready
│   └── Returns bytes read
│
│ write(vt_num, buf)
│   └── tty.write() → TtyDriver → console output
│
└── VT switching: Ctrl+Alt+F1-F6
      └── Updates ACTIVE_VT
```

### 5. TTY + Line Discipline

**File:** `crates/tty/tty/src/tty.rs`, `crates/tty/tty/src/ldisc.rs`

```
Tty {
    ldisc: Mutex<LineDiscipline>,
    winsize: Mutex<Winsize>,
    foreground_pgid: Mutex<i32>,
    driver: Arc<dyn TtyDriver>,
}

LineDiscipline {
    termios: Termios,           ← c_iflag, c_oflag, c_cflag, c_lflag, c_cc
    input_queue: VecDeque<u8>,  ← 4096 bytes, cooked data ready for read()
    edit_buf: Vec<u8>,          ← 255 bytes max (canonical mode line)
    output_queue: VecDeque<u8>, ← 4096 bytes
}

┌─────────────────────────────────────────────────────────────────┐
│                    Line Discipline Flow                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   input_char(c, write_echo)                                     │
│     │                                                           │
│     ├── Signal check (ISIG):                                    │
│     │     VINTR (^C) → Signal::Int → SIGINT                     │
│     │     VQUIT (^\) → Signal::Quit → SIGQUIT                   │
│     │     VSUSP (^Z) → Signal::Tstp → SIGTSTP                   │
│     │                                                           │
│     ├── Canonical mode (ICANON):                                │
│     │     │                                                     │
│     │     ├── '\n' → commit edit_buf → input_queue              │
│     │     ├── VERASE (^H/DEL) → backspace in edit_buf           │
│     │     ├── VKILL (^U) → clear edit_buf                       │
│     │     ├── VEOF (^D) → commit (possibly empty)               │
│     │     └── Regular char → edit_buf.push(c)                   │
│     │                                                           │
│     └── Raw mode (~ICANON):                                     │
│           └── c → input_queue.push_back(c)                      │
│                                                                 │
│   Echo (ECHO flag):                                             │
│     write_echo() callback → driver.write()                      │
│                                                                 │
│   read(buf):                                                    │
│     Canonical: return up to '\n' from input_queue               │
│     Raw: return based on VMIN/VTIME settings                    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 6. Device Files (/dev)

**File:** `crates/vfs/devfs/src/devices.rs`, `crates/tty/vt/src/lib.rs`

```
/dev/console   ← ConsoleDevice → delegates to active VT
/dev/tty1-6    ← VtDevice → VtManager.read/write(vt_num)
/dev/tty       ← Controlling TTY for process
/dev/ptmx      ← PTY master multiplexer
/dev/pts/*     ← PTY slave devices

/dev/null      ← NullDevice (discard writes, EOF reads)
/dev/zero      ← ZeroDevice (infinite zeros)
/dev/input/event0  ← Input subsystem events (evtest)
```

### 7. Pipe Implementation

**File:** `crates/vfs/vfs/src/pipe.rs`

```
pipe() syscall → (PipeRead, PipeWrite)

PipeBuffer {
    data: Vec<u8>,         ← 64KB ring buffer
    read_pos, write_pos,
    count,
    readers: AtomicUsize,  ← Reference count
    writers: AtomicUsize,
}

┌──────────────┐          ┌──────────────┐
│  PipeWrite   │ ──────▶  │  PipeRead    │
│  (stdout)    │  buffer  │  (stdin)     │
└──────────────┘          └──────────────┘

Shell: cmd1 | cmd2
  fork() cmd1 with stdout → PipeWrite
  fork() cmd2 with stdin  → PipeRead
```

### 8. Userspace Readline

**File:** `userspace/libc/src/readline.rs`

```
readline(prompt):
  1. tcgetattr(STDIN_FILENO, &orig) → save termios
  2. raw_termios = orig & ~(ICANON | ECHO | ISIG)
  3. tcsetattr(STDIN_FILENO, TCSANOW, &raw_termios)
  4. print(prompt)
  5. Loop:
       c = getchar()
       switch c:
         Printable     → LINE_BUF[CURSOR++] = c; putchar(c)
         Backspace     → delete char, reprint
         ESC sequence  → parse_escape() → cursor/history/etc
         Tab           → call rl_attempted_completion_function
         Enter         → return LINE_BUF
  6. tcsetattr(STDIN_FILENO, TCSANOW, &orig)
  7. return allocated copy of line

History:
  HISTORY: [[u8; 4096]; 500]  ← Ring buffer
  Up/Down arrows navigate
  add_history() appends
```

## Buffer Summary

| Location | Buffer | Size | Purpose |
|----------|--------|------|---------|
| PS/2 Driver | scancode ring | arch-specific | IRQ→poll bridge |
| Input Subsystem | event queue | 256 events | evdev-style events |
| VT Manager | input_buffer | 4096 bytes | IRQ→read bridge |
| Line Discipline | edit_buf | 255 bytes | Canonical line editing |
| Line Discipline | input_queue | 4096 bytes | Cooked data for read() |
| Pipe | ring buffer | 64KB | IPC between processes |
| Readline | LINE_BUF | 4096 bytes | User's current line |
| Readline | HISTORY | 500×4096 | Command history |

## Escape Sequence Flow

```
Keyboard: ← arrow key pressed
    │
    ├── PS/2 Driver: E0 4B (extended scancode)
    │     └── keycode_to_char_current() → None (not printable)
    │           └── push_to_console(b"\x1b[D")  ← ANSI left arrow
    │
    ▼
VT Manager: push_input(0x1B), push_input('['), push_input('D')
    │
    ▼
Line Discipline:
    ├── Canonical mode: accumulates in edit_buf (escape ignored)
    └── Raw mode: each byte goes to input_queue
    │
    ▼
Application (readline in raw mode):
    getchar() → 0x1B
    getchar() → '['
    getchar() → 'D'
    └── Recognized as CSI D → cursor left
```

## Signal Flow

```
User presses Ctrl+C (0x03)
    │
    ├── PS/2 Driver: scancode 0x2E with CTRL modifier
    │     └── push_to_console(&[0x03])
    │
    ▼
VT Manager: push_input(0x03)
    │
    ├── Immediate check: try_check_signal(0x03)
    │     └── termios.c_cc[VINTR] == 0x03 → Signal::Int
    │           └── SIGNAL_PGRP_CALLBACK(fg_pgid, SIGINT)
    │
    └── Byte still buffered for read() (to maintain data integrity)
```

## Mode Comparison

| Feature | Canonical Mode | Raw Mode |
|---------|----------------|----------|
| Line editing | Kernel (^H, ^U, ^W) | Application |
| Buffering | Until newline | VMIN/VTIME |
| Echo | Kernel (ECHO flag) | Application |
| Signals | Kernel (ISIG) | Application |
| Escape sequences | Accumulated | Passed through |
| Use case | Simple CLI | Editors, shells |

## Key Files Reference

- `crates/drivers/input/ps2/src/lib.rs` - PS/2 keyboard driver
- `crates/input/input/src/lib.rs` - Input subsystem
- `kernel/src/console.rs` - Console timer tick, scancode processing
- `crates/tty/vt/src/lib.rs` - Virtual terminal manager
- `crates/tty/tty/src/tty.rs` - TTY device
- `crates/tty/tty/src/ldisc.rs` - Line discipline
- `crates/vfs/devfs/src/devices.rs` - /dev device implementations
- `crates/vfs/vfs/src/pipe.rs` - Pipe implementation
- `userspace/libc/src/readline.rs` - Userspace readline library
- `userspace/shell/src/main.rs` - Shell using readline
