//! Input Handling & Timeout
//!
//! The event loop that mediates between human fingers and silicon destiny.
//! Uses UEFI stall for timing and SimpleTextInput for keystrokes.
//!
//! — InputShade: every keystroke is a fork in the boot timeline

use crate::efi::{self, EfiInputKey, EfiGraphicsOutputProtocol};
use crate::efi::text::*;

use crate::config::{BootConfig, MAX_OPTIONS};
use crate::editor::LineEditor;
use crate::menu::{self, MenuMode, MenuState};

/// Result of processing the boot menu
/// — InputShade: the boot manager's final answer
pub enum MenuResult {
    /// Boot entry at this index with these options
    Boot(usize),
    /// Enter diagnostic console
    Console,
    /// No valid selection (error state)
    Error,
}

/// Run the boot menu event loop. Returns when user selects an entry or timeout expires.
///
/// — InputShade: the loop that stands between POST and kernel_main
pub fn run_menu_loop(
    config: &mut BootConfig,
    state: &mut MenuState,
) -> MenuResult {
    // If no entries, this is an error state — no menu to show
    if state.entry_count == 0 {
        return MenuResult::Error;
    }

    // Instant boot (timeout = 0)
    if state.countdown == 0 {
        return MenuResult::Boot(state.selected);
    }

    // Simple polling-based loop with stall-based timing
    // — InputShade: 100ms poll intervals, count to 10 for 1 second
    let mut tick_counter: u32 = 0;

    loop {
        // Poll for key input (non-blocking)
        if let Some(key) = efi::read_key() {
            match process_key(key, config, state) {
                KeyAction::None => {}
                KeyAction::RedrawEntries => {
                    redraw_with_gop(|gop| menu::redraw_entries(gop, config, state));
                }
                KeyAction::RedrawFull => {
                    redraw_with_gop(|gop| menu::render_full_menu(gop, config, state));
                }
                KeyAction::Boot => return MenuResult::Boot(state.selected),
                KeyAction::Console => return MenuResult::Console,
                KeyAction::EditOptions => {
                    run_editor(config, state);
                    // After editor, redraw the full menu
                    redraw_with_gop(|gop| menu::render_full_menu(gop, config, state));
                }
            }
        }

        // 100ms stall between polls
        efi::stall(100_000);

        // Countdown tick every ~1 second (10 × 100ms)
        tick_counter += 1;
        if tick_counter >= 10 {
            tick_counter = 0;
            if state.tick_countdown() {
                return MenuResult::Boot(state.selected);
            }
            redraw_with_gop(|gop| menu::redraw_countdown(gop, state));
        }
    }
}

/// What to do after processing a keypress
enum KeyAction {
    None,
    RedrawEntries,
    RedrawFull,
    Boot,
    Console,
    EditOptions,
}

/// Process a key event and return the action to take
/// — InputShade: key → intent → action, the holy trinity of input handling
fn process_key(key: EfiInputKey, _config: &BootConfig, state: &mut MenuState) -> KeyAction {
    // — InputShade: special keys (scan_code != 0). VirtIO keyboard may set BOTH
    // scan_code and unicode_char for printable keys, so unrecognized scan codes
    // fall through to the unicode_char check instead of returning None.
    if key.scan_code != SCAN_NULL {
        match key.scan_code {
            SCAN_UP => {
                state.move_up();
                return KeyAction::RedrawEntries;
            }
            SCAN_DOWN => {
                state.move_down();
                return KeyAction::RedrawEntries;
            }
            SCAN_HOME => {
                state.selected = 0;
                state.cancel_countdown();
                return KeyAction::RedrawEntries;
            }
            SCAN_END => {
                if state.entry_count > 0 {
                    state.selected = state.entry_count - 1;
                }
                state.cancel_countdown();
                return KeyAction::RedrawEntries;
            }
            SCAN_ESC => {
                state.cancel_countdown();
                return KeyAction::RedrawFull;
            }
            _ => {} // — InputShade: fall through — VirtIO sets scan_code for printable chars too
        }
    }

    // Printable characters (unicode_char != 0)
    if key.unicode_char != 0 {
        let c = key.unicode_char;
        return match c {
            0x000D | 0x000A => {
                // Enter — boot selected entry
                state.mode = MenuMode::Boot;
                KeyAction::Boot
            }
            0x0065 | 0x0045 => {
                // 'e' | 'E'
                state.cancel_countdown();
                KeyAction::EditOptions
            }
            0x0063 | 0x0043 => {
                // 'c' | 'C'
                state.cancel_countdown();
                KeyAction::Console
            }
            _ => {
                // Any other key cancels countdown
                state.cancel_countdown();
                KeyAction::RedrawFull
            }
        };
    }

    KeyAction::None
}

/// Run the line editor for boot options
/// — InputShade: inline editing, because retyping the whole thing is medieval
fn run_editor(config: &mut BootConfig, state: &mut MenuState) {
    let idx = state.selected;
    if idx >= config.entry_count {
        return;
    }

    let entry = &config.entries[idx];
    let mut editor = LineEditor::new();

    // Initialize with current options
    editor.set_content(&entry.options[..entry.options_len]);

    // Render initial editor overlay
    redraw_with_gop(|gop| {
        menu::render_editor_overlay(
            gop,
            state,
            &entry.label,
            entry.label_len,
            editor.buffer(),
            editor.len(),
            editor.cursor(),
        );
    });

    loop {
        if let Some(key) = efi::read_key() {
            match editor.process_key(key) {
                crate::editor::EditorAction::Continue => {
                    // Redraw editor
                    let entry = &config.entries[idx];
                    redraw_with_gop(|gop| {
                        menu::render_editor_overlay(
                            gop,
                            state,
                            &entry.label,
                            entry.label_len,
                            editor.buffer(),
                            editor.len(),
                            editor.cursor(),
                        );
                    });
                }
                crate::editor::EditorAction::Accept => {
                    // Copy edited options back
                    let new_opts = editor.buffer();
                    let new_len = editor.len();
                    let dest = &mut config.entries[idx];
                    dest.options[..new_len].copy_from_slice(&new_opts[..new_len]);
                    if new_len < MAX_OPTIONS {
                        dest.options[new_len] = 0;
                    }
                    dest.options_len = new_len;
                    return;
                }
                crate::editor::EditorAction::Cancel => {
                    return; // — InputShade: user chickened out, options unchanged
                }
            }
        }

        efi::stall(10_000); // 10ms polling
    }
}

/// Helper to get GOP and run a closure with it
/// — InputShade: GOP access is a three-step dance with UEFI protocols
fn redraw_with_gop(f: impl FnOnce(*mut EfiGraphicsOutputProtocol)) {
    crate::with_gop(f);
}

/// Wait for any key press (used on error screens)
/// — InputShade: the "press any key to continue" of the UEFI world
pub fn wait_for_key() -> Option<EfiInputKey> {
    loop {
        if let Some(key) = efi::read_key() {
            return Some(key);
        }
        efi::stall(50_000); // 50ms
    }
}
