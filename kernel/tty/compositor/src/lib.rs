//! Tiling VT Compositor for OXIDE OS
//!
//! — SableWire: one ring to blit them all, one ring to find them,
//!   one ring to bring them all, and on the framebuffer bind them.
//!
//! Every VT gets its own backing pixel buffer. The compositor is the ONLY
//! thing that writes to the hardware framebuffer. Terminal renderers, /dev/fb0,
//! graphics apps — they all paint into their VT's backing buffer. The compositor
//! blits visible buffers into viewport rectangles on the physical display.

#![no_std]

extern crate alloc;

pub mod backing_fb;
pub mod events;
pub mod layout;
pub mod scrollbar;

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use spin::Mutex;

use backing_fb::BackingFramebuffer;
use events::{EventHandler, DragState, HitZone, MouseState};
use fb::Framebuffer;
use layout::{Layout, LayoutManager, ScrollbarFlags, Viewport, ViewportGeometry, MAX_VTS, MAX_TILES,
             SCROLLBAR_WIDTH, SCROLLBAR_HEIGHT};
use scrollbar::{Scrollbar, Orientation, ScrollContent, ScrollbarHitZone, PartState};

/// VT display mode — text terminal or raw graphics
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VtMode {
    /// Terminal emulator active — ANSI parsing, text rendering, scrollback
    Text,
    /// Raw graphics mode — /dev/fb0 writes go here, no terminal processing
    Graphics,
}

/// Per-VT dirty flags — set by writers, cleared by compositor
/// — SableWire: atomics because terminal write + compositor blit can race.
/// Dirty flag = "this VT has new pixels since last composite." That's it.
/// Sized to MAX_VTS — unused slots stay false forever, zero cost.
static VT_DIRTY: [AtomicBool; MAX_VTS] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];

/// Global full-redraw flag — set on layout change or VT switch
static FULL_REDRAW: AtomicBool = AtomicBool::new(true);

/// Active VT for input routing (mirrors vt::ACTIVE_VT but compositor-managed)
static COMPOSITOR_FOCUS_VT: AtomicUsize = AtomicUsize::new(0);

/// — SoftGlyph: cursor position changed — needs redraw even if no VT is dirty
static CURSOR_DIRTY: AtomicBool = AtomicBool::new(false);

/// — EchoFrame: scrollbar visual state changed — needs redraw without full VT blit
static SCROLLBAR_DIRTY: AtomicBool = AtomicBool::new(false);

/// — SoftGlyph: lock-free mouse init flag. Set once when compositor creates the
/// cursor. ISR code checks this instead of try_lock() on the compositor mutex —
/// try_lock fails when tick() holds the lock, which killed the entire mouse path.
static MOUSE_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Callback for notifying the VT layer about per-VT winsize changes.
/// — GlassSignal: compositor can't depend on vt (circular), so the kernel
/// registers this callback during init. Args: (vt_num, rows, cols, xpixel, ypixel).
type WinsizeCallbackFn = fn(usize, u16, u16, u16, u16);
static mut WINSIZE_CALLBACK: Option<WinsizeCallbackFn> = None;

/// Register the winsize callback. Called once during kernel init.
///
/// # Safety
/// Must be called during single-threaded initialization.
pub unsafe fn set_winsize_callback(f: WinsizeCallbackFn) {
    unsafe { WINSIZE_CALLBACK = Some(f); }
}

/// The global compositor instance
static COMPOSITOR: Mutex<Option<Compositor>> = Mutex::new(None);

/// — GlassSignal: Default font cell dimensions (PSF2 8x16). Used for text grid
/// calculations. Updated if a different font is loaded.
const DEFAULT_CELL_WIDTH: u32 = 8;
const DEFAULT_CELL_HEIGHT: u32 = 16;

/// Compositor state
pub struct Compositor {
    /// The real hardware framebuffer — ONLY the compositor touches this
    hw_fb: Arc<dyn Framebuffer>,
    /// Per-VT virtual framebuffers (pixel canvases) — allocated lazily on first use.
    /// — SableWire: only VT0 gets a VFB at init. The rest spawn on demand
    /// when you split the screen or switch VTs. Sized to viewport usable area,
    /// allocated from buddy allocator physical frames, freed on Drop. No waste.
    vt_buffers: [Option<Arc<BackingFramebuffer>>; MAX_VTS],
    /// Per-VT display mode
    vt_modes: [VtMode; MAX_VTS],
    /// Per-VT viewport geometries — None for off-screen VTs.
    /// — GlassSignal: single source of truth for VFB dimensions, text grid sizes,
    /// and compositor blit positions. Recomputed on every layout change.
    vt_geometries: [Option<ViewportGeometry>; MAX_VTS],
    /// Layout manager — viewport geometry
    layout: LayoutManager,
    /// Font cell dimensions for text grid calculations
    cell_width: u32,
    cell_height: u32,
    /// Border color for split-mode dividers (cyan highlight)
    border_color: u32,
    /// Focus highlight color
    focus_color: u32,
    /// — GlassSignal: per-VT scrollbar visibility flags
    vt_scrollbar_flags: [ScrollbarFlags; MAX_VTS],
    /// — EchoFrame: Win95-style scrollbar widgets, one vertical + one horizontal per VT
    vscrollbars: [Scrollbar; MAX_VTS],
    hscrollbars: [Scrollbar; MAX_VTS],
    /// — GlassSignal: mouse event handler — owns drag state, hit-testing, the works
    event_handler: EventHandler,
    /// — SoftGlyph: mouse cursor — compositor draws it last, on top of everything.
    /// Position tracked here, save/restore for cursor-only movement (no VT dirty).
    mouse_cursor: Option<fb::mouse::MouseCursor>,
}

impl Compositor {
    /// Create a new compositor. Only VT0 gets a VFB at init —
    /// the rest are allocated on demand when split/switch triggers them.
    /// — NeonRoot: saves ~15MB at boot. Buffers appear when you need them.
    fn new(hw_fb: Arc<dyn Framebuffer>) -> Self {
        let width = hw_fb.width();
        let height = hw_fb.height();
        let format = hw_fb.format();

        os_log::println!("[COMP] init {}x{} stride={} bpp={} (lazy alloc, VT0 only)",
            width, height, hw_fb.stride(), format.bytes_per_pixel() * 8);

        let layout = LayoutManager::new(width, height);
        let cell_width = DEFAULT_CELL_WIDTH;
        let cell_height = DEFAULT_CELL_HEIGHT;

        // — GlassSignal: VT0 gets vertical scrollbar track always reserved
        let mut vt_scrollbar_flags = [ScrollbarFlags::default(); MAX_VTS];
        vt_scrollbar_flags[0] = ScrollbarFlags { vscroll: true, hscroll: false };

        // — GlassSignal: compute initial geometries (Fullscreen, VT0 only)
        let vt_geometries = layout.recompute_geometries(cell_width, cell_height, &vt_scrollbar_flags);

        // — NeonRoot: VT0's VFB sized to its usable viewport area, not full screen.
        // In fullscreen mode with no chrome, usable == total == hw_fb dimensions.
        let vt0_geom = vt_geometries[0].unwrap();
        let vt0_stride = vt0_geom.usable_width * format.bytes_per_pixel() as u32;
        let vt0_buf = BackingFramebuffer::new(
            vt0_geom.usable_width, vt0_geom.usable_height,
            vt0_stride, format,
        );
        os_log::println!("[COMP] VT0 buffer: {}KB ({}x{})",
            vt0_buf.size() / 1024, vt0_geom.usable_width, vt0_geom.usable_height);

        let mut vt_buffers: [Option<Arc<BackingFramebuffer>>; MAX_VTS] =
            core::array::from_fn(|_| None);
        vt_buffers[0] = Some(Arc::new(vt0_buf));

        // — GlassSignal: border colors — dark gray divider, cyan focus highlight
        let border_color = 0xFF333333; // dark gray ARGB
        let focus_color = 0xFF00AACC;  // cyan ARGB

        // — EchoFrame: create scrollbar widget instances for each VT
        let vscrollbars: [Scrollbar; MAX_VTS] = core::array::from_fn(|_| Scrollbar::new(Orientation::Vertical));
        let hscrollbars: [Scrollbar; MAX_VTS] = core::array::from_fn(|_| Scrollbar::new(Orientation::Horizontal));

        let mut comp = Compositor {
            hw_fb,
            vt_buffers,
            vt_modes: [VtMode::Text; MAX_VTS],
            vt_geometries,
            layout,
            cell_width,
            cell_height,
            border_color,
            focus_color,
            vt_scrollbar_flags,
            vscrollbars,
            hscrollbars,
            event_handler: EventHandler::new(),
            mouse_cursor: Some(fb::mouse::MouseCursor::new(width, height)),
        };
        // — EchoFrame: position scrollbar widgets based on initial geometry
        comp.update_scrollbar_rects();
        comp
    }

