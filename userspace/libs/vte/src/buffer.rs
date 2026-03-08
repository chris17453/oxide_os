//! Terminal screen buffer
//!
//! Provides primary, alternate, and scrollback buffers.
//! No kernel deps - pure alloc.
//!
//! -- WireSaint: Screen buffer storage - the VRAM of the virtual terminal

extern crate alloc;

use crate::cell::{Cell, CellAttrs};
use alloc::collections::VecDeque;
use alloc::vec::Vec;

/// Screen buffer for terminal content
pub struct ScreenBuffer {
    /// Buffer cells (row-major order)
    cells: Vec<Cell>,
    /// Number of columns
    cols: u32,
    /// Number of rows
    rows: u32,
    /// Default cell attributes for clearing
    default_attrs: CellAttrs,
}

impl ScreenBuffer {
    /// Create a new screen buffer with given dimensions
    pub fn new(cols: u32, rows: u32) -> Self {
        let size = (cols * rows) as usize;
        let mut cells = Vec::with_capacity(size);
        cells.resize(size, Cell::default());

        ScreenBuffer {
            cells,
            cols,
            rows,
            default_attrs: CellAttrs::default(),
        }
    }

    /// Get buffer dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.cols, self.rows)
    }

    /// Get number of columns
    pub fn cols(&self) -> u32 {
        self.cols
    }

    /// Get number of rows
    pub fn rows(&self) -> u32 {
        self.rows
    }

    /// Set default attributes for clearing operations
    pub fn set_default_attrs(&mut self, attrs: CellAttrs) {
        self.default_attrs = attrs;
    }

    /// Get cell at position
    pub fn get(&self, row: u32, col: u32) -> Option<&Cell> {
        if row < self.rows && col < self.cols {
            Some(&self.cells[(row * self.cols + col) as usize])
        } else {
            None
        }
    }

    /// Get mutable cell at position
    pub fn get_mut(&mut self, row: u32, col: u32) -> Option<&mut Cell> {
        if row < self.rows && col < self.cols {
            Some(&mut self.cells[(row * self.cols + col) as usize])
        } else {
            None
        }
    }

    /// Set cell at position
    pub fn set(&mut self, row: u32, col: u32, cell: Cell) {
        if row < self.rows && col < self.cols {
            self.cells[(row * self.cols + col) as usize] = cell;
        }
    }

    /// Set character at position with given attributes
    pub fn set_char(&mut self, row: u32, col: u32, ch: char, attrs: CellAttrs) {
        if row < self.rows && col < self.cols {
            self.cells[(row * self.cols + col) as usize] = Cell::new(ch, attrs);
        }
    }

    /// Clear the entire buffer
    pub fn clear(&mut self) {
        let empty = Cell::new(' ', self.default_attrs);
        self.cells.fill(empty);
    }

    /// Clear a single row
    pub fn clear_row(&mut self, row: u32) {
        if row < self.rows {
            let empty = Cell::new(' ', self.default_attrs);
            let start = (row * self.cols) as usize;
            let end = start + self.cols as usize;
            self.cells[start..end].fill(empty);
        }
    }

    /// Clear from cursor to end of line
    pub fn clear_to_eol(&mut self, row: u32, col: u32) {
        if row < self.rows {
            let empty = Cell::new(' ', self.default_attrs);
            let start = (row * self.cols + col) as usize;
            let end = ((row + 1) * self.cols) as usize;
            let len = self.cells.len();
            self.cells[start..end.min(len)].fill(empty);
        }
    }

    /// Clear from beginning of line to cursor
    pub fn clear_to_bol(&mut self, row: u32, col: u32) {
        if row < self.rows {
            let empty = Cell::new(' ', self.default_attrs);
            let start = (row * self.cols) as usize;
            let end = (row * self.cols + col + 1) as usize;
            let len = self.cells.len();
            self.cells[start..end.min(len)].fill(empty);
        }
    }

    /// Clear from cursor to end of screen
    pub fn clear_to_eos(&mut self, row: u32, col: u32) {
        // Clear rest of current line
        self.clear_to_eol(row, col);
        // Clear remaining rows
        for r in (row + 1)..self.rows {
            self.clear_row(r);
        }
    }

    /// Clear from beginning of screen to cursor
    pub fn clear_to_bos(&mut self, row: u32, col: u32) {
        // Clear rows above
        for r in 0..row {
            self.clear_row(r);
        }
        // Clear beginning of current line
        self.clear_to_bol(row, col);
    }

    /// Scroll up by n lines within a region
    pub fn scroll_up(&mut self, top: u32, bottom: u32, n: u32) {
        let top = top.min(self.rows - 1);
        let bottom = bottom.min(self.rows - 1);
        if top >= bottom || n == 0 {
            return;
        }

        let n = n.min(bottom - top + 1);
        let cols = self.cols as usize;

        // Move lines up using copy_within (faster than cell-by-cell)
        let src_start = ((top + n) * self.cols) as usize;
        let dst_start = (top * self.cols) as usize;
        let lines_to_move = (bottom - top - n + 1) as usize;
        let cells_to_move = lines_to_move * cols;

        if cells_to_move > 0 {
            self.cells
                .copy_within(src_start..(src_start + cells_to_move), dst_start);
        }

        // Clear bottom lines (batch fill)
        let empty = Cell::new(' ', self.default_attrs);
        let clear_start = ((bottom - n + 1) * self.cols) as usize;
        let clear_end = ((bottom + 1) * self.cols) as usize;
        self.cells[clear_start..clear_end].fill(empty);
    }

    /// Scroll down by n lines within a region
    pub fn scroll_down(&mut self, top: u32, bottom: u32, n: u32) {
        let top = top.min(self.rows - 1);
        let bottom = bottom.min(self.rows - 1);
        if top >= bottom || n == 0 {
            return;
        }

        let n = n.min(bottom - top + 1);
        let cols = self.cols as usize;

        // Move lines down using copy_within (faster than cell-by-cell)
        let src_start = (top * self.cols) as usize;
        let dst_start = ((top + n) * self.cols) as usize;
        let lines_to_move = (bottom - top - n + 1) as usize;
        let cells_to_move = lines_to_move * cols;

        if cells_to_move > 0 {
            self.cells
                .copy_within(src_start..(src_start + cells_to_move), dst_start);
        }

        // Clear top lines (batch fill)
        let empty = Cell::new(' ', self.default_attrs);
        let clear_start = (top * self.cols) as usize;
        let clear_end = ((top + n) * self.cols) as usize;
        self.cells[clear_start..clear_end].fill(empty);
    }

    /// Get an entire row for scrollback
    pub fn get_row(&self, row: u32) -> Option<Vec<Cell>> {
        if row < self.rows {
            let start = (row * self.cols) as usize;
            let end = start + self.cols as usize;
            Some(self.cells[start..end].to_vec())
        } else {
            None
        }
    }

    /// Insert characters at position, shifting right
    pub fn insert_chars(&mut self, row: u32, col: u32, n: u32) {
        if row >= self.rows || col >= self.cols {
            return;
        }

        let empty = Cell::new(' ', self.default_attrs);
        let start = (row * self.cols) as usize;
        let n = n.min(self.cols - col) as usize;
        let col = col as usize;

        // Shift right using copy_within
        let src_start = start + col;
        let src_end = start + (self.cols as usize) - n;
        let dst_start = start + col + n;
        if src_end > src_start {
            self.cells.copy_within(src_start..src_end, dst_start);
        }

        // Clear inserted positions
        self.cells[start + col..start + col + n].fill(empty);
    }

    /// Delete characters at position, shifting left
    pub fn delete_chars(&mut self, row: u32, col: u32, n: u32) {
        if row >= self.rows || col >= self.cols {
            return;
        }

        let empty = Cell::new(' ', self.default_attrs);
        let start = (row * self.cols) as usize;
        let n = n.min(self.cols - col) as usize;
        let col = col as usize;
        let cols = self.cols as usize;

        // Shift left using copy_within
        let src_start = start + col + n;
        let src_end = start + cols;
        let dst_start = start + col;
        if src_end > src_start {
            self.cells.copy_within(src_start..src_end, dst_start);
        }

        // Clear end of line
        self.cells[start + cols - n..start + cols].fill(empty);
    }

    /// Erase characters at position (replace with spaces)
    pub fn erase_chars(&mut self, row: u32, col: u32, n: u32) {
        if row >= self.rows || col >= self.cols {
            return;
        }

        let empty = Cell::new(' ', self.default_attrs);
        let start = (row * self.cols + col) as usize;
        let n = n.min(self.cols - col) as usize;

        // Batch fill instead of loop
        self.cells[start..start + n].fill(empty);
    }

    /// Insert lines at row, shifting down
    pub fn insert_lines(&mut self, row: u32, n: u32, scroll_bottom: u32) {
        self.scroll_down(row, scroll_bottom, n);
    }

    /// Delete lines at row, shifting up
    pub fn delete_lines(&mut self, row: u32, n: u32, scroll_bottom: u32) {
        self.scroll_up(row, scroll_bottom, n);
    }

    /// Resize the buffer to new dimensions, preserving content where possible.
    /// — WireSaint: like xterm's resize — copy old cells into new grid, clip or pad.
    /// Content anchored at top-left. New cells get default attrs.
    pub fn resize(&mut self, new_cols: u32, new_rows: u32) {
        if new_cols == self.cols && new_rows == self.rows {
            return;
        }
        let new_size = (new_cols * new_rows) as usize;
        let mut new_cells = Vec::with_capacity(new_size);
        new_cells.resize(new_size, Cell::new(' ', self.default_attrs));

        // — WireSaint: copy min(old, new) rows × min(old, new) cols
        let copy_rows = self.rows.min(new_rows);
        let copy_cols = self.cols.min(new_cols);
        for row in 0..copy_rows {
            let old_start = (row * self.cols) as usize;
            let new_start = (row * new_cols) as usize;
            let count = copy_cols as usize;
            new_cells[new_start..new_start + count]
                .copy_from_slice(&self.cells[old_start..old_start + count]);
        }

        self.cells = new_cells;
        self.cols = new_cols;
        self.rows = new_rows;
    }
}

