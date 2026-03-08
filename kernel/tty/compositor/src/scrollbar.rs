//! Win95-Style Scrollbar Widget — the real deal, not that flat-UI garbage.
//! — EchoFrame: bevels, arrows, proper thumb drag, hover states.
//! Built like Microsoft intended before the design world lost its mind.
//!
//! The scrollbar is a self-contained geometry + rendering object. The compositor
//! owns instances per-VT and delegates hit-testing, drawing, and event handling.

/// — EchoFrame: scrollbar orientation. Vertical = right edge, Horizontal = bottom edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Orientation {
    Vertical,
    Horizontal,
}

/// — EchoFrame: which part of the scrollbar the mouse is touching.
/// Granular enough for proper Win95 behavior: arrow repeat, page scroll, thumb drag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollbarHitZone {
    /// The decrement arrow button (up / left)
    ArrowDec,
    /// The increment arrow button (down / right)
    ArrowInc,
    /// Track area between dec arrow and thumb (page up / page left)
    TrackBefore,
    /// Track area between thumb and inc arrow (page down / page right)
    TrackAfter,
    /// The draggable thumb itself
    Thumb,
    /// Corner dead zone (both scrollbars meet)
    Corner,
    /// Not on the scrollbar at all
    None,
}

/// — EchoFrame: visual state of individual scrollbar parts.
/// Determines which 3D bevel style to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PartState {
    Normal,
    Hovered,
    Pressed,
}

/// — EchoFrame: scrollbar content state — how much stuff and where we're looking.
/// Passed in from the terminal layer each frame.
#[derive(Clone, Copy, Debug)]
pub struct ScrollContent {
    /// Total content size (lines for vert, columns for horiz)
    pub total: usize,
    /// Visible window size (rows for vert, cols for horiz)
    pub visible: usize,
    /// Current scroll position (0 = start/bottom depending on convention)
    pub position: usize,
}

/// — EchoFrame: the scrollbar widget. One per VT per orientation.
/// Owns its geometry, visual state, and can render itself to any pixel buffer.
#[derive(Clone, Debug)]
pub struct Scrollbar {
    pub orientation: Orientation,
    /// Bounding rect in screen coordinates
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Visual states for each part — EchoFrame: because hover feedback matters
    pub arrow_dec_state: PartState,
    pub arrow_inc_state: PartState,
    pub thumb_state: PartState,
    pub track_state: PartState,
    /// Content state (updated each frame before draw)
    pub content: ScrollContent,
    /// Computed thumb geometry (recalculated in update_thumb_geometry)
    thumb_pos: u32,   // offset from track start in pixels
    thumb_size: u32,  // size in pixels along scroll axis
    track_start: u32, // pixel offset where track begins (after dec arrow)
    track_len: u32,   // track length in pixels (between arrows)
}

// — EchoFrame: dimensions. 16px wide scrollbar like God and Raymond Chen intended.
pub const SCROLLBAR_THICKNESS: u32 = 16;
/// Minimum thumb size so you can actually grab the damn thing
const THUMB_MIN_SIZE: u32 = 24;
/// Arrow button is a square: SCROLLBAR_THICKNESS × SCROLLBAR_THICKNESS
const ARROW_SIZE: u32 = SCROLLBAR_THICKNESS;

// ─── Win95 Color Palette ─────────────────────────────────────────────────────
// — EchoFrame: the sacred palette. Every Windows 95 scrollbar used exactly these.
// Highlight (top-left bevel edge)
const COL_HIGHLIGHT: u32    = 0xFFFFFFFF; // white
// Light shadow (inner top-left bevel)
const COL_LIGHT: u32        = 0xFFDFDFDF; // light gray
// Face / button surface
const COL_FACE: u32         = 0xFFC0C0C0; // classic Win95 gray
// Dark shadow (outer bottom-right bevel)
const COL_SHADOW: u32       = 0xFF808080; // dark gray
// Darkest edge (outer bottom-right corner)
const COL_DARK_SHADOW: u32  = 0xFF404040; // near-black
// Track (sunken channel behind the thumb)
const COL_TRACK: u32        = 0xFFA0A0A0; // slightly darker than face
// Track dither pattern color (alternating pixels like the real thing)
#[allow(dead_code)]
const COL_TRACK_DARK: u32   = 0xFF808080;
// Hover tint — EchoFrame: subtle brightening when mouse hovers
const COL_HOVER_FACE: u32   = 0xFFD0D0D0;
// Pressed face — darker when you mash that button
const COL_PRESSED_FACE: u32 = 0xFFB0B0B0;
// Arrow glyph color
const COL_ARROW_GLYPH: u32  = 0xFF000000; // black

