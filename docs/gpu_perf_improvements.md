# GPU Rendering Performance Improvements

## Overview

Four-phase optimization that eliminates the compositor's biggest bottlenecks:
full-screen GPU flushes, per-pixel volatile writes, redundant scrollbar redraws,
and repeated viewport recomputations. Combined effect: cursor blinks go from
flushing ~3MB to ~2KB, idle frames cost zero GPU bandwidth, and scrollbar draws
drop from ~30 fill_rect calls to zero when nothing changed.

## Phase 1: DirtyRect Tracking + Regional GPU Flush

**Problem:** The compositor flushed the ENTIRE 1024x768x4 = 3MB framebuffer to
VirtIO-GPU on every tick (~100Hz), even for a cursor blink that touched 12x19 pixels.

**Solution:** Added `DirtyRect` struct to the Compositor that tracks the bounding
box of all pixels written since the last flush. Every function that touches the
hardware framebuffer (`blit_vt_to_hw`, `fill_hw_rect`, `draw_scrollbars`,
`draw_statusbar`, mouse cursor erase/redraw) extends the dirty rect. At flush
time, `hw_fb.flush_region(x, y, w, h)` sends only the changed rectangle to
VirtIO-GPU's `transfer_to_host_2d` + `resource_flush` commands.

**Files:**
- `kernel/tty/compositor/src/lib.rs` -- DirtyRect struct, integration into all draw paths
- `kernel/graphics/fb/src/framebuffer.rs` -- flush_region already existed on LinearFramebuffer

**Impact:**
- Cursor-only frame: ~2KB flush vs ~3MB (1500x reduction)
- Scrollbar hover: ~16x400x4 = 25KB flush vs ~3MB (120x reduction)
- Full redraw: no change (still flushes full screen, as it should)

## Phase 2: LinearFramebuffer fill_rect Row-Batching

**Problem:** `LinearFramebuffer::fill_rect` did per-pixel volatile writes. Each
volatile write to uncacheable MMIO costs ~1000 cycles. A 200x16 scrollbar track =
3200 individual MMIO writes = 3.2M cycles.

**Solution:** Build one row of pixel data in a stack buffer (cacheable RAM, ~1
cycle/pixel), then blast the entire row to MMIO with a single
`copy_nonoverlapping`. Same pattern already used by `fill_hw_rect_static` in
the compositor. Stack buffer is 256px x 4bpp = 1KB; wider rects tile automatically.

**Files:**
- `kernel/graphics/fb/src/framebuffer.rs` -- LinearFramebuffer::fill_rect rewrite

**Impact:** MMIO transaction count reduced by W-fold (16x for scrollbars, 200x for
wide rects). Real-world: scrollbar track fill drops from ~3.2M cycles to ~200K cycles.

## Phase 3: Scrollbar Fixes

### 3a: ScrollContent Caching

**Problem:** Every `draw_scrollbars()` call queried terminal state and rendered all
~30 fill_rect calls per scrollbar, even when the thumb position hadn't changed.

**Solution:** Cache the last `ScrollContent` (total, visible, position) per VT.
Compare before drawing -- if content state and all part states (arrow, thumb) are
unchanged, skip the entire scrollbar draw. Also added `PartialEq` to `ScrollContent`.

**Files:**
- `kernel/tty/compositor/src/lib.rs` -- `cached_scroll_content` field, comparison in draw_scrollbars
- `kernel/tty/compositor/src/scrollbar.rs` -- PartialEq derive on ScrollContent

### 3b: Catch-All Drag Mode Fix

**Problem:** The catch-all `_ =>` arm in the vertical/horizontal scrollbar mouse press
handler entered `ScrollbarDrag` state for ANY sub-zone hit, including
`ScrollbarHitZone::None` and `ScrollbarHitZone::Corner`. This caused phantom drag
behavior when clicking near scrollbar edges.

**Solution:** Changed catch-all to simply consume the event without entering drag mode.
Track area clicks (TrackBefore/TrackAfter) already handle page scroll. Thumb clicks
handle real drag. The catch-all only fires for corner/none hits that don't need drag.

**Files:**
- `kernel/tty/compositor/src/lib.rs` -- both VScrollbar and HScrollbar catch-all arms

## Phase 4: Viewport Caching

**Problem:** `compute_viewports()` was called 4-6 times per tick: once each in
`composite()`, `update_scrollbar_rects()`, `draw_scrollbars()`, `visible_vts()`,
`draw_borders()`, and `try_flush_pending_layout()`. Pure math but wasteful repetition.

**Solution:** Cache `compute_viewports()` result in `Compositor::cached_viewports`.
Invalidated only on actual layout changes (resize, VT add/remove, OSK toggle) via
`invalidate_viewport_cache()`. All consumers read from the cache.

**Files:**
- `kernel/tty/compositor/src/lib.rs` -- `cached_viewports`, `viewport_generation`,
  `invalidate_viewport_cache()`, all consumer sites updated

**Impact:** 4-6 viewport computations per tick reduced to 0 (uses cache). Recomputation
only on layout change events (rare -- maybe once per session).