/// Scrollback buffer for terminal history
pub struct ScrollbackBuffer {
    /// Lines of cells (newest at back)
    lines: VecDeque<Vec<Cell>>,
    /// Maximum number of lines to keep
    max_lines: usize,
    /// — GlassSignal: widest line in scrollback (for horizontal scroll range).
    /// Updated on push, may overestimate after eviction. Recalc on demand.
    max_line_width: usize,
}

impl ScrollbackBuffer {
    /// Create a new scrollback buffer
    pub fn new(max_lines: usize) -> Self {
        ScrollbackBuffer {
            lines: VecDeque::with_capacity(max_lines),
            max_lines,
            max_line_width: 0,
        }
    }

    /// Add a line to scrollback
    /// — GlassSignal: tracks max line width for horizontal scrollbar range
    pub fn push(&mut self, line: Vec<Cell>) {
        let width = line.len();
        if self.lines.len() >= self.max_lines {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
        if width > self.max_line_width {
            self.max_line_width = width;
        }
    }

    /// — GlassSignal: widest line seen (may overestimate after eviction)
    pub fn max_line_width(&self) -> usize {
        self.max_line_width
    }

    /// Get number of lines in scrollback
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Check if scrollback is empty
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Get a line from scrollback (0 = oldest)
    pub fn get(&self, index: usize) -> Option<&Vec<Cell>> {
        self.lines.get(index)
    }

    /// Get a line from the end (0 = newest)
    pub fn get_from_end(&self, offset: usize) -> Option<&Vec<Cell>> {
        if offset < self.lines.len() {
            self.lines.get(self.lines.len() - 1 - offset)
        } else {
            None
        }
    }

    /// Clear all scrollback
    pub fn clear(&mut self) {
        self.lines.clear();
    }
}
