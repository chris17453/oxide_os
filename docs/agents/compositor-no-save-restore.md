# Compositor: No Save/Restore for Mouse Cursor

## Rule
The compositor MUST NOT use save_buffer/restore patterns for the mouse cursor.
Always reblit from the VT backing buffer before drawing the cursor sprite.

## Why
The save_buffer pattern captures hw_fb pixels at the cursor position during `redraw()`.
Between ticks, VT content changes (terminal writes, cursor blink, scrolling). When
`erase()` pastes the stale saved pixels back to hw_fb, the cursor carries a ghost
rectangle of old content. The initial save at `(save_x:0, save_y:0)` with an all-zero
buffer makes this especially visible as a black rectangle dragged around the screen.

The fundamental problem: save_buffer is a **point-in-time snapshot** of hw_fb, but
the VT backing buffer is the **source of truth** for what should be under the cursor.
Any approach that caches hw_fb pixels will go stale.

## Correct Pattern
In `compositor::tick()`:
1. Track cursor bounds for dirty_rect
2. `composite()` — blit VT backing buffers to hw_fb (overwrites old cursor area if VT dirty)
3. `reblit_cursor_area()` — always pull fresh pixels from VT backing buffer over cursor bbox
4. `cursor.redraw()` — stamp the sprite on top of fresh content
5. `flush_region()` — push dirty rect to GPU

## Never Do
- `cursor.erase()` with save_buffer restore
- `cursor.save_under()` before drawing (captures stale hw_fb state)
- Any pattern that reads from hw_fb and writes it back later

## The Linux Way
X11/Wayland compositors don't do save/restore either. They composite from
backing stores (damage tracking) and draw the cursor as a final overlay pass.
The backing store is always authoritative.

— NeonVale
