# Complete compositor and scrollbar architecture analysis for Win95-style scrollbar widget

📌 Pinned

| Field | Value |
|-------|-------|
| ID | `2e1a536ac9b1` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-08T16:13:54.280921+00:00 |
| Accessed | 0 times |
| Keywords | compositor, scrollbar, eventsystem, viewport, rendering, mouseevents, layoutmanager |
| Files | `kernel/tty/compositor/src/lib.rs`, `kernel/tty/compositor/src/events.rs`, `kernel/tty/compositor/src/layout.rs`, `kernel/tty/compositor/src/backing_fb.rs`, `kernel/src/console.rs`, `kernel/tty/terminal/src/lib.rs` |

## Content

## COMPOSITOR ARCHITECTURE - OXIDE OS

### Key Files and Structure

**Main Compositor (kernel/tty/compositor/src/lib.rs):**
- Global `COMPOSITOR: Mutex<Option<Compositor>>` - singleton instance
- Main struct `Compositor` contains:
  - `hw_fb: Arc<dyn Framebuffer>` - hardware framebuffer (ONLY composer writes to it)
  - `vt_buffers: [Option<Arc<BackingFramebuffer>>; MAX_VTS]` - per-VT pixel canvases
  - `vt_geometries: [Option<ViewportGeometry>; MAX_VTS]` - screen positions and sizes
  - `layout: LayoutManager` - viewport manager
  - `cell_width/height: u32` - font metrics (8x16 default)
  - `vt_scrollbar_flags: [ScrollbarFlags; MAX_VTS]` - per-VT scrollbar visibility
  - `scrollbar_track_color/thumb_color/active_color: u32` - ARGB colors
  - `event_handler: EventHandler` - mouse state machine
  - `mouse_cursor: Option<MouseCursor>` - cursor state and redraw

**Scrollbar Dimensions (layout.rs):**
- `SCROLLBAR_WIDTH: u32 = 10` pixels
- `SCROLLBAR_HEIGHT: u32 = 10` pixels
- `SCROLLBAR_THUMB_MIN: u32 = 20` pixels minimum thumb size

**Scrollbar Flags (layout.rs):**
```rust
pub struct ScrollbarFlags {
    pub vscroll: bool,  // vertical scrollbar visible
    pub hscroll: bool,  // horizontal scrollbar visible
}
```

### Scrollbar Rendering (lib.rs:443-528)

**draw_scrollbars()** method:
1. Queries each visible VT's scrollbar state via `terminal::get_scrollbar_state(vt_idx)`
2. For vertical scrollbars:
   - Track position: right edge, width=SCROLLBAR_WIDTH, height=usable_height
   - Thumb calculation:
     - `thumb_h = ((visible * track_h) / total).max(SCROLLBAR_THUMB_MIN).min(track_h)`
     - Position: based on `scroll_offset` (0=bottom, higher=scrolled up)
     - Formula: `pos_from_top = (total - visible) - scroll_offset`
     - Pixel position: `track_y + (pos_from_top * usable_track) / scrollable`
3. For horizontal scrollbars:
   - Track position: bottom edge, width=usable_width, height=SCROLLBAR_HEIGHT
   - Similar proportional thumb calculation
   - Position based on `h_scroll_offset` (columns scrolled left)
4. Corner block drawn where both bars meet (same color as track)

**Colors:**
- Track: `0xFF1A1A1A` (near-black)
- Thumb: `0xFF555555` (medium gray)
- Active (on drag): `0xFF00AACC` (cyan)
- All colors are ARGB format, converted to pixel format in fill_hw_rect()

### Event System (events.rs)

**HitZone enum** - what mouse hit:
- `VtContent { vt: usize }` - terminal content area
- `VScrollbar { vt: usize }` - vertical scrollbar track
- `HScrollbar { vt: usize }` - horizontal scrollbar track
- `ScrollbarCorner { vt: usize }` - where both bars meet
- `Border` - split-mode divider
- `None` - outside any region

**MouseAction enum** - what to do:
- `Consumed` - compositor handled it
- `ForwardToTerminal { vt: usize }` - send to terminal
- `Nothing` - no action

**DragState struct** (lines 50-65):
- `vt: usize` - which VT
- `vertical: bool` - which scrollbar
- `start_pos: i32` - where drag started
- `start_offset: usize` - scroll position at drag start
- `track_length: usize` - track length in pixels
- `total_content: usize` - total scrollable content
- `visible_content: usize` - visible area

