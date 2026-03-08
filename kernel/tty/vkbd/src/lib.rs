//! Virtual On-Screen Keyboard for OXIDE OS
//!
//! — InputShade: When there's no hardware keyboard, you paint your own.
//! Renders a standard QWERTY layout with nav cluster and numpad overlay
//! on top of the compositor output. Touch/click a key, bytes go into the
//! active VT's input ring.
//!
//! Layout: Main QWERTY (left-aligned) | Arrow/Nav cluster | Numpad
//! Toggle with Alt+K. Draws directly on the hardware framebuffer after
//! compositor blit, before mouse cursor.

#![cfg_attr(not(test), no_std)]

use core::sync::atomic::{AtomicBool, Ordering};
use fb::{Framebuffer, PixelFormat};
use fb::font::PSF2_FONT;

// ═══════════════════════════════════════════════════════════════════
//  Layout Constants
// ═══════════════════════════════════════════════════════════════════

/// — InputShade: key dimensions tuned for 1280x800 display.
const KEY_W: u32 = 40;
const KEY_H: u32 = 32;
const KEY_GAP: u32 = 2;
const KEY_UNIT: u32 = KEY_W + KEY_GAP;  // 42px per key slot
const KB_PADDING: u32 = 4;

/// Number of keyboard rows (F-key row + 5 main rows)
const KB_ROWS: usize = 6;

/// Total keyboard height including padding
const KB_HEIGHT: u32 = KB_ROWS as u32 * (KEY_H + KEY_GAP) + KB_PADDING * 2;

/// — InputShade: section layout. Standard keyboard: main left-aligned,
/// nav cluster offset to the right, numpad on far right. Like IBM Model M.
/// Pixel gaps between sections, not unit gaps — real keyboards don't have
/// key-width chasms between sections. That looked like shit. — SableWire
const LEFT_MARGIN: u32 = 8;
const MAIN_UNITS: u32 = 15;     // max width of main section
const NAV_WIDTH_UNITS: u32 = 3; // nav cluster width (Ins/Home/PgU, arrows)
/// — InputShade: numpad section width. Used in tests and section positioning.
pub const NUMPAD_WIDTH_UNITS: u32 = 4;
const SECTION_GAP_PX: u32 = 16; // 16px gap between sections (like a real keyboard)

/// — InputShade: computed section X positions in pixels.
const NAV_X: u32 = LEFT_MARGIN + MAIN_UNITS * KEY_UNIT + SECTION_GAP_PX;
const NUMPAD_X: u32 = NAV_X + NAV_WIDTH_UNITS * KEY_UNIT + SECTION_GAP_PX;

/// — InputShade: F-key row sub-grouping gaps (Esc | F1-F4 | F5-F8 | F9-F12).
/// Real keyboards have visible gaps between F-key groups. 12px per gap.
const FK_GROUP_GAP: u32 = 12;

/// Background color (dark gray) — ARGB
const BG_COLOR: u32 = 0xFF1A1A2E;
/// Key color (medium gray) — ARGB
/// — InputShade: bumped from 0x2D2D44 to better contrast against 0x1A1A2E background
const KEY_COLOR: u32 = 0xFF3A3A52;
/// Function key row color (distinct from main keys) — ARGB
/// — InputShade: F-keys use a slightly lighter shade so the row is clearly
/// visible as a separate functional group, like a real keyboard
const FK_COLOR: u32 = 0xFF484868;
/// Nav cluster key color — slightly different tint for visual grouping
const NAV_COLOR: u32 = 0xFF3A4A52;
/// Numpad key color — cool-tinted to separate from main keys
const NUMPAD_COLOR: u32 = 0xFF3A3A5A;
/// Key hovered color — between normal and pressed
const KEY_HOVER_COLOR: u32 = 0xFF4A4A66;
/// Key pressed color (lighter) — ARGB
const KEY_PRESSED_COLOR: u32 = 0xFF5A5A7A;
/// Key text color (white) — ARGB
const TEXT_COLOR: u32 = 0xFFE0E0E0;
/// Modifier active color (accent blue) — ARGB
const MOD_ACTIVE_COLOR: u32 = 0xFF0088CC;

// ═══════════════════════════════════════════════════════════════════
//  Key Definition
// ═══════════════════════════════════════════════════════════════════

/// — InputShade: a single key on the virtual keyboard.
/// Output bytes are what gets pushed into the VT input ring.
/// Width is in key-units (1 = normal, 2 = double-wide, etc.)
#[derive(Clone, Copy)]
struct VKey {
    /// Display label (lowercase)
    label: &'static [u8],
    /// Display label (shifted/caps)
    shifted_label: &'static [u8],
    /// Output bytes (lowercase)
    output: &'static [u8],
    /// Output bytes (shifted)
    shifted_output: &'static [u8],
    /// Width in key units (1 = normal key)
    width: u8,
    /// Is this a modifier key (shift, ctrl, etc.)
    is_modifier: bool,
}

impl VKey {
    const fn normal(label: &'static [u8], shifted: &'static [u8],
                    out: &'static [u8], shifted_out: &'static [u8]) -> Self {
        VKey { label, shifted_label: shifted, output: out,
               shifted_output: shifted_out, width: 1, is_modifier: false }
    }

    const fn wide(label: &'static [u8], out: &'static [u8], w: u8) -> Self {
        VKey { label, shifted_label: label, output: out,
               shifted_output: out, width: w, is_modifier: false }
    }

    const fn modifier(label: &'static [u8], w: u8) -> Self {
        VKey { label, shifted_label: label, output: &[],
               shifted_output: &[], width: w, is_modifier: true }
    }
}

// ═══════════════════════════════════════════════════════════════════
//  Keyboard Row — three sections per row
// ═══════════════════════════════════════════════════════════════════

/// — InputShade: a keyboard row with up to 3 sections.
/// Main section left-aligned, nav cluster offset right, numpad far right.
/// Per-row pixel offsets simulate the stagger of a real keyboard.
struct KbRow {
    main: &'static [VKey],
    nav: &'static [VKey],
    numpad: &'static [VKey],
    /// — InputShade: pixel offset added to main section start (row stagger)
    main_px: u32,
    /// — InputShade: pixel offset added to nav section start (center Up arrow)
    nav_px: u32,
}

impl KbRow {
    const fn new(main: &'static [VKey], nav: &'static [VKey], numpad: &'static [VKey]) -> Self {
        KbRow { main, nav, numpad, main_px: 0, nav_px: 0 }
    }

    const fn stagger(main: &'static [VKey], nav: &'static [VKey], numpad: &'static [VKey],
                     main_px: u32, nav_px: u32) -> Self {
        KbRow { main, nav, numpad, main_px, nav_px }
    }
}

// ═══════════════════════════════════════════════════════════════════
//  Standard QWERTY Layout — IBM Model M spirit, InputShade execution
// ═══════════════════════════════════════════════════════════════════

