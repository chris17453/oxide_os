//! # Pad Support
//!
//! -- NeonRoot: Pad system - virtual screens larger than terminal

use crate::{WINDOW, Result};

pub fn newpad(lines: i32, cols: i32) -> WINDOW {
    crate::window::newwin(lines, cols, 0, 0)
}

pub fn prefresh(_pad: WINDOW, _pminrow: i32, _pmincol: i32,
                _sminrow: i32, _smincol: i32,
                _smaxrow: i32, _smaxcol: i32) -> Result<()> {
    Ok(())
}

pub fn pnoutrefresh(_pad: WINDOW, _pminrow: i32, _pmincol: i32,
                    _sminrow: i32, _smincol: i32,
                    _smaxrow: i32, _smaxcol: i32) -> Result<()> {
    Ok(())
}