impl Scrollbar {
    /// Create a new scrollbar. Position and size set later by layout.
    pub fn new(orientation: Orientation) -> Self {
        Scrollbar {
            orientation,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            arrow_dec_state: PartState::Normal,
            arrow_inc_state: PartState::Normal,
            thumb_state: PartState::Normal,
            track_state: PartState::Normal,
            content: ScrollContent { total: 0, visible: 0, position: 0 },
            thumb_pos: 0,
            thumb_size: 0,
            track_start: 0,
            track_len: 0,
        }
    }

    /// — EchoFrame: update the scrollbar's bounding rect. Called on layout change.
    pub fn set_rect(&mut self, x: u32, y: u32, width: u32, height: u32) {
        self.x = x;
        self.y = y;
        self.width = width;
        self.height = height;
        self.recompute_track();
    }

    /// — EchoFrame: update content state and recompute thumb geometry.
    pub fn set_content(&mut self, content: ScrollContent) {
        self.content = content;
        self.recompute_thumb();
    }

    /// Reset all part states to Normal
    pub fn reset_states(&mut self) {
        self.arrow_dec_state = PartState::Normal;
        self.arrow_inc_state = PartState::Normal;
        self.thumb_state = PartState::Normal;
        self.track_state = PartState::Normal;
    }

    /// — EchoFrame: compute track region (the channel between arrows).
    fn recompute_track(&mut self) {
        let total_axis = self.axis_length();
        self.track_start = ARROW_SIZE;
        self.track_len = total_axis.saturating_sub(ARROW_SIZE * 2);
        self.recompute_thumb();
    }

    /// — EchoFrame: compute thumb position and size from content state.
    fn recompute_thumb(&mut self) {
        let content = &self.content;
        if content.total <= content.visible || self.track_len == 0 {
            // — EchoFrame: everything fits — thumb fills entire track
            self.thumb_pos = 0;
            self.thumb_size = self.track_len;
            return;
        }

        // Thumb size proportional to visible/total, clamped to minimum
        let ratio_size = ((content.visible as u64 * self.track_len as u64)
            / content.total as u64) as u32;
        self.thumb_size = ratio_size.max(THUMB_MIN_SIZE).min(self.track_len);

        // — EchoFrame: thumb position. Scrollable range maps to usable track pixels.
        let scrollable = content.total.saturating_sub(content.visible);
        let usable_track = self.track_len.saturating_sub(self.thumb_size);

        if scrollable > 0 {
            match self.orientation {
                Orientation::Vertical => {
                    // — EchoFrame: vertical: position 0 = bottom (live), scrollable = top
                    // Display is top-to-bottom, so pos_from_top = scrollable - position
                    let pos_from_top = scrollable.saturating_sub(content.position);
                    self.thumb_pos = ((pos_from_top as u64 * usable_track as u64)
                        / scrollable as u64) as u32;
                }
                Orientation::Horizontal => {
                    // — EchoFrame: horizontal: position 0 = left, increases rightward
                    // Direct mapping — position maps straight to thumb offset
                    self.thumb_pos = ((content.position as u64 * usable_track as u64)
                        / scrollable as u64) as u32;
                }
            }
        } else {
            self.thumb_pos = 0;
        }
    }

