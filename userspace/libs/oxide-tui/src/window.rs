//! # Window Management
//!
//! Core window data structure and manipulation functions.
//! Windows are the fundamental abstraction in ncurses.
//!
//! -- GraveShift: Window system - every TUI starts here

use alloc::boxed::Box;
use alloc::vec::Vec;
use crate::{chtype, attrs, Error, Result};

/// Window data structure
#[repr(C)]
pub struct WindowData {
    /// Window height (lines)
    pub lines: i32,
    /// Window width (columns)
    pub cols: i32,
    /// Y position relative to parent (or screen)
    pub beg_y: i32,
    /// X position relative to parent (or screen)
    pub beg_x: i32,
    /// Current cursor Y position within window
    pub cur_y: i32,
    /// Current cursor X position within window
    pub cur_x: i32,
    /// Window contents (lines * cols)
    pub cells: Vec<chtype>,
    /// Window attributes
    pub attrs: u32,
    /// Background character and attributes
    pub bkgd: chtype,
    /// Scrolling enabled
    pub scroll: bool,
    /// Scroll region top
    pub scroll_top: i32,
    /// Scroll region bottom
    pub scroll_bottom: i32,
    /// Keypad enabled (translate special keys)
    pub keypad: bool,
    /// Nodelay mode (getch returns ERR immediately)
    pub nodelay: bool,
    /// Notimeout mode
    pub notimeout: bool,
    /// Clear on next refresh
    pub clear_flag: bool,
    /// Leave cursor where it is
    pub leaveok: bool,
    /// Immediate mode (auto refresh)
    pub immedok: bool,
    /// Synchronize with ancestors on refresh
    pub syncok: bool,
    /// Window needs refresh
    pub touched: bool,
    /// Parent window (for subwindows)
    pub parent: Option<*mut WindowData>,
}

impl WindowData {
    /// Create a new window
    pub fn new(lines: i32, cols: i32, beg_y: i32, beg_x: i32) -> Box<Self> {
        let size = (lines * cols) as usize;
        let mut cells = Vec::with_capacity(size);
        cells.resize(size, chtype::new(' ', attrs::A_NORMAL));
        
        Box::new(Self {
            lines,
            cols,
            beg_y,
            beg_x,
            cur_y: 0,
            cur_x: 0,
            cells,
            attrs: attrs::A_NORMAL,
            bkgd: chtype::new(' ', attrs::A_NORMAL),
            scroll: false,
            scroll_top: 0,
            scroll_bottom: lines - 1,
            keypad: false,
            nodelay: false,
            notimeout: false,
            clear_flag: false,
            leaveok: false,
            immedok: false,
            syncok: false,
            touched: true,
            parent: None,
        })
    }
    
    /// Get cell at position
    pub fn get_cell(&self, y: i32, x: i32) -> Option<&chtype> {
        if y >= 0 && y < self.lines && x >= 0 && x < self.cols {
            let index = (y * self.cols + x) as usize;
            self.cells.get(index)
        } else {
            None
        }
    }
    
    /// Set cell at position
    pub fn set_cell(&mut self, y: i32, x: i32, ch: chtype) -> Result<()> {
        if y >= 0 && y < self.lines && x >= 0 && x < self.cols {
            let index = (y * self.cols + x) as usize;
            if let Some(cell) = self.cells.get_mut(index) {
                *cell = ch;
                self.touched = true;
                Ok(())
            } else {
                Err(Error::Err)
            }
        } else {
            Err(Error::Err)
        }
    }
    
    /// Move cursor
    pub fn move_cursor(&mut self, y: i32, x: i32) -> Result<()> {
        if y >= 0 && y < self.lines && x >= 0 && x < self.cols {
            self.cur_y = y;
            self.cur_x = x;
            Ok(())
        } else {
            Err(Error::Err)
        }
    }
    
    /// Clear window
    pub fn clear(&mut self) {
        // ⚡ NeonVale: Per ncurses spec, clear() sets clear_flag so
        // the next wrefresh emits ESC[2J before redrawing. Without this,
        // TUI apps like top never actually clear the physical screen. ⚡
        let blank = chtype { ch: ' ' as u32, attr: self.attrs };
        for cell in self.cells.iter_mut() {
            *cell = blank;
        }
        self.cur_y = 0;
        self.cur_x = 0;
        self.clear_flag = true;
        self.touched = true;
    }
    
    /// Erase window (fill with background)
    pub fn erase(&mut self) {
        for cell in self.cells.iter_mut() {
            *cell = self.bkgd;
        }
        self.cur_y = 0;
        self.cur_x = 0;
        self.touched = true;
    }
    
    /// Clear to end of line
    pub fn clrtoeol(&mut self) {
        let blank = self.bkgd;
        for x in self.cur_x..self.cols {
            if let Err(_) = self.set_cell(self.cur_y, x, blank) {
                break;
            }
        }
        self.touched = true;
    }
    
    /// Clear to bottom of window
    pub fn clrtobot(&mut self) {
        // Clear current line from cursor
        self.clrtoeol();
        
        // Clear all lines below
        let blank = self.bkgd;
        for y in (self.cur_y + 1)..self.lines {
            for x in 0..self.cols {
                let _ = self.set_cell(y, x, blank);
            }
        }
        self.touched = true;
    }
    
