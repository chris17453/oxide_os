//! # Color Support
//!
//! -- BlackLatch: Color system - paint the terminal

use crate::{Result, Error};
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy)]
struct ColorPair {
    fg: i16,
    bg: i16,
}

static mut COLOR_PAIRS: Option<Vec<ColorPair>> = None;
static mut HAS_COLORS: bool = false;
static mut CAN_CHANGE: bool = false;

pub fn start_color() -> Result<()> {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(COLOR_PAIRS);
        if (*ptr).is_none() {
            let mut pairs = Vec::new();
            pairs.resize(256, ColorPair { fg: 7, bg: 0 });
            *ptr = Some(pairs);
        }
        let hc = core::ptr::addr_of_mut!(HAS_COLORS);
        *hc = true;
    }
    Ok(())
}

pub fn has_colors() -> bool {
    unsafe { *core::ptr::addr_of!(HAS_COLORS) }
}

pub fn can_change_color() -> bool {
    unsafe { *core::ptr::addr_of!(CAN_CHANGE) }
}

pub fn init_pair(pair: i16, fg: i16, bg: i16) -> Result<()> {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(COLOR_PAIRS);
        if let Some(ref mut pairs) = *ptr {
            if pair > 0 && (pair as usize) < pairs.len() {
                pairs[pair as usize] = ColorPair { fg, bg };
                return Ok(());
            }
        }
    }
    Err(Error::Err)
}

pub fn init_color(_color: i16, _r: i16, _g: i16, _b: i16) -> Result<()> {
    Ok(())
}

pub fn color_pair(n: i32) -> u32 {
    ((n as u32) << 17) & crate::attrs::A_COLOR
}

pub fn pair_content(pair: i16) -> Result<(i16, i16)> {
    unsafe {
        let ptr = core::ptr::addr_of!(COLOR_PAIRS);
        if let Some(ref pairs) = *ptr {
            if pair > 0 && (pair as usize) < pairs.len() {
                let p = pairs[pair as usize];
                return Ok((p.fg, p.bg));
            }
        }
    }
    Err(Error::Err)
}