/// — InputShade: 6-row layout. Each row has main + nav + numpad sections.
/// Main: 15 units max. Nav: 3 units. Numpad: 4 units.
/// Sections at fixed X offsets so columns align vertically.
static KB_LAYOUT: [KbRow; KB_ROWS] = [
    // ── Row 0: F-key row ──────────────────────────────────────────
    // — InputShade: F1-F12 in main section. Nav and numpad empty.
    KbRow::new(
        &[
            VKey::wide(b"Esc", b"\x1b", 1),
            VKey::wide(b"F1",  b"\x1bOP",  1),
            VKey::wide(b"F2",  b"\x1bOQ",  1),
            VKey::wide(b"F3",  b"\x1bOR",  1),
            VKey::wide(b"F4",  b"\x1bOS",  1),
            VKey::wide(b"F5",  b"\x1b[15~", 1),
            VKey::wide(b"F6",  b"\x1b[17~", 1),
            VKey::wide(b"F7",  b"\x1b[18~", 1),
            VKey::wide(b"F8",  b"\x1b[19~", 1),
            VKey::wide(b"F9",  b"\x1b[20~", 1),
            VKey::wide(b"F10", b"\x1b[21~", 1),
            VKey::wide(b"F11", b"\x1b[23~", 1),
            VKey::wide(b"F12", b"\x1b[24~", 1),
        ],
        &[],
        &[],
    ),
    // ── Row 1: Number row ─────────────────────────────────────────
    KbRow::new(
        &[
            VKey::normal(b"`", b"~", b"`", b"~"),
            VKey::normal(b"1", b"!", b"1", b"!"),
            VKey::normal(b"2", b"@", b"2", b"@"),
            VKey::normal(b"3", b"#", b"3", b"#"),
            VKey::normal(b"4", b"$", b"4", b"$"),
            VKey::normal(b"5", b"%", b"5", b"%"),
            VKey::normal(b"6", b"^", b"6", b"^"),
            VKey::normal(b"7", b"&", b"7", b"&"),
            VKey::normal(b"8", b"*", b"8", b"*"),
            VKey::normal(b"9", b"(", b"9", b"("),
            VKey::normal(b"0", b")", b"0", b")"),
            VKey::normal(b"-", b"_", b"-", b"_"),
            VKey::normal(b"=", b"+", b"=", b"+"),
            VKey::wide(b"Bksp", b"\x7f", 2),
        ],
        &[
            VKey::wide(b"Ins", b"\x1b[2~", 1),
            VKey::wide(b"Hom", b"\x1b[H",  1),
            VKey::wide(b"PgU", b"\x1b[5~", 1),
        ],
        &[
            VKey::modifier(b"Num", 1),
            VKey::wide(b"/", b"/", 1),
            VKey::wide(b"*", b"*", 1),
            VKey::wide(b"-", b"-", 1),
        ],
    ),
    // ── Row 2: QWERTY ─────────────────────────────────────────────
    // — InputShade: Tab is 1.5u on real keyboards. Half-key stagger
    // from row 1 makes keys align more naturally. 21px ≈ half KEY_UNIT.
    KbRow::stagger(
        &[
            VKey::wide(b"Tab", b"\t", 1),
            VKey::normal(b"q", b"Q", b"q", b"Q"),
            VKey::normal(b"w", b"W", b"w", b"W"),
            VKey::normal(b"e", b"E", b"e", b"E"),
            VKey::normal(b"r", b"R", b"r", b"R"),
            VKey::normal(b"t", b"T", b"t", b"T"),
            VKey::normal(b"y", b"Y", b"y", b"Y"),
            VKey::normal(b"u", b"U", b"u", b"U"),
            VKey::normal(b"i", b"I", b"i", b"I"),
            VKey::normal(b"o", b"O", b"o", b"O"),
            VKey::normal(b"p", b"P", b"p", b"P"),
            VKey::normal(b"[", b"{", b"[", b"{"),
            VKey::normal(b"]", b"}", b"]", b"}"),
            VKey::normal(b"\\", b"|", b"\\", b"|"),
        ],
        &[
            VKey::wide(b"Del", b"\x1b[3~", 1),
            VKey::wide(b"End", b"\x1b[F",  1),
            VKey::wide(b"PgD", b"\x1b[6~", 1),
        ],
        &[
            VKey::wide(b"7", b"7", 1),
            VKey::wide(b"8", b"8", 1),
            VKey::wide(b"9", b"9", 1),
            VKey::wide(b"+", b"+", 1),
        ],
        // — InputShade: half-key stagger for QWERTY row (Tab is ~1.5u on real keyboards)
        KEY_UNIT / 2, 0,
    ),
    // ── Row 3: Home row ───────────────────────────────────────────
    // — InputShade: Caps Lock is ~1.75u, slight stagger beyond QWERTY row
    KbRow::stagger(
        &[
            VKey::modifier(b"Caps", 2),
            VKey::normal(b"a", b"A", b"a", b"A"),
            VKey::normal(b"s", b"S", b"s", b"S"),
            VKey::normal(b"d", b"D", b"d", b"D"),
            VKey::normal(b"f", b"F", b"f", b"F"),
            VKey::normal(b"g", b"G", b"g", b"G"),
            VKey::normal(b"h", b"H", b"h", b"H"),
            VKey::normal(b"j", b"J", b"j", b"J"),
            VKey::normal(b"k", b"K", b"k", b"K"),
            VKey::normal(b"l", b"L", b"l", b"L"),
            VKey::normal(b";", b":", b";", b":"),
            VKey::normal(b"'", b"\"", b"'", b"\""),
            VKey::wide(b"Enter", b"\n", 2),
        ],
        &[],
        &[
            VKey::wide(b"4", b"4", 1),
            VKey::wide(b"5", b"5", 1),
            VKey::wide(b"6", b"6", 1),
        ],
        // — InputShade: home row stagger — Caps is wider, shift by ~0.75 key
        KEY_UNIT * 3 / 4, 0,
    ),
    // ── Row 4: Shift row ──────────────────────────────────────────
    // — InputShade: Up arrow centered over Down arrow (which is at nav col 1).
    // nav_px = KEY_UNIT offsets Up one slot right to center over Down below.
    KbRow::stagger(
        &[
            VKey::modifier(b"Shift", 2),
            VKey::normal(b"z", b"Z", b"z", b"Z"),
            VKey::normal(b"x", b"X", b"x", b"X"),
            VKey::normal(b"c", b"C", b"c", b"C"),
            VKey::normal(b"v", b"V", b"v", b"V"),
            VKey::normal(b"b", b"B", b"b", b"B"),
            VKey::normal(b"n", b"N", b"n", b"N"),
            VKey::normal(b"m", b"M", b"m", b"M"),
            VKey::normal(b",", b"<", b",", b"<"),
            VKey::normal(b".", b">", b".", b">"),
            VKey::normal(b"/", b"?", b"/", b"?"),
            VKey::modifier(b"Shift", 2),
        ],
        &[
            VKey::wide(b"Up", b"\x1b[A", 1),
        ],
        &[
            VKey::wide(b"1", b"1", 1),
            VKey::wide(b"2", b"2", 1),
            VKey::wide(b"3", b"3", 1),
            VKey::wide(b"Ent", b"\n", 1),
        ],
        // — InputShade: shift row stagger. Up arrow offset by 1 key unit to center
        // over Down arrow in row 5 nav section (Left=0, Down=1, Right=2).
        0, KEY_UNIT,
    ),
    // ── Row 5: Bottom row ─────────────────────────────────────────
    KbRow::new(
        &[
            VKey::modifier(b"Ctrl", 1),
            VKey::modifier(b"Alt", 1),
            VKey::wide(b"Space", b" ", 9),
            VKey::modifier(b"Alt", 1),
            VKey::modifier(b"Ctrl", 1),
        ],
        &[
            VKey::wide(b"Lt", b"\x1b[D", 1),
            VKey::wide(b"Dn", b"\x1b[B", 1),
            VKey::wide(b"Rt", b"\x1b[C", 1),
        ],
        &[
            VKey::wide(b"0", b"0", 2),
            VKey::wide(b".", b".", 1),
        ],
    ),
];

