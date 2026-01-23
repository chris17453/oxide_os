#![cfg(test)]

use super::*;

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

#[test]
fn linear_framebuffer_display_info_matches() {
    let fb = LinearFramebuffer::new(make_info());
    let info = fb.get_info();
    assert_eq!(info.width, 4);
    assert_eq!(info.height, 4);
    assert_eq!(info.stride, 16);
    assert_eq!(info.format, PixelFormat::RGBA8888);
}

#[test]
fn linear_framebuffer_modes_single_entry() {
    let fb = LinearFramebuffer::new(make_info());
    let modes = fb.get_modes();
    assert_eq!(modes.len(), 1);
    let mode = modes[0];
    assert_eq!(mode.width, 4);
    assert_eq!(mode.height, 4);
    assert_eq!(mode.stride, 16);
    assert_eq!(mode.bpp, 32);
    assert_eq!(mode.format, PixelFormat::RGBA8888);
}

#[test]
fn linear_framebuffer_set_mode_unsupported() {
    let fb = LinearFramebuffer::new(make_info());
    let mode = Mode {
        width: 8,
        height: 8,
        stride: 32,
        bpp: 32,
        format: PixelFormat::RGBA8888,
    };
    assert_eq!(fb.set_mode(mode), Err(DisplayError::Unsupported));
}

#[test]
fn linear_framebuffer_flush_noop_ok() {
    let fb = LinearFramebuffer::new(make_info());
    assert!(fb.flush(None).is_ok());
    assert!(fb.flush(Some(Rect {
        x: 0,
        y: 0,
        width: 1,
        height: 1
    }))
    .is_ok());
}
