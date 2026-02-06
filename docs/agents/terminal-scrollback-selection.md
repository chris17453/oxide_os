# Terminal Scrollback and Selection Rendering Rules

## Scrollback Rendering

When `scroll_offset > 0`, the terminal is viewing history. The renderer must composite
scrollback buffer lines with primary buffer lines into a temporary ScreenBuffer.

### Content Layout
```
Full content = [scrollback_line_0 ... scrollback_line_N] [primary_row_0 ... primary_row_M]
                    oldest ───────────────> newest          top ────────> bottom

Viewport start = total_lines - scroll_offset - visible_rows
```

### Rules
1. **Always hide cursor when scrolled up** — cursor lives in primary buffer which may not be visible
2. **Alternate screen ignores scrollback** — full-screen apps (vim, top) don't use scrollback
3. `render()` resets `scroll_offset = 0` — new content always snaps to live view
4. `render_with_scrollback()` builds a temp ScreenBuffer for the composited viewport
5. `scroll_view_up()` / `scroll_view_down()` call `render_with_scrollback()`, not `render()`
6. Max scrollback offset = `scrollback.len()` (can't scroll past oldest line)
7. `ScrollbackBuffer::get(index)` uses 0=oldest; `get_from_end(offset)` uses 0=newest

## Selection Highlighting

Selection state is pushed from `TerminalEmulator::selection` to `Renderer::selection`
before each render call via `push_selection_to_renderer()`.

### Rules
1. **Push selection BEFORE borrowing buffer** — avoids borrow checker conflict
2. Selection coordinates are `(start_col, start_row, end_col, end_row)` — may be in any order
3. Renderer normalizes direction: if start > end, swap for iteration
4. Selected cells render with fg/bg swapped (reverse video)
5. Single-row selection: only cells between start_col and end_col are highlighted
6. Multi-row selection: first row from start_col to end, last row from 0 to end_col, middle rows fully selected
7. `clear_selection()` must also call `renderer.set_selection(None)` to clear the highlight
8. Selection highlight applies after bold brightening (so bold+selected text stays readable)

## Borrow Safety
- `push_selection_to_renderer()` borrows `self.selection` (Copy type) and `self.renderer` (mutable)
- Buffer borrow (`self.primary` or `self.alternate`) conflicts with `&mut self` for push_selection
- Always call `push_selection_to_renderer()` FIRST, then borrow the buffer
