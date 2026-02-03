//! # Panel Library
//!
//! -- NeonRoot: Panel management - z-order window stacking

use crate::{Result, WINDOW};

pub struct Panel {
    win: WINDOW,
    below: Option<*mut Panel>,
    above: Option<*mut Panel>,
}

pub fn new_panel(win: WINDOW) -> *mut Panel {
    core::ptr::null_mut()
}

pub fn del_panel(_panel: *mut Panel) -> Result<()> {
    Ok(())
}

pub fn show_panel(_panel: *mut Panel) -> Result<()> {
    Ok(())
}

pub fn hide_panel(_panel: *mut Panel) -> Result<()> {
    Ok(())
}

pub fn panel_above(_panel: *mut Panel) -> *mut Panel {
    core::ptr::null_mut()
}

pub fn panel_below(_panel: *mut Panel) -> *mut Panel {
    core::ptr::null_mut()
}

pub fn update_panels() {}
