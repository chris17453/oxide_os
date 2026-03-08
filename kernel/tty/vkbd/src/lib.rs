//! Virtual On-Screen Keyboard for OXIDE OS
//!
//! — InputShade: When there's no hardware keyboard, you paint your own.
//! Renders a QWERTY overlay on top of the compositor output. Touch/click
//! a key, bytes go into the active VT's input ring. Simple as dirt.
//!
//! Toggle with Alt+K. Draws directly on the hardware framebuffer after
//! compositor blit, before mouse cursor. No backing buffer needed —
//! keyboard area is repainted every frame it's visible.

#![cfg_attr(not(test), no_std)]

use core::sync::atomic::{AtomicBool, Ordering};
use fb::{Framebuffer, PixelFormat};
use fb::font::PSF2_FONT;

// ═══════════════════════════════════════════════════════════════════
//  Layout Constants
// ═══════════════════════════════════════════════════════════════════

/// — InputShade: key dimensions tuned for 1280x800 display.
/// 32 keys across × 40px = 1280px. Perfect fit, no wasted space.
const KEY_W: u32 = 40;
const KEY_H: u32 = 32;
const KEY_GAP: u32 = 2;
const KEY_UNIT: u32 = KEY_W + KEY_GAP;  // 42px per key slot
const KB_PADDING: u32 = 4;

/// Number of keyboard rows
const KB_ROWS: usize = 5;

/// Total keyboard height including padding
const KB_HEIGHT: u32 = KB_ROWS as u32 * (KEY_H + KEY_GAP) + KB_PADDING * 2;

/// Background color (dark gray) — ARGB
const BG_COLOR: u32 = 0xFF1A1A2E;
/// Key color (medium gray) — ARGB
const KEY_COLOR: u32 = 0xFF2D2D44;
/// Key pressed color (lighter) — ARGB
const KEY_PRESSED_COLOR: u32 = 0xFF4A4A6A;
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
//  QWERTY Layout
// ═══════════════════════════════════════════════════════════════════

/// — InputShade: row 0 — number row
static ROW0: &[VKey] = &[
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
];

/// Row 1 — QWERTY
static ROW1: &[VKey] = &[
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
];

/// Row 2 — ASDF
static ROW2: &[VKey] = &[
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
];

/// Row 3 — ZXCV
static ROW3: &[VKey] = &[
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
    VKey::wide(b"Up", b"\x1b[A", 1),
    VKey::modifier(b"Shift", 2),
];

/// Row 4 — bottom row (Ctrl, space, arrows)
static ROW4: &[VKey] = &[
    VKey::modifier(b"Ctrl", 1),
    VKey::modifier(b"Alt", 1),
    VKey::wide(b"Esc", b"\x1b", 1),
    VKey::wide(b"Space", b" ", 8),
    VKey::wide(b"Left", b"\x1b[D", 1),
    VKey::wide(b"Down", b"\x1b[B", 1),
    VKey::wide(b"Right", b"\x1b[C", 1),
];

/// All rows
static ROWS: [&[VKey]; KB_ROWS] = [ROW0, ROW1, ROW2, ROW3, ROW4];

// ═══════════════════════════════════════════════════════════════════
//  Global State
// ═══════════════════════════════════════════════════════════════════

/// — InputShade: visibility flag. AtomicBool = ISR-safe, lock-free.
static VISIBLE: AtomicBool = AtomicBool::new(false);

/// Keyboard state — modifier tracking
static VKBD: spin::Mutex<VkbdState> = spin::Mutex::new(VkbdState::new());

struct VkbdState {
    shift: bool,
    caps: bool,
    ctrl: bool,
    alt: bool,
    /// Currently pressed key position (row, col) for visual feedback
    pressed: Option<(usize, usize)>,
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
    let mut state = VKBD.try_lock()?;
    if state.screen_w == 0 || state.screen_h == 0 {
        return None;
    }

    let kb_y = state.screen_h - KB_HEIGHT;
    if (y as u32) < kb_y {
        return None; // — InputShade: click above keyboard, not ours
    }

    let rel_y = y as u32 - kb_y - KB_PADDING;
    let row_idx = (rel_y / (KEY_H + KEY_GAP)) as usize;
    if row_idx >= KB_ROWS {
        return None;
    }

    let row = ROWS[row_idx];
    let rel_x = x as u32;
    let row_start_x = compute_row_start_x(row, state.screen_w);
    if rel_x < row_start_x {
        return None;
    }