// ═══════════════════════════════════════════════════════════════════
//  Global State
// ═══════════════════════════════════════════════════════════════════

/// — InputShade: visibility flag. AtomicBool = ISR-safe, lock-free.
static VISIBLE: AtomicBool = AtomicBool::new(false);

/// — InputShade: dirty flag. Set on toggle/tap/release, cleared after draw.
/// When vkbd is visible but NOT dirty, compositor skips the full repaint.
/// Without this, draw_overlay fires 100×/sec even when nothing changed = 1 FPS.
static VKBD_DIRTY: AtomicBool = AtomicBool::new(false);

/// — InputShade: hover-only dirty flag. When only the hovered key changed,
/// we repaint just 2 keys (old + new hover) instead of all ~100. The full
/// draw_overlay was burning cycles painting 100 keys with glyph rendering
/// on every mouse move — absolute lunacy for a 16×32 highlight change. — SableWire
static VKBD_HOVER_DIRTY: AtomicBool = AtomicBool::new(false);

/// Keyboard state — modifier tracking
static VKBD: spin::Mutex<VkbdState> = spin::Mutex::new(VkbdState::new());

struct VkbdState {
    shift: bool,
    caps: bool,
    ctrl: bool,
    alt: bool,
    /// Currently pressed key: (row, section, col_in_section) for visual feedback
    /// Section: 0=main, 1=nav, 2=numpad
    pressed: Option<(usize, usize, usize)>,
    /// Currently hovered key (mouse cursor over it)
    hovered: Option<(usize, usize, usize)>,
    /// — InputShade: previous hovered key — kept for targeted repaint.
    /// Only repaint these two keys instead of the whole damn keyboard.
    prev_hovered: Option<(usize, usize, usize)>,
    /// Screen dimensions (set on first draw)
    screen_w: u32,
    screen_h: u32,
}

impl VkbdState {
    const fn new() -> Self {
        VkbdState {
            shift: false,
            caps: false,
            ctrl: false,
            alt: false,
            pressed: None,
            hovered: None,
            prev_hovered: None,
            screen_w: 0,
            screen_h: 0,
        }
    }

    fn is_shifted(&self) -> bool {
        self.shift ^ self.caps
    }
}

// ═══════════════════════════════════════════════════════════════════
//  Public API
// ═══════════════════════════════════════════════════════════════════

/// Toggle virtual keyboard visibility (Alt+K).
/// — InputShade: flip the atomic, request compositor full redraw to
/// either show the overlay or repaint the area it was covering.
pub fn toggle() {
    let was_visible = VISIBLE.fetch_xor(true, Ordering::AcqRel);
    VKBD_DIRTY.store(true, Ordering::Release);
    // — InputShade: if we just hid the keyboard, clear modifier state
    // so next show starts clean. Nobody wants sticky shift from last time.
    if was_visible {
        if let Some(mut state) = VKBD.try_lock() {
            state.shift = false;
            state.ctrl = false;
            state.alt = false;
            state.pressed = None;
        }
    }
}

/// Check if virtual keyboard is visible (lock-free, ISR-safe).
#[inline]
pub fn is_visible() -> bool {
    VISIBLE.load(Ordering::Acquire)
}

/// — InputShade: check + clear dirty flag in one atomic op.
/// Returns true if vkbd needs full redraw. Compositor calls this in tick().
#[inline]
pub fn take_dirty() -> bool {
    VKBD_DIRTY.swap(false, Ordering::AcqRel)
}

/// — InputShade: check + clear hover-only dirty flag. When true, only 2 keys
/// need repaint (old hover + new hover). Compositor uses this for the fast path.
#[inline]
pub fn take_hover_dirty() -> bool {
    VKBD_HOVER_DIRTY.swap(false, Ordering::AcqRel)
}

/// Get keyboard height in pixels (0 if hidden).
#[inline]
pub fn keyboard_height() -> u32 {
    if is_visible() { KB_HEIGHT } else { 0 }
}

/// Handle a tap/click at screen coordinates. Returns bytes to inject
/// into the VT input ring, or None if the click missed all keys.
///
/// — InputShade: no heap allocation. Output bytes live in a stack array.
/// Returns (buffer, length) — caller pushes bytes[0..len] to VT ring.
pub fn handle_tap(x: i32, y: i32) -> Option<([u8; 8], usize)> {
    VKBD_DIRTY.store(true, Ordering::Release);
    let mut state = VKBD.try_lock()?;
    if state.screen_w == 0 || state.screen_h == 0 {
        return None;
    }

    let kb_y = state.screen_h - KB_HEIGHT;
    if (y as u32) < kb_y {
        return None; // — InputShade: click above keyboard, not ours
    }

    // — InputShade: hit-test against all sections
    let result = hit_test(x, y, state.screen_w, state.screen_h);
    let (row_idx, section, col_idx, key) = result?;

    state.pressed = Some((row_idx, section, col_idx));

    // — InputShade: handle modifier keys
    if key.is_modifier {
        match key.label {
            b"Shift" => state.shift = !state.shift,
            b"Caps" => state.caps = !state.caps,
            b"Ctrl" => state.ctrl = !state.ctrl,
            b"Alt" => state.alt = !state.alt,
            _ => {}
        }
        return None; // modifiers don't produce output bytes
    }

    // — InputShade: get output bytes based on shift state
    let shifted = state.is_shifted();
    let output = if shifted { key.shifted_output } else { key.output };

    if output.is_empty() {
        return None;
    }

    let mut buf = [0u8; 8];
    let len = output.len().min(8);

    // — InputShade: apply Ctrl modifier (mask with 0x1F for ASCII letters)
    if state.ctrl && len == 1 && output[0] >= b'a' && output[0] <= b'z' {
        buf[0] = output[0] & 0x1F;
        // — InputShade: auto-release Ctrl after one keypress
        state.ctrl = false;
        return Some((buf, 1));
    }

    for i in 0..len {
        buf[i] = output[i];
    }

    // — InputShade: auto-release shift after one keypress (like phone keyboards)
    if state.shift && !state.caps {
        state.shift = false;
    }

    Some((buf, len))
}

