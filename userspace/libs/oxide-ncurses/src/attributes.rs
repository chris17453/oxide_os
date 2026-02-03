//! # Attributes Management
//!
//! -- GraveShift: Attribute system - style your output

use crate::{Error, Result, WINDOW, attrs};

pub fn attron(attr: u32) -> Result<()> {
    wattron(crate::screen::stdscr(), attr)
}

pub fn wattron(win: WINDOW, attr: u32) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }
    unsafe {
        (*win).attrs |= attr;
    }
    Ok(())
}

pub fn attroff(attr: u32) -> Result<()> {
    wattroff(crate::screen::stdscr(), attr)
}

pub fn wattroff(win: WINDOW, attr: u32) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }
    unsafe {
        (*win).attrs &= !attr;
    }
    Ok(())
}

pub fn attrset(attr: u32) -> Result<()> {
    wattrset(crate::screen::stdscr(), attr)
}

pub fn wattrset(win: WINDOW, attr: u32) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }
    unsafe {
        (*win).attrs = attr;
    }
    Ok(())
}

pub fn attr_get() -> (u32, i16) {
    wattr_get(crate::screen::stdscr())
}

pub fn wattr_get(win: WINDOW) -> (u32, i16) {
    if win.is_null() {
        return (attrs::A_NORMAL, 0);
    }
    unsafe {
        let attr = (*win).attrs;
        let pair = ((attr & attrs::A_COLOR) >> 17) as i16;
        (attr, pair)
    }
}

pub fn attr_set(attr: u32, pair: i16) -> Result<()> {
    wattr_set(crate::screen::stdscr(), attr, pair)
}

pub fn wattr_set(win: WINDOW, attr: u32, pair: i16) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }
    unsafe {
        let color_attr = crate::color::color_pair(pair as i32);
        (*win).attrs = (attr & !attrs::A_COLOR) | color_attr;
    }
    Ok(())
}
