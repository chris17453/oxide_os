#![no_std]
#![no_main]

extern crate alloc;

use fb::{Display, DisplayError, FramebufferInfo, LinearFramebuffer, Mode, PixelFormat, Rect};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    run();
    loop {}
}

fn run() {
    linear_framebuffer_display_info_matches();
    linear_framebuffer_modes_single_entry();
    linear_framebuffer_set_mode_unsupported();
    linear_framebuffer_flush_noop_ok();
}

fn make_info() -> FramebufferInfo {
    FramebufferInfo {
        base: 0x1000,
        size: 64,
        width: 4,
        height: 4,
        stride: 4 * 4, // width * bpp (4 bytes)
        format: PixelFormat::RGBA8888,
    }
}

fn linear_framebuffer_display_info_matches() {
    let fb = LinearFramebuffer::new(make_info());
    let info = fb.get_info();
    assert!(info.width == 4);
    assert!(info.height == 4);
    assert!(info.stride == 16);
    assert!(info.format == PixelFormat::RGBA8888);
}

fn linear_framebuffer_modes_single_entry() {
    let fb = LinearFramebuffer::new(make_info());
    let modes = fb.get_modes();
    assert!(modes.len() == 1);
    let mode = modes[0];
    assert!(mode.width == 4);
    assert!(mode.height == 4);
    assert!(mode.stride == 16);
    assert!(mode.bpp == 32);
    assert!(mode.format == PixelFormat::RGBA8888);
}

fn linear_framebuffer_set_mode_unsupported() {
    let fb = LinearFramebuffer::new(make_info());
    let mode = Mode {
        width: 8,
        height: 8,
        stride: 32,
        bpp: 32,
        format: PixelFormat::RGBA8888,
    };
    assert!(fb.set_mode(mode) == Err(DisplayError::Unsupported));
}

fn linear_framebuffer_flush_noop_ok() {
    let fb = LinearFramebuffer::new(make_info());
    assert!(Display::flush(&fb, None).is_ok());
    assert!(Display::flush(
        &fb,
        Some(Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1
        })
    )
    .is_ok());
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
