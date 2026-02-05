//! # Curses VGA Demo for Oxide OS
//!
//! A cyberpunk-themed terminal demo showcasing ncurses capabilities:
//! - Color pairs and attributes
//! - Box drawing characters (VGA-style borders)
//! - Animated moving objects
//! - Text effects (blink, bold, reverse)
//!
//! Press 'q' or Ctrl+C to quit.
//!
//! -- NeonVale: VGA nostalgia meets modern terminal tech. Runs until you kill it.

#![no_std]
#![no_main]

extern crate libc;
extern crate oxide_ncurses as ncurses;

use ncurses::{
    attrs::*, color_pair, colors::*, endwin, erase, getch, has_colors, init_pair, initscr,
    mvprintw, refresh, start_color,
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

/// Get monotonic time in nanoseconds
/// -- GraveShift: The clock that never lies and never goes backwards
fn now_ns() -> u64 {
    let mut ts = libc::time::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    libc::time::clock_gettime(libc::time::clocks::CLOCK_MONOTONIC, &mut ts);
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

/// FPS counter — exponential moving average so it doesn't flicker
/// like a dying neon sign. — NeonVale
struct FpsCounter {
    last_time: u64,
    avg_fps: u32,
    frame_count: u32,
    accum_ns: u64,
}

impl FpsCounter {
    fn new() -> Self {
        FpsCounter {
            last_time: now_ns(),
            avg_fps: 0,
            frame_count: 0,
            accum_ns: 0,
        }
    }

    /// Call once per frame. Returns smoothed FPS.
    fn tick(&mut self) -> u32 {
        let now = now_ns();
        let delta = now.saturating_sub(self.last_time);
        self.last_time = now;

        self.accum_ns += delta;
        self.frame_count += 1;

        // Update display FPS every ~500ms worth of frames
        if self.accum_ns >= 500_000_000 {
            self.avg_fps = (self.frame_count as u64 * 1_000_000_000 / self.accum_ns) as u32;
            self.frame_count = 0;
            self.accum_ns = 0;
        }

        self.avg_fps
    }
}

/// Format a u32 into a fixed-size decimal string buffer
/// -- GraveShift: No alloc, no format!, just raw digit extraction
fn fmt_u32(buf: &mut [u8; 12], val: u32) -> &[u8] {
    if val == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }
    let mut n = val;
    let mut i = 12;
    while n > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    &buf[i..]
}

/// Set attribute on stdscr — NeonVale: less boilerplate, more neon
fn set_attr(attr: u32) {
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = attr;
        }
    }
}

/// Draw a fancy box with VGA-style borders
/// -- NeonVale: Box renderer - classic terminal aesthetics
fn draw_box(y: i32, x: i32, height: i32, width: i32, color_idx: i16) {
    set_attr(color_pair(color_idx as i32));

    let _ = mvprintw(y, x, "┌");
    for i in 1..width - 1 {
        let _ = mvprintw(y, x + i, "─");
    }
    let _ = mvprintw(y, x + width - 1, "┐");

    for i in 1..height - 1 {
        let _ = mvprintw(y + i, x, "│");
        let _ = mvprintw(y + i, x + width - 1, "│");
    }

    let _ = mvprintw(y + height - 1, x, "└");
    for i in 1..width - 1 {
        let _ = mvprintw(y + height - 1, x + i, "─");
    }
    let _ = mvprintw(y + height - 1, x + width - 1, "┘");

    set_attr(A_NORMAL);
}

/// Draw a filled block at position
/// -- NeonVale: Block primitive - the building block of VGA graphics
fn draw_block(y: i32, x: i32, color_idx: i16, ch: &str) {
    set_attr(color_pair(color_idx as i32));
    let _ = mvprintw(y, x, ch);
    set_attr(A_NORMAL);
}

