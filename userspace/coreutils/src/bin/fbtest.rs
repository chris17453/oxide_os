//! fbtest - framebuffer test utility
//!
//! Enumerates video modes and draws test patterns to /dev/fb0

#![no_std]
#![no_main]

use libc::*;

/// Linux-compatible variable screen info
#[repr(C)]
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

/// Linux-compatible fixed screen info
#[repr(C)]
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

/// Video mode info structure (matches kernel's VideoModeDeviceInfo)
#[repr(C)]
struct VideoModeInfo {
    mode_number: u32,
    width: u32,
    height: u32,
    bpp: u32,
    stride: u32,
    framebuffer_size: u64,
    is_bgr: bool,
    _pad: [u8; 7],
}

/// Request structure for EFFLUX_FB_GET_MODE
#[repr(C)]
struct GetModeRequest {
    index: u32,
    info: VideoModeInfo,
}

// Linux IOCTL commands
const FBIOGET_VSCREENINFO: u64 = 0x4600;
const FBIOGET_FSCREENINFO: u64 = 0x4602;

// EFFLUX-specific IOCTL commands
const EFFLUX_FB_GET_MODE_COUNT: u64 = 0x4700;
const EFFLUX_FB_GET_MODE: u64 = 0x4701;

fn print_str(s: &str) {
    write(STDOUT_FILENO, s.as_bytes());
}

fn print_num(mut n: u32) {
    if n == 0 {
        print_str("0");
        return;
    }
    let mut buf = [0u8; 12];
    let mut i = 11;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i -= 1;
    }
    write(STDOUT_FILENO, &buf[i + 1..]);
}

fn print_hex(n: u32) {
    let hex_chars = b"0123456789ABCDEF";
    let mut buf = [0u8; 8];
    for i in 0..8 {
        let nibble = ((n >> (28 - i * 4)) & 0xF) as usize;
        buf[i] = hex_chars[nibble];
    }
    write(STDOUT_FILENO, &buf);
}

