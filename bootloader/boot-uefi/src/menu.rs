//! Graphical Boot Menu
//!
//! The beating heart of the OXIDE boot manager — a cyberpunk-themed menu rendered
//! via UEFI GOP that lets you pick which kernel to die with today.
//! All rendering is through the font.rs bitmap renderer.
//!
//! — NeonVale: where aesthetics meet the pre-boot void

use crate::efi::{EfiBltPixel, EfiGraphicsOutputProtocol};

use crate::config::BootConfig;
use crate::font::{
    self, draw_buf_string, draw_string, fill_rect, format_u32, FONT_HEIGHT, FONT_WIDTH,
    COLOR_BG, COLOR_CYAN, COLOR_DARK_GRAY, COLOR_GRAY, COLOR_GREEN, COLOR_MUTED, COLOR_ORANGE,
    COLOR_RED, COLOR_WHITE,
};

/// Menu state — tracks what the user is looking at and what we're doing about it
/// — NeonVale: the ephemeral consciousness of the boot manager
pub struct MenuState {
    /// Currently highlighted entry index
    pub selected: usize,
    /// Total number of entries
    pub entry_count: usize,
    /// Default entry index (shown with [default] tag)
    pub default_index: usize,
    /// Countdown seconds remaining (-1 = no countdown, 0 = boot now)
    pub countdown: i32,
    /// Whether user has interacted (cancels countdown)
    pub interactive: bool,
    /// Current mode of the menu
    pub mode: MenuMode,
    /// Screen dimensions
    pub screen_width: usize,
    pub screen_height: usize,
}

/// What the menu is currently doing
#[derive(PartialEq, Clone, Copy)]
pub enum MenuMode {
    /// Showing the boot menu
    Select,
    /// Editing boot options for selected entry
    EditOptions,
    /// Diagnostic console
    Console,
    /// Booting the selected entry
    Boot,
}