    /// Ensure a VT has a VFB, allocating one on demand sized to its viewport.
    /// — SableWire: the lazy allocation hot path. First split/switch to a VT
    /// triggers a buddy alloc sized to viewport. Subsequent accesses are free.
    /// Returns true if the buffer exists (or was just created).
    fn ensure_vt_buffer(&mut self, vt_num: usize) -> bool {
        if vt_num >= MAX_VTS { return false; }
        if self.vt_buffers[vt_num].is_some() { return true; }

        // — GlassSignal: new text-mode VT gets vertical scrollbar track reserved
        if self.vt_modes[vt_num] == VtMode::Text {
            self.vt_scrollbar_flags[vt_num].vscroll = true;
        }

        // — GlassSignal: size VFB to viewport usable area if geometry exists,
        // otherwise fall back to full screen (off-screen VT being accessed early)
        let (w, h) = if let Some(geom) = self.vt_geometries[vt_num] {
            (geom.usable_width, geom.usable_height)
        } else {
            (self.hw_fb.width(), self.hw_fb.height())
        };
        let format = self.hw_fb.format();
        let stride = w * format.bytes_per_pixel() as u32;

        let buf = BackingFramebuffer::new(w, h, stride, format);
        os_log::println!("[COMP] VT{} buffer: {}KB ({}x{} on-demand)",
            vt_num, buf.size() / 1024, w, h);
        self.vt_buffers[vt_num] = Some(Arc::new(buf));
        true
    }

    /// Resize a VT's VFB to match new viewport dimensions.
    /// — GlassSignal: called on layout change. Allocates new buffer, copies
    /// old content clipped to min dimensions, frees old buffer. If alloc fails,
    /// keeps old buffer (graceful degradation — stale dimensions until next try).
    /// Returns the new Arc<BackingFramebuffer> if resize happened.
    fn resize_vt_buffer(&mut self, vt_num: usize, new_w: u32, new_h: u32) -> bool {
        if vt_num >= MAX_VTS { return false; }

        let old_buf = match self.vt_buffers[vt_num].take() {
            Some(b) => b,
            None => return false,
        };

        // — GlassSignal: skip if dimensions unchanged
        if old_buf.width() == new_w && old_buf.height() == new_h {
            self.vt_buffers[vt_num] = Some(old_buf);
            return false;
        }

        let format = old_buf.format();
        let new_stride = new_w * format.bytes_per_pixel() as u32;
        let new_buf = BackingFramebuffer::new(new_w, new_h, new_stride, format);

        // — GlassSignal: copy old content clipped to min(old, new) dimensions
        let copy_w = old_buf.width().min(new_w) as usize;
        let copy_h = old_buf.height().min(new_h) as usize;
        let bpp = format.bytes_per_pixel() as usize;
        let row_bytes = copy_w * bpp;
        let old_stride = old_buf.stride() as usize;
        let new_stride_usize = new_buf.stride() as usize;

        unsafe {
            let src = old_buf.raw_ptr();
            let dst = new_buf.raw_ptr() as *mut u8;
            for row in 0..copy_h {
                core::ptr::copy_nonoverlapping(
                    src.add(row * old_stride),
                    dst.add(row * new_stride_usize),
                    row_bytes,
                );
            }
        }

        os_log::println!("[COMP] VT{} resized: {}x{} → {}x{} ({}KB)",
            vt_num, old_buf.width(), old_buf.height(), new_w, new_h,
            new_buf.size() / 1024);

        // — GlassSignal: old_buf dropped here, frees physical frames
        self.vt_buffers[vt_num] = Some(Arc::new(new_buf));
        true
    }

    /// Recompute all VT geometries and resize VFBs to match.
    /// — GlassSignal: called on layout change, VT switch, screen resize.
    /// Returns list of (vt_num, old_geom, new_geom) for VTs that changed size,
    /// so the caller can trigger terminal resize + SIGWINCH.
    fn apply_layout_change(&mut self) -> [(usize, Option<ViewportGeometry>, Option<ViewportGeometry>); MAX_VTS] {
        let old_geometries = self.vt_geometries;
        self.vt_geometries = self.layout.recompute_geometries(self.cell_width, self.cell_height, &self.vt_scrollbar_flags);

        let changes: [(usize, Option<ViewportGeometry>, Option<ViewportGeometry>); MAX_VTS] =
            core::array::from_fn(|i| (i, old_geometries[i], self.vt_geometries[i]));

        for vt_num in 0..MAX_VTS {
            let new_geom = self.vt_geometries[vt_num];
            let old_geom = old_geometries[vt_num];

            match (old_geom, new_geom) {
                (_, Some(geom)) => {
                    // — GlassSignal: VT is visible — ensure buffer exists at correct size
                    if self.vt_buffers[vt_num].is_some() {
                        self.resize_vt_buffer(vt_num, geom.usable_width, geom.usable_height);
                    }
                    // — GlassSignal: tell the terminal emulator about the new VFB.
                    // Only if dimensions actually changed and VT has a buffer.
                    let dims_changed = match old_geom {
                        Some(old) => old.usable_width != geom.usable_width
                            || old.usable_height != geom.usable_height,
                        None => true,
                    };
                    if dims_changed {
                        if let Some(ref buf) = self.vt_buffers[vt_num] {
                            terminal::resize_vt(vt_num, buf.clone() as Arc<dyn Framebuffer>);
                        }
                        // — GlassSignal: notify VT layer about new winsize + SIGWINCH
                        unsafe {
                            if let Some(cb) = WINSIZE_CALLBACK {
                                cb(
                                    vt_num,
                                    geom.text_rows as u16,
                                    geom.text_cols as u16,
                                    geom.usable_width as u16,
                                    geom.usable_height as u16,
                                );
                            }
                        }
                    }
                }
                (Some(_), None) => {
                    // — GlassSignal: VT went off-screen — keep buffer, just stop blitting
                }
                (None, None) => {
                    // — GlassSignal: was off-screen, still off-screen — no-op
                }
            }
        }

        // — EchoFrame: scrollbar widgets need updated positions after layout shift
        self.update_scrollbar_rects();

        changes
    }

    /// Get the backing framebuffer for a VT, allocating on demand.
    /// — SableWire: terminal renderers call this on VT switch. First call
    /// for a new VT triggers the backing buffer allocation.
    pub fn get_vt_framebuffer(&mut self, vt_num: usize) -> Option<Arc<dyn Framebuffer>> {
        if vt_num >= MAX_VTS { return None; }
        self.ensure_vt_buffer(vt_num);
        self.vt_buffers[vt_num]
            .as_ref()
            .map(|b| b.clone() as Arc<dyn Framebuffer>)
    }

    /// Get the backing buffer reference for direct blit access (no lazy alloc)
    fn get_vt_buffer(&self, vt_num: usize) -> Option<&Arc<BackingFramebuffer>> {
        if vt_num >= MAX_VTS { return None; }
        self.vt_buffers[vt_num].as_ref()
    }