    /// Total length along the scroll axis
    #[inline]
    fn axis_length(&self) -> u32 {
        match self.orientation {
            Orientation::Vertical => self.height,
            Orientation::Horizontal => self.width,
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Hit Testing — EchoFrame: pixel-perfect sub-region detection
    // ═══════════════════════════════════════════════════════════════════════

    /// — EchoFrame: hit-test a screen coordinate against this scrollbar.
    /// Returns which sub-region was hit (arrow, track, thumb, or miss).
    pub fn hit_test(&self, screen_x: i32, screen_y: i32) -> ScrollbarHitZone {
        let sx = screen_x as u32;
        let sy = screen_y as u32;

        // Bounds check
        if sx < self.x || sx >= self.x + self.width
            || sy < self.y || sy >= self.y + self.height
        {
            return ScrollbarHitZone::None;
        }

        // Local coordinate along scroll axis
        let local = match self.orientation {
            Orientation::Vertical => sy - self.y,
            Orientation::Horizontal => sx - self.x,
        };

        // Dec arrow (top / left)
        if local < ARROW_SIZE {
            return ScrollbarHitZone::ArrowDec;
        }

        // Inc arrow (bottom / right)
        let axis_len = self.axis_length();
        if local >= axis_len.saturating_sub(ARROW_SIZE) {
            return ScrollbarHitZone::ArrowInc;
        }

        // Track region — figure out if it's thumb, before-thumb, or after-thumb
        let track_local = local - self.track_start;
        let thumb_start = self.thumb_pos;
        let thumb_end = self.thumb_pos + self.thumb_size;

        if track_local >= thumb_start && track_local < thumb_end {
            ScrollbarHitZone::Thumb
        } else if track_local < thumb_start {
            ScrollbarHitZone::TrackBefore
        } else {
            ScrollbarHitZone::TrackAfter
        }
    }

    /// — EchoFrame: convert a screen coordinate to a scroll position.
    /// Used for thumb dragging — maps pixel offset to content position.
    pub fn screen_to_scroll_position(&self, screen_x: i32, screen_y: i32) -> usize {
        let local = match self.orientation {
            Orientation::Vertical => screen_y as i32 - self.y as i32 - self.track_start as i32,
            Orientation::Horizontal => screen_x as i32 - self.x as i32 - self.track_start as i32,
        };

        let usable_track = self.track_len.saturating_sub(self.thumb_size);
        if usable_track == 0 || self.content.total <= self.content.visible {
            return 0;
        }

        let scrollable = self.content.total.saturating_sub(self.content.visible);
        // — EchoFrame: center the drag on the thumb
        let adjusted = local - (self.thumb_size as i32 / 2);
        let clamped = adjusted.max(0).min(usable_track as i32) as u64;
        let pos_from_top = (clamped * scrollable as u64) / usable_track as u64;

        match self.orientation {
            Orientation::Vertical => {
                // — EchoFrame: vertical: convert top-based pixel pos to bottom-based offset
                scrollable.saturating_sub(pos_from_top as usize)
            }
            Orientation::Horizontal => {
                // — EchoFrame: horizontal: direct mapping (0 = left)
                pos_from_top as usize
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Rendering — EchoFrame: Win95 bevels or bust
    // ═══════════════════════════════════════════════════════════════════════

    /// — EchoFrame: render the entire scrollbar to the hardware framebuffer.
    /// Uses fill_rect callback to stay decoupled from the FB implementation.
    pub fn draw(&self, fill: &mut dyn FnMut(usize, usize, usize, usize, u32)) {
        // — EchoFrame: bail if scrollbar has no dimensions (VT not yet sized)
        if self.width == 0 || self.height == 0 {
            return;
        }
        self.draw_track(fill);
        self.draw_thumb(fill);
        self.draw_arrow_dec(fill);
        self.draw_arrow_inc(fill);
    }

    /// — EchoFrame: draw the sunken track channel (the trough behind the thumb).
    /// Win95 uses a subtle dither pattern here — we approximate with solid color.
    fn draw_track(&self, fill: &mut dyn FnMut(usize, usize, usize, usize, u32)) {
        let (tx, ty, tw, th) = self.track_rect();
        // — EchoFrame: track gets a slightly darker face color, sunken look
        fill(tx, ty, tw, th, COL_TRACK);
    }

    /// — EchoFrame: draw the draggable thumb with full Win95 3D bevels.
    fn draw_thumb(&self, fill: &mut dyn FnMut(usize, usize, usize, usize, u32)) {
        if self.track_len == 0 { return; }
        if self.content.total <= self.content.visible { return; }

        let (tx, ty, tw, th) = self.thumb_rect();
        if tw == 0 || th == 0 { return; }

        let face = match self.thumb_state {
            PartState::Normal => COL_FACE,
            PartState::Hovered => COL_HOVER_FACE,
            PartState::Pressed => COL_PRESSED_FACE,
        };

        if self.thumb_state == PartState::Pressed {
            // — EchoFrame: pressed thumb gets inverted bevel (sunken look)
            draw_sunken_rect(fill, tx, ty, tw, th, face);
        } else {
            // — EchoFrame: normal/hover thumb gets raised bevel
            draw_raised_rect(fill, tx, ty, tw, th, face);
        }

        // — EchoFrame: grip lines on the thumb (3 small ridges in the center)
        self.draw_thumb_grip(fill, tx, ty, tw, th);
    }

    /// — EchoFrame: those little grip lines in the center of the thumb.
    /// Three pairs of highlight/shadow lines — the signature Win95 touch.
    fn draw_thumb_grip(&self, fill: &mut dyn FnMut(usize, usize, usize, usize, u32),
                        tx: usize, ty: usize, tw: usize, th: usize) {
        match self.orientation {
            Orientation::Vertical => {
                if th < 12 { return; } // — EchoFrame: too small for grip lines, skip
                let cx = tx + tw / 2;
                let cy = ty + th / 2;
                let half_w = (tw as i32 / 2 - 4).max(1) as usize;
                let x_start = cx.saturating_sub(half_w);
                let grip_w = half_w * 2;
                // — EchoFrame: three horizontal grip lines, 2px apart
                for i in 0..3 {
                    let gy = (cy as i32 - 2 + i * 2) as usize;
                    fill(x_start, gy, grip_w, 1, COL_HIGHLIGHT);
                    fill(x_start, gy + 1, grip_w, 1, COL_SHADOW);
                }
            }
            Orientation::Horizontal => {
                if tw < 12 { return; }
                let cx = tx + tw / 2;
                let cy = ty + th / 2;
                let half_h = (th as i32 / 2 - 4).max(1) as usize;
                let y_start = cy.saturating_sub(half_h);
                let grip_h = half_h * 2;
                for i in 0..3 {
                    let gx = (cx as i32 - 2 + i * 2) as usize;
                    fill(gx, y_start, 1, grip_h, COL_HIGHLIGHT);
                    fill(gx + 1, y_start, 1, grip_h, COL_SHADOW);
                }
            }
        }
    }

    /// — EchoFrame: draw the decrement arrow button (up / left).
    fn draw_arrow_dec(&self, fill: &mut dyn FnMut(usize, usize, usize, usize, u32)) {
        let (bx, by, bw, bh) = self.arrow_dec_rect();
        let face = match self.arrow_dec_state {
            PartState::Normal => COL_FACE,
            PartState::Hovered => COL_HOVER_FACE,
            PartState::Pressed => COL_PRESSED_FACE,
        };

        if self.arrow_dec_state == PartState::Pressed {
            draw_sunken_rect(fill, bx, by, bw, bh, face);
        } else {
            draw_raised_rect(fill, bx, by, bw, bh, face);
        }

        // — EchoFrame: draw arrow glyph (offset 1px when pressed for that satisfying click)
        let offset = if self.arrow_dec_state == PartState::Pressed { 1i32 } else { 0 };
        match self.orientation {
            Orientation::Vertical => {
                draw_arrow_up(fill, bx as i32 + offset, by as i32 + offset, bw, bh);
            }
            Orientation::Horizontal => {
                draw_arrow_left(fill, bx as i32 + offset, by as i32 + offset, bw, bh);
            }
        }
    }

    /// — EchoFrame: draw the increment arrow button (down / right).
    fn draw_arrow_inc(&self, fill: &mut dyn FnMut(usize, usize, usize, usize, u32)) {
        let (bx, by, bw, bh) = self.arrow_inc_rect();
        let face = match self.arrow_inc_state {
            PartState::Normal => COL_FACE,
            PartState::Hovered => COL_HOVER_FACE,
            PartState::Pressed => COL_PRESSED_FACE,
        };

        if self.arrow_inc_state == PartState::Pressed {
            draw_sunken_rect(fill, bx, by, bw, bh, face);
        } else {
            draw_raised_rect(fill, bx, by, bw, bh, face);
        }

        let offset = if self.arrow_inc_state == PartState::Pressed { 1i32 } else { 0 };
        match self.orientation {
            Orientation::Vertical => {
                draw_arrow_down(fill, bx as i32 + offset, by as i32 + offset, bw, bh);
            }
            Orientation::Horizontal => {
                draw_arrow_right(fill, bx as i32 + offset, by as i32 + offset, bw, bh);
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Geometry helpers — screen-space rects for each scrollbar part
    // ═══════════════════════════════════════════════════════════════════════

    /// Decrement arrow button rect (top for vert, left for horiz)
    fn arrow_dec_rect(&self) -> (usize, usize, usize, usize) {
        match self.orientation {
            Orientation::Vertical => {
                (self.x as usize, self.y as usize,
                 self.width as usize, ARROW_SIZE as usize)
            }
            Orientation::Horizontal => {
                (self.x as usize, self.y as usize,
                 ARROW_SIZE as usize, self.height as usize)
            }
        }
    }

    /// Increment arrow button rect (bottom for vert, right for horiz)
    fn arrow_inc_rect(&self) -> (usize, usize, usize, usize) {
        match self.orientation {
            Orientation::Vertical => {
                let by = self.y + self.height.saturating_sub(ARROW_SIZE);
                (self.x as usize, by as usize,
                 self.width as usize, ARROW_SIZE as usize)
            }
            Orientation::Horizontal => {
                let bx = self.x + self.width.saturating_sub(ARROW_SIZE);
                (bx as usize, self.y as usize,
                 ARROW_SIZE as usize, self.height as usize)
            }
        }
    }

    /// Track channel rect (between arrows)
    fn track_rect(&self) -> (usize, usize, usize, usize) {
        match self.orientation {
            Orientation::Vertical => {
                (self.x as usize, (self.y + self.track_start) as usize,
                 self.width as usize, self.track_len as usize)
            }
            Orientation::Horizontal => {
                ((self.x + self.track_start) as usize, self.y as usize,
                 self.track_len as usize, self.height as usize)
            }
        }
    }

    /// Thumb rect (within track)
    fn thumb_rect(&self) -> (usize, usize, usize, usize) {
        match self.orientation {
            Orientation::Vertical => {
                let ty = self.y + self.track_start + self.thumb_pos;
                (self.x as usize, ty as usize,
                 self.width as usize, self.thumb_size as usize)
            }
            Orientation::Horizontal => {
                let tx = self.x + self.track_start + self.thumb_pos;
                (tx as usize, self.y as usize,
                 self.thumb_size as usize, self.height as usize)
            }
        }
    }

    /// — EchoFrame: get the track length for drag calculations
    pub fn track_pixel_length(&self) -> u32 {
        self.track_len
    }

    /// — EchoFrame: get the usable track (track_len - thumb_size) for drag math
    pub fn usable_track(&self) -> u32 {
        self.track_len.saturating_sub(self.thumb_size)
    }

    /// — EchoFrame: get the track start screen coordinate (for drag offset calculation)
    pub fn track_screen_start(&self) -> u32 {
        match self.orientation {
            Orientation::Vertical => self.y + self.track_start,
            Orientation::Horizontal => self.x + self.track_start,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3D Bevel Drawing — EchoFrame: the Win95 aesthetic in pure pixel manipulation
// ═══════════════════════════════════════════════════════════════════════════════

/// — EchoFrame: draw a raised 3D rectangle (button look).
/// Outer highlight on top-left, dark shadow on bottom-right.
/// Inner light on top-left, shadow on bottom-right.
fn draw_raised_rect(
    fill: &mut dyn FnMut(usize, usize, usize, usize, u32),
    x: usize, y: usize, w: usize, h: usize,
    face_color: u32,
) {
    if w < 4 || h < 4 { // — EchoFrame: too small for bevels, just fill solid
        fill(x, y, w, h, face_color);
        return;
    }

    // Face fill
    fill(x + 2, y + 2, w - 4, h - 4, face_color);

    // — EchoFrame: outer highlight (top + left edges)
    fill(x, y, w, 1, COL_HIGHLIGHT);       // top
    fill(x, y, 1, h, COL_HIGHLIGHT);       // left

    // — EchoFrame: inner light (1px inside top-left)
    fill(x + 1, y + 1, w - 2, 1, COL_LIGHT); // top inner
    fill(x + 1, y + 1, 1, h - 2, COL_LIGHT); // left inner

    // — EchoFrame: outer dark shadow (bottom + right edges)
    fill(x, y + h - 1, w, 1, COL_DARK_SHADOW);     // bottom
    fill(x + w - 1, y, 1, h, COL_DARK_SHADOW);     // right

    // — EchoFrame: inner shadow (1px inside bottom-right)
    fill(x + 1, y + h - 2, w - 2, 1, COL_SHADOW);  // bottom inner
    fill(x + w - 2, y + 1, 1, h - 2, COL_SHADOW);  // right inner
}

/// — EchoFrame: draw a sunken 3D rectangle (pressed button / track).
/// Shadow on top-left, highlight on bottom-right — the inverse of raised.
fn draw_sunken_rect(
    fill: &mut dyn FnMut(usize, usize, usize, usize, u32),
    x: usize, y: usize, w: usize, h: usize,
    face_color: u32,
) {
    if w < 4 || h < 4 {
        fill(x, y, w, h, face_color);
        return;
    }

    // Face fill
    fill(x + 2, y + 2, w - 4, h - 4, face_color);

    // — EchoFrame: outer shadow (top + left — inverted from raised)
    fill(x, y, w, 1, COL_SHADOW);
    fill(x, y, 1, h, COL_SHADOW);

    // — EchoFrame: inner dark (1px inside top-left)
    fill(x + 1, y + 1, w - 2, 1, COL_DARK_SHADOW);
    fill(x + 1, y + 1, 1, h - 2, COL_DARK_SHADOW);

    // — EchoFrame: outer highlight (bottom + right)
    fill(x, y + h - 1, w, 1, COL_HIGHLIGHT);
    fill(x + w - 1, y, 1, h, COL_HIGHLIGHT);

    // — EchoFrame: inner light (1px inside bottom-right)
    fill(x + 1, y + h - 2, w - 2, 1, COL_LIGHT);
    fill(x + w - 2, y + 1, 1, h - 2, COL_LIGHT);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Arrow Glyphs — EchoFrame: tiny triangles rendered pixel by pixel
// ═══════════════════════════════════════════════════════════════════════════════

/// — EchoFrame: draw an upward-pointing arrow glyph centered in the given rect.
fn draw_arrow_up(fill: &mut dyn FnMut(usize, usize, usize, usize, u32),
                  bx: i32, by: i32, bw: usize, bh: usize) {
    // — EchoFrame: 5-row triangle, centered. Each row is 1+2*row pixels wide.
    let cx = bx + bw as i32 / 2;
    let cy = by + bh as i32 / 2 - 2;
    for row in 0..5i32 {
        let y = cy + row;
        let half = row;
        let x_start = cx - half;
        if y >= 0 && x_start >= 0 {
            fill(x_start as usize, y as usize, (1 + half * 2) as usize, 1, COL_ARROW_GLYPH);
        }
    }
}

/// — EchoFrame: draw a downward-pointing arrow glyph centered in the given rect.
fn draw_arrow_down(fill: &mut dyn FnMut(usize, usize, usize, usize, u32),
                    bx: i32, by: i32, bw: usize, bh: usize) {
    let cx = bx + bw as i32 / 2;
    let cy = by + bh as i32 / 2 + 2;
    for row in 0..5i32 {
        let y = cy - row;
        let half = row;
        let x_start = cx - half;
        if y >= 0 && x_start >= 0 {
            fill(x_start as usize, y as usize, (1 + half * 2) as usize, 1, COL_ARROW_GLYPH);
        }
    }
}

/// — EchoFrame: draw a left-pointing arrow glyph centered in the given rect.
fn draw_arrow_left(fill: &mut dyn FnMut(usize, usize, usize, usize, u32),
                    bx: i32, by: i32, bw: usize, bh: usize) {
    let cx = bx + bw as i32 / 2 - 2;
    let cy = by + bh as i32 / 2;
    for col in 0..5i32 {
        let x = cx + col;
        let half = col;
        let y_start = cy - half;
        if x >= 0 && y_start >= 0 {
            fill(x as usize, y_start as usize, 1, (1 + half * 2) as usize, COL_ARROW_GLYPH);
        }
    }
}

/// — EchoFrame: draw a right-pointing arrow glyph centered in the given rect.
fn draw_arrow_right(fill: &mut dyn FnMut(usize, usize, usize, usize, u32),
                     bx: i32, by: i32, bw: usize, bh: usize) {
    let cx = bx + bw as i32 / 2 + 2;
    let cy = by + bh as i32 / 2;
    for col in 0..5i32 {
        let x = cx - col;
        let half = col;
        let y_start = cy - half;
        if x >= 0 && y_start >= 0 {
            fill(x as usize, y_start as usize, 1, (1 + half * 2) as usize, COL_ARROW_GLYPH);
        }
    }
}
