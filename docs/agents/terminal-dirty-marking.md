# Terminal Dirty Marking Optimization

**Rule:** Terminal escape sequence handlers MUST mark only affected rows dirty, not the entire screen.

## Problem

The terminal's `mark_all_dirty()` was being called unconditionally on EVERY CSI and ESC dispatch, causing the renderer to redraw all 24+ rows even for simple cursor movements. This created a ~24x performance penalty for escape-heavy workloads like ncurses applications.

## Solution

Classify CSI/ESC sequences by their screen impact:

### CSI Commands That DON'T Need Dirty Marking

| Command | Code | Reason |
|---------|------|--------|
| Cursor Up/Down/Forward/Back | A, B, C, D | Renderer tracks cursor row automatically |
| Cursor Next/Prev Line | E, F | Cursor movement only |
| Cursor Horizontal Absolute | G | Cursor movement only |
| Cursor Position | H, f | Cursor movement only |
| Vertical Position Absolute | d | Cursor movement only |
| Save/Restore Cursor | s, u | Cursor state only |
| Device Status Report | n | Query, no screen change |
| SGR (Set Graphics Rendition) | m | Attribute change, affects future writes |

### CSI Commands That Need Single-Row Dirty

| Command | Code | Reason |
|---------|------|--------|
| Erase Line (EL) | K | Only modifies cursor row |

### CSI Commands That Need mark_all_dirty()

| Command | Code | Reason |
|---------|------|--------|
| Erase Display (ED) | J | Can clear entire screen |
| Insert/Delete Lines | L, M | Shifts content across rows |
| Scroll Up/Down | S, T | Moves all content |
| Insert/Delete Chars | @, P | May affect line wrapping |
| Erase Chars | X | Content modification |
| Set Scroll Region | r | Changes scroll behavior |
| Mode changes | h, l | May switch alt screen |

### ESC Commands That DON'T Need Dirty Marking

| Command | Code | Reason |
|---------|------|--------|
| Save/Restore Cursor | 7, 8 | Cursor state only |
| Tab Set | H | Tab stop metadata |
| Character Set Selection | (, ) | Charset metadata |
| Line Attribute Variants | #3-6 | Metadata only |

## Implementation

In `kernel/tty/terminal/src/lib.rs`, the CsiDispatch and EscDispatch handlers now use match statements to classify commands and call the appropriate dirty marking:

```rust
// After handle_csi()
match final_char {
    b'A' | b'B' | b'C' | ... => {} // cursor - no marking
    b'm' => {} // SGR - no marking
    b'K' => self.renderer.mark_dirty(cursor_row), // single row
    _ => self.renderer.mark_all_dirty(), // everything else
}
```

## Performance Impact

For a typical ncurses application doing cursor positioning and attribute changes:
- **Before:** ~24 row renders per escape sequence
- **After:** 0-1 row renders per escape sequence
- **Improvement:** Up to 24x fewer pixel operations

— GraveShift: "Every CPU cycle spent redrawing unchanged rows is a cycle stolen from the user's program. The terminal should be invisible, not a bottleneck."