    /// Composite visible VT buffers onto the hardware framebuffer.
    /// — SableWire: only blits VTs that have geometry (are visible). Off-screen
    /// VTs are skipped entirely — no buffer access, no dirty flag check.
    /// Dirty flags are passed in from tick() — never double-consume atomics.
    fn composite(&mut self, full_redraw: bool, sb_dirty: bool) {
        let viewports = self.layout.compute_viewports();
        let tile_count = self.layout.tile_count();

        // — SoftGlyph: erase mouse cursor before any content blit.
        if let Some(ref mut cursor) = self.mouse_cursor {
            cursor.erase(&*self.hw_fb);
        }

        // — InputShade: when vkbd is visible, clip VT blit height so we don't
        // overwrite the keyboard pixels on hw_fb. The vkbd overlay persists
        // between frames — only redrawn when vkbd_d or full_r. Without this clip,
        // every cursor blink → VT blit overwrites keyboard → must repaint 80 keys
        // → 1.4MB of pixel writes → 1 FPS. With clip: keyboard area untouched.
        let vkbd_clip = vkbd::keyboard_height();

        let mut any_vt_blitted = false;
        for slot_idx in 0..tile_count {
            let (vt_idx, viewport) = viewports[slot_idx];
            if viewport.width == 0 || viewport.height == 0 {
                continue;
            }

            // — SableWire: lazy-allocate VFB for newly-visible VTs
            self.ensure_vt_buffer(vt_idx);

            // — SableWire: skip clean buffers unless full redraw requested
            if !full_redraw && !VT_DIRTY[vt_idx].swap(false, Ordering::AcqRel) {
                continue;
            }

            if let Some(src_buf) = self.get_vt_buffer(vt_idx) {
                // — GlassSignal: blit VFB content into viewport rect on hardware FB.
                let geom = self.vt_geometries[vt_idx];
                let mut blit_vp = if let Some(g) = geom {
                    Viewport::new(
                        g.screen_x + g.border_left,
                        g.screen_y + g.border_top,
                        g.usable_width,
                        g.usable_height,
                    )
                } else {
                    viewport
                };

                // — InputShade: clip blit height to avoid overwriting vkbd area.
                // Only clip when NOT doing full_redraw (full_redraw repaints vkbd too).
                if vkbd_clip > 0 && !full_redraw {
                    let screen_h = self.hw_fb.height();
                    let kb_top = screen_h.saturating_sub(vkbd_clip);
                    let blit_bottom = blit_vp.y + blit_vp.height;
                    if blit_bottom > kb_top {
                        blit_vp.height = kb_top.saturating_sub(blit_vp.y);
                    }
                }

                if blit_vp.height > 0 {
                    self.blit_vt_to_hw(src_buf, &blit_vp);
                    any_vt_blitted = true;
                }
            }
        }

        // — GlassSignal: draw borders between tiles in split modes
        if self.layout.layout() != Layout::Fullscreen {
            self.draw_borders(&viewports, tile_count);
        }

        // — GlassSignal: draw scrollbar chrome after VT content + borders.
        // Redraw when: scrollbar visual state changed (hover/press), any VT was
        // blitted (thumb position may have changed), or full layout redraw.
        // — EchoFrame: this skips ~30 fill_rect calls per frame when truly idle.
        if full_redraw || sb_dirty || any_vt_blitted {
            self.draw_scrollbars();
        }
    }

    /// Blit a VT backing buffer into a viewport rectangle on the hardware fb.
    /// — SableWire: the hot inner loop. ~0.3ms for full-screen at 1024×768.
    fn blit_vt_to_hw(&self, src: &BackingFramebuffer, viewport: &Viewport) {
        let src_ptr = src.raw_ptr();
        let dst_ptr = self.hw_fb.buffer();
        let src_stride = src.stride() as usize;
        let dst_stride = self.hw_fb.stride() as usize;
        let bpp = src.format().bytes_per_pixel() as usize;

        // — SableWire: blit min(viewport.width, src.width) × min(viewport.height, src.height)
        let blit_w = viewport.width.min(src.width()) as usize;
        let blit_h = viewport.height.min(src.height()) as usize;
        let row_bytes = blit_w * bpp;

        // — CrashBloom: Validate pointers before blitting. Rust nightly 2024 panics
        // on null/overlap in debug mode. Bail silently rather than crashing the kernel.
        if src_ptr.is_null() || dst_ptr.is_null() {
            return;
        }

        unsafe {
            for row in 0..blit_h {
                let src_offset = row * src_stride;
                let dst_offset = ((viewport.y as usize + row) * dst_stride)
                    + (viewport.x as usize * bpp);

                core::ptr::copy_nonoverlapping(
                    src_ptr.add(src_offset),
                    dst_ptr.add(dst_offset),
                    row_bytes,
                );
            }
        }
    }

    /// — EchoFrame: update scrollbar widget positions from current viewport geometries.
    /// Called after any layout change (split, VT switch, resize).
    fn update_scrollbar_rects(&mut self) {
        let tile_count = self.layout.tile_count();
        let viewports = self.layout.compute_viewports();

        for slot_idx in 0..tile_count {
            let (vt_idx, viewport) = viewports[slot_idx];
            if vt_idx >= MAX_VTS || viewport.width == 0 || viewport.height == 0 {
                continue;
            }
            let geom = match self.vt_geometries[vt_idx] {
                Some(g) => g,
                None => continue,
            };
            let flags = self.vt_scrollbar_flags[vt_idx];

            // — EchoFrame: vertical scrollbar sits on the right edge
            if flags.vscroll {
                let sb_x = (geom.screen_x + geom.total_width).saturating_sub(SCROLLBAR_WIDTH);
                let sb_y = geom.screen_y + geom.border_top;
                let sb_h = geom.usable_height;
                self.vscrollbars[vt_idx].set_rect(sb_x, sb_y, SCROLLBAR_WIDTH, sb_h);
            }

            // — EchoFrame: horizontal scrollbar sits on the bottom edge
            if flags.hscroll {
                let sb_x = geom.screen_x + geom.border_left;
                let sb_y = (geom.screen_y + geom.total_height).saturating_sub(SCROLLBAR_HEIGHT);
                let sb_w = geom.usable_width;
                self.hscrollbars[vt_idx].set_rect(sb_x, sb_y, sb_w, SCROLLBAR_HEIGHT);
            }
        }
    }

    /// — EchoFrame: Draw Win95-style scrollbar widgets for all visible VTs.
    /// Each scrollbar is a self-contained object that knows how to render itself.
    fn draw_scrollbars(&mut self) {
        let tile_count = self.layout.tile_count();
        let viewports = self.layout.compute_viewports();

        for slot_idx in 0..tile_count {
            let (vt_idx, viewport) = viewports[slot_idx];
            if vt_idx >= MAX_VTS || viewport.width == 0 || viewport.height == 0 {
                continue;
            }
            let flags = self.vt_scrollbar_flags[vt_idx];
            let geom = match self.vt_geometries[vt_idx] {
                Some(g) => g,
                None => continue,
            };

            // — EchoFrame: query terminal state and update scrollbar content
            let sb_state = terminal::get_scrollbar_state(vt_idx);

            // ── Vertical scrollbar ──
            if flags.vscroll {
                if let Some(state) = sb_state {
                    let total = state.scrollback_len + state.rows as usize;
                    let visible = state.rows as usize;
                    self.vscrollbars[vt_idx].set_content(ScrollContent {
                        total,
                        visible,
                        position: state.scroll_offset,
                    });
                }
                // — EchoFrame: render the widget. Closure bridges to fill_hw_rect.
                let hw_fb = &self.hw_fb;
                self.vscrollbars[vt_idx].draw(&mut |x, y, w, h, color| {
                    fill_hw_rect_static(hw_fb.as_ref(), x, y, w, h, color);
                });
            }

            // ── Horizontal scrollbar ──
            if flags.hscroll {
                if let Some(state) = sb_state {
                    let total_w = state.max_line_width;
                    let visible_w = state.cols as usize;
                    self.hscrollbars[vt_idx].set_content(ScrollContent {
                        total: total_w,
                        visible: visible_w,
                        position: state.h_scroll_offset,
                    });
                }
                let hw_fb = &self.hw_fb;
                self.hscrollbars[vt_idx].draw(&mut |x, y, w, h, color| {
                    fill_hw_rect_static(hw_fb.as_ref(), x, y, w, h, color);
                });
            }

            // — EchoFrame: corner block where both scrollbars meet — raised face
            if flags.vscroll && flags.hscroll {
                let corner_x = (geom.screen_x + geom.total_width).saturating_sub(SCROLLBAR_WIDTH) as usize;
                let corner_y = (geom.screen_y + geom.total_height).saturating_sub(SCROLLBAR_HEIGHT) as usize;
                self.fill_hw_rect(corner_x, corner_y, SCROLLBAR_WIDTH as usize, SCROLLBAR_HEIGHT as usize, 0xFFC0C0C0);
            }
        }
    }