    // — InputShade: walk keys in the row to find which one was hit
    let mut key_x = row_start_x;
    for (col_idx, key) in row.iter().enumerate() {
        let key_w = key.width as u32 * KEY_UNIT - KEY_GAP;
        if rel_x >= key_x && rel_x < key_x + key_w {
            state.pressed = Some((row_idx, col_idx));

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

            return Some((buf, len));
        }
        key_x += key.width as u32 * KEY_UNIT;
    }

    None
}

/// Clear pressed-key visual state on button release.
pub fn handle_release() {
    if let Some(mut state) = VKBD.try_lock() {
        state.pressed = None;
    }
}

/// Draw the virtual keyboard overlay onto the hardware framebuffer.
/// Called by compositor after VT blit, before mouse cursor draw.
///
/// — InputShade: direct pixel writes to hw_fb. No allocation. The keyboard
/// area was just painted by the compositor's VT blit, so we're overwriting
/// stale terminal content with our key grid. Next compositor tick repaints
/// the VT underneath, then we repaint the keyboard on top. 30 FPS, no tearing.
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

        // — InputShade: draw each row of keys
        for (row_idx, row) in ROWS.iter().enumerate() {
            let key_y = kb_y + KB_PADDING + row_idx as u32 * (KEY_H + KEY_GAP);
            let row_start_x = compute_row_start_x(row, screen_w);
            let mut key_x = row_start_x;

            for (col_idx, key) in row.iter().enumerate() {
                let key_w = key.width as u32 * KEY_UNIT - KEY_GAP;

                // — InputShade: pick color based on state
                let color = if state.pressed == Some((row_idx, col_idx)) {
                    KEY_PRESSED_COLOR
                } else if key.is_modifier && is_modifier_active(&state, key.label) {
                    MOD_ACTIVE_COLOR
                } else {
                    KEY_COLOR
                };

                // Draw key background
                fill_rect(buf, stride, bpp, format, key_x, key_y, key_w, KEY_H, color);

                // — InputShade: draw key label using PSF2 font
                let label = if state.is_shifted() { key.shifted_label } else { key.label };
                draw_key_label(buf, stride, bpp, format, key_x, key_y, key_w, label);

                key_x += key.width as u32 * KEY_UNIT;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
//  Internal Helpers
// ═══════════════════════════════════════════════════════════════════

/// Compute the starting X position for a row to center it on screen.
fn compute_row_start_x(row: &[VKey], screen_w: u32) -> u32 {
    let total_units: u32 = row.iter().map(|k| k.width as u32).sum();
    let total_px = total_units * KEY_UNIT;
    if total_px >= screen_w { 0 } else { (screen_w - total_px) / 2 }
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

/// Fill a rectangle on the framebuffer.
/// — InputShade: the pixel loop that makes rectangles happen. Raw pointers,
/// no bounds checks beyond our own. We trust the compositor gave us valid memory.
fn fill_rect(buf: *mut u8, stride: usize, bpp: usize, format: PixelFormat,
             x: u32, y: u32, w: u32, h: u32, color_argb: u32) {
    let pixel = argb_to_pixel(format, color_argb);
    unsafe {
        for row in 0..h {
            let row_offset = (y + row) as usize * stride;
            for col in 0..w {
                let offset = row_offset + (x + col) as usize * bpp;
                core::ptr::copy_nonoverlapping(pixel.as_ptr(), buf.add(offset), bpp.min(4));
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
//  Hit-test helper — extracts the core tap logic for testability.
//  Used by handle_tap() and unit tests.
// ═══════════════════════════════════════════════════════════════════

/// — InputShade: pure hit-test on a virtual keyboard state.
/// Returns (row_idx, col_idx, &VKey) for the key at (x, y), or None.
fn hit_test(x: i32, y: i32, screen_w: u32, screen_h: u32) -> Option<(usize, usize, &'static VKey)> {
    let kb_y = screen_h - KB_HEIGHT;
    if (y as u32) < kb_y || y < 0 || x < 0 {
        return None;
    }

    let rel_y = y as u32 - kb_y - KB_PADDING;
    let row_idx = (rel_y / (KEY_H + KEY_GAP)) as usize;
    if row_idx >= KB_ROWS {
        return None;
    }

    let row = ROWS[row_idx];
    let row_start_x = compute_row_start_x(row, screen_w);
    if (x as u32) < row_start_x {
        return None;
    }

    let mut key_x = row_start_x;
    for (col_idx, key) in row.iter().enumerate() {
        let key_w = key.width as u32 * KEY_UNIT - KEY_GAP;
        if (x as u32) >= key_x && (x as u32) < key_x + key_w {
            return Some((row_idx, col_idx, key));
        }
        key_x += key.width as u32 * KEY_UNIT;
    }

    None
}

// ═══════════════════════════════════════════════════════════════════
//  Unit Tests — CrashBloom: validating the vkbd before it leaves the lab
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN_W: u32 = 1280;
    const SCREEN_H: u32 = 800;

    // — CrashBloom: layout constants sanity — if these break, the keyboard
    // renders off-screen or keys overlap. Catch it before the user does.

    #[test]
    fn kb_height_fits_screen() {
        assert!(KB_HEIGHT < SCREEN_H, "keyboard taller than screen");
        assert!(KB_HEIGHT > 0, "keyboard has zero height");
        // — CrashBloom: 5 rows × 34px + 8px padding = 178px. Sanity check.
        assert_eq!(KB_HEIGHT, 5 * (KEY_H + KEY_GAP) + KB_PADDING * 2);
    }

    #[test]
    fn all_rows_fit_screen_width() {
        // — CrashBloom: every row must fit within 1280px
        for (i, row) in ROWS.iter().enumerate() {
            let total_units: u32 = row.iter().map(|k| k.width as u32).sum();
            let total_px = total_units * KEY_UNIT;
            assert!(total_px <= SCREEN_W,
                "row {} is {}px wide, exceeds screen {}px", i, total_px, SCREEN_W);
        }
    }

    #[test]
    fn row_centering() {
        // — CrashBloom: rows center horizontally. Verify start_x is positive.
        for (i, row) in ROWS.iter().enumerate() {
            let start_x = compute_row_start_x(row, SCREEN_W);
            assert!(start_x > 0, "row {} not centered (start_x=0)", i);
            let total_units: u32 = row.iter().map(|k| k.width as u32).sum();
            let end_x = start_x + total_units * KEY_UNIT;
            assert!(end_x <= SCREEN_W + KEY_GAP,
                "row {} extends past screen: end_x={}", i, end_x);
        }
    }

    #[test]
    fn row_centering_narrow_screen() {
        // — CrashBloom: if screen is narrower than keyboard, start_x = 0
        let narrow = 200;
        for row in &ROWS {
            let start_x = compute_row_start_x(row, narrow);
            assert_eq!(start_x, 0, "should clamp to 0 on narrow screen");
        }
    }

    #[test]
    fn argb_to_pixel_bgra() {
        // — CrashBloom: BGRA swizzle — R and B swap, A stays in byte 3
        let px = argb_to_pixel(PixelFormat::BGRA8888, 0xFF112233);
        // ARGB = FF 11 22 33 → BGRA = [33, 22, 11, FF]
        assert_eq!(px, [0x33, 0x22, 0x11, 0xFF]);
    }

    #[test]
    fn argb_to_pixel_rgba() {
        // — CrashBloom: RGBA keeps R/G/B order, A in byte 3
        let px = argb_to_pixel(PixelFormat::RGBA8888, 0xFF112233);
        // ARGB = FF 11 22 33 → RGBA = [11, 22, 33, FF]
        assert_eq!(px, [0x11, 0x22, 0x33, 0xFF]);
    }

    #[test]
    fn vkbd_state_shifted() {
        // — CrashBloom: shift XOR caps — same as every keyboard since 1984
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
        // — CrashBloom: verify key counts match expected QWERTY layout
        assert_eq!(ROW0.len(), 14, "number row: 13 keys + Bksp");
        assert_eq!(ROW1.len(), 14, "QWERTY row: Tab + 13 keys");
        assert_eq!(ROW2.len(), 13, "ASDF row: Caps + 11 keys + Enter");
        assert_eq!(ROW3.len(), 13, "ZXCV row: Shift + 10 keys + Up + Shift");
        assert_eq!(ROW4.len(), 7,  "bottom row: Ctrl + Alt + Esc + Space + 3 arrows");
    }

    #[test]
    fn all_normal_keys_have_output() {
        // — CrashBloom: every non-modifier key must produce at least one byte
        for (i, row) in ROWS.iter().enumerate() {
            for (j, key) in row.iter().enumerate() {
                if !key.is_modifier {
                    assert!(!key.output.is_empty(),
                        "row {} key {} ({:?}) has empty output",
                        i, j, core::str::from_utf8(key.label));
                    assert!(!key.shifted_output.is_empty(),
                        "row {} key {} ({:?}) has empty shifted_output",
                        i, j, core::str::from_utf8(key.label));
                }
            }
        }
    }

    #[test]
    fn modifier_keys_have_no_output() {
        // — CrashBloom: modifier keys don't produce bytes
        for (i, row) in ROWS.iter().enumerate() {
            for (j, key) in row.iter().enumerate() {
                if key.is_modifier {
                    assert!(key.output.is_empty(),
                        "modifier at row {} key {} should have empty output", i, j);
                }
            }
        }
    }

    // — CrashBloom: hit-test validation — make sure clicks on keys resolve correctly

    #[test]
    fn hit_test_above_keyboard_misses() {
        // — CrashBloom: clicking above the keyboard area returns None
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
        // — CrashBloom: space bar is row 4, center of screen, 8 units wide.
        // Click dead center should hit it.
        let row4_y = SCREEN_H - KB_HEIGHT + KB_PADDING + 4 * (KEY_H + KEY_GAP) + KEY_H / 2;
        let result = hit_test(640, row4_y as i32, SCREEN_W, SCREEN_H);
        assert!(result.is_some(), "should hit something in row 4 center");
        let (row, _col, key) = result.unwrap();
        assert_eq!(row, 4, "should be row 4");
        assert_eq!(key.label, b"Space", "center of row 4 should be Space");
    }

    #[test]
    fn hit_test_finds_first_key_row0() {
        // — CrashBloom: backtick key is first in row 0
        let row0_y = SCREEN_H - KB_HEIGHT + KB_PADDING + KEY_H / 2;
        let row0_start = compute_row_start_x(ROW0, SCREEN_W);
        let result = hit_test((row0_start + 5) as i32, row0_y as i32, SCREEN_W, SCREEN_H);
        assert!(result.is_some(), "should hit backtick key");
        let (row, col, key) = result.unwrap();
        assert_eq!(row, 0);
        assert_eq!(col, 0);
        assert_eq!(key.label, b"`");
    }

    #[test]
    fn hit_test_left_of_row_misses() {
        // — CrashBloom: click left of the centered row should miss
        let row0_y = SCREEN_H - KB_HEIGHT + KB_PADDING + KEY_H / 2;
        let row0_start = compute_row_start_x(ROW0, SCREEN_W);
        assert!(row0_start > 0, "row should be centered");
        let result = hit_test((row0_start - 5) as i32, row0_y as i32, SCREEN_W, SCREEN_H);
        assert!(result.is_none(), "click left of row should miss");
    }

    #[test]
    fn toggle_flips_visibility() {
        // — CrashBloom: toggle flips the atomic flag
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
        // — CrashBloom: Enter must produce '\n', not '\r' or empty
        let enter = ROW2.iter().find(|k| k.label == b"Enter").unwrap();
        assert_eq!(enter.output, b"\n");
        assert_eq!(enter.shifted_output, b"\n");
    }

    #[test]
    fn backspace_produces_delete() {
        // — CrashBloom: Bksp must produce DEL (0x7F)
        let bksp = ROW0.iter().find(|k| k.label == b"Bksp").unwrap();
        assert_eq!(bksp.output, b"\x7f");
    }

    #[test]
    fn escape_key_produces_esc() {
        let esc = ROW4.iter().find(|k| k.label == b"Esc").unwrap();
        assert_eq!(esc.output, b"\x1b");
    }

    #[test]
    fn arrow_keys_produce_ansi() {
        let up = ROW3.iter().find(|k| k.label == b"Up").unwrap();
        assert_eq!(up.output, b"\x1b[A");
        let left = ROW4.iter().find(|k| k.label == b"Left").unwrap();
        assert_eq!(left.output, b"\x1b[D");
        let down = ROW4.iter().find(|k| k.label == b"Down").unwrap();
        assert_eq!(down.output, b"\x1b[B");
        let right = ROW4.iter().find(|k| k.label == b"Right").unwrap();
        assert_eq!(right.output, b"\x1b[C");
    }

    #[test]
    fn shifted_letters_are_uppercase() {
        // — CrashBloom: every letter key's shifted output should be uppercase
        let letter_rows = [ROW1, ROW2, ROW3];
        for row in &letter_rows {
            for key in row.iter() {
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
        // — CrashBloom: verify the standard US keyboard symbol row
        let expected = [
            (b"1", b"!"), (b"2", b"@"), (b"3", b"#"), (b"4", b"$"),
            (b"5", b"%"), (b"6", b"^"), (b"7", b"&"), (b"8", b"*"),
            (b"9", b"("), (b"0", b")"),
        ];
        for (label, shifted) in &expected {
            let key = ROW0.iter().find(|k| k.label == *label).unwrap();
            assert_eq!(key.shifted_output, *shifted,
                "key '{}' shifted should produce '{}'",
                core::str::from_utf8(key.label).unwrap(),
                core::str::from_utf8(key.shifted_output).unwrap());
        }
    }
}
