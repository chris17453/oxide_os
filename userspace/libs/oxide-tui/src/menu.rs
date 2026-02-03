//! # Menu Library
//!
//! -- IronGhost: Menu system - application menus

use crate::{WINDOW, Result};

pub struct Menu;
pub struct Item;

pub fn new_menu(_items: &[*mut Item]) -> *mut Menu {
    core::ptr::null_mut()
}

pub fn free_menu(_menu: *mut Menu) -> Result<()> {
    Ok(())
}

pub fn post_menu(_menu: *mut Menu) -> Result<()> {
    Ok(())
}

pub fn unpost_menu(_menu: *mut Menu) -> Result<()> {
    Ok(())
}

pub fn menu_driver(_menu: *mut Menu, _ch: i32) -> Result<()> {
    Ok(())
}