    /// Draw border lines between tiles and a focus highlight on the active tile.
    /// — GlassSignal: 2px dark gray dividers + 1px cyan focus border
    fn draw_borders(&self, viewports: &[(usize, Viewport); MAX_TILES], tile_count: usize) {
        let screen_w = self.hw_fb.width() as usize;
        let screen_h = self.hw_fb.height() as usize;
        let focused = self.layout.focused_slot();

        // — GlassSignal: fill gap pixels between tiles with border color
        match self.layout.layout() {
            Layout::HSplit => {
                // Horizontal border between top and bottom tiles
                let (_, top_vp) = viewports[0];
                let border_y = top_vp.y as usize + top_vp.height as usize;
                let border_h = 2usize.min(screen_h.saturating_sub(border_y));
                self.fill_hw_rect(0, border_y, screen_w, border_h, self.border_color);
            }
            Layout::VSplit => {
                // Vertical border between left and right tiles
                let (_, left_vp) = viewports[0];
                let border_x = left_vp.x as usize + left_vp.width as usize;
                let border_w = 2usize.min(screen_w.saturating_sub(border_x));
                self.fill_hw_rect(border_x, 0, border_w, screen_h, self.border_color);
            }
            Layout::Quad => {
                // Cross-shaped border (horizontal + vertical)
                let (_, tl) = viewports[0];
                let border_x = tl.x as usize + tl.width as usize;
                let border_y = tl.y as usize + tl.height as usize;
                let bw = 2usize.min(screen_w.saturating_sub(border_x));
                let bh = 2usize.min(screen_h.saturating_sub(border_y));
                // Vertical bar
                self.fill_hw_rect(border_x, 0, bw, screen_h, self.border_color);
                // Horizontal bar
                self.fill_hw_rect(0, border_y, screen_w, bh, self.border_color);
            }
            _ => {}
        }

        // — GlassSignal: draw 1px focus highlight around the focused tile
        if tile_count > 1 {
            let (_, vp) = viewports[focused];
            let x = vp.x as usize;
            let y = vp.y as usize;
            let w = vp.width as usize;
            let h = vp.height as usize;
            // Top edge
            self.fill_hw_rect(x, y, w, 1, self.focus_color);
            // Bottom edge
            if y + h > 0 {
                self.fill_hw_rect(x, y + h - 1, w, 1, self.focus_color);
            }
            // Left edge
            self.fill_hw_rect(x, y, 1, h, self.focus_color);
            // Right edge
            if x + w > 0 {
                self.fill_hw_rect(x + w - 1, y, 1, h, self.focus_color);
            }
        }
    }

    /// Fill a rectangle on the hardware framebuffer with a raw ARGB color.
    /// — GlassSignal: used for borders, focus highlights, corner blocks.
    /// — SableWire: row-batched like fill_hw_rect_static — see that function's
    /// comment for why per-pixel MMIO writes are a death sentence in ISR context.
    fn fill_hw_rect(&self, x: usize, y: usize, w: usize, h: usize, color_argb: u32) {
        fill_hw_rect_static(self.hw_fb.as_ref(), x, y, w, h, color_argb);
    }

    // ════════════════════════════════════════════════════════════════════════
    // Event handling — GlassSignal: the compositor IS the window manager
    // EchoFrame: scrollbar widget event helpers below
    // ════════════════════════════════════════════════════════════════════════

    /// — GlassSignal: get the list of visible VTs for hit-testing.
    /// Returns (vt_num, is_visible) pairs for each tile slot.
    fn visible_vts(&self) -> [(usize, bool); MAX_TILES] {
        let viewports = self.layout.compute_viewports();
        let tile_count = self.layout.tile_count();
        let mut result = [(0usize, false); MAX_TILES];
        for i in 0..tile_count {
            let (vt_idx, vp) = viewports[i];
            result[i] = (vt_idx, vp.width > 0 && vp.height > 0);
        }
        result
    }

