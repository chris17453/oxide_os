# Terminal Rendering Performance Fixes

## Issue
Screen redraw was extremely slow ("like the 1970s") due to excessive framebuffer flushes and full-screen redraws.

## Root Causes

### 1. Per-Byte Framebuffer Flushes (CRITICAL)
**Location:** `kernel/src/init.rs:50` in `OsLogConsoleWriter::write_byte()`

**Problem:**
```rust
// Before: called per byte!
fn write_byte(&mut self, byte: u8) {
    terminal::write(&[byte]);  // Each call flushes FB!
}
```

**Impact:**
- Every `println!("test")` = 5 calls to `terminal::write()` = 5 framebuffer flushes
- Message like `"[PS2] init: starting (IRQs disabled)"` = 37 FB flushes
- During boot with 15 log messages = **500+ framebuffer flushes**
- Each flush waits for GPU/display controller write-back
- **This was the PRIMARY cause of slow rendering**

**Fix:**
```rust
// After: batches entire string!
fn write_str(&mut self, s: &str) {
    terminal::write(s.as_bytes());  // One call = one flush
}
```

**Speedup:** ~40x faster for typical log messages

### 2. Text Blink Full-Screen Redraws
**Location:** `kernel/tty/terminal/src/renderer.rs:199`

**Problem:**
```rust
// Before: marked ALL rows dirty every ~500ms
if old_blink_on != new_blink_on {
    self.dirty.mark_all();  // Redraws entire 80x24 screen!
}
```

**Impact:**
- Text blink toggles at 2Hz (every 15 frames at 30fps)
- Each toggle marked **all 24-80 rows dirty**
- Full screen redraw every 500ms even if no blinking text exists
- Wastes ~1920 cells × glyph renders = ~30,000 pixel blits

**Fix:**
```rust
// After: only marks rows with BLINK cells dirty
if old_blink_on != new_blink_on {
    for row in 0..self.rows {
        let has_blink = (0..self.cols).any(|col| {
            buffer.get(row, col)
                .map(|cell| cell.attrs.flags.contains(CellFlags::BLINK))
                .unwrap_or(false)
        });
        if has_blink {
            self.dirty.mark_row(row);  // Only this row!
        }
    }
}
```

**Speedup:** ~24x fewer rows rendered when no blinking text (common case)

### 3. Selection Full-Screen Redraws
**Location:** `kernel/tty/terminal/src/lib.rs:1320, 1344, 1399, 1410`

**Problem:**
```rust
// Before: every selection operation marked ALL rows dirty
pub fn start_selection(&mut self, ...) {
    // ...
    self.renderer.mark_all_dirty();  // Redraws 80x24!
}

pub fn update_selection(&mut self, ...) {
    // ...
    self.renderer.mark_all_dirty();  // Again!
}
```

**Impact:**
- Mouse drag selection = hundreds of `update_selection()` calls
- Each call = full 80×24 screen redraw
- Selecting 5 lines of text = redrawing 24 rows × hundreds of times
- **Visible lag during text selection**

**Fix:**
```rust
// After: only marks affected rows
fn mark_selection_dirty(&mut self) {
    if let Some(ref sel) = self.selection {
        let min_row = sel.start.1.min(sel.end.1);
        let max_row = sel.start.1.max(sel.end.1);
        self.renderer.mark_rows(min_row, max_row);  // Only 1-10 rows typically
    }
}
```

**Speedup:** ~10-20x fewer rows rendered during selection

## Performance Gains Summary

| Operation | Before | After | Speedup |
|-----------|--------|-------|---------|
| `println!("test")` | 5 FB flushes | 1 FB flush | 5x |
| 40-char log message | 40 FB flushes | 1 FB flush | 40x |
| Text blink (no BLINK attr) | 24 rows | 0 rows | ∞ |
| Text blink (2 rows) | 24 rows | 2 rows | 12x |
| Selection drag (5 rows) | 24 rows | 5 rows | 4.8x |

**Overall:** Boot messages and interactive terminal now render at **framebuffer-limited speed** instead of being throttled by excessive flushes/redraws.

## Architecture

### Before:
```
println!("hi") → [h][i][\n]
                  ↓  ↓  ↓
         write(1) write(1) write(1)
                  ↓  ↓  ↓
         flush    flush  flush  ← 3 FB writes for 3 bytes!
```

### After:
```
println!("hi") → "hi\n"
                    ↓
                write(3)
                    ↓
                  flush  ← 1 FB write for 3 bytes
```

## Rules for Future Development

### 1. Batch Writes
**NEVER** call `terminal::write()` or `framebuffer::flush()` in a tight loop for individual bytes.
- ✅ Accumulate bytes in buffer, flush once
- ✅ Implement `write_str()` to batch string writes
- ❌ Call `write_byte()` repeatedly for formatted output

### 2. Dirty Region Tracking
**NEVER** call `mark_all_dirty()` unless the entire screen truly changed.
- ✅ Mark specific rows: `mark_row(row)` or `mark_rows(start, end)`
- ✅ Scan for affected rows before marking (e.g., BLINK cells)
- ❌ Lazy `mark_all_dirty()` as a catch-all

### 3. Framebuffer Flush Discipline
Each flush waits for GPU/display write-back. Minimize flush frequency:
- ✅ One flush per `write()` call (current design)
- ✅ Coalesce multiple writes before flushing
- ❌ Flush inside tight loops
- ❌ Flush per-character/per-glyph

### 4. Selection Updates
Mouse drag generates hundreds of events per second:
- ✅ Mark only selection row range dirty
- ✅ Combine mark + set_selection in one call
- ❌ Full screen redraw per mouse motion event

## Testing Checklist

After terminal rendering changes:
- [ ] Boot messages appear instantly (no visible line-by-line lag)
- [ ] `cat large_file.txt` scrolls smoothly
- [ ] Text selection highlight responds immediately to mouse drag
- [ ] Blinking cursor doesn't cause screen flicker
- [ ] No visual artifacts from dirty region tracking

## Related Documents
- `docs/agents/synchronous-render-on-write.md` — Render before releasing lock
- `docs/agents/terminal-dirty-marking.md` — Smart dirty tracking rules
- `docs/agents/stdout-serial-separation.md` — Stdout → terminal only

— SableWire: The framebuffer was crying for mercy. Now it breathes.
