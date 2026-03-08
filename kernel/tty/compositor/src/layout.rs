//! Layout Manager — screen real estate is the only finite resource that matters.
//! — GlassSignal

/// Maximum VTs the compositor can address (compile-time array ceiling).
/// — GlassSignal: all VTs up to this limit can be created on demand.
/// Change this only if you need more than 6 VT slots system-wide.
pub const MAX_VTS: usize = 6;

/// Maximum number of visible tiles in any layout
pub const MAX_TILES: usize = 4;

/// — EchoFrame: scrollbar dimensions in pixels. 16px like Win95 intended.
/// Vertical scrollbar occupies right edge, horizontal occupies bottom edge.
pub const SCROLLBAR_WIDTH: u32 = 16;
pub const SCROLLBAR_HEIGHT: u32 = 16;
pub const SCROLLBAR_THUMB_MIN: u32 = 24;

/// — GlassSignal: per-VT scrollbar visibility flags.
/// Compositor sets these based on terminal state (scrollback len, wrap mode).
#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollbarFlags {
    /// Vertical scrollbar visible (always true for text VTs — track always shown)
    pub vscroll: bool,
    /// Horizontal scrollbar visible (only when wrap mode OFF and content wider than viewport)
    pub hscroll: bool,
}

/// Screen layout modes
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    /// Single VT fills entire screen
    Fullscreen,
    /// Two VTs stacked vertically (top/bottom)
    HSplit,
    /// Two VTs side by side (left/right)
    VSplit,
    /// Four VTs in a 2×2 grid
    Quad,
}

/// A viewport rectangle in physical screen coordinates
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Viewport {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Viewport { x, y, width, height }
    }

    /// Terminal grid dimensions for this viewport given font cell size
    pub fn terminal_cols(&self, cell_width: u32) -> u32 {
        if cell_width == 0 { return 0; }
        self.width / cell_width
    }

    pub fn terminal_rows(&self, cell_height: u32) -> u32 {
        if cell_height == 0 { return 0; }
        self.height / cell_height
    }
}

/// — GlassSignal: Full geometry for a VT's viewport — everything an app or the
/// compositor needs to know about where this VT lives and how big it is.
/// Screen position is compositor-internal. Apps only see usable dimensions.
#[derive(Clone, Copy, Debug)]
pub struct ViewportGeometry {
    /// Position on hardware FB (compositor-internal, apps never see this)
    pub screen_x: u32,
    pub screen_y: u32,

    /// Full viewport including chrome
    pub total_width: u32,
    pub total_height: u32,

    /// Chrome dimensions (borders, future title bar, future scrollbar)
    pub border_top: u32,
    pub border_bottom: u32,
    pub border_left: u32,
    pub border_right: u32,

    /// Content area = total - chrome (this is what apps see as their VFB size)
    pub usable_width: u32,
    pub usable_height: u32,

    /// Text grid derived from usable area ÷ font size
    pub text_cols: u32,
    pub text_rows: u32,
}

impl ViewportGeometry {
    /// Compute geometry from a viewport rectangle, chrome widths, and font metrics.
    /// — GlassSignal: pure math, no allocations, no side effects
    pub fn from_viewport(
        vp: &Viewport,
        border_top: u32,
        border_bottom: u32,
        border_left: u32,
        border_right: u32,
        cell_width: u32,
        cell_height: u32,
    ) -> Self {
        let chrome_h = border_top + border_bottom;
        let chrome_w = border_left + border_right;
        let usable_width = vp.width.saturating_sub(chrome_w);
        let usable_height = vp.height.saturating_sub(chrome_h);
        let text_cols = if cell_width > 0 { usable_width / cell_width } else { 0 };
        let text_rows = if cell_height > 0 { usable_height / cell_height } else { 0 };

        ViewportGeometry {
            screen_x: vp.x,
            screen_y: vp.y,
            total_width: vp.width,
            total_height: vp.height,
            border_top,
            border_bottom,
            border_left,
            border_right,
            usable_width,
            usable_height,
            text_cols,
            text_rows,
        }
    }
}

