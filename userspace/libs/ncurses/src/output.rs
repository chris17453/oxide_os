//! # Output Functions
//!
//! Character and string output to windows.
//!
//! -- GraveShift: Output pipeline - render everything to screen

use crate::{Error, Result, WINDOW, attrs, chtype};
use alloc::format;

/// Add a character to standard screen
pub fn addch(ch: chtype) -> Result<()> {
    waddch(crate::screen::stdscr(), ch)
}

/// Add a character to a window
pub fn waddch(win: WINDOW, ch: chtype) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }

    unsafe {
        let y = (*win).cur_y;
        let x = (*win).cur_x;
        (*win).set_cell(y, x, ch)?;

        // Advance cursor
        (*win).cur_x += 1;
        if (*win).cur_x >= (*win).cols {
            (*win).cur_x = 0;
            (*win).cur_y += 1;
            if (*win).cur_y >= (*win).lines {
                if (*win).scroll {
                    (*win).scroll_up(1);
                    (*win).cur_y = (*win).lines - 1;
                } else {
                    (*win).cur_y = (*win).lines - 1;
                }
            }
        }
    }

    Ok(())
}

/// Add a string to standard screen
pub fn addstr(s: &str) -> Result<()> {
    waddstr(crate::screen::stdscr(), s)
}

/// Add a string to a window
pub fn waddstr(win: WINDOW, s: &str) -> Result<()> {
    for ch in s.chars() {
        let ch_val = chtype::new(ch, attrs::A_NORMAL);
        waddch(win, ch_val)?;
    }
    Ok(())
}

/// Formatted print to standard screen
pub fn printw(fmt: &str) -> Result<()> {
    wprintw(crate::screen::stdscr(), fmt)
}

/// Formatted print to a window
pub fn wprintw(win: WINDOW, s: &str) -> Result<()> {
    waddstr(win, s)
}

/// Move cursor and print
pub fn mvprintw(y: i32, x: i32, s: &str) -> Result<()> {
    mvwprintw(crate::screen::stdscr(), y, x, s)
}

/// Move cursor and print to window
pub fn mvwprintw(win: WINDOW, y: i32, x: i32, s: &str) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }

    unsafe {
        (*win).move_cursor(y, x)?;
    }

    waddstr(win, s)
}

/// Move cursor and add character
pub fn mvaddch(y: i32, x: i32, ch: chtype) -> Result<()> {
    mvwaddch(crate::screen::stdscr(), y, x, ch)
}

/// Move cursor and add character to window
pub fn mvwaddch(win: WINDOW, y: i32, x: i32, ch: chtype) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }

    unsafe {
        (*win).move_cursor(y, x)?;
    }

    waddch(win, ch)
}

/// Move cursor
pub fn move_cursor(y: i32, x: i32) -> Result<()> {
    wmove(crate::screen::stdscr(), y, x)
}

/// Move cursor in window
pub fn wmove(win: WINDOW, y: i32, x: i32) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }

    unsafe { (*win).move_cursor(y, x) }
}

/// Clear screen
pub fn clear() -> Result<()> {
    wclear(crate::screen::stdscr())
}

/// Clear window
pub fn wclear(win: WINDOW) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }

    unsafe {
        (*win).clear();
    }

    Ok(())
}

/// Erase window
pub fn erase() -> Result<()> {
    werase(crate::screen::stdscr())
}

/// Erase window
pub fn werase(win: WINDOW) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }

    unsafe {
        (*win).erase();
    }

    Ok(())
}

/// Clear to end of line
pub fn clrtoeol() -> Result<()> {
    wclrtoeol(crate::screen::stdscr())
}

/// Clear to end of line in window
pub fn wclrtoeol(win: WINDOW) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }

    unsafe {
        (*win).clrtoeol();
    }

    Ok(())
}

/// Clear to bottom of screen
pub fn clrtobot() -> Result<()> {
    wclrtobot(crate::screen::stdscr())
}

/// Clear to bottom of window
pub fn wclrtobot(win: WINDOW) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }

    unsafe {
        (*win).clrtobot();
    }

    Ok(())
}

/// Draw a box around window
pub fn border(
    win: WINDOW,
    ls: chtype,
    rs: chtype,
    ts: chtype,
    bs: chtype,
    tl: chtype,
    tr: chtype,
    bl: chtype,
    br: chtype,
) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }

    unsafe {
        let h = (*win).lines;
        let w = (*win).cols;

        // Top border
        (*win).set_cell(0, 0, tl)?;
        for x in 1..(w - 1) {
            (*win).set_cell(0, x, ts)?;
        }
        (*win).set_cell(0, w - 1, tr)?;

        // Sides
        for y in 1..(h - 1) {
            (*win).set_cell(y, 0, ls)?;
            (*win).set_cell(y, w - 1, rs)?;
        }

        // Bottom border
        (*win).set_cell(h - 1, 0, bl)?;
        for x in 1..(w - 1) {
            (*win).set_cell(h - 1, x, bs)?;
        }
        (*win).set_cell(h - 1, w - 1, br)?;
    }

    Ok(())
}

/// Draw a simple box
pub fn box_win(win: WINDOW, verch: chtype, horch: chtype) -> Result<()> {
    border(win, verch, verch, horch, horch, verch, verch, verch, verch)
}