/// Update hover state based on mouse position. Called by compositor on idle
/// mouse move when vkbd is visible. Only marks dirty if hover changed.
/// — InputShade: uses VKBD_HOVER_DIRTY instead of VKBD_DIRTY so the compositor
/// can do a targeted 2-key repaint instead of repainting all ~100 keys.
/// Zero-cost when mouse is outside keyboard or hover unchanged.
pub fn update_hover(x: i32, y: i32) {
    if let Some(mut state) = VKBD.try_lock() {
        if state.screen_w == 0 || state.screen_h == 0 {
            return;
        }
        let old = state.hovered;
        let result = hit_test(x, y, state.screen_w, state.screen_h);
        state.hovered = result.map(|(row, section, col, _key)| (row, section, col));
        if state.hovered != old {
            // — InputShade: stash the old hover so redraw_hover_keys knows
            // which two keys to repaint. Only set hover-dirty, not full-dirty.
            state.prev_hovered = old;
            VKBD_HOVER_DIRTY.store(true, Ordering::Release);
        }
    }
}

/// Clear pressed-key visual state on button release.
pub fn handle_release() {
    VKBD_DIRTY.store(true, Ordering::Release);
    if let Some(mut state) = VKBD.try_lock() {
        state.pressed = None;
    }
}

/// Draw the virtual keyboard overlay onto the hardware framebuffer.
/// Called by compositor after VT blit, before mouse cursor draw.
///
/// — InputShade: direct pixel writes to hw_fb. No allocation. Three sections
/// rendered at fixed X offsets: main QWERTY left-aligned, nav cluster, numpad.
pub fn draw_overlay(hw_fb: &dyn Framebuffer) {
    if !is_visible() {
        return;
    }

    let screen_w = hw_fb.width();
    let screen_h = hw_fb.height();
    let stride = hw_fb.stride() as usize;
    let bpp = hw_fb.format().bytes_per_pixel() as usize;
    let format = hw_fb.format();
    let buf = hw_fb.buffer();

    if buf.is_null() || screen_h < KB_HEIGHT {
        return;
    }

    // — InputShade: update cached screen dimensions for hit-testing
    if let Some(mut state) = VKBD.try_lock() {
        state.screen_w = screen_w;
        state.screen_h = screen_h;

        let kb_y = screen_h - KB_HEIGHT;

        // — InputShade: fill keyboard background
        fill_rect(buf, stride, bpp, format, 0, kb_y, screen_w, KB_HEIGHT, BG_COLOR);

        // — InputShade: draw each row's three sections with per-row pixel stagger
        for (row_idx, kbrow) in KB_LAYOUT.iter().enumerate() {
            let key_y = kb_y + KB_PADDING + row_idx as u32 * (KEY_H + KEY_GAP);

            // Main section (left-aligned + row stagger). F-key row gets group gaps.
            draw_section(&state, buf, stride, bpp, format,
                         kbrow.main, LEFT_MARGIN + kbrow.main_px, key_y, row_idx, 0,
                         if row_idx == 0 { FK_COLOR } else { KEY_COLOR },
                         if row_idx == 0 { FK_GROUP_GAP } else { 0 });

            // Nav cluster (+ row-specific nav offset)
            draw_section(&state, buf, stride, bpp, format,
                         kbrow.nav, NAV_X + kbrow.nav_px, key_y, row_idx, 1, NAV_COLOR, 0);

            // Numpad
            draw_section(&state, buf, stride, bpp, format,
                         kbrow.numpad, NUMPAD_X, key_y, row_idx, 2, NUMPAD_COLOR, 0);
        }
    }
}

/// — InputShade: targeted hover repaint. Redraws ONLY the old + new hovered keys
/// instead of all ~100 keys. 2 key repaints vs 100 = ~50× faster hover response.
/// Called by compositor when only VKBD_HOVER_DIRTY is set (no full dirty).
/// — SableWire: the old draw_overlay was doing 100 fill_rects + 100 glyph renders
/// per mouse move. This does 2 of each. The difference between 2 FPS and 60 FPS.
pub fn redraw_hover_keys(hw_fb: &dyn Framebuffer) {
    if !is_visible() {
        return;
    }

    let screen_h = hw_fb.height();
    let stride = hw_fb.stride() as usize;
    let bpp = hw_fb.format().bytes_per_pixel() as usize;
    let format = hw_fb.format();
    let buf = hw_fb.buffer();

    if buf.is_null() || screen_h < KB_HEIGHT {
        return;
    }

    if let Some(state) = VKBD.try_lock() {
        let kb_y = screen_h - KB_HEIGHT;

        // — InputShade: repaint the key that LOST hover (restore to normal color)
        if let Some(prev) = state.prev_hovered {
            redraw_single_key(&state, buf, stride, bpp, format, kb_y, prev.0, prev.1, prev.2);
        }
        // — InputShade: repaint the key that GAINED hover (apply hover color)
        if let Some(curr) = state.hovered {
            redraw_single_key(&state, buf, stride, bpp, format, kb_y, curr.0, curr.1, curr.2);
        }
    }
}

