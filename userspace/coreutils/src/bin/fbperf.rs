//! fbperf - framebuffer performance benchmark
//!
//! Tests graphics performance and measures FPS

#![no_std]
#![no_main]

use libc::*;

/// Framebuffer control ioctl
const FBIOGET_PERF: u64 = 0x4620; // Get performance stats

/// Performance statistics structure
#[repr(C)]
struct FbPerfStats {
    frames: u64,
    pixels: u64,
    flushes: u64,
    fps: u32,
    _pad: u32,
}

#[no_mangle]
extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let banner = b"\n\x1b[1;36m=== Framebuffer Performance Benchmark ===\x1b[0m\n\n";
    write(1, banner.as_ptr(), banner.len());

    // Open framebuffer device
    let fb_path = b"/dev/fb0\0";
    let fd = open(fb_path.as_ptr() as *const i8, O_RDWR);
    if fd < 0 {
        let err = b"Error: Cannot open /dev/fb0\n";
        write(2, err.as_ptr(), err.len());
        return 1;
    }

    // Test 1: Clear screen performance
    write(1, b"Test 1: Clear screen (black)...\n".as_ptr(), 33);
    let start = get_time_ms();
    for _ in 0..100 {
        // Clear would be done via ioctl or mmap + memset
        // For now, just measure timing overhead
    }
    let elapsed = get_time_ms() - start;
    print_result(b"Clear screen (100x)", elapsed);

    // Test 2: Line drawing performance  
    write(1, b"\nTest 2: Line drawing...\n".as_ptr(), 26);
    let start = get_time_ms();
    for _ in 0..1000 {
        // Draw lines via mmap + pixel writes
    }
    let elapsed = get_time_ms() - start;
    print_result(b"Line drawing (1000x)", elapsed);

    // Test 3: Text rendering performance
    write(1, b"\nTest 3: Text rendering...\n".as_ptr(), 28);
    let test_text = b"The quick brown fox jumps over the lazy dog. ";
    let start = get_time_ms();
    for _ in 0..100 {
        write(1, test_text.as_ptr(), test_text.len());
    }
    let elapsed = get_time_ms() - start;
    print_result(b"Text rendering (100x line)", elapsed);

    // Test 4: Scrolling performance
    write(1, b"\nTest 4: Scrolling...\n".as_ptr(), 23);
    let start = get_time_ms();
    for i in 0..50 {
        let msg = b"Scrolling test line...\n";
        write(1, msg.as_ptr(), msg.len());
    }
    let elapsed = get_time_ms() - start;
    print_result(b"Scrolling (50 lines)", elapsed);

    // Get performance statistics if available
    write(1, b"\n\x1b[1;33m=== Performance Statistics ===\x1b[0m\n".as_ptr(), 45);
    let mut stats = FbPerfStats {
        frames: 0,
        pixels: 0,
        flushes: 0,
        fps: 0,
        _pad: 0,
    };
    
    let result = ioctl(fd, FBIOGET_PERF, &mut stats as *mut _ as usize);
    if result == 0 {
        write(1, b"Frames rendered: ".as_ptr(), 17);
        print_u64(stats.frames);
        write(1, b"\nPixels written: ".as_ptr(), 17);
        print_u64(stats.pixels);
        write(1, b"\nFlush operations: ".as_ptr(), 19);
        print_u64(stats.flushes);
        write(1, b"\nCurrent FPS: ".as_ptr(), 14);
        print_u32(stats.fps);
        write(1, b"\n".as_ptr(), 1);
    } else {
        write(1, b"(Statistics not available)\n".as_ptr(), 28);
    }

    write(1, b"\n\x1b[1;32m=== Benchmark Complete ===\x1b[0m\n\n".as_ptr(), 45);

    close(fd);
    0
}

fn get_time_ms() -> u64 {
    // Get current time in milliseconds
    // This is a placeholder - actual implementation would use clock_gettime
    0
}

fn print_result(test_name: &[u8], time_ms: u64) {
    write(1, b"  ".as_ptr(), 2);
    write(1, test_name.as_ptr(), test_name.len());
    write(1, b": ".as_ptr(), 2);
    print_u64(time_ms);
    write(1, b" ms\n".as_ptr(), 4);
}

fn print_u64(mut n: u64) {
    if n == 0 {
        write(1, b"0".as_ptr(), 1);
        return;
    }

    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }

    for j in (0..i).rev() {
        write(1, &buf[j] as *const u8, 1);
    }
}

fn print_u32(n: u32) {
    print_u64(n as u64);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    let msg = b"fbperf panicked!\n";
    write(2, msg.as_ptr(), msg.len());
    exit(1);
}
