//! oxide-wayland — OXIDE OS Wayland Compositor
//!
//! — NeonVale: The glass canvas where windows live. This is a custom Wayland
//! compositor that renders directly to the UEFI GOP framebuffer via /dev/fb0.
//! No GPU acceleration needed — pure software compositing.
//!
//! Architecture:
//! 1. mmap /dev/fb0 for direct framebuffer access
//! 2. Listen on $XDG_RUNTIME_DIR/wayland-0 (AF_UNIX socket)
//! 3. Accept client connections, speak Wayland wire protocol
//! 4. Implement core protocols: wl_compositor, wl_shm, wl_seat, xdg_shell
//! 5. Composite client surfaces onto framebuffer in z-order
//! 6. Read input from /dev/input/* and distribute to focused client
//!
//! The Wayland wire protocol is simpler than D-Bus:
//! - 8-byte header: object_id(u32) + opcode_and_size(u32)
//! - Arguments follow: int(i32), uint(u32), fixed(i32), string(len+data+nul),
//!   object(u32), new_id(u32), array(len+data), fd(via SCM_RIGHTS)

#![no_std]
#![no_main]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Wayland protocol object ID
type ObjectId = u32;

/// A client connection
struct WaylandClient {
    fd: i32,
    /// Object registry: id -> interface name
    objects: BTreeMap<ObjectId, String>,
    /// Next server-allocated object ID (even numbers for server)
    next_id: ObjectId,
    /// Receive buffer
    recv_buf: Vec<u8>,
}

/// A surface (wl_surface) — client-owned pixel buffer
struct Surface {
    client_fd: i32,
    object_id: ObjectId,
    /// Pending buffer (committed on wl_surface.commit)
    pending_buffer: Option<Buffer>,
    /// Committed buffer (currently displayed)
    current_buffer: Option<Buffer>,
    /// Position on screen
    x: i32,
    y: i32,
    /// Size
    width: u32,
    height: u32,
}

/// A shared memory buffer (wl_buffer backed by wl_shm)
struct Buffer {
    /// File descriptor (from SCM_RIGHTS)
    shm_fd: i32,
    /// Offset into the shm pool
    offset: u32,
    /// Buffer dimensions
    width: u32,
    height: u32,
    /// Stride (bytes per row)
    stride: u32,
    /// Pixel format (WL_SHM_FORMAT_ARGB8888 = 0, XRGB8888 = 1)
    format: u32,
    /// mmap'd pointer to pixel data
    data_ptr: *mut u8,
    data_len: usize,
}

/// The compositor state
struct Compositor {
    /// Listen socket fd
    listen_fd: i32,
    /// Framebuffer fd
    fb_fd: i32,
    /// Framebuffer mmap'd pointer
    fb_ptr: *mut u8,
    fb_width: u32,
    fb_height: u32,
    fb_stride: u32,
    /// Connected clients
    clients: BTreeMap<i32, WaylandClient>,
    /// All surfaces in z-order (back to front)
    surfaces: Vec<Surface>,
    /// Focused surface index
    focused: Option<usize>,
}

impl Compositor {
    fn new() -> Self {
        Compositor {
            listen_fd: -1,
            fb_fd: -1,
            fb_ptr: core::ptr::null_mut(),
            fb_width: 0,
            fb_height: 0,
            fb_stride: 0,
            clients: BTreeMap::new(),
            surfaces: Vec::new(),
            focused: None,
        }
    }

    /// Composite all surfaces onto the framebuffer.
    /// — NeonVale: The render loop. Back-to-front painter's algorithm.
    /// Each surface's pixel buffer is blitted to the framebuffer at its position.
    fn composite(&self) {
        if self.fb_ptr.is_null() {
            return;
        }

        // Clear to dark background (0x1a1a2e — cyberpunk navy)
        let pixel_count = (self.fb_stride * self.fb_height) as usize;
        let fb = unsafe { core::slice::from_raw_parts_mut(self.fb_ptr as *mut u32, pixel_count / 4) };
        for pixel in fb.iter_mut() {
            *pixel = 0xFF1a1a2e; // ARGB dark navy
        }

        // Blit surfaces back-to-front
        for surface in &self.surfaces {
            if let Some(ref buffer) = surface.current_buffer {
                self.blit_surface(surface, buffer, fb);
            }
        }
    }

    fn blit_surface(&self, surface: &Surface, buffer: &Buffer, fb: &mut [u32]) {
        if buffer.data_ptr.is_null() {
            return;
        }

        let src = unsafe {
            core::slice::from_raw_parts(buffer.data_ptr as *const u32, buffer.data_len / 4)
        };

        for row in 0..buffer.height {
            let dst_y = surface.y + row as i32;
            if dst_y < 0 || dst_y >= self.fb_height as i32 {
                continue;
            }
            for col in 0..buffer.width {
                let dst_x = surface.x + col as i32;
                if dst_x < 0 || dst_x >= self.fb_width as i32 {
                    continue;
                }

                let src_idx = (row * buffer.stride / 4 + col) as usize;
                let dst_idx = (dst_y as u32 * self.fb_stride / 4 + dst_x as u32) as usize;

                if src_idx < src.len() && dst_idx < fb.len() {
                    let pixel = src[src_idx];
                    let alpha = (pixel >> 24) & 0xFF;
                    if alpha == 0xFF {
                        fb[dst_idx] = pixel;
                    } else if alpha > 0 {
                        // Alpha blend
                        let inv_alpha = 255 - alpha;
                        let dst = fb[dst_idx];
                        let r = ((pixel >> 16 & 0xFF) * alpha + (dst >> 16 & 0xFF) * inv_alpha) / 255;
                        let g = ((pixel >> 8 & 0xFF) * alpha + (dst >> 8 & 0xFF) * inv_alpha) / 255;
                        let b = ((pixel & 0xFF) * alpha + (dst & 0xFF) * inv_alpha) / 255;
                        fb[dst_idx] = 0xFF000000 | (r << 16) | (g << 8) | b;
                    }
                }
            }
        }
    }
}

/// — NeonVale: Placeholder main. Full implementation will:
/// 1. Open + mmap /dev/fb0
/// 2. Create AF_UNIX socket at /run/wayland-0
/// 3. epoll event loop
/// 4. Parse Wayland wire protocol
/// 5. Implement wl_display, wl_registry, wl_compositor, wl_shm, wl_seat, xdg_wm_base
/// 6. Composite surfaces to framebuffer at 60fps
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let _compositor = Compositor::new();

    // — NeonVale: The neon compositor awakens. Glass and light.
    // Full implementation follows after the GTK dependency chain is built.
    0
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[global_allocator]
static ALLOC: SimpleAlloc = SimpleAlloc;

struct SimpleAlloc;

unsafe impl core::alloc::GlobalAlloc for SimpleAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        unsafe extern "C" {
            fn malloc(size: usize) -> *mut u8;
        }
        unsafe { malloc(layout.size()) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: core::alloc::Layout) {
        unsafe extern "C" {
            fn free(ptr: *mut u8);
        }
        unsafe { free(ptr) }
    }
}