/// — GlassSignal: Manages which VTs are visible and where they appear on screen.
/// Computes viewport rectangles from layout mode + screen dimensions.
pub struct LayoutManager {
    /// Current layout mode
    layout: Layout,
    /// Which VTs are assigned to tile slots (up to 4)
    /// Slot 0 = primary (or top-left), Slot 1 = secondary (or top-right), etc.
    slots: [usize; MAX_TILES],
    /// Focused slot index (receives keyboard input)
    focused_slot: usize,
    /// Physical screen dimensions
    screen_width: u32,
    screen_height: u32,
    /// Previous layout (for Alt+Enter toggle)
    prev_layout: Layout,
    prev_slots: [usize; MAX_TILES],
}

impl LayoutManager {
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        LayoutManager {
            layout: Layout::Fullscreen,
            slots: [0, 1, 2, 3],
            focused_slot: 0,
            screen_width,
            screen_height,
            prev_layout: Layout::Fullscreen,
            prev_slots: [0, 1, 2, 3],
        }
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// Number of visible tiles in current layout
    pub fn tile_count(&self) -> usize {
        match self.layout {
            Layout::Fullscreen => 1,
            Layout::HSplit | Layout::VSplit => 2,
            Layout::Quad => 4,
        }
    }

    /// Get the VT index assigned to a given tile slot
    pub fn slot_vt(&self, slot: usize) -> usize {
        self.slots[slot.min(MAX_TILES - 1)]
    }

    /// Get the focused VT index
    pub fn focused_vt(&self) -> usize {
        self.slots[self.focused_slot]
    }

    pub fn focused_slot(&self) -> usize {
        self.focused_slot
    }

    /// Compute viewport rectangles for all visible tiles
    /// — GlassSignal: pure geometry, no side effects, no locks
    pub fn compute_viewports(&self) -> [(usize, Viewport); MAX_TILES] {
        let w = self.screen_width;
        let h = self.screen_height;
        // — GlassSignal: 2px border between tiles, subtracted from tile dimensions
        let border = if self.layout == Layout::Fullscreen { 0 } else { 2 };

        let mut result = [(0usize, Viewport::new(0, 0, 0, 0)); MAX_TILES];

        match self.layout {
            Layout::Fullscreen => {
                result[0] = (self.slots[0], Viewport::new(0, 0, w, h));
            }
            Layout::HSplit => {
                // — GlassSignal: top half / bottom half, 2px border between
                let half_h = (h - border) / 2;
                result[0] = (self.slots[0], Viewport::new(0, 0, w, half_h));
                result[1] = (self.slots[1], Viewport::new(0, half_h + border, w, h - half_h - border));
            }
            Layout::VSplit => {
                // — GlassSignal: left half / right half, 2px border between
                let half_w = (w - border) / 2;
                result[0] = (self.slots[0], Viewport::new(0, 0, half_w, h));
                result[1] = (self.slots[1], Viewport::new(half_w + border, 0, w - half_w - border, h));
            }
            Layout::Quad => {
                // — GlassSignal: 2×2 grid with borders
                let half_w = (w - border) / 2;
                let half_h = (h - border) / 2;
                result[0] = (self.slots[0], Viewport::new(0, 0, half_w, half_h));
                result[1] = (self.slots[1], Viewport::new(half_w + border, 0, w - half_w - border, half_h));
                result[2] = (self.slots[2], Viewport::new(0, half_h + border, half_w, h - half_h - border));
                result[3] = (self.slots[3], Viewport::new(half_w + border, half_h + border, w - half_w - border, h - half_h - border));
            }
        }
        result
    }

    /// Set layout mode. Saves previous layout for toggle.
    pub fn set_layout(&mut self, layout: Layout) {
        if layout == self.layout {
            return;
        }
        self.prev_layout = self.layout;
        self.prev_slots = self.slots;
        self.layout = layout;
        // — GlassSignal: clamp focused_slot to valid range for new layout
        let max_slot = self.tile_count().saturating_sub(1);
        if self.focused_slot > max_slot {
            self.focused_slot = max_slot;
        }
    }

