# Terminal Write + Render Architecture (Per-Glyph)

**Author:** GraveShift, SoftGlyph
**Scope:** `kernel/tty/terminal/src/lib.rs` — `write()`, `tick()`, `flush()`
**Scope:** `kernel/tty/terminal/src/renderer.rs` — `render_cell()`, `scroll_up_pixels()`

## The Rule

**`terminal::write()` renders each character to framebuffer inline as it's processed
(per-glyph, like Linux's `fbcon_putcs()`). Scrolls use `fb.copy_rect()` pixel memmove.
CSI bulk ops (ED/IL/DL) fall back to dirty-row render. Timer ISR `tick()` handles
cursor blink + catch-up for CSI dirty rows.**

## How Linux Does It

Linux's `do_con_write()` in `drivers/tty/vt/vt.c`:

```
write(fd, buf, len)
  → tty_write()
    → do_con_write()              // acquires console_lock (semaphore)
      → hide_cursor()             // erase cursor from framebuffer
      → for each byte:
          → vc_con_write_normal()  // update screen buffer
          → con_flush()           // calls fbcon_putcs() → pixels to framebuffer NOW
      → con_flush()              // final flush
      → set_cursor()             // draw cursor back
    // releases console_lock
```

Linux renders synchronously because `fbcon_putcs()` **paints individual character
glyphs** at exact pixel positions. Cost = proportional to characters written.
A 50-byte write paints ~50 glyphs. Cheap.

## How We Do It (Per-Glyph, matching Linux)

```
write(data)
  → TERMINAL.lock()                    // single lock (like console_lock)
    → push_selection_to_renderer()     // selection state for highlight
    → erase_cursor()                   // hide cursor before writing
    → for each byte:
        → process_byte(byte)
          → Action::Print(ch):
              → put_char(ch, buffer)   // update screen buffer cell
              → render_cell(row, col)  // paint glyph to framebuffer NOW
          → LF (0x0A):
              → linefeed()             // scroll buffer if at bottom
              → scroll_up_pixels()     // fb.copy_rect() pixel memmove
    → if has_dirty():                  // CSI bulk ops left dirty rows?
        → render()                     // full dirty-row render (fallback)
    → else:
        → paint_cursor()              // draw cursor at final position
        → flush_fb()                  // hardware flush
  // release lock
```

Timer ISR at 30fps (`tick()`):
```
tick()
  → TERMINAL.try_lock()               // like fb_flashcursor → console_trylock
    → if needs_render: render()        // catch-up for CSI dirty rows
    → else: cursor blink               // toggle cursor blink only
  // if lock held by writer: skip (next tick in 33ms)
```

## Performance Impact

| Operation | Before (dirty-row) | After (per-glyph) | Speedup |
|-----------|--------------------|--------------------|---------|
| Print 1 char | Deferred to tick | 1 cell (~128 pixels) | Instant |
| 50-byte println + scroll | 24 rows × 80 cells | 50 glyphs + 1 memmove | ~40x fewer pixels |
| Boot (500 println's) | 500 × 245K pixels | 500 × (50 glyphs + memmove) | ~40x |
| curses-demo 28KB frame | Timer-deferred | ~2000 glyphs inline | Synchronous |

## Key Methods

### `render_cell(buffer, row, col)` — The fbcon_putcs equivalent
Paints exactly ONE cell to framebuffer. Reuses `render_cell_inner()` for all the
glyph rendering, font resolution, bold/italic/underline/selection logic. Zero new
rendering code — just a public entry point to the existing inner renderer.

### `scroll_up_pixels(lines, bg_color)` — Pixel memmove
Uses `fb.copy_rect()` to shift all scanlines up by `lines * font_height` pixels.
Clears the vacated bottom area with bg_color. Cost: one memcpy (~4MB for 1 line
scroll on 1280-wide@32bpp framebuffer) instead of repainting 50×160=8000 cells.

**Critical: Framebuffer is WB-cached** (no PCD/PWT flags set in page tables).
`copy_rect` reads from CPU cache, not slow MMIO. 150 boot scrolls = ~590MB through
L3 cache @ ~20GB/s = ~31ms total. This was initially assumed to be MMIO reads
(catastrophically slow), but the page table analysis proved WB caching.

### `erase_cursor(buffer)` / `paint_cursor(buffer, cursor)` — Cursor bookkeeping
Erase old cursor position by repainting the cell underneath. Paint new cursor at
final position. Called once at start/end of write, not per-byte.

## The Three Rendering Eras

### Era 1: Timer-deferred full-row render
`write()` updated buffer only. `tick()` rendered ALL dirty rows at 30fps.
Problem: tight write loops (curses-demo) held the lock so long that tick() couldn't
get in. Screen froze.

### Era 2: Single-lock buffer-only write (temporary fix)
Removed chunking, single lock per write. Timer had ample gap between writes.
Problem: still 33ms latency for every screen update.

### Era 3: Per-glyph synchronous render (current — Linux style)
Each `Action::Print` paints its glyph inline. Scrolls use pixel memmove. Characters
appear on screen as they're processed — zero latency, proportional cost.

## Rules

### 1. Print/LF render inline — no dirty flags
`Action::Print` calls `render_cell()`. LF scroll calls `scroll_up_pixels()`.
Neither sets dirty flags or `needs_render`. The pixels are already on screen.

### 2. CSI bulk ops use dirty-row fallback
CSI commands that affect multiple rows (J=ED, L=IL, M=DL, S=SU, T=SD, @=ICH,
P=DCH, X=ECH) still call `mark_all_dirty()` or `mark_dirty(row)`. After the byte
loop, `write()` checks `has_dirty()` and does a full `render()` if needed.

### 3. Cursor erased once at write start, painted once at write end
`erase_cursor()` at the top of `write()`. `paint_cursor()` at the bottom.
Not per-byte — that would be wasteful (cursor invisible during write anyway).

### 4. tick() = cursor blink + catch-up
Timer ISR `tick()` handles cursor blink (toggle + repaint) and renders any
lingering dirty rows from CSI ops that write() already flushed (safety net).
Uses `try_lock()` — ISR safe.

### 5. Single lock per write (unchanged)
One `TERMINAL.lock()` for the entire data buffer. Short hold time because we're
rendering proportional to characters, not full screen.

## Affected Files

- `kernel/tty/terminal/src/lib.rs` — `write()` (per-glyph), `tick()` (cursor blink)
- `kernel/tty/terminal/src/renderer.rs` — `render_cell()`, `scroll_up_pixels()`,
  `paint_cursor()`, `erase_cursor()`, `flush_fb()`, `has_dirty()`

## Boot Message Display (BootWriter)

Kernel boot messages (`writeln!(writer, ...)` in `init.rs`) go to both serial AND
terminal via `BootWriter`. Before terminal init, `console_enabled = false` (serial
only). After `terminal::clear()`, `console_enabled = true` routes through
`console::console_write()` → `terminal::write()` (per-glyph rendering).

**NOT** registered with os_log. Routing all kernel `println!` to the display would
pollute userspace output with debug noise. BootWriter is init.rs-only — subsystem
debug stays on serial where it belongs.

Getty's `clear_screen()` (`\x1b[2J\x1b[H`) wipes boot messages on login — same as
Linux clearing dmesg output. This is expected behavior.

## See Also

- `docs/agents/stdout-serial-separation.md` — serial removed from stdout path
- `docs/agents/terminal-dirty-marking.md` — dirty-row tracking rules
