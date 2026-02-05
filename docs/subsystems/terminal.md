# Terminal & TTY Subsystem

## Crates

| Crate | Purpose |
|-------|---------|
| `tty` | TTY device layer (line discipline, job control) |
| `pty` | Pseudo-terminal pairs (master/slave) |
| `vt` | VT100/xterm terminal emulator (escape sequences, colors) |
| `terminal` | High-level terminal rendering (framebuffer console) |
| `oxide-ncurses` | Userspace ncurses-compatible TUI (sole TUI library; previously also named `oxide-tui`) |

## Architecture

The TTY subsystem implements Unix terminal semantics. Physical consoles use
the `vt` crate for VT100 emulation rendered through the framebuffer via
`terminal`. Remote sessions (SSH) use pseudo-terminals from `pty`.

The `tty` crate provides line discipline (canonical/raw mode), job control
signals (SIGINT, SIGTSTP), and the termios interface.

Features include wide character support, synthetic bold/italic rendering,
clipboard via OSC 52, and DCS (Device Control String) framework.