impl MenuState {
    pub fn new(config: &BootConfig, screen_width: usize, screen_height: usize) -> Self {
        Self {
            selected: config.default_index,
            entry_count: config.entry_count,
            default_index: config.default_index,
            countdown: config.timeout_secs,
            interactive: false,
            mode: MenuMode::Select,
            screen_width,
            screen_height,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
        self.cancel_countdown();
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entry_count {
            self.selected += 1;
        }
        self.cancel_countdown();
    }

    pub fn cancel_countdown(&mut self) {
        self.interactive = true;
        self.countdown = -1;
    }

    pub fn tick_countdown(&mut self) -> bool {
        if self.countdown > 0 {
            self.countdown -= 1;
            if self.countdown == 0 {
                self.mode = MenuMode::Boot;
                return true; // time to boot
            }
        }
        false
    }
}

/// Layout constants — calculated relative to screen size
/// — NeonVale: pixel-perfect positioning because we're not savages
struct Layout {
    /// Left margin of the menu box
    menu_x: usize,
    /// Top of the title text
    title_y: usize,
    /// Top of the menu box
    menu_box_y: usize,
    /// Width of the menu box
    menu_width: usize,
    /// Y position of the first menu entry
    first_entry_y: usize,
    /// Height of each entry row
    entry_height: usize,
    /// Y position of the options display
    options_y: usize,
    /// Y position of the separator line
    separator_y: usize,
    /// Y position of the footer text
    footer_y: usize,
    /// Y position of the countdown
    countdown_y: usize,
}

impl Layout {
    fn calculate(width: usize, height: usize, entry_count: usize) -> Self {
        let menu_width = (width * 60) / 100; // 60% of screen width
        let menu_x = (width - menu_width) / 2;

        // — NeonVale: minimum 5 visible slots so the menu doesn't look like a
        // postage stamp when there's only 1 kernel configured
        let visible_slots = if entry_count < 5 { 5 } else { entry_count };

        // — NeonVale: Vertical layout flows TOP-DOWN: logo → title → menu.
        // Logo is 150px tall starting at y=20, so it ends at y=170.
        // Title sits below the logo with breathing room, menu box below that.
        let logo_bottom = 20 + 150; // must match draw_oxide_logo() in main.rs
        let title_y = logo_bottom + 12; // 12px gap between logo and title
        let menu_box_y = title_y + FONT_HEIGHT * 2 + 8; // title + version + divider + padding
        let entry_height = FONT_HEIGHT + 8; // 8px padding between entries
        let first_entry_y = menu_box_y + 8; // 8px padding inside box
        let entries_height = visible_slots * entry_height;
        let options_y = first_entry_y + entries_height + 16;
        let separator_y = options_y + FONT_HEIGHT + 16;
        let footer_y = separator_y + 8;
        let countdown_y = footer_y + FONT_HEIGHT + 4;

        Self {
            menu_x,
            title_y,
            menu_box_y,
            menu_width,
            first_entry_y,
            entry_height,
            options_y,
            separator_y,
            footer_y,
            countdown_y,
        }
    }
}

/// Render the complete boot menu from scratch.
/// — NeonVale: painting the void with purpose — every pixel deliberate
pub fn render_full_menu(
    gop: *mut EfiGraphicsOutputProtocol,
    config: &BootConfig,
    state: &MenuState,
) {
    let width = state.screen_width;
    let height = state.screen_height;
    let layout = Layout::calculate(width, height, state.entry_count);

    // 1. Clear entire screen to background
    fill_rect(gop, 0, 0, width, height, COLOR_BG);

    // 2. Draw OXIDE logo
    crate::draw_oxide_logo(gop, width, height);

    // 3. Draw title — NeonVale: the marquee that greets the operator
    let title = "OXIDE Boot Manager";
    let title_x = (width - title.len() * FONT_WIDTH) / 2;
    draw_string(gop, title_x, layout.title_y, title, COLOR_ORANGE, COLOR_BG);

    // Build version string below title
    // — PatchBay: NT-style build number, stamped in silicon
    let mut ver_buf = [0u8; 48];
    let ver_len = format_version_string(&mut ver_buf);
    let ver_str = core::str::from_utf8(&ver_buf[..ver_len]).unwrap_or("?.?.?");
    let ver_x = (width - ver_len * FONT_WIDTH) / 2;
    draw_string(gop, ver_x, layout.title_y + FONT_HEIGHT, ver_str, COLOR_MUTED, COLOR_BG);

    // Subtitle divider — NeonVale: using our custom box-drawing glyph 21 (─)
    // instead of UTF-8 which renders as ????? through our byte-level font renderer
    let divider_bytes: [u8; 32] = [21u8; 32]; // glyph 21 = ─ in our font
    let div_x = (width - 32 * FONT_WIDTH) / 2;
    draw_buf_string(gop, div_x, layout.title_y + FONT_HEIGHT + 2, &divider_bytes, COLOR_DARK_GRAY, COLOR_BG);

    // 4. Draw menu box border
    draw_menu_box(gop, &layout, state.entry_count);

    // 5. Draw entries
    for i in 0..state.entry_count {
        draw_entry(gop, config, state, &layout, i);
    }

    // 6. Draw current entry's options
    draw_options_line(gop, config, state, &layout);

    // 7. Draw separator
    for x in layout.menu_x..layout.menu_x + layout.menu_width {
        font::draw_hline(gop, x, layout.separator_y, 1, COLOR_DARK_GRAY);
    }

    // 8. Draw footer
    draw_footer(gop, state, &layout);

    // 9. Draw countdown
    draw_countdown(gop, state, &layout);
}

/// Redraw only the entries (highlight change) — faster than full repaint
/// — NeonVale: selective repaint because redrawing the whole screen is for amateurs
pub fn redraw_entries(
    gop: *mut EfiGraphicsOutputProtocol,
    config: &BootConfig,
    state: &MenuState,
) {
    let layout = Layout::calculate(state.screen_width, state.screen_height, state.entry_count);

    for i in 0..state.entry_count {
        draw_entry(gop, config, state, &layout, i);
    }

    // Also update the options line (selected entry changed)
    draw_options_line(gop, config, state, &layout);
}

/// Redraw only the countdown text — called every second
pub fn redraw_countdown(
    gop: *mut EfiGraphicsOutputProtocol,
    state: &MenuState,
) {
    let layout = Layout::calculate(state.screen_width, state.screen_height, state.entry_count);
    draw_countdown(gop, state, &layout);
}

/// Draw the menu box border using line characters
fn draw_menu_box(
    gop: *mut EfiGraphicsOutputProtocol,
    layout: &Layout,
    entry_count: usize,
) {
    // — NeonVale: box height matches visible_slots (min 5), not raw entry_count
    let visible_slots = if entry_count < 5 { 5 } else { entry_count };
    let box_height = visible_slots * layout.entry_height + 16; // 8px padding top + bottom
    let x = layout.menu_x;
    let y = layout.menu_box_y;
    let w = layout.menu_width;

    // Top border
    font::draw_hline(gop, x, y, w, COLOR_DARK_GRAY);
    // Bottom border
    font::draw_hline(gop, x, y + box_height, w, COLOR_DARK_GRAY);
    // Left border
    for dy in 0..box_height {
        crate::blt_fill(gop, COLOR_DARK_GRAY, x, y + dy, 1, 1);
    }
    // Right border
    for dy in 0..box_height {
        crate::blt_fill(gop, COLOR_DARK_GRAY, x + w - 1, y + dy, 1, 1);
    }
}

/// Draw a single menu entry
fn draw_entry(
    gop: *mut EfiGraphicsOutputProtocol,
    config: &BootConfig,
    state: &MenuState,
    layout: &Layout,
    index: usize,
) {
    let entry = &config.entries[index];
    let y = layout.first_entry_y + index * layout.entry_height;
    let x = layout.menu_x + 8; // padding from box edge
    let entry_width = layout.menu_width - 16; // padding both sides

    let is_selected = index == state.selected;
    let (fg, bg) = if is_selected {
        (COLOR_BG, COLOR_ORANGE) // — NeonVale: highlighted = inverted orange
    } else {
        (COLOR_GRAY, COLOR_BG)
    };

    // Clear entry background
    fill_rect(gop, x, y, entry_width, layout.entry_height - 2, bg);

    // Draw selection arrow
    let arrow_x = x + 4;
    if is_selected {
        // ► character (our custom glyph at position 16)
        font::draw_char(gop, arrow_x, y + 2, 16, fg, bg);
    }

    // Draw label
    let label_x = arrow_x + FONT_WIDTH + 8;
    draw_buf_string(gop, label_x, y + 2, &entry.label[..entry.label_len.min(entry.label.len())], fg, bg);

    // Draw [default] tag if applicable
    if index == state.default_index {
        let tag = "[default]";
        let tag_x = x + entry_width - (tag.len() * FONT_WIDTH) - 8;
        let tag_color = if is_selected { COLOR_BG } else { COLOR_CYAN };
        draw_string(gop, tag_x, y + 2, tag, tag_color, bg);
    }

    // Draw validity indicator
    if !entry.valid && entry.path_len > 0 {
        let warn = "[missing]";
        let warn_x = x + entry_width - (warn.len() * FONT_WIDTH) - 8;
        let warn_color = if is_selected { COLOR_BG } else { COLOR_RED };
        draw_string(gop, warn_x, y + 2, warn, warn_color, bg);
    }
}

/// Draw the boot options line below the menu box
fn draw_options_line(
    gop: *mut EfiGraphicsOutputProtocol,
    config: &BootConfig,
    state: &MenuState,
    layout: &Layout,
) {
    let x = layout.menu_x;
    let y = layout.options_y;
    let width = layout.menu_width;

    // Clear the line
    fill_rect(gop, x, y, width, FONT_HEIGHT, COLOR_BG);

    let prefix = "Options: ";
    let cx = draw_string(gop, x + 8, y, prefix, COLOR_MUTED, COLOR_BG);

    if state.selected < config.entry_count {
        let entry = &config.entries[state.selected];
        if entry.options_len > 0 {
            draw_buf_string(gop, cx, y, &entry.options[..entry.options_len], COLOR_GREEN, COLOR_BG);
        } else {
            draw_string(gop, cx, y, "(none)", COLOR_DARK_GRAY, COLOR_BG);
        }
    }
}

/// Draw the footer with keybinding hints
fn draw_footer(
    gop: *mut EfiGraphicsOutputProtocol,
    state: &MenuState,
    layout: &Layout,
) {
    let y = layout.footer_y;
    let x = layout.menu_x + 8;

    // Clear footer area
    fill_rect(gop, layout.menu_x, y, layout.menu_width, FONT_HEIGHT, COLOR_BG);

    // — NeonVale: ASCII-safe footer — no fancy arrows, just clarity
    let footer = "Up/Dn Select   Enter Boot   E Edit   C Console";
    draw_string(gop, x, y, footer, COLOR_MUTED, COLOR_BG);
}

/// Draw the countdown timer
fn draw_countdown(
    gop: *mut EfiGraphicsOutputProtocol,
    state: &MenuState,
    layout: &Layout,
) {
    let y = layout.countdown_y;
    let x = layout.menu_x + 8;
    let clear_width = layout.menu_width - 16;

    // Clear countdown area
    fill_rect(gop, x, y, clear_width, FONT_HEIGHT, COLOR_BG);

    if state.countdown > 0 && !state.interactive {
        let prefix = "Auto-boot in ";
        let cx = draw_string(gop, x, y, prefix, COLOR_MUTED, COLOR_BG);

        let mut num_buf = [0u8; 10];
        let num_len = format_u32(&mut num_buf, state.countdown as u32);
        let cx = draw_buf_string(gop, cx, y, &num_buf[..num_len], COLOR_ORANGE, COLOR_BG);

        draw_string(gop, cx, y, "...", COLOR_MUTED, COLOR_BG);
    } else if state.countdown == -1 || state.interactive {
        // — NeonVale: countdown cancelled, they're in control now
    }
}

/// Render an error screen when no kernels are found
/// — NeonVale: the scariest screen in any boot manager
pub fn render_error_screen(
    gop: *mut EfiGraphicsOutputProtocol,
    width: usize,
    height: usize,
    message: &str,
) {
    fill_rect(gop, 0, 0, width, height, COLOR_BG);

    // Draw logo
    crate::draw_oxide_logo(gop, width, height);

    let title = "OXIDE Boot Manager - ERROR";
    let title_x = (width - title.len() * FONT_WIDTH) / 2;
    let title_y = height / 3;
    draw_string(gop, title_x, title_y, title, COLOR_RED, COLOR_BG);

    let msg_x = (width - message.len() * FONT_WIDTH) / 2;
    let msg_y = title_y + FONT_HEIGHT * 3;
    draw_string(gop, msg_x, msg_y, message, COLOR_GRAY, COLOR_BG);

    let hint = "Press any key to reboot, or C for diagnostic console";
    let hint_x = (width - hint.len() * FONT_WIDTH) / 2;
    let hint_y = msg_y + FONT_HEIGHT * 3;
    draw_string(gop, hint_x, hint_y, hint, COLOR_MUTED, COLOR_BG);
}

/// Render the boot options editor overlay
/// — NeonVale: a tiny text editor floating over the void
pub fn render_editor_overlay(
    gop: *mut EfiGraphicsOutputProtocol,
    state: &MenuState,
    label: &[u8],
    label_len: usize,
    buffer: &[u8],
    buf_len: usize,
    cursor_pos: usize,
) {
    let width = state.screen_width;
    let height = state.screen_height;

    // Editor box dimensions
    let box_width = (width * 70) / 100;
    let box_height = FONT_HEIGHT * 7;
    let box_x = (width - box_width) / 2;
    let box_y = (height - box_height) / 2;

    // Background
    fill_rect(gop, box_x, box_y, box_width, box_height, COLOR_BG);

    // Border
    font::draw_hline(gop, box_x, box_y, box_width, COLOR_ORANGE);
    font::draw_hline(gop, box_x, box_y + box_height - 1, box_width, COLOR_ORANGE);
    for dy in 0..box_height {
        crate::blt_fill(gop, COLOR_ORANGE, box_x, box_y + dy, 1, 1);
        crate::blt_fill(gop, COLOR_ORANGE, box_x + box_width - 1, box_y + dy, 1, 1);
    }

    // Title
    let title_prefix = "Edit Boot Options for: ";
    let title_y = box_y + 8;
    let cx = draw_string(gop, box_x + 16, title_y, title_prefix, COLOR_GRAY, COLOR_BG);
    draw_buf_string(gop, cx, title_y, &label[..label_len], COLOR_ORANGE, COLOR_BG);

    // Input field background
    let field_y = title_y + FONT_HEIGHT + 8;
    let field_x = box_x + 16;
    let field_width = box_width - 32;
    fill_rect(gop, field_x, field_y, field_width, FONT_HEIGHT + 4, COLOR_DARK_GRAY);

    // Draw buffer contents
    let text_x = field_x + 4;
    let text_y = field_y + 2;
    draw_buf_string(gop, text_x, text_y, &buffer[..buf_len], COLOR_WHITE, COLOR_DARK_GRAY);

    // Draw cursor (block cursor at position)
    let cursor_x = text_x + cursor_pos * FONT_WIDTH;
    if cursor_x < field_x + field_width {
        let cursor_char = if cursor_pos < buf_len {
            buffer[cursor_pos]
        } else {
            b' '
        };
        font::draw_char(gop, cursor_x, text_y, cursor_char, COLOR_DARK_GRAY, COLOR_ORANGE);
    }

    // Footer hints
    let hint_y = field_y + FONT_HEIGHT + 12;
    draw_string(gop, box_x + 16, hint_y, "Enter: Accept    Escape: Cancel    Ctrl+U: Clear", COLOR_MUTED, COLOR_BG);
}

/// Format the NT-style version string: "v0.1.0 (Build 147)"
/// — PatchBay: the version stamp that graces the boot menu marquee
fn format_version_string(buf: &mut [u8; 48]) -> usize {
    let ver = env!("OXIDE_VERSION_STRING");
    let build = env!("OXIDE_BUILD_NUMBER");

    let mut pos = 0;

    // "v"
    buf[pos] = b'v';
    pos += 1;

    // Copy version string
    for &b in ver.as_bytes() {
        if pos >= 47 {
            break;
        }
        buf[pos] = b;
        pos += 1;
    }

    // " (Build "
    let suffix = b" (Build ";
    for &b in suffix {
        if pos >= 47 {
            break;
        }
        buf[pos] = b;
        pos += 1;
    }

    // Copy build number
    for &b in build.as_bytes() {
        if pos >= 47 {
            break;
        }
        buf[pos] = b;
        pos += 1;
    }

    // ")"
    if pos < 48 {
        buf[pos] = b')';
        pos += 1;
    }

    pos
}
