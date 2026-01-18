//! Framebuffer Graphics for EFFLUX OS
//!
//! Provides framebuffer abstraction, text console, and font rendering.

#![no_std]

extern crate alloc;

pub mod color;
pub mod framebuffer;
pub mod console;
pub mod font;

pub use color::{Color, PixelFormat};
pub use framebuffer::{Framebuffer, FramebufferInfo, LinearFramebuffer};
pub use console::{FbConsole, Cell};
pub use font::{Font, Glyph, PSF2_FONT};

use alloc::sync::Arc;
use spin::Mutex;

/// Global framebuffer instance
static FRAMEBUFFER: Mutex<Option<Arc<dyn Framebuffer>>> = Mutex::new(None);

/// Global console instance
static CONSOLE: Mutex<Option<FbConsole>> = Mutex::new(None);

/// Initialize framebuffer from boot info
pub fn init(info: FramebufferInfo) {
    let fb = Arc::new(LinearFramebuffer::new(info));
    *FRAMEBUFFER.lock() = Some(fb.clone());

    // Initialize console
    let console = FbConsole::new(fb);
    *CONSOLE.lock() = Some(console);
}

/// Get the framebuffer
pub fn framebuffer() -> Option<Arc<dyn Framebuffer>> {
    FRAMEBUFFER.lock().clone()
}

/// Get the console
pub fn console() -> &'static Mutex<Option<FbConsole>> {
    &CONSOLE
}

/// Write a character to the console
pub fn putchar(ch: char) {
    if let Some(ref mut console) = *CONSOLE.lock() {
        console.putchar(ch);
    }
}

/// Write a string to the console
pub fn puts(s: &str) {
    if let Some(ref mut console) = *CONSOLE.lock() {
        for ch in s.chars() {
            console.putchar(ch);
        }
    }
}

/// Clear the console
pub fn clear() {
    if let Some(ref mut console) = *CONSOLE.lock() {
        console.clear();
    }
}

/// Set console colors
pub fn set_colors(fg: Color, bg: Color) {
    if let Some(ref mut console) = *CONSOLE.lock() {
        console.set_fg_color(fg);
        console.set_bg_color(bg);
    }
}
