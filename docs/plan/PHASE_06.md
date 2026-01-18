# Phase 6: TTY + PTY

**Stage:** 2 - Core OS
**Status:** Not Started
**Dependencies:** Phase 5 (VFS + Filesystems)

---

## Goal

Implement terminal subsystem with line discipline and pseudo-terminals.

---

## Deliverables

| Item | Status |
|------|--------|
| TTY device abstraction | [ ] |
| Line discipline (canonical mode) | [ ] |
| Raw mode support | [ ] |
| PTY master/slave pairs | [ ] |
| Foreground process group | [ ] |
| Window size (TIOCGWINSZ/TIOCSWINSZ) | [ ] |
| Job control basics | [ ] |

---

## Architecture Status

| Arch | TTY | LineDis | PTY | JobCtl | Done |
|------|-----|---------|-----|--------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
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

- [ ] Line editing works (backspace, ^U)
- [ ] Echo works in canonical mode
- [ ] ^C sends SIGINT to foreground group
- [ ] ^Z sends SIGTSTP (after signals implemented)
- [ ] PTY pairs created via /dev/ptmx
- [ ] Window size ioctl works
- [ ] Works on all 8 architectures

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

*Add implementation notes here as work progresses*

---

*Phase 6 of EFFLUX Implementation*