    /// — GlassSignal: handle mouse button press. Returns what console.rs should do.
    fn handle_mouse_press(&mut self, button: MouseButton, x: i32, y: i32) -> MouseAction {
        let tiles = self.visible_vts();
        let zone = events::hit_test(
            x, y, &self.vt_geometries, &self.vt_scrollbar_flags, &tiles,
        );

        match button {
            MouseButton::Left => {
                self.event_handler.left_pressed = true;

                match zone {
                    HitZone::VScrollbar { vt } => {
                        // — EchoFrame: sub-hit-test with the scrollbar widget
                        let sub_zone = self.vscrollbars[vt].hit_test(x, y);
                        let sb_state = terminal::get_scrollbar_state(vt);
                        let (total, visible, cur_offset) = if let Some(s) = sb_state {
                            (s.scrollback_len + s.rows as usize, s.rows as usize, s.scroll_offset)
                        } else {
                            (0, 0, 0)
                        };
                        let scrollable = total.saturating_sub(visible);

                        match sub_zone {
                            ScrollbarHitZone::ArrowDec => {
                                // — EchoFrame: scroll up one line
                                self.vscrollbars[vt].arrow_dec_state = PartState::Pressed;
                                if scrollable > 0 {
                                    let new_off = (cur_offset + 1).min(scrollable);
                                    terminal::scroll_to_line(vt, new_off);
                                }
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::ArrowInc => {
                                // — EchoFrame: scroll down one line
                                self.vscrollbars[vt].arrow_inc_state = PartState::Pressed;
                                if cur_offset > 0 {
                                    terminal::scroll_to_line(vt, cur_offset - 1);
                                }
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::TrackBefore => {
                                // — EchoFrame: page up (scroll up by visible rows)
                                if scrollable > 0 {
                                    let page = visible.max(1);
                                    let new_off = (cur_offset + page).min(scrollable);
                                    terminal::scroll_to_line(vt, new_off);
                                }
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::TrackAfter => {
                                // — EchoFrame: page down (scroll down by visible rows)
                                if cur_offset > 0 {
                                    let page = visible.max(1);
                                    let new_off = cur_offset.saturating_sub(page);
                                    terminal::scroll_to_line(vt, new_off);
                                }
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::Thumb => {
                                // — EchoFrame: start thumb drag
                                self.event_handler.state = MouseState::ScrollbarDrag;
                                self.vscrollbars[vt].thumb_state = PartState::Pressed;
                                let track_h = self.vscrollbars[vt].track_pixel_length() as usize;
                                self.event_handler.drag = Some(DragState {
                                    vt,
                                    vertical: true,
                                    start_pos: y,
                                    start_offset: cur_offset,
                                    track_length: track_h,
                                    total_content: total,
                                    visible_content: visible,
                                });
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            _ => {
                                // — EchoFrame: jump-to-position fallback
                                self.event_handler.state = MouseState::ScrollbarDrag;
                                let track_h = self.vscrollbars[vt].track_pixel_length() as usize;
                                self.event_handler.drag = Some(DragState {
                                    vt,
                                    vertical: true,
                                    start_pos: y,
                                    start_offset: cur_offset,
                                    track_length: track_h,
                                    total_content: total,
                                    visible_content: visible,
                                });
                                // Jump to click position
                                let new_pos = self.vscrollbars[vt].screen_to_scroll_position(x, y);
                                terminal::scroll_to_line(vt, new_pos);
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                        }
                    }

                    HitZone::HScrollbar { vt } => {
                        // — EchoFrame: sub-hit-test horizontal scrollbar widget
                        let sub_zone = self.hscrollbars[vt].hit_test(x, y);
                        let sb_state = terminal::get_scrollbar_state(vt);
                        let (total_w, visible_w, cur_offset) = if let Some(s) = sb_state {
                            (s.max_line_width, s.cols as usize, s.h_scroll_offset)
                        } else {
                            (0, 0, 0)
                        };
                        let scrollable = total_w.saturating_sub(visible_w);

                        match sub_zone {
                            ScrollbarHitZone::ArrowDec => {
                                self.hscrollbars[vt].arrow_dec_state = PartState::Pressed;
                                if cur_offset > 0 {
                                    terminal::scroll_to_col(vt, cur_offset - 1);
                                }
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::ArrowInc => {
                                self.hscrollbars[vt].arrow_inc_state = PartState::Pressed;
                                if cur_offset < scrollable {
                                    terminal::scroll_to_col(vt, cur_offset + 1);
                                }
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::TrackBefore => {
                                let page = visible_w.max(1);
                                let new_off = cur_offset.saturating_sub(page);
                                terminal::scroll_to_col(vt, new_off);
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::TrackAfter => {
                                let page = visible_w.max(1);
                                let new_off = (cur_offset + page).min(scrollable);
                                terminal::scroll_to_col(vt, new_off);
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::Thumb => {
                                self.event_handler.state = MouseState::ScrollbarDrag;
                                self.hscrollbars[vt].thumb_state = PartState::Pressed;
                                let track_w = self.hscrollbars[vt].track_pixel_length() as usize;
                                self.event_handler.drag = Some(DragState {
                                    vt,
                                    vertical: false,
                                    start_pos: x,
                                    start_offset: cur_offset,
                                    track_length: track_w,
                                    total_content: total_w,
                                    visible_content: visible_w,
                                });
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            _ => {
                                // — EchoFrame: jump-to-position fallback
                                let new_pos = self.hscrollbars[vt].screen_to_scroll_position(x, y);
                                terminal::scroll_to_col(vt, new_pos);
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                        }
                    }

                    HitZone::ScrollbarCorner { .. } | HitZone::Border => {
                        MouseAction::Consumed
                    }

                    HitZone::VtContent { vt } => {
                        self.event_handler.state = MouseState::ContentPress;
                        MouseAction::ForwardToTerminal { vt }
                    }

                    HitZone::None => MouseAction::Nothing,
                }
            }

            MouseButton::Middle => {
                self.event_handler.middle_pressed = true;
                match zone {
                    HitZone::VtContent { vt } => MouseAction::ForwardToTerminal { vt },
                    _ => MouseAction::Nothing,
                }
            }

            MouseButton::Right => {
                match zone {
                    HitZone::VtContent { vt } => MouseAction::ForwardToTerminal { vt },
                    _ => MouseAction::Nothing,
                }
            }
        }
    }

    /// — GlassSignal: handle mouse button release.
    fn handle_mouse_release(&mut self, button: MouseButton, _x: i32, _y: i32) -> MouseAction {
        match button {
            MouseButton::Left => {
                self.event_handler.left_pressed = false;
                let was_dragging = self.event_handler.state == MouseState::ScrollbarDrag;
                let drag_vt = self.event_handler.drag.as_ref().map(|d| d.vt);
                self.event_handler.state = MouseState::Idle;
                self.event_handler.drag = None;

                // — EchoFrame: reset all scrollbar visual states on release
                for sb in self.vscrollbars.iter_mut() {
                    sb.reset_states();
                }
                for sb in self.hscrollbars.iter_mut() {
                    sb.reset_states();
                }
                if let Some(vt) = drag_vt {
                    mark_dirty(vt);
                }
                SCROLLBAR_DIRTY.store(true, Ordering::Release);

                if was_dragging {
                    MouseAction::Consumed
                } else {
                    // — GlassSignal: was content press — let console.rs finish selection
                    MouseAction::ForwardToTerminal { vt: self.layout.focused_vt() }
                }
            }

            MouseButton::Middle => {
                self.event_handler.middle_pressed = false;
                MouseAction::Nothing
            }

            MouseButton::Right => MouseAction::Nothing,
        }
    }

    /// — GlassSignal: handle mouse motion. Drags scrollbars, hovers, or forwards to terminal.
    fn handle_mouse_move(&mut self, x: i32, y: i32) -> MouseAction {
        match self.event_handler.state {
            MouseState::ScrollbarDrag => {
                if let Some(ref drag) = self.event_handler.drag {
                    let vt = drag.vt;
                    let scrollable = drag.total_content.saturating_sub(drag.visible_content);
                    if scrollable == 0 || drag.track_length == 0 {
                        return MouseAction::Consumed;
                    }

                    if drag.vertical {
                        // — EchoFrame: vertical drag — delta Y maps to scroll lines
                        let delta_px = y - drag.start_pos;
                        let delta_lines = (delta_px as i64 * scrollable as i64) / drag.track_length as i64;
                        let new_offset = (drag.start_offset as i64 - delta_lines)
                            .max(0).min(scrollable as i64) as usize;
                        terminal::scroll_to_line(vt, new_offset);
                    } else {
                        // — EchoFrame: horizontal drag — delta X maps to scroll columns
                        let delta_px = x - drag.start_pos;
                        let delta_cols = (delta_px as i64 * drag.total_content as i64) / drag.track_length as i64;
                        let max_scroll = drag.total_content.saturating_sub(drag.visible_content);
                        let new_offset = (drag.start_offset as i64 + delta_cols)
                            .max(0).min(max_scroll as i64) as usize;
                        terminal::scroll_to_col(vt, new_offset);
                    }
                    mark_dirty(vt);
                    SCROLLBAR_DIRTY.store(true, Ordering::Release);
                    return MouseAction::Consumed;
                }
                MouseAction::Nothing
            }

            MouseState::ContentPress => {
                MouseAction::ForwardToTerminal { vt: self.layout.focused_vt() }
            }

            MouseState::Idle => {
                // — EchoFrame: update hover states on scrollbar widgets
                self.update_hover_states(x, y);
                // — InputShade: update vkbd key hover when keyboard is visible.
                // Cheap: hit_test is pure math, only marks dirty if hover changed.
                if vkbd::is_visible() {
                    vkbd::update_hover(x, y);
                }
                MouseAction::Nothing
            }
        }
    }

    /// — EchoFrame: update hover visual states for scrollbar widgets.
    /// Called on idle mouse move. Only marks dirty if state actually changed.
    fn update_hover_states(&mut self, x: i32, y: i32) {
        let tiles = self.visible_vts();
        let zone = events::hit_test(
            x, y, &self.vt_geometries, &self.vt_scrollbar_flags, &tiles,
        );

        // — EchoFrame: snapshot ALL states BEFORE any changes. Compare after.
        // Old approach reset first then compared — but that meant "old" was always
        // Normal, so any hover = "changed" = SCROLLBAR_DIRTY every mouse move = 2 FPS.
        type StateTriple = (PartState, PartState, PartState);
        let mut old_vstates: [StateTriple; MAX_VTS] = [(PartState::Normal, PartState::Normal, PartState::Normal); MAX_VTS];
        let mut old_hstates: [StateTriple; MAX_VTS] = [(PartState::Normal, PartState::Normal, PartState::Normal); MAX_VTS];
        for (i, sb) in self.vscrollbars.iter().enumerate() {
            old_vstates[i] = (sb.arrow_dec_state, sb.arrow_inc_state, sb.thumb_state);
        }
        for (i, sb) in self.hscrollbars.iter().enumerate() {
            old_hstates[i] = (sb.arrow_dec_state, sb.arrow_inc_state, sb.thumb_state);
        }

        // — EchoFrame: reset all scrollbar states, then set the one being hovered
        for sb in self.vscrollbars.iter_mut() {
            sb.reset_states();
        }
        for sb in self.hscrollbars.iter_mut() {
            sb.reset_states();
        }

        match zone {
            HitZone::VScrollbar { vt } => {
                let sub = self.vscrollbars[vt].hit_test(x, y);
                match sub {
                    ScrollbarHitZone::ArrowDec => self.vscrollbars[vt].arrow_dec_state = PartState::Hovered,
                    ScrollbarHitZone::ArrowInc => self.vscrollbars[vt].arrow_inc_state = PartState::Hovered,
                    ScrollbarHitZone::Thumb => self.vscrollbars[vt].thumb_state = PartState::Hovered,
                    _ => {}
                }
            }
            HitZone::HScrollbar { vt } => {
                let sub = self.hscrollbars[vt].hit_test(x, y);
                match sub {
                    ScrollbarHitZone::ArrowDec => self.hscrollbars[vt].arrow_dec_state = PartState::Hovered,
                    ScrollbarHitZone::ArrowInc => self.hscrollbars[vt].arrow_inc_state = PartState::Hovered,
                    ScrollbarHitZone::Thumb => self.hscrollbars[vt].thumb_state = PartState::Hovered,
                    _ => {}
                }
            }
            _ => {}
        }

        // — EchoFrame: compare final states against pre-reset snapshots.
        // Only mark dirty if something actually changed visually.
        let mut changed = false;
        for (i, sb) in self.vscrollbars.iter().enumerate() {
            if (sb.arrow_dec_state, sb.arrow_inc_state, sb.thumb_state) != old_vstates[i] {
                changed = true;
                break;
            }
        }
        if !changed {
            for (i, sb) in self.hscrollbars.iter().enumerate() {
                if (sb.arrow_dec_state, sb.arrow_inc_state, sb.thumb_state) != old_hstates[i] {
                    changed = true;
                    break;
                }
            }
        }

        if changed {
            SCROLLBAR_DIRTY.store(true, Ordering::Release);
        }
    }

    /// — GlassSignal: handle mouse wheel. Shift+wheel = horizontal scroll.
    fn handle_mouse_wheel(&mut self, delta: i32, _x: i32, _y: i32, shift_held: bool) -> MouseAction {
        let vt = self.layout.focused_vt();
        let scroll_lines = (delta.unsigned_abs() as usize) * 3;

        if shift_held {
            // — GlassSignal: shift+wheel = horizontal scroll
            if delta > 0 {
                terminal::scroll_left(scroll_lines);
            } else {
                terminal::scroll_right(scroll_lines);
            }
            mark_dirty(vt);
            MouseAction::Consumed
        } else {
            // — GlassSignal: normal wheel = vertical scroll (handled by terminal in console.rs)
            // Return Nothing so console.rs can decide based on mouse mode
            MouseAction::Nothing
        }
    }
}

/// — EchoFrame: static fill_rect that doesn't need &self — used by scrollbar widget
/// draw callbacks. Same pixel-format-aware logic as Compositor::fill_hw_rect.
///
/// — SableWire: row-batched MMIO writes. The old per-pixel path did W×H individual
/// MMIO writes at ~1000 cycles each. A 16×400 scrollbar track = 6,400 writes = 6.4M
/// cycles IN THE TIMER ISR. Now we fill one row in a stack buffer (RAM, ~1 cycle/pixel),
/// then blast the whole row to MMIO in one copy_nonoverlapping. Reduces MMIO transaction
/// count by W× (16× for scrollbars). The difference between "works" and "deadlock."
fn fill_hw_rect_static(hw_fb: &dyn Framebuffer, x: usize, y: usize, w: usize, h: usize, color_argb: u32) {
    let bpp = hw_fb.format().bytes_per_pixel() as usize;
    let dst_ptr = hw_fb.buffer();
    let dst_stride = hw_fb.stride() as usize;
    let screen_w = hw_fb.width() as usize;
    let screen_h = hw_fb.height() as usize;

    let x_end = (x + w).min(screen_w);
    let y_end = (y + h).min(screen_h);
    if x >= screen_w || y >= screen_h { return; }
    let actual_w = x_end - x;
    if actual_w == 0 || y >= y_end { return; }

    let pixel_bytes = match hw_fb.format() {
        fb::PixelFormat::BGRA8888 => [
            (color_argb & 0xFF) as u8,
            ((color_argb >> 8) & 0xFF) as u8,
            ((color_argb >> 16) & 0xFF) as u8,
            ((color_argb >> 24) & 0xFF) as u8,
        ],
        _ => [
            ((color_argb >> 16) & 0xFF) as u8,
            ((color_argb >> 8) & 0xFF) as u8,
            (color_argb & 0xFF) as u8,
            ((color_argb >> 24) & 0xFF) as u8,
        ],
    };

    // — SableWire: build one row of pixels in stack RAM, then copy to MMIO per row.
    // 256px × 4bpp = 1024 bytes on stack — covers any scrollbar/widget width.
    // For wider rects, we tile the row buffer. Scrollbars are always 16px wide.
    const MAX_ROW_PX: usize = 256;
    let row_px = actual_w.min(MAX_ROW_PX);
    let mut row_buf = [0u8; MAX_ROW_PX * 4];
    let pb = bpp.min(4);

    // — SableWire: stamp the pixel pattern into the row template
    for col in 0..row_px {
        let off = col * bpp;
        row_buf[off..off + pb].copy_from_slice(&pixel_bytes[..pb]);
    }
    let row_bytes = row_px * bpp;

    unsafe {
        for row in y..y_end {
            let dst_offset = row * dst_stride + x * bpp;
            let dst = dst_ptr.add(dst_offset);

            if actual_w <= MAX_ROW_PX {
                // — SableWire: common case — entire row fits in one blast
                core::ptr::copy_nonoverlapping(row_buf.as_ptr(), dst, row_bytes);
            } else {
                // — SableWire: wide rect — tile the row buffer across the width
                let mut remaining = actual_w;
                let mut col_off = 0usize;
                while remaining > 0 {
                    let chunk = remaining.min(MAX_ROW_PX);
                    let chunk_bytes = chunk * bpp;
                    core::ptr::copy_nonoverlapping(
                        row_buf.as_ptr(),
                        dst.add(col_off * bpp),
                        chunk_bytes,
                    );
                    col_off += chunk;
                    remaining -= chunk;
                }
            }
        }
    }
}

// ============================================================================
// Public API — called from kernel init, VT switch, terminal write, timer tick
// ============================================================================

/// Initialize the compositor. Called once during kernel boot after fb::init_from_boot().
/// Returns the VT0 backing framebuffer for the terminal renderer.
/// — NeonRoot: only VT0 gets a buffer at init. The rest are lazy-allocated
/// on first split/switch. No upfront memory waste.
pub fn init(hw_fb: Arc<dyn Framebuffer>) -> Option<Arc<dyn Framebuffer>> {
    let mut compositor = Compositor::new(hw_fb);
    let vt0_fb = compositor.get_vt_framebuffer(0);
    *COMPOSITOR.lock() = Some(compositor);
    FULL_REDRAW.store(true, Ordering::Release);
    // — SoftGlyph: set lock-free flag so ISR mouse processing knows cursor exists
    MOUSE_INITIALIZED.store(true, Ordering::Release);
    os_log::println!("[COMP] compositor initialized (VT0 buffer allocated, rest on-demand)");
    vt0_fb
}

/// Get the backing framebuffer for a specific VT (allocates on demand).
/// Used by terminal::update_framebuffer() on VT switch.
/// — SableWire: first call for a new VT triggers ~4MB backing buffer allocation.
pub fn get_vt_framebuffer(vt_num: usize) -> Option<Arc<dyn Framebuffer>> {
    let mut guard = COMPOSITOR.lock();
    guard.as_mut().and_then(|c| c.get_vt_framebuffer(vt_num))
}

/// Mark a VT as dirty (its backing buffer has new content).
/// Called after terminal::write() or /dev/fb0 write.
/// — SableWire: lock-free, ISR-safe
#[inline]
pub fn mark_dirty(vt_num: usize) {
    if vt_num < MAX_VTS {
        VT_DIRTY[vt_num].store(true, Ordering::Release);
    }
}

/// Request a full redraw (e.g., after layout change or VT switch).
/// — SableWire: lock-free, ISR-safe
#[inline]
pub fn request_full_redraw() {
    FULL_REDRAW.store(true, Ordering::Release);
}

/// Composite all dirty VTs onto the hardware framebuffer.
/// Called at ~100 Hz from timer ISR. Only actually composites when something changed.
/// — SableWire: this is the only function that touches the hardware framebuffer.
pub fn tick() {
    // — SableWire: fast path — consume all dirty flags atomically.
    // When nothing is dirty, bail immediately — zero cost idle path.
    let full_r = FULL_REDRAW.swap(false, Ordering::AcqRel);
    let cursor_d = CURSOR_DIRTY.swap(false, Ordering::AcqRel);
    let vkbd_d = vkbd::take_dirty();
    let vkbd_hover_d = vkbd::take_hover_dirty();
    let vt_dirty = VT_DIRTY.iter().any(|d| d.load(Ordering::Acquire));
    let sb_dirty = SCROLLBAR_DIRTY.swap(false, Ordering::AcqRel);
    let any_dirty = full_r || cursor_d || vkbd_d || vkbd_hover_d || vt_dirty || sb_dirty;

    if !any_dirty {
        return;
    }

    // — SableWire: content dirty = needs full composite (VT blit, borders, scrollbars).
    // Cursor-only, vkbd-only, or scrollbar-hover-only changes skip the expensive path.
    // — EchoFrame: scrollbar hover used to trigger full composite — that's insane.
    // A 16×16 arrow highlight shouldn't blit 1024×768 of VT content. Now sb-only
    // redraws just the scrollbar widgets directly on hw_fb, no VT blit needed.
    let content_dirty = full_r || vt_dirty;

    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            if content_dirty {
                // — SableWire: full composite — blit VTs, borders, scrollbars
                compositor.composite(full_r, true);
            } else if sb_dirty {
                // — EchoFrame: scrollbar-only redraw — skip VT blit, just repaint
                // scrollbar widgets directly on hw_fb. ~30 fill_rects vs full blit.
                if let Some(ref mut cursor) = compositor.mouse_cursor {
                    cursor.erase(&*compositor.hw_fb);
                }
                compositor.draw_scrollbars();
            } else {
                // — SoftGlyph: cursor/vkbd only — just erase old cursor, skip VT blit
                if let Some(ref mut cursor) = compositor.mouse_cursor {
                    cursor.erase(&*compositor.hw_fb);
                }
            }
            // — InputShade: draw virtual keyboard overlay after VT blit.
            // Full repaint on toggle/press or full redraw — the VT blit is clipped to
            // avoid overwriting the keyboard area, so the overlay persists on hw_fb.
            // Hover-only changes use the fast path: repaint just 2 keys, not all ~100.
            // — SableWire: the old approach repainted 100 keys per mouse move = 1 FPS.
            // Now hover = 2 fill_rects + 2 glyph renders. Night and day.
            if vkbd::is_visible() {
                if vkbd_d || full_r {
                    vkbd::draw_overlay(&*compositor.hw_fb);
                } else if vkbd_hover_d {
                    vkbd::redraw_hover_keys(&*compositor.hw_fb);
                }
            }
            // — SoftGlyph: mouse cursor last — the final layer before GPU flush
            if let Some(ref mut cursor) = compositor.mouse_cursor {
                cursor.redraw(&*compositor.hw_fb);
            }
            // — GlassSignal: ONE flush after ALL layers (VT content + scrollbars +
            // borders + vkbd overlay + mouse cursor). This is the ONLY place that
            // sends pixels to VirtIO-GPU. — SableWire
            compositor.hw_fb.flush();
        }
    }
}

/// Switch focus to a VT. Updates layout manager, recomputes geometries,
/// resizes VFBs, and triggers redraw.
/// Called from Alt+Fn keyboard shortcut.
pub fn focus_vt(vt_num: usize) {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        compositor.layout.focus_vt(vt_num);
        compositor.apply_layout_change();
        COMPOSITOR_FOCUS_VT.store(compositor.layout.focused_vt(), Ordering::Release);
        request_full_redraw();
    }
}

/// Get the currently focused VT index (lock-free, ISR-safe).
#[inline]
pub fn focused_vt() -> usize {
    COMPOSITOR_FOCUS_VT.load(Ordering::Acquire)
}

/// Set the tiling layout. Recomputes geometries, resizes VFBs, triggers redraw.
pub fn set_layout(layout: Layout) {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        compositor.layout.set_layout(layout);
        compositor.apply_layout_change();
        COMPOSITOR_FOCUS_VT.store(compositor.layout.focused_vt(), Ordering::Release);
        request_full_redraw();
        os_log::println!("[COMP] layout={:?} tiles={}", layout, compositor.layout.tile_count());
    }
}

/// Toggle fullscreen ↔ last split layout (Alt+Enter).
pub fn toggle_fullscreen() {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        compositor.layout.toggle_fullscreen();
        compositor.apply_layout_change();
        COMPOSITOR_FOCUS_VT.store(compositor.layout.focused_vt(), Ordering::Release);
        request_full_redraw();
        os_log::println!("[COMP] toggle → {:?}", compositor.layout.layout());
    }
}

/// Cycle focus to next visible tile (Alt+Tab).
pub fn cycle_focus() {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        compositor.layout.cycle_focus();
        compositor.apply_layout_change();
        COMPOSITOR_FOCUS_VT.store(compositor.layout.focused_vt(), Ordering::Release);
        request_full_redraw();
    }
}

/// Get the current layout mode.
pub fn current_layout() -> Layout {
    COMPOSITOR.lock().as_ref()
        .map(|c| c.layout.layout())
        .unwrap_or(Layout::Fullscreen)
}

/// — GlassSignal: update scrollbar flags for a VT based on terminal state.
/// Called after wrap mode toggle — may trigger geometry recomputation + VFB resize
/// if horizontal scrollbar visibility changes.
pub fn update_scrollbar_flags(vt_num: usize) {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        if vt_num >= MAX_VTS { return; }
        let old_flags = compositor.vt_scrollbar_flags[vt_num];
        let mut new_flags = ScrollbarFlags { vscroll: true, hscroll: false };

        // — GlassSignal: horizontal scrollbar only when wrap OFF and content wider than viewport
        if let Some(state) = terminal::get_scrollbar_state(vt_num) {
            if !state.wrap_mode && state.max_line_width > state.cols as usize {
                new_flags.hscroll = true;
            }
        }

        compositor.vt_scrollbar_flags[vt_num] = new_flags;

        // — GlassSignal: if flags changed, recompute geometry (VFB resize)
        if old_flags.hscroll != new_flags.hscroll {
            compositor.apply_layout_change();
            request_full_redraw();
        }
    }
}

/// Get viewport info for a VT (used by /dev/fb0 ioctl to report resolution).
/// — GlassSignal: legacy API, returns raw Viewport for backward compat
pub fn get_vt_viewport(vt_num: usize) -> Option<Viewport> {
    let guard = COMPOSITOR.lock();
    let compositor = guard.as_ref()?;
    let viewports = compositor.layout.compute_viewports();
    let tile_count = compositor.layout.tile_count();
    for i in 0..tile_count {
        let (idx, viewport) = viewports[i];
        if idx == vt_num {
            return Some(viewport);
        }
    }
    // — GlassSignal: VT not currently visible — return full screen as fallback
    Some(Viewport::new(0, 0, compositor.hw_fb.width(), compositor.hw_fb.height()))
}

/// Get the full ViewportGeometry for a VT. None if VT is off-screen.
/// — GlassSignal: the real API. fb0, terminal resize, and winsize all use this.
pub fn get_vt_geometry(vt_num: usize) -> Option<ViewportGeometry> {
    let guard = COMPOSITOR.lock();
    let compositor = guard.as_ref()?;
    if vt_num < MAX_VTS {
        compositor.vt_geometries[vt_num]
    } else {
        None
    }
}

/// Check if a VT is currently visible on screen.
/// — GlassSignal: lock-free would be nicer but geometry changes are rare
pub fn is_vt_visible(vt_num: usize) -> bool {
    let guard = COMPOSITOR.lock();
    guard.as_ref()
        .map(|c| vt_num < MAX_VTS && c.vt_geometries[vt_num].is_some())
        .unwrap_or(false)
}

/// Get VFB dimensions for a VT (usable area). For fb0 ioctl and terminal init.
/// Returns (width, height) — usable pixels, not including chrome.
/// Falls back to full screen size if VT has no geometry (off-screen).
pub fn get_vfb_dimensions(vt_num: usize) -> (u32, u32) {
    let guard = COMPOSITOR.lock();
    if let Some(compositor) = guard.as_ref() {
        if vt_num < MAX_VTS {
            if let Some(geom) = compositor.vt_geometries[vt_num] {
                return (geom.usable_width, geom.usable_height);
            }
        }
        (compositor.hw_fb.width(), compositor.hw_fb.height())
    } else {
        (0, 0)
    }
}

/// Get a VT's VFB info for /dev/fb0 redirection.
/// — GlassSignal: returns (base_ptr, size, width, height, stride, bpp, is_bgr).
/// The kernel's memory module constructs FramebufferDeviceInfo from these.
/// Returns None if VT has no buffer.
pub fn get_vfb_info_raw(vt_num: usize) -> Option<(usize, usize, u32, u32, u32, u32, bool)> {
    let guard = COMPOSITOR.lock();
    let compositor = guard.as_ref()?;
    if vt_num >= MAX_VTS { return None; }
    let buf = compositor.vt_buffers[vt_num].as_ref()?;
    let is_bgr = matches!(compositor.hw_fb.format(), fb::PixelFormat::BGRA8888);
    Some((
        buf.buffer() as usize,
        buf.size(),
        buf.width(),
        buf.height(),
        buf.stride(),
        compositor.hw_fb.format().bytes_per_pixel() as u32 * 8,
        is_bgr,
    ))
}

/// Set a VT's display mode (Text/Graphics).
pub fn set_vt_mode(vt_num: usize, mode: VtMode) {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        if vt_num < MAX_VTS {
            compositor.vt_modes[vt_num] = mode;
        }
    }
}

/// Get a VT's display mode.
pub fn get_vt_mode(vt_num: usize) -> VtMode {
    COMPOSITOR.lock().as_ref()
        .map(|c| {
            if vt_num < MAX_VTS { c.vt_modes[vt_num] } else { VtMode::Text }
        })
        .unwrap_or(VtMode::Text)
}

/// Check if compositor is initialized (lock-free quick check).
pub fn is_initialized() -> bool {
    // — SableWire: try_lock instead of load(AtomicBool) because we don't
    // want a separate atomic just for this. try_lock is cheap when uncontended.
    COMPOSITOR.try_lock().map_or(false, |g| g.is_some())
}

/// Update the hardware framebuffer reference (e.g., after VirtIO-GPU init).
/// — GlassSignal: hot-swap the compositor's output target, recompute everything
pub fn update_hw_framebuffer(hw_fb: Arc<dyn Framebuffer>) {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        let w = hw_fb.width();
        let h = hw_fb.height();
        compositor.hw_fb = hw_fb;
        compositor.layout.update_screen_size(w, h);
        // — SoftGlyph: re-create cursor for new screen dimensions
        compositor.mouse_cursor = Some(fb::mouse::MouseCursor::new(w, h));
        compositor.apply_layout_change();
        request_full_redraw();
    }
}

// ============================================================================
// Event System — GlassSignal: the compositor as window manager
// ============================================================================
// Console.rs forwards raw mouse events here. The compositor hit-tests against
// its known geometry (viewports, scrollbars, borders) and returns what to do.
// No more geometry math in console.rs — the compositor owns all screen layout.

/// Re-export event types for console.rs
pub use events::{MouseAction, MouseButton};

/// Handle a mouse button press at screen coordinates (x, y).
/// — GlassSignal: ISR-safe via try_lock. Returns Consumed if compositor handled it.
pub fn handle_mouse_press(button: MouseButton, x: i32, y: i32) -> MouseAction {
    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            return compositor.handle_mouse_press(button, x, y);
        }
    }
    MouseAction::Nothing
}

/// Handle a mouse button release at screen coordinates (x, y).
pub fn handle_mouse_release(button: MouseButton, x: i32, y: i32) -> MouseAction {
    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            return compositor.handle_mouse_release(button, x, y);
        }
    }
    MouseAction::Nothing
}