/// — InputShade: repaint a single key at (row, section, col). Computes its screen
/// rect from the layout tables, then draws background + label. No full-keyboard
/// background fill, no iteration over other keys. Surgical precision. — EchoFrame
fn redraw_single_key(state: &VkbdState, buf: *mut u8, stride: usize, bpp: usize,
                     format: PixelFormat, kb_y: u32,
                     row_idx: usize, section: usize, col_idx: usize) {
    if row_idx >= KB_ROWS {
        return;
    }
    let kbrow = &KB_LAYOUT[row_idx];
    let (keys, start_x, fk_gap) = match section {
        0 => (kbrow.main, LEFT_MARGIN + kbrow.main_px,
              if row_idx == 0 { FK_GROUP_GAP } else { 0 }),
        1 => (kbrow.nav, NAV_X + kbrow.nav_px, 0u32),
        2 => (kbrow.numpad, NUMPAD_X, 0u32),
        _ => return,
    };

    if col_idx >= keys.len() {
        return;
    }

    // — InputShade: walk keys to compute X position (keys have variable widths)
    let mut key_x = start_x;
    for (i, key) in keys.iter().enumerate() {
        if i == col_idx {
            let key_w = key.width as u32 * KEY_UNIT - KEY_GAP;
            let key_y = kb_y + KB_PADDING + row_idx as u32 * (KEY_H + KEY_GAP);

            // — InputShade: same color logic as draw_section
            let base_color = match section {
                0 => if row_idx == 0 { FK_COLOR } else { KEY_COLOR },
                1 => NAV_COLOR,
                2 => NUMPAD_COLOR,
                _ => KEY_COLOR,
            };
            let color = if state.pressed == Some((row_idx, section, col_idx)) {
                KEY_PRESSED_COLOR
            } else if state.hovered == Some((row_idx, section, col_idx)) {
                KEY_HOVER_COLOR
            } else if key.is_modifier && is_modifier_active(state, key.label) {
                MOD_ACTIVE_COLOR
            } else {
                base_color
            };

            fill_rect(buf, stride, bpp, format, key_x, key_y, key_w, KEY_H, color);
            let label = if state.is_shifted() { key.shifted_label } else { key.label };
            draw_key_label(buf, stride, bpp, format, key_x, key_y, key_w, label);
            return;
        }
        key_x += key.width as u32 * KEY_UNIT;
        if fk_gap > 0 && (i == 0 || i == 4 || i == 8) {
            key_x += fk_gap;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
//  Internal Helpers
// ═══════════════════════════════════════════════════════════════════

/// — InputShade: draw a section of keys at a fixed X offset.
/// `fk_gap` > 0 inserts gaps after Esc (idx 0), F4 (idx 4), F8 (idx 8) —
/// standard F-key grouping. 0 = no gaps. Only used for F-key row.
fn draw_section(state: &VkbdState, buf: *mut u8, stride: usize, bpp: usize,
                format: PixelFormat, keys: &[VKey], start_x: u32, key_y: u32,
                row_idx: usize, section: usize, base_color: u32, fk_gap: u32) {
    let mut key_x = start_x;
    for (col_idx, key) in keys.iter().enumerate() {
        let key_w = key.width as u32 * KEY_UNIT - KEY_GAP;

        // — InputShade: pick color based on state — pressed > hovered > modifier > base.
        // Three visual tiers: pressed (brightest), hover (mid), normal (base).
        let color = if state.pressed == Some((row_idx, section, col_idx)) {
            KEY_PRESSED_COLOR
        } else if state.hovered == Some((row_idx, section, col_idx)) {
            KEY_HOVER_COLOR
        } else if key.is_modifier && is_modifier_active(state, key.label) {
            MOD_ACTIVE_COLOR
        } else {
            base_color
        };

        // Draw key background
        fill_rect(buf, stride, bpp, format, key_x, key_y, key_w, KEY_H, color);

        // — InputShade: draw key label
        let label = if state.is_shifted() { key.shifted_label } else { key.label };
        draw_key_label(buf, stride, bpp, format, key_x, key_y, key_w, label);

        key_x += key.width as u32 * KEY_UNIT;

        // — InputShade: F-key group gaps after Esc, F4, F8
        if fk_gap > 0 && (col_idx == 0 || col_idx == 4 || col_idx == 8) {
            key_x += fk_gap;
        }
    }
}

/// Check if a modifier key is currently active.
fn is_modifier_active(state: &VkbdState, label: &[u8]) -> bool {
    match label {
        b"Shift" => state.shift,
        b"Caps" => state.caps,
        b"Ctrl" => state.ctrl,
        b"Alt" => state.alt,
        _ => false,
    }
}

/// — InputShade: hit-test a section of keys at a given X offset.
/// Returns (col_index, &VKey) if click lands on a key, None otherwise.
/// `fk_gap` adds extra pixels after indices 0, 4, 8 (F-key grouping).
fn hit_test_section(keys: &'static [VKey], start_x: u32, click_x: u32, fk_gap: u32) -> Option<(usize, &'static VKey)> {
    let mut key_x = start_x;
    for (col_idx, key) in keys.iter().enumerate() {
        let key_w = key.width as u32 * KEY_UNIT - KEY_GAP;
        if click_x >= key_x && click_x < key_x + key_w {
            return Some((col_idx, key));
        }
        key_x += key.width as u32 * KEY_UNIT;
        if fk_gap > 0 && (col_idx == 0 || col_idx == 4 || col_idx == 8) {
            key_x += fk_gap;
        }
    }
    None
}

/// — InputShade: full hit-test across all sections of all rows.
/// Returns (row, section, col, &VKey) or None.
fn hit_test(x: i32, y: i32, screen_w: u32, screen_h: u32) -> Option<(usize, usize, usize, &'static VKey)> {
    if x < 0 || y < 0 {
        return None;
    }
    let _ = screen_w; // — InputShade: unused now, sections have fixed positions

    let kb_y = screen_h - KB_HEIGHT;
    let abs_y = y as u32;
    if abs_y < kb_y {
        return None;
    }
    if abs_y < kb_y + KB_PADDING {
        return None;
    }

    let rel_y = abs_y - kb_y - KB_PADDING;
    let row_idx = (rel_y / (KEY_H + KEY_GAP)) as usize;
    if row_idx >= KB_ROWS {
        return None;
    }

    let click_x = x as u32;
    let kbrow = &KB_LAYOUT[row_idx];

    // — InputShade: check each section at its fixed X position + row stagger.
    // F-key row (row 0) main section has group gaps.
    let fk_gap = if row_idx == 0 { FK_GROUP_GAP } else { 0 };
    if let Some((col, key)) = hit_test_section(kbrow.main, LEFT_MARGIN + kbrow.main_px, click_x, fk_gap) {
        return Some((row_idx, 0, col, key));
    }
    if let Some((col, key)) = hit_test_section(kbrow.nav, NAV_X + kbrow.nav_px, click_x, 0) {
        return Some((row_idx, 1, col, key));
    }
    if let Some((col, key)) = hit_test_section(kbrow.numpad, NUMPAD_X, click_x, 0) {
        return Some((row_idx, 2, col, key));
    }

    None
}

/// Fill a rectangle on the framebuffer.
/// — InputShade: the pixel loop that makes rectangles happen. Raw pointers,
/// no bounds checks beyond our own. We trust the compositor gave us valid memory.
/// — SableWire: row-batched fill. Old per-pixel MMIO path murdered ISR latency —
/// 16×400 = 6400 MMIO writes at 1000 cyc each. Row batching: build pixel row in stack
/// RAM (~1 cyc/pixel), blast entire row to MMIO in one copy. W× speedup. — InputShade
fn fill_rect(buf: *mut u8, stride: usize, bpp: usize, _format: PixelFormat,
             x: u32, y: u32, w: u32, h: u32, color_argb: u32) {
    let pixel = argb_to_pixel(_format, color_argb);
    let actual_w = w as usize;
    if actual_w == 0 || h == 0 { return; }
    let pb = bpp.min(4);

    // — SableWire: 256px row template in stack RAM — covers any key width
    const MAX_ROW_PX: usize = 256;
    let row_px = actual_w.min(MAX_ROW_PX);
    let mut row_buf = [0u8; MAX_ROW_PX * 4];
    for col in 0..row_px {
        let off = col * bpp;
        row_buf[off..off + pb].copy_from_slice(&pixel[..pb]);
    }
    let row_bytes = row_px * bpp;

    unsafe {
        for row in 0..h {
            let dst_offset = (y + row) as usize * stride + x as usize * bpp;
            let dst = buf.add(dst_offset);
            if actual_w <= MAX_ROW_PX {
                core::ptr::copy_nonoverlapping(row_buf.as_ptr(), dst, row_bytes);
            } else {
                let mut remaining = actual_w;
                let mut col_off = 0usize;
                while remaining > 0 {
                    let chunk = remaining.min(MAX_ROW_PX);
                    core::ptr::copy_nonoverlapping(row_buf.as_ptr(), dst.add(col_off * bpp), chunk * bpp);
                    col_off += chunk;
                    remaining -= chunk;
                }
            }
        }
    }
}

/// Draw a text label centered in a key rectangle.
/// — InputShade: glyph rendering stolen from fb::console. One byte at a time,
/// bitmap rows from PSF2_FONT. It's not pretty but it's ours.
fn draw_key_label(buf: *mut u8, stride: usize, bpp: usize, format: PixelFormat,
                  key_x: u32, key_y: u32, key_w: u32, label: &[u8]) {
    let font = &PSF2_FONT;
    let glyph_w = font.width;
    let glyph_h = font.height;

    let label_w = label.len() as u32 * glyph_w;
    let text_x = key_x + (key_w.saturating_sub(label_w)) / 2;
    let text_y = key_y + (KEY_H.saturating_sub(glyph_h)) / 2;

    let fg_pixel = argb_to_pixel(format, TEXT_COLOR);

    for (i, &ch) in label.iter().enumerate() {
        if let Some(glyph) = font.glyph(ch as char) {
            let gx = text_x + i as u32 * glyph_w;
            draw_glyph(buf, stride, bpp, gx, text_y, &glyph, &fg_pixel);
        }
    }
}

/// Draw a single glyph bitmap onto the framebuffer.
/// — InputShade: PSF2 glyphs are 1-bit-per-pixel bitmaps, MSB-first.
/// We only draw foreground pixels (set bits) — background is already filled.
fn draw_glyph(buf: *mut u8, stride: usize, bpp: usize,
              x: u32, y: u32, glyph: &fb::font::Glyph, fg_pixel: &[u8; 4]) {
    let bytes_per_row = ((glyph.width + 7) / 8) as usize;

    unsafe {
        for row in 0..glyph.height {
            let row_offset = (y + row) as usize * stride;
            let glyph_row_start = row as usize * bytes_per_row;

            for col in 0..glyph.width {
                let byte_idx = glyph_row_start + (col / 8) as usize;
                let bit_idx = 7 - (col % 8);

                if byte_idx < glyph.data.len() && (glyph.data[byte_idx] >> bit_idx) & 1 != 0 {
                    let offset = row_offset + (x + col) as usize * bpp;
                    core::ptr::copy_nonoverlapping(fg_pixel.as_ptr(), buf.add(offset), bpp.min(4));
                }
            }
        }
    }
}

/// Convert ARGB color to framebuffer pixel bytes.
fn argb_to_pixel(format: PixelFormat, color_argb: u32) -> [u8; 4] {
    match format {
        PixelFormat::BGRA8888 => [
            (color_argb & 0xFF) as u8,         // B
            ((color_argb >> 8) & 0xFF) as u8,  // G
            ((color_argb >> 16) & 0xFF) as u8, // R
            ((color_argb >> 24) & 0xFF) as u8, // A
        ],
        _ => [
            ((color_argb >> 16) & 0xFF) as u8, // R
            ((color_argb >> 8) & 0xFF) as u8,  // G
            (color_argb & 0xFF) as u8,         // B
            ((color_argb >> 24) & 0xFF) as u8, // A
        ],
    }
}

// ═══════════════════════════════════════════════════════════════════
//  Unit Tests — CrashBloom: validating the vkbd before it leaves the lab
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN_W: u32 = 1280;
    const SCREEN_H: u32 = 800;

    // — CrashBloom: layout constants sanity

    #[test]
    fn kb_height_fits_screen() {
        assert!(KB_HEIGHT < SCREEN_H, "keyboard taller than screen");
        assert!(KB_HEIGHT > 0, "keyboard has zero height");
        assert_eq!(KB_HEIGHT, KB_ROWS as u32 * (KEY_H + KEY_GAP) + KB_PADDING * 2);
    }

    #[test]
    fn section_positions_fit_screen() {
        // — CrashBloom: all three sections must fit within 1280px
        let numpad_end = NUMPAD_X + NUMPAD_WIDTH_UNITS * KEY_UNIT;
        assert!(numpad_end <= SCREEN_W,
            "numpad extends past screen: {}px > {}px", numpad_end, SCREEN_W);
        assert!(NAV_X > LEFT_MARGIN + MAIN_UNITS * KEY_UNIT,
            "nav section overlaps main section");
        assert!(NUMPAD_X > NAV_X + NAV_WIDTH_UNITS * KEY_UNIT,
            "numpad overlaps nav section");
    }

    #[test]
    fn all_main_sections_fit() {
        // — CrashBloom: main section keys must not exceed MAIN_UNITS width
        for (i, kbrow) in KB_LAYOUT.iter().enumerate() {
            let total: u32 = kbrow.main.iter().map(|k| k.width as u32).sum();
            assert!(total <= MAIN_UNITS,
                "row {} main section is {} units, max is {}", i, total, MAIN_UNITS);
        }
    }

    #[test]
    fn all_nav_sections_fit() {
        for (i, kbrow) in KB_LAYOUT.iter().enumerate() {
            let total: u32 = kbrow.nav.iter().map(|k| k.width as u32).sum();
            assert!(total <= NAV_WIDTH_UNITS,
                "row {} nav section is {} units, max is {}", i, total, NAV_WIDTH_UNITS);
        }
    }

    #[test]
    fn all_numpad_sections_fit() {
        for (i, kbrow) in KB_LAYOUT.iter().enumerate() {
            let total: u32 = kbrow.numpad.iter().map(|k| k.width as u32).sum();
            assert!(total <= NUMPAD_WIDTH_UNITS,
                "row {} numpad section is {} units, max is {}", i, total, NUMPAD_WIDTH_UNITS);
        }
    }

    #[test]
    fn argb_to_pixel_bgra() {
        let px = argb_to_pixel(PixelFormat::BGRA8888, 0xFF112233);
        assert_eq!(px, [0x33, 0x22, 0x11, 0xFF]);
    }

    #[test]
    fn argb_to_pixel_rgba() {
        let px = argb_to_pixel(PixelFormat::RGBA8888, 0xFF112233);
        assert_eq!(px, [0x11, 0x22, 0x33, 0xFF]);
    }

    #[test]
    fn vkbd_state_shifted() {
        let mut s = VkbdState::new();
        assert!(!s.is_shifted(), "default should not be shifted");
        s.shift = true;
        assert!(s.is_shifted(), "shift alone = shifted");
        s.caps = true;
        assert!(!s.is_shifted(), "shift + caps = unshifted");
        s.shift = false;
        assert!(s.is_shifted(), "caps alone = shifted");
    }

    #[test]
    fn modifier_active_check() {
        let mut s = VkbdState::new();
        assert!(!is_modifier_active(&s, b"Shift"));
        assert!(!is_modifier_active(&s, b"Ctrl"));
        s.shift = true;
        s.ctrl = true;
        assert!(is_modifier_active(&s, b"Shift"));
        assert!(is_modifier_active(&s, b"Ctrl"));
        assert!(!is_modifier_active(&s, b"Alt"));
        assert!(!is_modifier_active(&s, b"Bogus"));
    }

    #[test]
    fn key_counts_per_row() {
        // — CrashBloom: verify key counts for each section
        // Row 0 (F-keys): 13 main (Esc + F1-F12), no nav, no numpad
        assert_eq!(KB_LAYOUT[0].main.len(), 13);
        assert_eq!(KB_LAYOUT[0].nav.len(), 0);
        assert_eq!(KB_LAYOUT[0].numpad.len(), 0);
        // Row 1 (numbers): 14 main, 3 nav (Ins/Hom/PgU), 4 numpad (Num / * -)
        assert_eq!(KB_LAYOUT[1].main.len(), 14);
        assert_eq!(KB_LAYOUT[1].nav.len(), 3);
        assert_eq!(KB_LAYOUT[1].numpad.len(), 4);
        // Row 2 (QWERTY): 14 main, 3 nav (Del/End/PgD), 4 numpad (7 8 9 +)
        assert_eq!(KB_LAYOUT[2].main.len(), 14);
        assert_eq!(KB_LAYOUT[2].nav.len(), 3);
        assert_eq!(KB_LAYOUT[2].numpad.len(), 4);
        // Row 3 (Home): 13 main, 0 nav, 3 numpad (4 5 6)
        assert_eq!(KB_LAYOUT[3].main.len(), 13);
        assert_eq!(KB_LAYOUT[3].nav.len(), 0);
        assert_eq!(KB_LAYOUT[3].numpad.len(), 3);
        // Row 4 (Shift): 12 main, 1 nav (Up), 4 numpad (1 2 3 Ent)
        assert_eq!(KB_LAYOUT[4].main.len(), 12);
        assert_eq!(KB_LAYOUT[4].nav.len(), 1);
        assert_eq!(KB_LAYOUT[4].numpad.len(), 4);
        // Row 5 (Bottom): 5 main, 3 nav (Lt Dn Rt), 3 numpad (0(2w) .)
        assert_eq!(KB_LAYOUT[5].main.len(), 5);
        assert_eq!(KB_LAYOUT[5].nav.len(), 3);
        assert_eq!(KB_LAYOUT[5].numpad.len(), 2);
    }

    #[test]
    fn all_normal_keys_have_output() {
        // — CrashBloom: every non-modifier key must produce at least one byte
        for (i, kbrow) in KB_LAYOUT.iter().enumerate() {
            for (section_name, keys) in [("main", kbrow.main), ("nav", kbrow.nav), ("numpad", kbrow.numpad)] {
                for (j, key) in keys.iter().enumerate() {
                    if !key.is_modifier {
                        assert!(!key.output.is_empty(),
                            "row {} {} key {} ({:?}) has empty output",
                            i, section_name, j, core::str::from_utf8(key.label));
                        assert!(!key.shifted_output.is_empty(),
                            "row {} {} key {} ({:?}) has empty shifted_output",
                            i, section_name, j, core::str::from_utf8(key.label));
                    }
                }
            }
        }
    }

    #[test]
    fn modifier_keys_have_no_output() {
        for (i, kbrow) in KB_LAYOUT.iter().enumerate() {
            for (section_name, keys) in [("main", kbrow.main), ("nav", kbrow.nav), ("numpad", kbrow.numpad)] {
                for (j, key) in keys.iter().enumerate() {
                    if key.is_modifier {
                        assert!(key.output.is_empty(),
                            "modifier at row {} {} key {} should have empty output", i, section_name, j);
                    }
                }
            }
        }
    }

    // — CrashBloom: hit-test validation

    #[test]
    fn hit_test_above_keyboard_misses() {
        let kb_top = SCREEN_H - KB_HEIGHT;
        assert!(hit_test(640, (kb_top - 1) as i32, SCREEN_W, SCREEN_H).is_none());
        assert!(hit_test(640, 0, SCREEN_W, SCREEN_H).is_none());
        assert!(hit_test(640, 400, SCREEN_W, SCREEN_H).is_none());
    }

    #[test]
    fn hit_test_negative_coords_miss() {
        assert!(hit_test(-1, 700, SCREEN_W, SCREEN_H).is_none());
        assert!(hit_test(640, -1, SCREEN_W, SCREEN_H).is_none());
    }

    #[test]
    fn hit_test_finds_space_bar() {
        // — CrashBloom: Space is in row 5 main section, after Ctrl(1) + Alt(1)
        let row5_y = SCREEN_H - KB_HEIGHT + KB_PADDING + 5 * (KEY_H + KEY_GAP) + KEY_H / 2;
        // Space starts at LEFT_MARGIN + 2*KEY_UNIT (after Ctrl + Alt)
        let space_mid = LEFT_MARGIN + 2 * KEY_UNIT + 4 * KEY_UNIT; // middle of 9-unit space
        let result = hit_test(space_mid as i32, row5_y as i32, SCREEN_W, SCREEN_H);
        assert!(result.is_some(), "should hit Space bar");
        let (_row, _section, _col, key) = result.unwrap();
        assert_eq!(key.label, b"Space");
    }

    #[test]
    fn hit_test_finds_f1_key() {
        // — CrashBloom: F1 is second key in row 0 main (after Esc + FK_GROUP_GAP)
        let row0_y = SCREEN_H - KB_HEIGHT + KB_PADDING + KEY_H / 2;
        let f1_x = LEFT_MARGIN + 1 * KEY_UNIT + FK_GROUP_GAP + 5; // after Esc + gap
        let result = hit_test(f1_x as i32, row0_y as i32, SCREEN_W, SCREEN_H);
        assert!(result.is_some(), "should hit F1 key");
        let (row, section, col, key) = result.unwrap();
        assert_eq!(row, 0);
        assert_eq!(section, 0); // main
        assert_eq!(col, 1);     // second key (after Esc)
        assert_eq!(key.label, b"F1");
    }

    #[test]
    fn hit_test_finds_esc_key() {
        let row0_y = SCREEN_H - KB_HEIGHT + KB_PADDING + KEY_H / 2;
        let result = hit_test((LEFT_MARGIN + 5) as i32, row0_y as i32, SCREEN_W, SCREEN_H);
        assert!(result.is_some(), "should hit Esc key");
        let (_row, _section, _col, key) = result.unwrap();
        assert_eq!(key.label, b"Esc");
    }

    #[test]
    fn hit_test_finds_numpad_7() {
        // — CrashBloom: numpad 7 is in row 2 numpad section
        let row2_y = SCREEN_H - KB_HEIGHT + KB_PADDING + 2 * (KEY_H + KEY_GAP) + KEY_H / 2;
        let result = hit_test((NUMPAD_X + 5) as i32, row2_y as i32, SCREEN_W, SCREEN_H);
        assert!(result.is_some(), "should hit numpad 7");
        let (row, section, _col, key) = result.unwrap();
        assert_eq!(row, 2);
        assert_eq!(section, 2); // numpad
        assert_eq!(key.label, b"7");
    }

    #[test]
    fn hit_test_finds_nav_insert() {
        // — CrashBloom: Ins is in row 1 nav section
        let row1_y = SCREEN_H - KB_HEIGHT + KB_PADDING + 1 * (KEY_H + KEY_GAP) + KEY_H / 2;
        let result = hit_test((NAV_X + 5) as i32, row1_y as i32, SCREEN_W, SCREEN_H);
        assert!(result.is_some(), "should hit Ins key");
        let (row, section, col, key) = result.unwrap();
        assert_eq!(row, 1);
        assert_eq!(section, 1); // nav
        assert_eq!(col, 0);
        assert_eq!(key.label, b"Ins");
    }

    #[test]
    fn hit_test_gap_between_sections_misses() {
        // — CrashBloom: gap between main and nav should miss
        let row1_y = SCREEN_H - KB_HEIGHT + KB_PADDING + 1 * (KEY_H + KEY_GAP) + KEY_H / 2;
        let gap_x = LEFT_MARGIN + MAIN_UNITS * KEY_UNIT + 5; // in the gap
        let result = hit_test(gap_x as i32, row1_y as i32, SCREEN_W, SCREEN_H);
        assert!(result.is_none(), "gap between sections should miss");
    }

    #[test]
    fn toggle_flips_visibility() {
        let initial = VISIBLE.load(Ordering::SeqCst);
        toggle();
        assert_ne!(VISIBLE.load(Ordering::SeqCst), initial);
        toggle();
        assert_eq!(VISIBLE.load(Ordering::SeqCst), initial);
    }

    #[test]
    fn keyboard_height_reflects_visibility() {
        let was = VISIBLE.load(Ordering::SeqCst);
        VISIBLE.store(false, Ordering::SeqCst);
        assert_eq!(keyboard_height(), 0);
        VISIBLE.store(true, Ordering::SeqCst);
        assert_eq!(keyboard_height(), KB_HEIGHT);
        VISIBLE.store(was, Ordering::SeqCst);
    }

    #[test]
    fn enter_key_produces_newline() {
        let enter = KB_LAYOUT[3].main.iter().find(|k| k.label == b"Enter").unwrap();
        assert_eq!(enter.output, b"\n");
        assert_eq!(enter.shifted_output, b"\n");
    }

    #[test]
    fn backspace_produces_delete() {
        let bksp = KB_LAYOUT[1].main.iter().find(|k| k.label == b"Bksp").unwrap();
        assert_eq!(bksp.output, b"\x7f");
    }

    #[test]
    fn escape_key_produces_esc() {
        let esc = KB_LAYOUT[0].main.iter().find(|k| k.label == b"Esc").unwrap();
        assert_eq!(esc.output, b"\x1b");
    }

    #[test]
    fn fkeys_produce_correct_sequences() {
        // — CrashBloom: F1-F4 use SS3 (ESC O), F5+ use CSI (ESC [)
        let main = KB_LAYOUT[0].main;
        assert_eq!(main[1].output, b"\x1bOP",    "F1");  // index 1 (after Esc)
        assert_eq!(main[2].output, b"\x1bOQ",    "F2");
        assert_eq!(main[3].output, b"\x1bOR",    "F3");
        assert_eq!(main[4].output, b"\x1bOS",    "F4");
        assert_eq!(main[5].output, b"\x1b[15~",  "F5");
        assert_eq!(main[12].output, b"\x1b[24~", "F12");
    }

    #[test]
    fn nav_keys_produce_correct_sequences() {
        // — CrashBloom: Ins/Del/PgUp/PgDn in nav sections
        let ins = KB_LAYOUT[1].nav.iter().find(|k| k.label == b"Ins").unwrap();
        assert_eq!(ins.output, b"\x1b[2~");
        let del = KB_LAYOUT[2].nav.iter().find(|k| k.label == b"Del").unwrap();
        assert_eq!(del.output, b"\x1b[3~");
        let pgu = KB_LAYOUT[1].nav.iter().find(|k| k.label == b"PgU").unwrap();
        assert_eq!(pgu.output, b"\x1b[5~");
        let pgd = KB_LAYOUT[2].nav.iter().find(|k| k.label == b"PgD").unwrap();
        assert_eq!(pgd.output, b"\x1b[6~");
    }

    #[test]
    fn arrow_keys_produce_ansi() {
        // — CrashBloom: arrows are in nav sections (rows 4-5)
        let up = KB_LAYOUT[4].nav.iter().find(|k| k.label == b"Up").unwrap();
        assert_eq!(up.output, b"\x1b[A");
        let left = KB_LAYOUT[5].nav.iter().find(|k| k.label == b"Lt").unwrap();
        assert_eq!(left.output, b"\x1b[D");
        let down = KB_LAYOUT[5].nav.iter().find(|k| k.label == b"Dn").unwrap();
        assert_eq!(down.output, b"\x1b[B");
        let right = KB_LAYOUT[5].nav.iter().find(|k| k.label == b"Rt").unwrap();
        assert_eq!(right.output, b"\x1b[C");
    }

    #[test]
    fn shifted_letters_are_uppercase() {
        // — CrashBloom: every letter key's shifted output should be uppercase
        for row_idx in [2, 3, 4] { // QWERTY, Home, Shift rows
            for key in KB_LAYOUT[row_idx].main.iter() {
                if key.output.len() == 1 && key.output[0] >= b'a' && key.output[0] <= b'z' {
                    assert_eq!(key.shifted_output.len(), 1);
                    assert_eq!(key.shifted_output[0], key.output[0] - 32,
                        "key '{}' shifted should be '{}'",
                        key.output[0] as char, (key.output[0] - 32) as char);
                }
            }
        }
    }

    #[test]
    fn number_row_shifted_symbols() {
        let main = KB_LAYOUT[1].main;
        let expected = [
            (b"1", b"!"), (b"2", b"@"), (b"3", b"#"), (b"4", b"$"),
            (b"5", b"%"), (b"6", b"^"), (b"7", b"&"), (b"8", b"*"),
            (b"9", b"("), (b"0", b")"),
        ];
        for (label, shifted) in &expected {
            let key = main.iter().find(|k| k.label == *label).unwrap();
            assert_eq!(key.shifted_output, *shifted,
                "key '{}' shifted should produce '{}'",
                core::str::from_utf8(key.label).unwrap(),
                core::str::from_utf8(key.shifted_output).unwrap());
        }
    }

    #[test]
    fn home_end_keys_present() {
        // — CrashBloom: Home/End are in nav cluster
        let hom = KB_LAYOUT[1].nav.iter().find(|k| k.label == b"Hom").unwrap();
        assert_eq!(hom.output, b"\x1b[H");
        let end = KB_LAYOUT[2].nav.iter().find(|k| k.label == b"End").unwrap();
        assert_eq!(end.output, b"\x1b[F");
    }

    #[test]
    fn numpad_complete() {
        // — CrashBloom: verify all numpad digits 0-9 are present
        let mut found = [false; 10];
        for kbrow in KB_LAYOUT.iter() {
            for key in kbrow.numpad.iter() {
                if key.output.len() == 1 && key.output[0] >= b'0' && key.output[0] <= b'9' {
                    found[(key.output[0] - b'0') as usize] = true;
                }
            }
        }
        for (digit, present) in found.iter().enumerate() {
            assert!(present, "numpad missing digit {}", digit);
        }
    }
}
