//! # DOOM for OXIDE OS
//!
//! Classic DOOM game engine ported to OXIDE OS with:
//! - Direct framebuffer rendering via /dev/fb0
//! - Keyboard input for controls
//! - Sound output via soundd daemon
//! - WAD file loading from filesystem
//!
//! This is a minimal implementation of the Doom engine,
//! adapted from the original id Software source code.
//!
//! Controls:
//!   Arrow keys: Move/Turn
//!   Space: Use/Open doors
//!   Ctrl: Fire
//!   Shift: Run
//!   ESC: Menu/Quit
//!
//! -- GlassSignal: Graphics pipeline + GPU acceleration
//! -- EchoFrame: Audio + media subsystems
//! -- InputShade: Input systems + device interaction

#![no_std]
#![no_main]

extern crate libc;

use libc::{write, close, exit};
use libc::unistd::open;
use libc::syscall::{sys_ioctl, sys_mmap, sys_munmap, prot, map_flags};
use libc::{O_RDWR};

mod wad;
mod render;
mod game;
mod input;
mod sound;

use wad::WadFile;
use render::Renderer;
use game::Game;
use input::InputState;
use sound::SoundSystem;

/// Framebuffer screen info structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct FbVarScreenInfo {
    xres: u32,
    yres: u32,
    xres_virtual: u32,
    yres_virtual: u32,
    xoffset: u32,
    yoffset: u32,
    bits_per_pixel: u32,
    grayscale: u32,
    red_offset: u32,
    red_length: u32,
    green_offset: u32,
    green_length: u32,
    blue_offset: u32,
    blue_length: u32,
    transp_offset: u32,
    transp_length: u32,
    nonstd: u32,
    activate: u32,
    height: u32,
    width: u32,
    accel_flags: u32,
    pixclock: u32,
    left_margin: u32,
    right_margin: u32,
    upper_margin: u32,
    lower_margin: u32,
    hsync_len: u32,
    vsync_len: u32,
    sync: u32,
    vmode: u32,
    rotate: u32,
    colorspace: u32,
    reserved: [u32; 4],
}

/// Fixed screen info structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct FbFixScreenInfo {
    id: [u8; 16],
    smem_start: u64,
    smem_len: u32,
    fb_type: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    _padding: u16,
    line_length: u32,
    mmio_start: u64,
    mmio_len: u32,
    accel: u32,
    capabilities: u16,
    reserved: [u16; 2],
}

const FBIOGET_VSCREENINFO: u64 = 0x4600;
const FBIOGET_FSCREENINFO: u64 = 0x4602;

/// Framebuffer device
struct Framebuffer {
    fd: i32,
    width: u32,
    height: u32,
    bpp: u32,
    fb_ptr: *mut u8,
    fb_size: usize,
}

impl Framebuffer {
    /// Open and map the framebuffer device
    /// -- GlassSignal: Direct hardware framebuffer access, no middleware bloat
    fn new() -> Option<Self> {
        let fd = open("/dev/fb0", O_RDWR, 0);
        if fd < 0 {
            return None;
        }

        // Get variable screen info
        let mut vinfo: FbVarScreenInfo = unsafe { core::mem::zeroed() };
        if sys_ioctl(fd, FBIOGET_VSCREENINFO, &mut vinfo as *mut _ as u64) < 0 {
            close(fd);
            return None;
        }

        // Get fixed screen info
        let mut finfo: FbFixScreenInfo = unsafe { core::mem::zeroed() };
        if sys_ioctl(fd, FBIOGET_FSCREENINFO, &mut finfo as *mut _ as u64) < 0 {
            close(fd);
            return None;
        }

        let fb_size = finfo.smem_len as usize;
        let fb_ptr = sys_mmap(
            core::ptr::null_mut(),
            fb_size,
            prot::PROT_READ | prot::PROT_WRITE,
            map_flags::MAP_SHARED,
            fd,
            0,
        );

        if fb_ptr as isize == -1 {
            close(fd);
            return None;
        }

        Some(Framebuffer {
            fd,
            width: vinfo.xres,
            height: vinfo.yres,
            bpp: vinfo.bits_per_pixel,
            fb_ptr: fb_ptr as *mut u8,
            fb_size,
        })
    }

    /// Get framebuffer as a mutable slice
    fn as_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.fb_ptr, self.fb_size) }
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        if !self.fb_ptr.is_null() {
            sys_munmap(self.fb_ptr, self.fb_size);
        }
        if self.fd >= 0 {
            close(self.fd);
        }
    }
}

/// Print a message to stdout
/// -- NeonRoot: Simple logging when framebuffer takes over
fn print(s: &str) {
    write(1, s.as_bytes());
}

/// Main game loop
/// -- GraveShift: Core game engine timing and orchestration
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    print("DOOM for OXIDE OS\n");
    print("=================\n\n");

    // Initialize framebuffer
    print("Initializing framebuffer...\n");
    let mut fb = match Framebuffer::new() {
        Some(fb) => fb,
        None => {
            print("ERROR: Failed to open /dev/fb0\n");
            exit(1);
        }
    };

    print("Framebuffer initialized: ");
    print_u32(fb.width);
    print("x");
    print_u32(fb.height);
    print(" @ ");
    print_u32(fb.bpp);
    print(" bpp\n");

    // Initialize sound system
    print("Initializing sound system...\n");
    let mut sound = SoundSystem::new();
    if sound.is_connected() {
        print("Sound system connected to soundd\n");
    } else {
        print("WARNING: Sound system not available\n");
    }

    // Load WAD file
    print("Loading WAD file...\n");
    let wad = match WadFile::load("/usr/share/doom/doom1.wad") {
        Some(w) => {
            print("WAD file loaded successfully\n");
            w
        }
        None => {
            print("ERROR: Failed to load WAD file\n");
            print("Please ensure doom1.wad is in /usr/share/doom/\n");
            exit(1);
        }
    };

    // Initialize renderer
    print("Initializing renderer...\n");
    let mut renderer = Renderer::new(fb.width, fb.height, &wad);

    // Initialize game state
    print("Initializing game...\n");
    let mut game = Game::new(&wad);

    // Initialize input
    let mut input = InputState::new();

    print("\nStarting DOOM...\n");
    print("Controls: Arrow keys=Move, Space=Use, Ctrl=Fire, ESC=Quit\n\n");

    // Main game loop
    // -- GraveShift: Tight loop, each frame matters in the pit
    loop {
        // Read keyboard input
        input.update();

        // Check for quit
        if input.is_quit() {
            break;
        }

        // Update game state
        game.update(&input);

        // Render frame
        renderer.render(&game, fb.as_slice());

        // Update sound
        sound.update(&game);

        // Frame timing (35 FPS - classic Doom timing)
        // -- GraveShift: 28ms sleep = 35 FPS, keeping it old school
        sleep_ms(28);
    }

    print("\nExiting DOOM...\n");
    exit(0);
}

/// Sleep for milliseconds
/// -- GraveShift: Timing primitive for frame pacing
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

/// Print a u32 value
fn print_u32(val: u32) {
    let mut buf = [0u8; 16];
    let mut n = val;
    let mut i = 0;
    
    if n == 0 {
        write(1, b"0");
        return;
    }
    
    while n > 0 {
        buf[i] = (n % 10) as u8 + b'0';
        n /= 10;
        i += 1;
    }
    
    while i > 0 {
        i -= 1;
        write(1, &buf[i..i+1]);
    }
}

// Note: panic handler is provided by libc crate