/// Handle mouse motion to screen coordinates (x, y).
/// — GlassSignal: during scrollbar drag, compositor handles it entirely.
/// During content press, forwards to terminal for selection tracking.
pub fn handle_mouse_move(x: i32, y: i32) -> MouseAction {
    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            return compositor.handle_mouse_move(x, y);
        }
    }
    MouseAction::Nothing
}

/// Handle mouse wheel at screen coordinates.
/// — GlassSignal: shift+wheel = horizontal scroll (compositor handles).
/// Normal wheel without shift = vertical scroll (console.rs decides based on mouse mode).
pub fn handle_mouse_wheel(delta: i32, x: i32, y: i32, shift_held: bool) -> MouseAction {
    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            return compositor.handle_mouse_wheel(delta, x, y, shift_held);
        }
    }
    MouseAction::Nothing
}

/// Hit-test a screen coordinate. Returns which zone (VT content, scrollbar, etc.).
/// — GlassSignal: useful for cursor shape changes (future: resize cursors on borders)
pub fn hit_test(x: i32, y: i32) -> HitZone {
    if let Some(guard) = COMPOSITOR.try_lock() {
        if let Some(ref compositor) = *guard {
            let tiles = compositor.visible_vts();
            return events::hit_test(
                x, y, &compositor.vt_geometries, &compositor.vt_scrollbar_flags, &tiles,
            );
        }
    }
    HitZone::None
}

