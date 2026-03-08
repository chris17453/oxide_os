//! Compositor Event System — the thin membrane between raw input and pixel truth.
//! — GlassSignal: every mouse click, drag, and scroll wheel tick passes through
//! here. The compositor knows where everything is on screen. Console.rs just
//! forwards raw coordinates and gets back "I handled it" or "your problem now."

use crate::layout::{ViewportGeometry, SCROLLBAR_WIDTH, SCROLLBAR_HEIGHT, MAX_VTS};

/// — GlassSignal: what kind of screen region the mouse landed on.
/// The compositor owns all geometry, so only it can answer this question.
#[derive(Clone, Copy, Debug)]
pub enum HitZone {
    /// Content area of a VT — coordinates are screen-global
    VtContent { vt: usize },
    /// Vertical scrollbar track of a VT
    VScrollbar { vt: usize },
    /// Horizontal scrollbar track of a VT
    HScrollbar { vt: usize },
    /// Border/divider between tiles
    Border,
    /// Scrollbar corner dead zone (where V and H meet)
    ScrollbarCorner { vt: usize },
    /// Outside any known region (shouldn't happen but hardware is cursed)
    None,
}

/// — GlassSignal: mouse buttons that the event system cares about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

/// — GlassSignal: what the compositor decided to do with your event.
/// Console.rs reads this and acts accordingly — no geometry math needed.
#[derive(Clone, Copy, Debug)]
pub enum MouseAction {
    /// Compositor consumed the event entirely (scrollbar interaction, border click)
    Consumed,
    /// Event is in VT content area — forward to terminal selection or mouse mode.
    /// (vt, screen_x, screen_y) — console.rs converts to terminal coords as needed.
    ForwardToTerminal { vt: usize },
    /// Nothing happened (event outside any interactive region, or compositor not ready)
    Nothing,
}

/// — GlassSignal: internal scrollbar drag tracking. Compositor owns this because
/// it owns the geometry that makes drag deltas meaningful.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DragState {
    /// Which VT's scrollbar is being dragged
    pub vt: usize,
    /// Vertical (true) or horizontal (false) scrollbar
    pub vertical: bool,
    /// Screen coordinate where drag started (Y for vertical, X for horizontal)
    pub start_pos: i32,
    /// Scroll offset at drag start (lines for vert, cols for horiz)
    pub start_offset: usize,
    /// Track length in pixels (for proportional drag calculation)
    pub track_length: usize,
    /// Total scrollable content (lines for vert, cols for horiz)
    pub total_content: usize,
    /// Visible content (rows for vert, cols for horiz)
    pub visible_content: usize,
}

/// — GlassSignal: compositor mouse state machine. Tracks what's happening
/// so we know the difference between "click on scrollbar" and "drag scrollbar."
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum MouseState {
    /// No buttons pressed, just vibing
    Idle,
    /// Left button down in VT content (terminal selection or mouse mode)
    ContentPress,
    /// Left button down, dragging a scrollbar
    ScrollbarDrag,
}

/// — GlassSignal: The mouse event handler state. Lives inside the Compositor
/// because it needs geometry access. No separate locks — uses compositor lock.
pub(crate) struct EventHandler {
    pub state: MouseState,
    pub drag: Option<DragState>,
    /// Last known left button state (for move events)
    pub left_pressed: bool,
    pub middle_pressed: bool,
}

impl EventHandler {
    pub fn new() -> Self {
        EventHandler {
            state: MouseState::Idle,
            drag: None,
            left_pressed: false,
            middle_pressed: false,
        }
    }
}

/// — GlassSignal: hit-test a screen coordinate against all visible VT regions.
/// Returns the most specific zone (scrollbar > content > border > none).
pub(crate) fn hit_test(
    x: i32, y: i32,
    geometries: &[Option<ViewportGeometry>; MAX_VTS],
    scrollbar_flags: &[crate::layout::ScrollbarFlags; MAX_VTS],
    tile_vts: &[(usize, bool)],  // (vt_num, is_visible) for each tile slot
) -> HitZone {
    let ux = x as u32;
    let uy = y as u32;

    for &(vt_idx, visible) in tile_vts {
        if !visible { continue; }
        let geom = match geometries[vt_idx] {
            Some(g) => g,
            None => continue,
        };

        // — GlassSignal: check if point is within this VT's total viewport
        let vp_left = geom.screen_x;
        let vp_top = geom.screen_y;
        let vp_right = geom.screen_x + geom.total_width;
        let vp_bottom = geom.screen_y + geom.total_height;

        if ux < vp_left || ux >= vp_right || uy < vp_top || uy >= vp_bottom {
            continue;
        }

        let flags = scrollbar_flags[vt_idx];

        // — GlassSignal: scrollbar corner (dead zone where both bars meet)
        if flags.vscroll && flags.hscroll {
            let corner_x = vp_right.saturating_sub(SCROLLBAR_WIDTH);
            let corner_y = vp_bottom.saturating_sub(SCROLLBAR_HEIGHT);
            if ux >= corner_x && uy >= corner_y {
                return HitZone::ScrollbarCorner { vt: vt_idx };
            }
        }

        // — GlassSignal: vertical scrollbar (right edge strip)
        if flags.vscroll {
            let sb_x = vp_right.saturating_sub(SCROLLBAR_WIDTH);
            let sb_y_top = geom.screen_y + geom.border_top;
            let sb_y_bot = sb_y_top + geom.usable_height;
            if ux >= sb_x && ux < vp_right && uy >= sb_y_top && uy < sb_y_bot {
                return HitZone::VScrollbar { vt: vt_idx };
            }
        }

        // — GlassSignal: horizontal scrollbar (bottom edge strip)
        if flags.hscroll {
            let sb_y = vp_bottom.saturating_sub(SCROLLBAR_HEIGHT);
            let sb_x_left = geom.screen_x + geom.border_left;
            let sb_x_right = sb_x_left + geom.usable_width;
            if uy >= sb_y && uy < vp_bottom && ux >= sb_x_left && ux < sb_x_right {
                return HitZone::HScrollbar { vt: vt_idx };
            }
        }

        // — GlassSignal: content area (everything else inside the viewport)
        return HitZone::VtContent { vt: vt_idx };
    }

    // — GlassSignal: not in any VT viewport — must be a border or dead pixel
    HitZone::None
}
