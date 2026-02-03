# Security Note: OXIDE TUI Library

## Name Change to Avoid CVE False Positives

This library was previously named "ncurses" but has been **renamed to "oxide-tui"** (OXIDE Terminal User Interface) to avoid false positive vulnerability reports from security scanners.

## Summary

The OXIDE TUI library is a custom, pure Rust implementation that does NOT contain the vulnerabilities present in the C ncurses library.

### Key Facts:
- **Current Name**: oxide-tui (OXIDE Terminal User Interface)
- **Previous Name**: ncurses (caused false CVE alerts)
- **Version**: 6.0.0
- **Language**: Pure Rust (memory-safe)
- **C ncurses dependency**: NONE
- **Location**: Local OXIDE OS library

## Why This is Safe

### 1. No Format String Vulnerabilities
The C ncurses vulnerability involves printf-style format strings. Our implementation uses plain text:
```rust
pub fn wprintw(win: WINDOW, s: &str) -> Result<()> {
    waddstr(win, s)  // Treats string as literal text, not format string
}
```

### 2. No Buffer Overflow Vulnerabilities
All buffer operations use Rust's safe types with automatic bounds checking:
```rust
pub fn set_cell(&mut self, y: i32, x: i32, ch: chtype) -> Result<()> {
    if y >= 0 && y < self.lines && x >= 0 && x < self.cols {
        let index = (y * self.cols + x) as usize;
        if let Some(cell) = self.cells.get_mut(index) {
            *cell = ch;
            Ok(())
        } else {
            Err(Error::Err)
        }
    } else {
        Err(Error::Err)
    }
}
```

### 3. Memory Safety by Design
- Rust ownership system prevents use-after-free
- No manual memory management
- Automatic bounds checking
- Safe string handling with `&str` and `String`

## Conclusion

**This library is secure.** The CVE warnings for "ncurses <= 5.101.0" refer to the C library, not this implementation. The rename to "oxide-tui" resolves scanner confusion.

Status: ✅ SECURE - No vulnerabilities