/// Check if the compositor is currently tracking a scrollbar drag.
/// — GlassSignal: console.rs uses this to suppress selection during drag.
pub fn is_dragging_scrollbar() -> bool {
    if let Some(guard) = COMPOSITOR.try_lock() {
        if let Some(ref compositor) = *guard {
            return compositor.event_handler.state == MouseState::ScrollbarDrag;
        }
    }
    false
}

// ============================================================================
// Mouse Cursor — SoftGlyph: compositor owns the cursor, draws it last
// ============================================================================

/// Check if the mouse cursor is initialized.
/// — SoftGlyph: lock-free atomic check. The old try_lock() approach failed when
/// tick() held the compositor lock, causing the entire mouse input block to be
/// skipped. This was the root cause of the invisible mouse cursor.
#[inline]
pub fn mouse_initialized() -> bool {
    MOUSE_INITIALIZED.load(Ordering::Acquire)
}

/// Move mouse cursor by relative delta.
/// — SoftGlyph: called from ISR context (terminal_tick), ISR-safe via try_lock.
pub fn mouse_move(dx: i32, dy: i32) {
    if dx == 0 && dy == 0 { return; }
    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            if let Some(ref mut cursor) = compositor.mouse_cursor {
                // — SoftGlyph: erase old cursor, update position, mark dirty.
                // Actual redraw happens in tick() after composite().
                cursor.erase(&*compositor.hw_fb);
                cursor.move_by(dx, dy, &*compositor.hw_fb);
            }
            CURSOR_DIRTY.store(true, Ordering::Release);
        }
    }
}

