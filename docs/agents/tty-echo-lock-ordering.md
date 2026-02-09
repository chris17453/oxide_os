# TTY Echo Lock Ordering: Collect-Then-Write Pattern

## Rule
**NEVER call `driver.write()` while holding the LDISC mutex.**
Collect echo data into a buffer while holding the lock, write AFTER releasing.

## Background
`Tty::input()` processes keyboard input with the LDISC mutex held. The echo
callback originally called `self.driver.write(echo_data)` directly, which goes
through:

```
LDISC lock held → driver.write() → VtTtyDriver → console_write() → terminal::write() → TERMINAL mutex
```

If the timer ISR on another CPU holds the TERMINAL mutex (for cursor blink in
`tick()`), the system deadlocks — the ISR can't release TERMINAL, and the input
path spins forever trying to acquire it.

## The Fix (Linux n_tty pattern)
Same pattern already used in `Tty::write()`:

```rust
// CORRECT: collect with lock, write without
let mut echo_buf = Vec::new();
{
    let mut ldisc = self.ldisc.lock();
    for &c in data {
        ldisc.input_char(c, |echo_data| {
            echo_buf.extend_from_slice(echo_data);  // No lock nesting
        });
    }
} // LDISC lock released

if !echo_buf.is_empty() {
    self.driver.write(&echo_buf);  // Safe: no LDISC lock held
}
```

```rust
// WRONG: writes while holding LDISC lock
let mut ldisc = self.ldisc.lock();
ldisc.input_char(c, |echo_data| {
    self.driver.write(echo_data);  // DEADLOCK: LDISC → TERMINAL
});
```

## Lock Ordering
The only safe ordering is: **TERMINAL before LDISC** (or never nest them).
- `Tty::write()` acquires LDISC, releases, then TERMINAL via driver — OK
- `Tty::input()` must follow the same pattern — OK after fix
- Timer ISR acquires TERMINAL only (for cursor blink) — OK

## Tab Echo
`input_char()` must expand tabs to spaces (not raw 0x09). The `echo_char()`
method handles this but wasn't called from `input_char()`. Fixed inline:

```rust
if c == b'\t' {
    let spaces = 8 - (self.column % 8);
    const SPACE_BUF: [u8; 8] = [b' '; 8];
    write_echo(&SPACE_BUF[..spaces]);
    self.column += spaces;
}
```

## Files
- `kernel/tty/tty/src/tty.rs` — `Tty::input()` collect-then-write
- `kernel/tty/tty/src/ldisc.rs` — `input_char()` tab expansion
