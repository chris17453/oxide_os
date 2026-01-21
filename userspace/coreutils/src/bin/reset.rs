//! reset - reset terminal to default state

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    // Send terminal reset sequence
    // This is the standard VT100 reset sequence
    
    // ESC c - Full reset
    prints("\x1B\x63");
    
    // Additional sequences for thorough reset:
    // ESC[!p - Soft reset
    prints("\x1B[!p");
    
    // ESC[?1049l - Exit alternate screen buffer
    prints("\x1B[?1049l");
    
    // ESC[?25h - Show cursor
    prints("\x1B[?25h");
    
    // ESC[0m - Reset attributes
    prints("\x1B[0m");
    
    // Clear screen
    prints("\x1B[2J");
    
    // Move cursor to home
    prints("\x1B[H");

    0
}
