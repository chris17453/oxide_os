//! # Curses VGA Demo for Oxide OS
//!
//! A cyberpunk-themed terminal demo showcasing ncurses capabilities:
//! - Color pairs and attributes
//! - Box drawing characters (VGA-style borders)
//! - Animated moving objects
//! - Text effects (blink, bold, reverse)
//!
//! Press Ctrl+C to quit.
//!
//! -- NeonVale: VGA nostalgia meets modern terminal tech

#![no_std]
#![no_main]

extern crate libc;
extern crate oxide_ncurses as ncurses;

use ncurses::{
    attrs::*, color_pair, colors::*, endwin, has_colors, init_pair, initscr, mvprintw, refresh,
    start_color,
};

/// Sleep for a short duration (animation delay)
/// -- GraveShift: Timing primitive - keeps the animations smooth
fn sleep_ms(ms: u32) {
    let ts = libc::time::Timespec {
        tv_sec: 0,
        tv_nsec: (ms as i64) * 1_000_000,
    };
    let mut rem = libc::time::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    libc::time::nanosleep(&ts, Some(&mut rem));
}

/// Draw a fancy box with VGA-style borders
/// -- NeonVale: Box renderer - classic terminal aesthetics
fn draw_box(y: i32, x: i32, height: i32, width: i32, color_idx: i16) {
    let pair = color_pair(color_idx as i32);

    // Draw corners and edges with color
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = pair;
        }
    }

    // Top left corner
    let _ = mvprintw(y, x, "┌");

    // Top border
    for i in 1..width - 1 {
        let _ = mvprintw(y, x + i, "─");
    }

    // Top right corner
    let _ = mvprintw(y, x + width - 1, "┐");

    // Sides
    for i in 1..height - 1 {
        let _ = mvprintw(y + i, x, "│");
        let _ = mvprintw(y + i, x + width - 1, "│");
    }

    // Bottom left corner
    let _ = mvprintw(y + height - 1, x, "└");

    // Bottom border
    for i in 1..width - 1 {
        let _ = mvprintw(y + height - 1, x + i, "─");
    }

    // Bottom right corner
    let _ = mvprintw(y + height - 1, x + width - 1, "┘");

    // Reset attributes
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = A_NORMAL;
        }
    }
}

/// Draw a filled block at position
/// -- NeonVale: Block primitive - the building block of VGA graphics
fn draw_block(y: i32, x: i32, color_idx: i16, ch: &str) {
    let pair = color_pair(color_idx as i32);

    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = pair;
        }
    }

    let _ = mvprintw(y, x, ch);

    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = A_NORMAL;
        }
    }
}

/// Draw animated bouncing ball
/// -- NeonVale: Physics simulation - basic collision detection
struct Ball {
    y: i32,
    x: i32,
    dy: i32,
    dx: i32,
    color: i16,
}

impl Ball {
    fn new(y: i32, x: i32, dy: i32, dx: i32, color: i16) -> Self {
        Ball {
            y,
            x,
            dy,
            dx,
            color,
        }
    }

    fn update(&mut self, max_y: i32, max_x: i32) {
        self.y += self.dy;
        self.x += self.dx;

        // Bounce off walls
        if self.y <= 8 || self.y >= max_y - 3 {
            self.dy = -self.dy;
        }
        if self.x <= 40 || self.x >= max_x - 3 {
            self.dx = -self.dx;
        }
    }

    fn draw(&self) {
        draw_block(self.y, self.x, self.color, "◆");
    }
}