fn print_num64(mut n: u64) {
    if n == 0 {
        print_str("0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 19;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i -= 1;
    }
    write(STDOUT_FILENO, &buf[i + 1..]);
}

/// Make a pixel value for BGRA8888 format
fn make_pixel_bgra(r: u8, g: u8, b: u8) -> u32 {
    // BGRA: blue at byte 0, green at byte 1, red at byte 2, alpha at byte 3
    (b as u32) | ((g as u32) << 8) | ((r as u32) << 16) | (0xFF << 24)
}

/// Make a pixel value for RGBA8888 format
fn make_pixel_rgba(r: u8, g: u8, b: u8) -> u32 {
    // RGBA: red at byte 0, green at byte 1, blue at byte 2, alpha at byte 3
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | (0xFF << 24)
}

#[unsafe(no_mangle)]
fn main() -> i32 {
    print_str("fbtest: Framebuffer test utility\n");
    print_str("========================================\n\n");

    // Open framebuffer device
    let fd = open("/dev/fb0", O_RDWR, 0);
    if fd < 0 {
        print_str("fbtest: Failed to open /dev/fb0\n");
        return 1;
    }

    print_str("fbtest: Opened /dev/fb0 (fd=");
    print_num(fd as u32);
    print_str(")\n\n");

    // Enumerate available video modes
    print_str("=== Available Video Modes ===\n");
    let mode_count = syscall::sys_ioctl(fd, EFFLUX_FB_GET_MODE_COUNT, 0);
    if mode_count > 0 {
        print_str("Mode count: ");
        print_num(mode_count as u32);
        print_str("\n\n");

        for i in 0..mode_count as u32 {
            let mut request: GetModeRequest = unsafe { core::mem::zeroed() };
            request.index = i;

            let ret = syscall::sys_ioctl(fd, EFFLUX_FB_GET_MODE, &mut request as *mut _ as u64);
            if ret == 0 {
                print_str("Mode ");
                print_num(i);
                print_str(": ");
                print_num(request.info.width);
                print_str("x");
                print_num(request.info.height);
                print_str(" @ ");
                print_num(request.info.bpp);
                print_str("bpp");
                if request.info.is_bgr {
                    print_str(" (BGR)");
                } else {
                    print_str(" (RGB)");
                }
                print_str(" stride=");
                print_num(request.info.stride);
                print_str(" size=");
                print_num64(request.info.framebuffer_size);
                print_str("\n");
            }
        }
    } else {
        print_str("Could not enumerate modes (count=");
        print_num(mode_count as u32);
        print_str(")\n");
    }

    print_str("\n=== Current Mode Info ===\n");

    // Get variable screen info
    let mut var_info: FbVarScreenInfo = unsafe { core::mem::zeroed() };
    let ret = syscall::sys_ioctl(fd, FBIOGET_VSCREENINFO, &mut var_info as *mut _ as u64);
    if ret < 0 {
        print_str("fbtest: Failed to get var screen info\n");
        close(fd);
        return 1;
    }

    print_str("Resolution: ");
    print_num(var_info.xres);
    print_str("x");
    print_num(var_info.yres);
    print_str(" @ ");
    print_num(var_info.bits_per_pixel);
    print_str("bpp\n");

    print_str("Red:   offset=");
    print_num(var_info.red_offset);
    print_str(" len=");
    print_num(var_info.red_length);
    print_str("\n");

    print_str("Green: offset=");
    print_num(var_info.green_offset);
    print_str(" len=");
    print_num(var_info.green_length);
    print_str("\n");

    print_str("Blue:  offset=");
    print_num(var_info.blue_offset);
    print_str(" len=");
    print_num(var_info.blue_length);
    print_str("\n");

    // Get fixed screen info
    let mut fix_info: FbFixScreenInfo = unsafe { core::mem::zeroed() };
    let ret = syscall::sys_ioctl(fd, FBIOGET_FSCREENINFO, &mut fix_info as *mut _ as u64);
    if ret < 0 {
        print_str("fbtest: Failed to get fix screen info\n");
        close(fd);
        return 1;
    }

    print_str("Line length: ");
    print_num(fix_info.line_length);
    print_str(" bytes\n");

    print_str("FB size: ");
    print_num(fix_info.smem_len);
    print_str(" bytes\n");

    // Determine if BGR or RGB
    let is_bgr = var_info.blue_offset == 0;
    let make_pixel: fn(u8, u8, u8) -> u32 = if is_bgr {
        print_str("Format: BGRA8888\n");
        make_pixel_bgra
    } else {
        print_str("Format: RGBA8888\n");
        make_pixel_rgba
    };

    print_str("\n=== Drawing Test Pattern ===\n");

    let width = var_info.xres as usize;
    let height = var_info.yres as usize;
    let stride = fix_info.line_length as usize;
    let bpp = (var_info.bits_per_pixel / 8) as usize;

    print_str("fbtest: width=");
    print_num(width as u32);
    print_str(" height=");
    print_num(height as u32);
    print_str(" stride=");
    print_num(stride as u32);
    print_str(" bpp=");
    print_num(bpp as u32);
    print_str("\n");

    // Create a line buffer
    let mut line_buf = [0u8; 5120]; // 1280 pixels * 4 bytes max

    // Draw colored stripes (top portion of screen)
    let stripe_height = height / 8;
    let colors: [(u8, u8, u8); 8] = [
        (255, 0, 0),     // Red
        (0, 255, 0),     // Green
        (0, 0, 255),     // Blue
        (255, 255, 0),   // Yellow
        (255, 0, 255),   // Magenta
        (0, 255, 255),   // Cyan
        (255, 255, 255), // White
        (128, 128, 128), // Gray
    ];

    // Test pixel creation
    let test_pixel = make_pixel(255, 0, 0); // Red
    print_str("fbtest: Test red pixel value: 0x");
    print_hex(test_pixel);
    print_str("\n");

    for stripe in 0..8 {
        let (r, g, b) = colors[stripe];
        let pixel = make_pixel(r, g, b);

        if stripe == 0 {
            print_str("fbtest: Stripe 0 pixel (red): 0x");
            print_hex(pixel);
            print_str("\n");
        }

        // Fill line buffer with this color
        for x in 0..width {
            let offset = x * bpp;
            if offset + 4 <= line_buf.len() {
                line_buf[offset] = (pixel & 0xFF) as u8;
                line_buf[offset + 1] = ((pixel >> 8) & 0xFF) as u8;
                line_buf[offset + 2] = ((pixel >> 16) & 0xFF) as u8;
                line_buf[offset + 3] = ((pixel >> 24) & 0xFF) as u8;
            }
        }

        // Write this line for stripe_height rows
        let start_y = stripe * stripe_height;
        for y in start_y..(start_y + stripe_height) {
            let file_offset = (y * stride) as i64;
            lseek(fd, file_offset, SEEK_SET);
            write(fd, &line_buf[..width * bpp]);
        }
    }

    // Draw a white border/frame
    let border = 10;
    let frame_color = make_pixel(255, 255, 255);

    // Fill line with white
    for x in 0..width {
        let offset = x * bpp;
        if offset + 4 <= line_buf.len() {
            line_buf[offset] = (frame_color & 0xFF) as u8;
            line_buf[offset + 1] = ((frame_color >> 8) & 0xFF) as u8;
            line_buf[offset + 2] = ((frame_color >> 16) & 0xFF) as u8;
            line_buf[offset + 3] = ((frame_color >> 24) & 0xFF) as u8;
        }
    }

    // Top border
    for y in 0..border {
        let file_offset = (y * stride) as i64;
        lseek(fd, file_offset, SEEK_SET);
        write(fd, &line_buf[..width * bpp]);
    }

    // Bottom border
    for y in (height - border)..height {
        let file_offset = (y * stride) as i64;
        lseek(fd, file_offset, SEEK_SET);
        write(fd, &line_buf[..width * bpp]);
    }

    // Left and right borders (draw a small segment for each row)
    let mut border_buf = [0u8; 40]; // 10 pixels * 4 bytes
    for i in 0..border {
        let offset = i * bpp;
        border_buf[offset] = (frame_color & 0xFF) as u8;
        border_buf[offset + 1] = ((frame_color >> 8) & 0xFF) as u8;
        border_buf[offset + 2] = ((frame_color >> 16) & 0xFF) as u8;
        border_buf[offset + 3] = ((frame_color >> 24) & 0xFF) as u8;
    }

    for y in border..(height - border) {
        // Left border
        let file_offset = (y * stride) as i64;
        lseek(fd, file_offset, SEEK_SET);
        write(fd, &border_buf[..border * bpp]);

        // Right border
        let right_offset = (y * stride + (width - border) * bpp) as i64;
        lseek(fd, right_offset, SEEK_SET);
        write(fd, &border_buf[..border * bpp]);
    }

    print_str("fbtest: Test pattern drawn!\n");
    print_str("fbtest: Done.\n");

    close(fd);
    0
}
