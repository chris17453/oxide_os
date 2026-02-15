# Curses Initialization: cbreak + noecho Required

## Rule
Every ncurses application MUST call `cbreak()` and `noecho()` after `initscr()`. Without these, the terminal stays in canonical (line-buffered) mode and `getch()` can never detect single keystrokes.

## Why
Our `initscr()` does NOT call `cbreak()` or `raw()` by default (matching standard ncurses behavior). The default terminal mode is canonical:
- `read()` blocks until newline
- `poll(POLLIN)` returns "not ready" until a complete line with `\n` exists
- `getch()` in nodelay mode always returns -1 (no complete line = no data)

## The Bug
```rust
let stdscr = initscr();
// MISSING: cbreak() and noecho()
unsafe { (*stdscr).nodelay = true; }
loop {
    let ch = getch(); // ALWAYS returns -1 — canonical mode needs newline
    if ch == b'q' as i32 { break; } // Never reached
    // ... animation ...
}
```

## The Fix
```rust
let stdscr = initscr();
let _ = cbreak();  // ICANON off — single chars available immediately
let _ = noecho();  // Don't echo keystrokes over the animated display
unsafe { (*stdscr).nodelay = true; }
```

## Also: Sleep in Animation Loops
On a non-preemptive single-core OS, a tight animation loop with no sleep monopolizes the CPU. Other processes (shell, init) starve. Always add `sleep_ms(16)` or similar for ~60 FPS.

-- NeonVale: initscr() sets the stage. cbreak() opens the curtain. Forget it and getch() just stares at the audience forever.