/// Animated bouncing ball — NeonVale: Physics simulation in a box
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
/// -- NeonVale: Demo orchestrator - runs forever until you pull the plug
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let stdscr = initscr();
    if stdscr.is_null() {
        return 1;
    }

    if !has_colors() {
        endwin();
        let msg = b"Terminal does not support colors!\n";
        libc::unistd::write(1, msg);
        return 1;
    }

    start_color();

    // -- ColdCipher: Color palette setup - cyberpunk theme
    init_pair(1, COLOR_RED, COLOR_BLACK);
    init_pair(2, COLOR_GREEN, COLOR_BLACK);
    init_pair(3, COLOR_YELLOW, COLOR_BLACK);
    init_pair(4, COLOR_BLUE, COLOR_BLACK);
    init_pair(5, COLOR_MAGENTA, COLOR_BLACK);
    init_pair(6, COLOR_CYAN, COLOR_BLACK);
    init_pair(7, COLOR_WHITE, COLOR_BLACK);

    // -- NeonVale: Nodelay mode — getch() returns immediately so we can
    // check for 'q' without blocking the animation loop. The demo runs
    // at full speed and eats keystrokes between frames.
    unsafe {
        (*stdscr).nodelay = true;
    }

    let mut max_y = 24;
    let mut max_x = 80;
    unsafe {
        max_y = (*stdscr).lines;
        max_x = (*stdscr).cols;
    }

    let mut balls = [
        Ball::new(10, 45, 1, 1, 1),
        Ball::new(12, 50, -1, 1, 2),
        Ball::new(14, 55, 1, -1, 3),
        Ball::new(16, 60, -1, -1, 4),
    ];

    // FPS counter — because if you can't measure it, it doesn't exist — NeonVale
    let mut fps = FpsCounter::new();

    // -- NeonVale: Infinite loop — runs until 'q' pressed or Ctrl+C kills us.
    // No more 200-frame time limit. This neon never sleeps.
    loop {
        // -- NeonVale: Check for quit key — nodelay getch returns -1 if nothing pressed
        let ch = getch();
        if ch == b'q' as i32 || ch == b'Q' as i32 || ch == 27 {
            break;
        }

        // -- NeonVale: erase() clears the virtual screen buffer in one shot.
        // The old cell-by-cell clear was doing 1920 individual mvprintw calls
        // per frame — roughly 70% of the frame budget wasted on spaces. — GraveShift
        let _ = erase();

        // Draw title box
        draw_box(0, 0, 5, max_x, 6);

        set_attr(color_pair(6) | A_BOLD);
        let _ = mvprintw(2, 10, "OXIDE OS - TERMINAL CURSES VGA DEMO");
        set_attr(A_NORMAL);

        // Draw info box
        draw_box(6, 2, 10, 35, 2);
        set_attr(color_pair(2));
        let _ = mvprintw(7, 8, "COLOR PALETTE");
        set_attr(A_NORMAL);

        // Show color samples
        for i in 1..=7 {
            set_attr(color_pair(i) | A_BOLD);
            let _ = mvprintw(8 + i as i32, 4, "█████");
            set_attr(A_NORMAL);
        }

        // Draw animation box
        draw_box(6, 39, 14, 39, 4);
        set_attr(color_pair(4));
        let _ = mvprintw(7, 44, "BOUNCING OBJECTS");
        set_attr(A_NORMAL);

        // Update and draw balls
        for ball in &mut balls {
            ball.update(19, 76);
            ball.draw();
        }

        // Draw effects box
        draw_box(17, 2, 6, 35, 5);
        set_attr(color_pair(5));
        let _ = mvprintw(18, 8, "TEXT EFFECTS");
        set_attr(A_NORMAL);

        // Blinking text
        set_attr(color_pair(1) | A_BLINK);
        let _ = mvprintw(19, 4, "BLINKING TEXT");
        set_attr(A_NORMAL);

        // Reverse video
        set_attr(color_pair(3) | A_REVERSE);
        let _ = mvprintw(20, 4, "REVERSE VIDEO");
        set_attr(A_NORMAL);

        // Underline
        set_attr(color_pair(6) | A_UNDERLINE);
        let _ = mvprintw(21, 4, "UNDERLINED");
        set_attr(A_NORMAL);

        // Status bar with FPS — NeonVale
        let current_fps = fps.tick();
        if max_y > 3 {
            draw_box(max_y - 3, 0, 3, max_x, 7);
            set_attr(color_pair(7));
            let _ = mvprintw(max_y - 2, 2, "Press 'q' or Ctrl+C to quit");

            // FPS readout — right-aligned HUD style — NeonVale
            let mut fps_num_buf = [0u8; 12];
            let fps_digits = fmt_u32(&mut fps_num_buf, current_fps);
            let mut fps_str = [0u8; 20];
            fps_str[0] = b'F';
            fps_str[1] = b'P';
            fps_str[2] = b'S';
            fps_str[3] = b':';
            fps_str[4] = b' ';
            let mut pos = 5;
            for &b in fps_digits {
                if pos < 20 {
                    fps_str[pos] = b;
                    pos += 1;
                }
            }

            if let Ok(s) = core::str::from_utf8(&fps_str[..pos]) {
                let fps_x = max_x - pos as i32 - 2;
                if fps_x > 0 {
                    set_attr(color_pair(2) | A_BOLD);
                    let _ = mvprintw(max_y - 2, fps_x, s);
                }
            }
            set_attr(A_NORMAL);
        }

        let _ = refresh();

        // -- NeonVale: No sleep - full throttle to see natural max FPS
    }

    let _ = endwin();
    0
}