    /// Toggle between fullscreen and last split layout (Alt+Enter)
    pub fn toggle_fullscreen(&mut self) {
        if self.layout == Layout::Fullscreen {
            let restore = self.prev_layout;
            let restore_slots = self.prev_slots;
            self.prev_layout = Layout::Fullscreen;
            self.prev_slots = self.slots;
            self.layout = if restore == Layout::Fullscreen { Layout::VSplit } else { restore };
            self.slots = restore_slots;
        } else {
            self.prev_layout = self.layout;
            self.prev_slots = self.slots;
            self.layout = Layout::Fullscreen;
        }
        let max_slot = self.tile_count().saturating_sub(1);
        if self.focused_slot > max_slot {
            self.focused_slot = max_slot;
        }
    }

    /// Cycle focus to next visible tile (Alt+Tab)
    pub fn cycle_focus(&mut self) {
        let count = self.tile_count();
        if count > 1 {
            self.focused_slot = (self.focused_slot + 1) % count;
        }
    }

    /// Focus a specific VT. If it's visible, focus its tile. If not, swap it
    /// into the focused slot (fullscreen) or the focused tile (split).
    /// — GlassSignal: VTs are created on demand by the compositor, so any
    /// index < MAX_VTS is valid. The compositor allocates the backing buffer.
    pub fn focus_vt(&mut self, vt_num: usize) {
        if vt_num >= MAX_VTS {
            return;
        }
        // — GlassSignal: check if the VT is already visible in a tile
        let count = self.tile_count();
        for i in 0..count {
            if self.slots[i] == vt_num {
                self.focused_slot = i;
                return;
            }
        }
        // — GlassSignal: VT not visible — swap it into the focused slot
        self.slots[self.focused_slot] = vt_num;
    }

    /// Assign a VT to a specific tile slot (Alt+Shift+Fn)
    pub fn assign_vt_to_slot(&mut self, slot: usize, vt_num: usize) {
        if slot < self.tile_count() && vt_num < MAX_VTS {
            self.slots[slot] = vt_num;
        }
    }

    /// Update screen dimensions (e.g., on mode switch)
    pub fn update_screen_size(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    /// — GlassSignal: Compute per-VT geometries from current layout + screen size.
    /// Returns array of Option<ViewportGeometry> — None for off-screen VTs.
    /// This is the single source of truth for VFB dimensions, text grid sizes,
    /// and compositor blit positions.
    /// scrollbar_flags controls per-VT scrollbar chrome (eats into usable area).
    pub fn recompute_geometries(
        &self,
        cell_width: u32,
        cell_height: u32,
        scrollbar_flags: &[ScrollbarFlags; MAX_VTS],
    ) -> [Option<ViewportGeometry>; MAX_VTS] {
        let viewports = self.compute_viewports();
        let tile_count = self.tile_count();
        let mut result: [Option<ViewportGeometry>; MAX_VTS] = [None; MAX_VTS];

        // — GlassSignal: chrome for focused vs unfocused tiles in split modes.
        // Focused tile gets a 1px highlight border. Unfocused gets nothing extra.
        // The 2px gap between tiles is handled by compute_viewports() already.
        for slot_idx in 0..tile_count {
            let (vt_idx, viewport) = viewports[slot_idx];
            if vt_idx >= MAX_VTS || viewport.width == 0 || viewport.height == 0 {
                continue;
            }

            // — GlassSignal: focus highlight border eats 1px from each edge
            // of the focused tile in split modes. Fullscreen gets no border.
            let is_focused = slot_idx == self.focused_slot;
            let has_chrome = tile_count > 1 && is_focused;
            let chrome = if has_chrome { 1 } else { 0 };

            // — GlassSignal: scrollbar chrome eats from right and/or bottom edge
            let sb = scrollbar_flags[vt_idx];
            let border_right = chrome + if sb.vscroll { SCROLLBAR_WIDTH } else { 0 };
            let border_bottom = chrome + if sb.hscroll { SCROLLBAR_HEIGHT } else { 0 };

            result[vt_idx] = Some(ViewportGeometry::from_viewport(
                &viewport,
                chrome, border_bottom, chrome, border_right,
                cell_width,
                cell_height,
            ));
        }

        result
    }

    pub fn screen_width(&self) -> u32 {
        self.screen_width
    }

    pub fn screen_height(&self) -> u32 {
        self.screen_height
    }
}