**hit_test() function** (lines 102-165):
- Tests point (x,y) against all visible VT regions
- Returns most specific zone (scrollbar > content > border > none)
- For vertical: checks if `ux >= sb_x && ux < vp_right && uy >= sb_y_top && uy < sb_y_bot`
- For horizontal: checks if `uy >= sb_y && uy < vp_bottom && ux >= sb_x_left && ux < sb_x_right`

### Scrollbar Event Handling (lib.rs:650-851)

**handle_mouse_press()** - lines 651-761:
- VScrollbar hit: creates DragState, does jump-to-position
  - Jump calculation: `line_from_top = (click_frac * scrollable) / track_h`
  - Then calls `terminal::scroll_to_line(vt, offset_from_bottom)`
- HScrollbar hit: similar for horizontal
  - Calls `terminal::scroll_to_col(vt, col)`

**handle_mouse_move()** - lines 790-830:
- During ScrollbarDrag:
  - Vertical: `delta_lines = (delta_px * scrollable) / track_length`
    - `new_offset = (start_offset - delta_lines).max(0).min(scrollable)`
    - Calls `terminal::scroll_to_line(vt, new_offset)`
  - Horizontal: similar formula for columns

**handle_mouse_wheel()** - lines 833-851:
- Shift+wheel = horizontal scroll (compositor handles)
- Normal wheel = vertical scroll (forwarded to terminal for decision)

### Viewport Geometry (layout.rs:70-128)

```rust
pub struct ViewportGeometry {
    pub screen_x: u32,           // position on hardware FB
    pub screen_y: u32,
    pub total_width: u32,        // full viewport including chrome
    pub total_height: u32,
    pub border_top: u32,         // chrome sizes
    pub border_bottom: u32,
    pub border_left: u32,
    pub border_right: u32,
    pub usable_width: u32,       // content area (total - chrome)
    pub usable_height: u32,
    pub text_cols: u32,          // grid dimensions
    pub text_rows: u32,
}
```

Scrollbars eat into borders:
- Vertical scrollbar: `border_right += SCROLLBAR_WIDTH` if vscroll flag set
- Horizontal scrollbar: `border_bottom += SCROLLBAR_HEIGHT` if hscroll flag set

### Terminal Scrollbar State (terminal/lib.rs:86-97)

```rust
pub struct ScrollbarState {
    pub scroll_offset: usize,      // 0=bottom (live), >0=scrolled up
    pub scrollback_len: usize,     // lines of history
    pub h_scroll_offset: usize,    // columns scrolled left (no-wrap mode)
    pub max_line_width: usize,     // longest line width in scrollback
    pub rows: u32,                 // viewport height in lines
    pub cols: u32,                 // viewport width in columns
    pub wrap_mode: bool,           // true=wrap, false=horizontal scroll
}
```

Queried via `terminal::get_scrollbar_state(vt_idx)` which is ISR-safe (try_lock).

### Layout Manager (layout.rs:131-357)

```rust
pub struct LayoutManager {
    layout: Layout,                // Fullscreen/HSplit/VSplit/Quad
    slots: [usize; MAX_TILES],    // which VT in each slot
    focused_slot: usize,
    screen_width/height: u32,
    prev_layout/prev_slots: for Alt+Enter toggle
}
```

Key methods:
- `compute_viewports() -> [(usize, Viewport); MAX_TILES]` - get screen rects for each tile
- `recompute_geometries()` - apply chrome/scrollbar to get full ViewportGeometry
- Layouts:
  - Fullscreen: single VT fills screen
  - HSplit: top/bottom 2x1
  - VSplit: left/right 1x2
  - Quad: 2x2 grid
  - Borders between tiles: 2px gap

### Console Integration (kernel/src/console.rs)

**terminal_tick()** function (lines 64-370) runs at ~100Hz:
1. Polls input devices (keyboard, mouse, tablet)
2. Drains mouse/tablet events:
   - Absolute (tablet): scales 0..32767 to screen pixels
   - Relative (mouse): accumulates deltas
   - Wheel: tracks wheel delta
3. Routes events through compositor:
   - Button press: `compositor::handle_mouse_press(button, x, y)`
   - Button release: `compositor::handle_mouse_release(button, x, y)`
   - Motion: `compositor::handle_mouse_move(x, y)`
   - Wheel: `compositor::handle_mouse_wheel(delta, x, y, shift_held)`
4. Compositor returns MouseAction:
   - Consumed: compositor handled (scrollbar, border)
   - ForwardToTerminal: content area, forward to terminal
   - Nothing: idle motion
