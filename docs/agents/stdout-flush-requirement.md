# stdout Flush Requirement for Terminal Control Sequences

**Problem:** ANSI/VT100 escape sequences sent via `prints()` or raw `write(1, ...)` don't reach the terminal immediately.

**Root Cause:** libc buffering. The stdout buffer (`STDOUT_BUFFER` in `userspace/libs/libc/src/stdio.rs`) only flushes when:
1. A newline (`\n`) is encountered
2. The buffer reaches 256 bytes
3. `fflush_stdout()` is explicitly called

**Impact:**
- Terminal control sequences (clear screen, cursor movement, colors) sit in buffer
- Applications appear frozen or unresponsive
- Ncurses/curses apps run at <20 FPS instead of target 60 FPS
- Commands like `clear` don't execute until next newline output

**Solution:** Always call `fflush_stdout()` after emitting terminal control sequences.

## Fixed Issues

### 1. `clear` command (coreutils)
**File:** `userspace/coreutils/src/bin/clear.rs`

**Before:**
```rust
prints("\x1b[2J\x1b[H");  // Sits in buffer forever
```

**After:**
```rust
prints("\x1b[2J\x1b[H");
fflush_stdout();  // Kicks to kernel immediately
```

### 2. ncurses `refresh()`
**File:** `userspace/libs/oxide-ncurses/src/screen.rs:136`

**Before:**
```rust
fn flush(&mut self) {
    if !self.buf.is_empty() {
        libc::unistd::write(1, &self.buf);
        self.buf.clear();
    }
}
```

**After:**
```rust
fn flush(&mut self) {
    if !self.buf.is_empty() {
        libc::unistd::write(1, &self.buf);
        self.buf.clear();
        libc::fflush_stdout();  // Force kernel write
    }
}
```

**Result:** curses-demo FPS jumped from <20 to ~60 FPS.

## Rule

**When writing ANSI/VT100 sequences without trailing newlines:**
```rust
// ❌ BAD - escape sequences sit in buffer
prints("\x1b[31mRED\x1b[0m");
prints("\x1b[2J\x1b[H");  // clear screen

// ✅ GOOD - explicit flush
prints("\x1b[31mRED\x1b[0m");
fflush_stdout();

// ✅ ALSO GOOD - newline auto-flushes
prints("\x1b[31mRED\x1b[0m\n");
```

**Exception:** If the output naturally ends with `\n` or accumulates >256 bytes before next flush point, explicit flush may be unnecessary.

## Related Files
- `userspace/libs/libc/src/stdio.rs` - stdout buffer implementation
- `kernel/tty/terminal/src/lib.rs` - terminal emulator that receives flushed output
- `docs/agents/vt-poll-drain.md` - VT polling drain requirement (input side)

— GlassSignal: Flush it or lose it. Buffering is for throughput, not interactivity.
