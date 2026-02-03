//! # C-Compatible API
//!
//! -- NeonRoot: C ABI - bridge to legacy applications

use core::ffi::c_int;
use crate::WINDOW;

#[unsafe(no_mangle)]
pub extern "C" fn nc_initscr() -> WINDOW {
    crate::screen::initscr()
}

#[unsafe(no_mangle)]
pub extern "C" fn nc_endwin() -> c_int {
    match crate::screen::endwin() {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn nc_newwin(lines: c_int, cols: c_int, y: c_int, x: c_int) -> WINDOW {
    crate::window::newwin(lines, cols, y, x)
}

#[unsafe(no_mangle)]
pub extern "C" fn nc_delwin(win: WINDOW) -> c_int {
    match crate::window::delwin(win) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn nc_refresh() -> c_int {
    match crate::screen::refresh() {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn nc_getch() -> c_int {
    crate::input::getch()
}

#[unsafe(no_mangle)]
pub extern "C" fn nc_start_color() -> c_int {
    match crate::color::start_color() {
        Ok(_) => 0,
        Err(_) => -1,
    }
}