/// Set mouse cursor to absolute position.
/// — SoftGlyph: for tablet devices (absolute coordinates from virtio-tablet).
/// Only marks dirty if position actually changed — tablets spam events even idle.
pub fn mouse_set_position(x: i32, y: i32) {
    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            if let Some(ref mut cursor) = compositor.mouse_cursor {
                let (old_x, old_y) = cursor.position();
                if x == old_x && y == old_y { return; }
                cursor.erase(&*compositor.hw_fb);
                cursor.move_to(x, y, &*compositor.hw_fb);
                CURSOR_DIRTY.store(true, Ordering::Release);
            }
        }
    }
}

/// Get current mouse position in screen pixels.
/// — SoftGlyph: ISR-safe via try_lock. Returns None if cursor not initialized.
pub fn mouse_position() -> Option<(i32, i32)> {
    if let Some(guard) = COMPOSITOR.try_lock() {
        if let Some(ref compositor) = *guard {
            if let Some(ref cursor) = compositor.mouse_cursor {
                return Some(cursor.position());
            }
        }
    }
    None
}

/// Get screen dimensions from the compositor's hardware framebuffer.
/// — SoftGlyph: for tablet coordinate scaling (0..32767 → screen pixels).
pub fn screen_dimensions() -> Option<(u32, u32)> {
    if let Some(guard) = COMPOSITOR.try_lock() {
        if let Some(ref compositor) = *guard {
            return Some((compositor.hw_fb.width(), compositor.hw_fb.height()));
        }
    }
    None
}