    /// Scroll window up by n lines
    pub fn scroll_up(&mut self, n: i32) {
        if n <= 0 {
            return;
        }
        
        let n = n.min(self.lines);
        let blank = self.bkgd;
        
        // Shift lines up
        for y in 0..(self.lines - n) {
            for x in 0..self.cols {
                let src_idx = ((y + n) * self.cols + x) as usize;
                let dst_idx = (y * self.cols + x) as usize;
                if src_idx < self.cells.len() && dst_idx < self.cells.len() {
                    self.cells[dst_idx] = self.cells[src_idx];
                }
            }
        }
        
        // Clear bottom lines
        for y in (self.lines - n)..self.lines {
            for x in 0..self.cols {
                let _ = self.set_cell(y, x, blank);
            }
        }
        
        self.touched = true;
    }
    
    /// Scroll window down by n lines
    pub fn scroll_down(&mut self, n: i32) {
        if n <= 0 {
            return;
        }
        
        let n = n.min(self.lines);
        let blank = self.bkgd;
        
        // Shift lines down
        for y in (n..self.lines).rev() {
            for x in 0..self.cols {
                let src_idx = ((y - n) * self.cols + x) as usize;
                let dst_idx = (y * self.cols + x) as usize;
                if src_idx < self.cells.len() && dst_idx < self.cells.len() {
                    self.cells[dst_idx] = self.cells[src_idx];
                }
            }
        }
        
        // Clear top lines
        for y in 0..n {
            for x in 0..self.cols {
                let _ = self.set_cell(y, x, blank);
            }
        }
        
        self.touched = true;
    }
}

/// Create a new window
pub fn newwin(lines: i32, cols: i32, beg_y: i32, beg_x: i32) -> *mut WindowData {
    Box::into_raw(WindowData::new(lines, cols, beg_y, beg_x))
}

/// Delete a window
pub fn delwin(win: *mut WindowData) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }
    
    unsafe {
        let _ = Box::from_raw(win);
    }
    
    Ok(())
}

/// Move a window
pub fn mvwin(win: *mut WindowData, y: i32, x: i32) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }
    
    unsafe {
        (*win).beg_y = y;
        (*win).beg_x = x;
        (*win).touched = true;
    }
    
    Ok(())
}

/// Create a subwindow
pub fn subwin(parent: *mut WindowData, lines: i32, cols: i32, beg_y: i32, beg_x: i32) -> *mut WindowData {
    if parent.is_null() {
        return core::ptr::null_mut();
    }
    
    let mut sub = WindowData::new(lines, cols, beg_y, beg_x);
    sub.parent = Some(parent);
    Box::into_raw(sub)
}

/// Create a derived window (shares content with parent)
pub fn derwin(parent: *mut WindowData, lines: i32, cols: i32, beg_y: i32, beg_x: i32) -> *mut WindowData {
    // For now, same as subwin. Full implementation would share cell buffer
    subwin(parent, lines, cols, beg_y, beg_x)
}

/// Duplicate a window
pub fn dupwin(win: *mut WindowData) -> *mut WindowData {
    if win.is_null() {
        return core::ptr::null_mut();
    }
    
    unsafe {
        let orig = &*win;
        let mut dup = WindowData::new(orig.lines, orig.cols, orig.beg_y, orig.beg_x);
        dup.cells = orig.cells.clone();
        dup.attrs = orig.attrs;
        dup.bkgd = orig.bkgd;
        dup.scroll = orig.scroll;
        dup.keypad = orig.keypad;
        Box::into_raw(dup)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_creation() {
        let win = newwin(24, 80, 0, 0);
        assert!(!win.is_null());
        
        unsafe {
            assert_eq!((*win).lines, 24);
            assert_eq!((*win).cols, 80);
            assert_eq!((*win).cur_y, 0);
            assert_eq!((*win).cur_x, 0);
        }
        
        let _ = delwin(win);
    }

    #[test]
    fn test_window_operations() {
        let win = newwin(10, 10, 0, 0);
        
        unsafe {
            // Test cursor movement
            assert!((*win).move_cursor(5, 5).is_ok());
            assert_eq!((*win).cur_y, 5);
            assert_eq!((*win).cur_x, 5);
            
            // Test out of bounds
            assert!((*win).move_cursor(20, 20).is_err());
            
            // Test cell operations
            let ch = chtype::new('X', attrs::A_BOLD);
            assert!((*win).set_cell(2, 3, ch).is_ok());
            assert_eq!((*win).get_cell(2, 3).unwrap().character(), 'X');
        }
        
        let _ = delwin(win);
    }

    #[test]
    fn test_window_clear() {
        let win = newwin(5, 5, 0, 0);
        
        unsafe {
            // Fill with X
            let ch = chtype::new('X', attrs::A_NORMAL);
            for y in 0..5 {
                for x in 0..5 {
                    let _ = (*win).set_cell(y, x, ch);
                }
            }
            
            // Clear
            (*win).clear();
            
            // Verify all cells are space
            for y in 0..5 {
                for x in 0..5 {
                    assert_eq!((*win).get_cell(y, x).unwrap().character(), ' ');
                }
            }
        }
        
        let _ = delwin(win);
    }
}