/// Main entry point
/// -- NeonVale: Demo orchestrator - brings all effects together
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    // Initialize ncurses
    let stdscr = initscr();
    if stdscr.is_null() {
        return 1;
    }

    // Check for color support
    if !has_colors() {
        endwin();
        let msg = b"Terminal does not support colors!\n";
        libc::unistd::write(1, msg);
        return 1;
    }

    // Initialize color pairs
    start_color();

    // -- ColdCipher: Color palette setup - cyberpunk theme
    init_pair(1, COLOR_RED, COLOR_BLACK);
    init_pair(2, COLOR_GREEN, COLOR_BLACK);
    init_pair(3, COLOR_YELLOW, COLOR_BLACK);
    init_pair(4, COLOR_BLUE, COLOR_BLACK);
    init_pair(5, COLOR_MAGENTA, COLOR_BLACK);
    init_pair(6, COLOR_CYAN, COLOR_BLACK);
    init_pair(7, COLOR_WHITE, COLOR_BLACK);

    // Get screen dimensions
    let mut max_y = 24;
    let mut max_x = 80;
    unsafe {
        if !stdscr.is_null() {
            max_y = (*stdscr).lines;
            max_x = (*stdscr).cols;
        }
    }

    // Initialize bouncing balls
    let mut balls = [
        Ball::new(10, 45, 1, 1, 1),
        Ball::new(12, 50, -1, 1, 2),
        Ball::new(14, 55, 1, -1, 3),
        Ball::new(16, 60, -1, -1, 4),
    ];

    // Animation loop
    for _frame in 0..200 {
        // Clear screen by printing spaces
        for y in 0..max_y {
            for x in 0..max_x {
                let _ = mvprintw(y, x, " ");
            }
        }

        // Draw title box
        draw_box(0, 0, 5, max_x, 6);

        // -- NeonVale: Title banner - retro VGA aesthetics
        let pair = color_pair(6) | A_BOLD;
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = pair;
            }
        }
        let _ = mvprintw(2, 10, "OXIDE OS - TERMINAL CURSES VGA DEMO");
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = A_NORMAL;
            }
        }

        // Draw info box
        draw_box(6, 2, 10, 35, 2);

        let pair2 = color_pair(2);
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = pair2;
            }
        }
        let _ = mvprintw(7, 8, "COLOR PALETTE");
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = A_NORMAL;
            }
        }

        // Show color samples
        for i in 1..=7 {
            let pair_i = color_pair(i) | A_BOLD;
            unsafe {
                let stdscr = ncurses::screen::stdscr();
                if !stdscr.is_null() {
                    (*stdscr).attrs = pair_i;
                }
            }
            let _ = mvprintw(8 + i as i32, 4, "█████");
            unsafe {
                let stdscr = ncurses::screen::stdscr();
                if !stdscr.is_null() {
                    (*stdscr).attrs = A_NORMAL;
                }
            }
        }

        // Draw animation box
        draw_box(6, 39, 14, 39, 4);

        let pair4 = color_pair(4);
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = pair4;
            }
        }
        let _ = mvprintw(7, 44, "BOUNCING OBJECTS");
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = A_NORMAL;
            }
        }

        // Update and draw balls within the animation box
        for ball in &mut balls {
            ball.update(19, 76);
            ball.draw();
        }

        // Draw effects box
        draw_box(17, 2, 6, 35, 5);

        let pair5 = color_pair(5);
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = pair5;
            }
        }
        let _ = mvprintw(18, 8, "TEXT EFFECTS");
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = A_NORMAL;
            }
        }

        // Show blinking text
        let blink_pair = color_pair(1) | A_BLINK;
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = blink_pair;
            }
        }
        let _ = mvprintw(19, 4, "BLINKING TEXT");
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = A_NORMAL;
            }
        }

        // Show reverse text
        let rev_pair = color_pair(3) | A_REVERSE;
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = rev_pair;
            }
        }
        let _ = mvprintw(20, 4, "REVERSE VIDEO");
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = A_NORMAL;
            }
        }

        // Show underline
        let under_pair = color_pair(6) | A_UNDERLINE;
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = under_pair;
            }
        }
        let _ = mvprintw(21, 4, "UNDERLINED");
        unsafe {
            let stdscr = ncurses::screen::stdscr();
            if !stdscr.is_null() {
                (*stdscr).attrs = A_NORMAL;
            }
        }

        // Draw status bar at bottom
        if max_y > 3 {
            draw_box(max_y - 3, 0, 3, max_x, 7);
            let pair7 = color_pair(7);
            unsafe {
                let stdscr = ncurses::screen::stdscr();
                if !stdscr.is_null() {
                    (*stdscr).attrs = pair7;
                }
            }
            let _ = mvprintw(max_y - 2, 2, "VGA Style Terminal Graphics Demo");
            unsafe {
                let stdscr = ncurses::screen::stdscr();
                if !stdscr.is_null() {
                    (*stdscr).attrs = A_NORMAL;
                }
            }
        }

        // Refresh screen
        let _ = refresh();

        // Animation delay
        sleep_ms(50);
    }

    // Cleanup
    let _ = endwin();

    0
}