5. Calls `compositor::tick()` to blit + draw scrollbars + flush GPU

**Shift handling** (line 181): Shift key state tracked to differentiate wheel directions

### Compositing Pipeline (lib.rs:345-403, 901-930)

**composite()** method:
1. Erase mouse cursor
2. For each visible VT:
   - Check if dirty (atomic flag)
   - Blit VT buffer to hardware FB at viewport position
3. Draw borders between tiles
4. Draw scrollbars (calls draw_scrollbars())
5. DON'T flush here (see tick())

**tick()** public API (lines 901-930):
1. Fast path: check dirty atomics (FULL_REDRAW, CURSOR_DIRTY, VT_DIRTY)
2. If anything dirty:
   - `try_lock()` compositor
   - Call `composite()`
   - Draw vkbd overlay
   - Draw mouse cursor
   - Call `hw_fb.flush()` - SINGLE GPU flush for all layers

### Backing Framebuffer (backing_fb.rs)

```rust
pub struct BackingFramebuffer {
    virt_ptr: *mut u8,            // virtual address (identity mapped)
    phys_base: u64,               // physical address
    num_pages: usize,
    width/height/stride: u32,
    format: PixelFormat,
    buf_size: usize,
}
```

- Allocated from buddy allocator (not heap) via `mm().alloc_contiguous()`
- ~4MB per VT at 1024x768 RGBA
- Freed on Drop via `mm().free_contiguous()`
- Implements Framebuffer trait
- No flush() needed - compositor blits it to hardware

### Key Constants

- `MAX_VTS: usize = 6` - max VT slots
- `MAX_TILES: usize = 4` - max visible tiles (Quad layout)
- `DEFAULT_CELL_WIDTH: u32 = 8` - PSF2 font
- `DEFAULT_CELL_HEIGHT: u32 = 16`

### Dirty Flag System

- Per-VT: `VT_DIRTY: [AtomicBool; MAX_VTS]` - set by writers, cleared by composite
- Global: `FULL_REDRAW: AtomicBool` - on layout change, VT switch
- Cursor: `CURSOR_DIRTY: AtomicBool` - on mouse move
- Vkbd: checked via `vkbd::is_visible()`

### Color System

ARGB format (32-bit):
- Bit 24-31: Alpha
- Bit 16-23: Red
- Bit 8-15: Green
- Bit 0-7: Blue

Converted in `fill_hw_rect()` to match hardware format (BGRA8888 or RGBA8888).

## SCROLLBAR WIDGET IMPLEMENTATION PLAN

### Current State
- Simple 10px flat rectangle thumb on track
- No decorative elements
- Proportional sizing only
- No wheel support (handled at console level)

### Win95-Style Additions
1. 3D beveled borders on thumb (raised when idle, pressed when dragging)
2. Up/Down arrow buttons at top/bottom (for vertical)
3. Left/Right arrow buttons at left/right (for horizontal)
4. Better visual feedback on hover/drag
5. Grip texture on thumb
6. Track has slight gradient/texture
7. Corner button (like Motif window managers)

### Implementation Strategy
1. Expand scrollbar dimensions to accommodate arrows
2. Create scrollbar_widget.rs in compositor crate
3. Add ScrollbarWidgetState to Compositor
4. Modify draw_scrollbars() to call widget drawing functions
5. Expand event handling for arrow button clicks
6. Add hover detection for visual feedback
7. Store dragging thumb vs clicking arrows differently

## CRITICAL DESIGN NOTES

1. **Scrollbar always at edges:** Vertical = right edge + border_right, Horizontal = bottom edge + border_bottom
2. **Thumb calculation:** Proportional to (visible / total). Must never go below SCROLLBAR_THUMB_MIN (20px).
3. **Offset semantics:** 
   - Vertical: scroll_offset=0 means at BOTTOM (live), higher = scrolled UP into history
   - Horizontal: h_scroll_offset=0 means at LEFT (normal), higher = scrolled RIGHT
4. **Jump-to-position:** Click on track jumps thumb to that position proportionally
5. **Drag calculation:** Track length, total content, visible content form proportional ratio
6. **Colors are ARGB:** Converted to pixel format per-framebuffer in fill_hw_rect()
7. **ISR-safe:** All scrollbar state queries use try_lock, event handling from ISR context via console.rs
8. **Dirty marking:** After any scroll operation, call mark_dirty(vt) to trigger redraw

