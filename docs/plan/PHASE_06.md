# Phase 6: TTY + PTY

**Stage:** 2 - Core OS
**Status:** Complete (x86_64)
**Dependencies:** Phase 5 (VFS + Filesystems)

---

## Goal

Implement terminal subsystem with line discipline and pseudo-terminals.

---

## Deliverables

| Item | Status |
|------|--------|
| TTY device abstraction | [x] |
| Line discipline (canonical mode) | [x] |
| Raw mode support | [x] |
| PTY master/slave pairs | [x] |
| Foreground process group | [x] |
| Window size (TIOCGWINSZ/TIOCSWINSZ) | [x] |
| Job control basics | [x] |

---

## Architecture Status

| Arch | TTY | LineDis | PTY | JobCtl | Done |
|------|-----|---------|-----|--------|------|
| x86_64 | [x] | [x] | [x] | [x] | [x] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Syscalls/Ioctls to Implement

| Name | Description |
|------|-------------|
| sys_ioctl | Generic ioctl interface |
| TCGETS | Get termios structure |
| TCSETS | Set termios structure |
| TCSETSW | Set termios after drain |
| TCSETSF | Set termios after flush |
| TIOCGWINSZ | Get window size |
| TIOCSWINSZ | Set window size |
| TIOCGPGRP | Get foreground pgrp |
| TIOCSPGRP | Set foreground pgrp |
| TIOCSCTTY | Set controlling terminal |

---

## TTY Architecture

```
┌─────────────┐     ┌─────────────┐
│  User Input │     │   Process   │
│  (keyboard) │     │  (shell)    │
└──────┬──────┘     └──────┬──────┘
       │                   │
       ▼                   ▼
┌─────────────────────────────────┐
│         Line Discipline         │
│  ┌─────────────────────────┐   │
│  │ Input Processing:       │   │
│  │ - Echo                  │   │
│  │ - Line editing (^H, ^U) │   │
│  │ - Signal generation     │   │
│  │   (^C, ^Z, ^\)          │   │
│  └─────────────────────────┘   │
└──────────────┬──────────────────┘
               │
               ▼
┌─────────────────────────────────┐
│           TTY Driver            │
│  (serial, console, pty)         │
└─────────────────────────────────┘
```

---

## Line Discipline Modes

**Canonical Mode (cooked):**
- Input available line-by-line (after Enter)
- Line editing: backspace, kill line (^U), word erase (^W)
- Echo characters as typed
- Signal generation: ^C (SIGINT), ^Z (SIGTSTP), ^\ (SIGQUIT)

**Raw Mode:**
- Input available immediately (character-by-character)
- No line editing
- No echo (unless explicitly enabled)
- No signal generation
- Used by: editors, games, terminals

**Cbreak Mode:**
- Hybrid: immediate input but with some processing
- Signal generation enabled
- Used by: some interactive programs

---

## PTY Architecture

```
┌─────────────┐         ┌─────────────┐
│  Terminal   │         │   Shell     │
│  Emulator   │         │  Process    │
│  (xterm)    │         │             │
└──────┬──────┘         └──────┬──────┘
       │                       │
       ▼                       ▼
┌─────────────┐         ┌─────────────┐
│  PTY Master │◄───────►│  PTY Slave  │
│  /dev/ptmx  │         │ /dev/pts/N  │
└─────────────┘         └─────────────┘
       │
       │ Data flows through
       │ line discipline
       │
```

---

## Key Files

```
crates/tty/efflux-tty/src/
├── lib.rs
├── tty.rs             # TTY device
├── ldisc.rs           # Line discipline
├── termios.rs         # termios structure
└── winsize.rs         # Window size

crates/tty/efflux-pty/src/
├── lib.rs
├── master.rs          # PTY master
├── slave.rs           # PTY slave
└── pts.rs             # /dev/pts filesystem
```

---

## Termios Structure

```rust
pub struct Termios {
    pub c_iflag: u32,   // Input modes
    pub c_oflag: u32,   // Output modes
    pub c_cflag: u32,   // Control modes
    pub c_lflag: u32,   // Local modes
    pub c_cc: [u8; 32], // Control characters
    pub c_ispeed: u32,  // Input baud rate
    pub c_ospeed: u32,  // Output baud rate
}

// Key c_lflag bits
const ECHO: u32 = 0x0008;    // Echo input
const ICANON: u32 = 0x0002;  // Canonical mode
const ISIG: u32 = 0x0001;    // Signal generation

// Control characters
const VINTR: usize = 0;   // ^C
const VQUIT: usize = 1;   // ^\
const VERASE: usize = 2;  // ^H
const VKILL: usize = 3;   // ^U
const VEOF: usize = 4;    // ^D
const VSUSP: usize = 10;  // ^Z
```

---

## Exit Criteria

- [x] Line editing works (backspace, ^U)
- [x] Echo works in canonical mode
- [x] ^C sends SIGINT to foreground group (infrastructure ready, signals in Phase 7)
- [x] ^Z sends SIGTSTP (infrastructure ready, signals in Phase 7)
- [x] PTY pairs created via /dev/ptmx
- [x] Window size ioctl works
- [ ] Works on all 8 architectures (x86_64 complete)

---

## Test Program

```c
int main() {
    struct termios old, new;

    // Save old settings
    tcgetattr(0, &old);

    // Set raw mode
    new = old;
    new.c_lflag &= ~(ICANON | ECHO);
    tcsetattr(0, TCSANOW, &new);

    printf("Press any key (q to quit):\n");
    char c;
    while (read(0, &c, 1) == 1 && c != 'q') {
        printf("Got: 0x%02x '%c'\n", c, isprint(c) ? c : '?');
    }

    // Restore
    tcsetattr(0, TCSANOW, &old);
    return 0;
}
```

---

## Notes

### Implementation (2026-01-18)

Phase 6 TTY + PTY infrastructure complete for x86_64:

**Crates Created:**

- `efflux-tty`: TTY subsystem with line discipline
  - `termios.rs`: Full termios structure with input/output/control/local flags
  - `winsize.rs`: Terminal window size structure
  - `ldisc.rs`: Line discipline with canonical/raw mode support
    - Line editing: backspace (^H), kill line (^U), word erase (^W)
    - Echo with control character display (^X format)
    - Signal character detection (^C, ^Z, ^\)
  - `tty.rs`: TTY device implementing VnodeOps
    - Integrates line discipline with hardware driver
    - ioctl support for termios and window size

- `efflux-pty`: Pseudo-terminal support
  - PTY master/slave pairs
  - `/dev/ptmx` device for allocating new PTYs
  - `/dev/pts/` directory for slave devices
  - PtyManager for PTY allocation

**Syscalls Added:**
- `sys_ioctl` (nr 40): Device I/O control for termios, winsize, pgrp

**VFS Extensions:**
- Added `ioctl()` method to `VnodeOps` trait
- Added `ioctl()` method to `File` struct
- Added `BrokenPipe` error variant to `VfsError`

**Kernel Integration:**
- PTY manager initialized at boot
- devpts filesystem mounted at `/dev/pts`
- Job control infrastructure (foreground pgrp) in place
- Signal generation infrastructure ready (delivery in Phase 7)

---

*Phase 6 of EFFLUX Implementation*
