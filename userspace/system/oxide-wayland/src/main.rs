//! oxide-wayland — OXIDE OS Wayland Compositor
//!
//! — NeonVale: The glass canvas where windows live. Custom Wayland compositor
//! that renders directly to the UEFI GOP framebuffer via /dev/fb0.
//!
//! Wayland wire protocol: 8-byte header (object_id:u32 + size_opcode:u32),
//! followed by argument payload. Messages flow over AF_UNIX SOCK_STREAM.
//! File descriptors (for wl_shm buffers) are passed via SCM_RIGHTS.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

// ============================================================================
// Syscall wrappers (raw — no libc dependency for the compositor binary)
// ============================================================================

unsafe fn syscall1(nr: u64, a1: usize) -> isize {
    let ret: isize;
    core::arch::asm!("syscall", in("rax") nr, in("rdi") a1,
        lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

unsafe fn syscall2(nr: u64, a1: usize, a2: usize) -> isize {
    let ret: isize;
    core::arch::asm!("syscall", in("rax") nr, in("rdi") a1, in("rsi") a2,
        lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

unsafe fn syscall3(nr: u64, a1: usize, a2: usize, a3: usize) -> isize {
    let ret: isize;
    core::arch::asm!("syscall", in("rax") nr, in("rdi") a1, in("rsi") a2,
        in("rdx") a3, lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

unsafe fn syscall4(nr: u64, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    let ret: isize;
    core::arch::asm!("syscall", in("rax") nr, in("rdi") a1, in("rsi") a2,
        in("rdx") a3, in("r10") a4,
        lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

unsafe fn syscall6(nr: u64, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize, a6: usize) -> isize {
    let ret: isize;
    core::arch::asm!("syscall", in("rax") nr, in("rdi") a1, in("rsi") a2,
        in("rdx") a3, in("r10") a4, in("r8") a5, in("r9") a6,
        lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

fn write_str(fd: i32, s: &str) {
    unsafe { syscall3(1, fd as usize, s.as_ptr() as usize, s.len()); }
}

fn write_bytes(fd: i32, b: &[u8]) {
    unsafe { syscall3(1, fd as usize, b.as_ptr() as usize, b.len()); }
}

// ============================================================================
// Wayland protocol constants
// ============================================================================

/// Wayland global object IDs
const WL_DISPLAY_ID: u32 = 1;

/// wl_display opcodes (client → server)
const WL_DISPLAY_SYNC: u16 = 0;
const WL_DISPLAY_GET_REGISTRY: u16 = 1;

/// wl_display events (server → client)
const WL_DISPLAY_ERROR: u16 = 0;
const WL_DISPLAY_DELETE_ID: u16 = 1;

/// wl_registry events
const WL_REGISTRY_GLOBAL: u16 = 0;
const WL_REGISTRY_GLOBAL_REMOVE: u16 = 1;

/// wl_registry requests
const WL_REGISTRY_BIND: u16 = 0;

/// wl_callback events
const WL_CALLBACK_DONE: u16 = 0;

/// wl_shm formats
const WL_SHM_FORMAT_ARGB8888: u32 = 0;
const WL_SHM_FORMAT_XRGB8888: u32 = 1;

/// wl_shm events
const WL_SHM_FORMAT: u16 = 0;

/// wl_shm requests
const WL_SHM_CREATE_POOL: u16 = 0;

/// wl_shm_pool requests
const WL_SHM_POOL_CREATE_BUFFER: u16 = 0;
const WL_SHM_POOL_DESTROY: u16 = 2;

/// wl_compositor requests
const WL_COMPOSITOR_CREATE_SURFACE: u16 = 0;

/// wl_surface requests
const WL_SURFACE_DESTROY: u16 = 0;
const WL_SURFACE_ATTACH: u16 = 1;
const WL_SURFACE_DAMAGE: u16 = 2;
const WL_SURFACE_FRAME: u16 = 3;
const WL_SURFACE_COMMIT: u16 = 6;

/// xdg_wm_base requests
const XDG_WM_BASE_GET_XDG_SURFACE: u16 = 2;
const XDG_WM_BASE_PONG: u16 = 3;

/// xdg_wm_base events
const XDG_WM_BASE_PING: u16 = 0;

/// xdg_surface requests
const XDG_SURFACE_GET_TOPLEVEL: u16 = 1;
const XDG_SURFACE_ACK_CONFIGURE: u16 = 4;

/// xdg_surface events
const XDG_SURFACE_CONFIGURE: u16 = 0;

/// xdg_toplevel events
const XDG_TOPLEVEL_CONFIGURE: u16 = 0;
const XDG_TOPLEVEL_CLOSE: u16 = 1;

// ============================================================================
// Interface names for wl_registry.global
// ============================================================================

const IFACE_WL_COMPOSITOR: &[u8] = b"wl_compositor\0";
const IFACE_WL_SHM: &[u8] = b"wl_shm\0";
const IFACE_WL_SEAT: &[u8] = b"wl_seat\0";
const IFACE_WL_OUTPUT: &[u8] = b"wl_output\0";
const IFACE_XDG_WM_BASE: &[u8] = b"xdg_wm_base\0";

// ============================================================================
// Wire protocol: message building
// ============================================================================

/// Build a Wayland message header + payload
fn build_msg(object_id: u32, opcode: u16, payload: &[u8]) -> Vec<u8> {
    let size = 8 + payload.len() as u32;
    let size_opcode = (size << 16) | (opcode as u32);
    let mut msg = Vec::with_capacity(size as usize);
    msg.extend_from_slice(&object_id.to_ne_bytes());
    msg.extend_from_slice(&size_opcode.to_ne_bytes());
    msg.extend_from_slice(payload);
    // Pad to 4-byte boundary
    while msg.len() % 4 != 0 {
        msg.push(0);
    }
    msg
}

/// Build a wl_registry.global event
fn build_registry_global(registry_id: u32, name: u32, interface: &[u8], version: u32) -> Vec<u8> {
    // Payload: uint(name) + string(interface) + uint(version)
    let iface_len = interface.len() as u32; // includes NUL
    let padded_len = (iface_len + 3) & !3;
    let mut payload = Vec::new();
    payload.extend_from_slice(&name.to_ne_bytes());
    payload.extend_from_slice(&iface_len.to_ne_bytes());
    payload.extend_from_slice(interface);
    // Pad string to 4 bytes
    while payload.len() % 4 != (8 % 4) {
        // Actually, pad the string portion to 4-byte boundary
        if (payload.len() - 8) % 4 != 0 {
            payload.push(0);
        } else {
            break;
        }
    }
    while (payload.len() - 4) % 4 != 0 { payload.push(0); }
    payload.extend_from_slice(&version.to_ne_bytes());
    build_msg(registry_id, WL_REGISTRY_GLOBAL, &payload)
}

/// Build a wl_callback.done event
fn build_callback_done(callback_id: u32, serial: u32) -> Vec<u8> {
    build_msg(callback_id, WL_CALLBACK_DONE, &serial.to_ne_bytes())
}

/// Build a wl_display.delete_id event
fn build_delete_id(object_id: u32) -> Vec<u8> {
    build_msg(WL_DISPLAY_ID, WL_DISPLAY_DELETE_ID, &object_id.to_ne_bytes())
}

/// Build a wl_shm.format event
fn build_shm_format(shm_id: u32, format: u32) -> Vec<u8> {
    build_msg(shm_id, WL_SHM_FORMAT, &format.to_ne_bytes())
}

// ============================================================================
// Object types tracked per-client
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
enum ObjectType {
    Display,
    Registry,
    Callback,
    Compositor,
    Shm,
    ShmPool,
    Buffer,
    Surface,
    Seat,
    Output,
    XdgWmBase,
    XdgSurface,
    XdgToplevel,
}

// ============================================================================
// Buffer and Surface types
// ============================================================================

struct ShmPool {
    fd: i32,
    data: *mut u8,
    size: usize,
}

struct Buffer {
    pool_data: *mut u8,
    offset: u32,
    width: u32,
    height: u32,
    stride: u32,
    format: u32,
}

struct Surface {
    id: u32,
    pending_buffer_id: Option<u32>,
    committed_buffer_id: Option<u32>,
    x: i32,
    y: i32,
}

// ============================================================================
// Compositor state
// ============================================================================

struct Compositor {
    listen_fd: i32,
    fb_ptr: *mut u8,
    fb_width: u32,
    fb_height: u32,
    fb_stride: u32,
    serial: u32,
    next_global_name: u32,
    // Per-client state
    client_fd: i32,
    objects: BTreeMap<u32, ObjectType>,
    pools: BTreeMap<u32, ShmPool>,
    buffers: BTreeMap<u32, Buffer>,
    surfaces: BTreeMap<u32, Surface>,
    recv_buf: Vec<u8>,
}

impl Compositor {
    fn new() -> Self {
        Compositor {
            listen_fd: -1,
            fb_ptr: core::ptr::null_mut(),
            fb_width: 1280,
            fb_height: 800,
            fb_stride: 1280 * 4,
            serial: 1,
            next_global_name: 1,
            client_fd: -1,
            objects: BTreeMap::new(),
            pools: BTreeMap::new(),
            buffers: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            recv_buf: Vec::new(),
        }
    }

    fn next_serial(&mut self) -> u32 {
        let s = self.serial;
        self.serial += 1;
        s
    }

    /// Send a message to the connected client
    fn send(&self, msg: &[u8]) {
        if self.client_fd >= 0 {
            write_bytes(self.client_fd, msg);
        }
    }

    /// Send registry globals to a new client
    fn send_globals(&mut self, registry_id: u32) {
        let mut name = 1u32;
        self.send(&build_registry_global(registry_id, name, IFACE_WL_COMPOSITOR, 4));
        name += 1;
        self.send(&build_registry_global(registry_id, name, IFACE_WL_SHM, 1));
        name += 1;
        self.send(&build_registry_global(registry_id, name, IFACE_WL_SEAT, 7));
        name += 1;
        self.send(&build_registry_global(registry_id, name, IFACE_WL_OUTPUT, 3));
        name += 1;
        self.send(&build_registry_global(registry_id, name, IFACE_XDG_WM_BASE, 2));
        self.next_global_name = name + 1;
    }

    /// Handle an incoming Wayland message
    fn handle_message(&mut self, object_id: u32, opcode: u16, payload: &[u8]) {
        let obj_type = self.objects.get(&object_id).copied();

        match obj_type {
            Some(ObjectType::Display) => match opcode {
                WL_DISPLAY_SYNC => {
                    // Client requests a sync callback
                    if payload.len() >= 4 {
                        let callback_id = u32::from_ne_bytes(payload[0..4].try_into().unwrap());
                        self.objects.insert(callback_id, ObjectType::Callback);
                        let serial = self.next_serial();
                        self.send(&build_callback_done(callback_id, serial));
                        self.send(&build_delete_id(callback_id));
                        self.objects.remove(&callback_id);
                    }
                }
                WL_DISPLAY_GET_REGISTRY => {
                    if payload.len() >= 4 {
                        let registry_id = u32::from_ne_bytes(payload[0..4].try_into().unwrap());
                        self.objects.insert(registry_id, ObjectType::Registry);
                        self.send_globals(registry_id);
                    }
                }
                _ => {}
            },

            Some(ObjectType::Registry) => match opcode {
                WL_REGISTRY_BIND => {
                    // Payload: uint(name) + string(interface) + uint(version) + new_id(id)
                    if payload.len() >= 4 {
                        let _name = u32::from_ne_bytes(payload[0..4].try_into().unwrap());
                        // Parse interface string
                        let str_len = if payload.len() >= 8 {
                            u32::from_ne_bytes(payload[4..8].try_into().unwrap()) as usize
                        } else { 0 };
                        let padded = (str_len + 3) & !3;
                        let after_str = 8 + padded;
                        // version + new_id
                        if payload.len() >= after_str + 8 {
                            let _version = u32::from_ne_bytes(payload[after_str..after_str+4].try_into().unwrap());
                            let new_id = u32::from_ne_bytes(payload[after_str+4..after_str+8].try_into().unwrap());

                            // Determine interface type from name
                            let iface_bytes = &payload[8..8 + str_len.min(payload.len() - 8)];
                            if iface_bytes.starts_with(b"wl_compositor") {
                                self.objects.insert(new_id, ObjectType::Compositor);
                            } else if iface_bytes.starts_with(b"wl_shm") {
                                self.objects.insert(new_id, ObjectType::Shm);
                                // Send supported formats
                                self.send(&build_shm_format(new_id, WL_SHM_FORMAT_ARGB8888));
                                self.send(&build_shm_format(new_id, WL_SHM_FORMAT_XRGB8888));
                            } else if iface_bytes.starts_with(b"wl_seat") {
                                self.objects.insert(new_id, ObjectType::Seat);
                            } else if iface_bytes.starts_with(b"wl_output") {
                                self.objects.insert(new_id, ObjectType::Output);
                            } else if iface_bytes.starts_with(b"xdg_wm_base") {
                                self.objects.insert(new_id, ObjectType::XdgWmBase);
                            }
                        }
                    }
                }
                _ => {}
            },

            Some(ObjectType::Compositor) => match opcode {
                WL_COMPOSITOR_CREATE_SURFACE => {
                    if payload.len() >= 4 {
                        let new_id = u32::from_ne_bytes(payload[0..4].try_into().unwrap());
                        self.objects.insert(new_id, ObjectType::Surface);
                        self.surfaces.insert(new_id, Surface {
                            id: new_id,
                            pending_buffer_id: None,
                            committed_buffer_id: None,
                            x: 100,
                            y: 100,
                        });
                    }
                }
                _ => {}
            },

            Some(ObjectType::Shm) => match opcode {
                WL_SHM_CREATE_POOL => {
                    // Payload: new_id(u32) + fd(via SCM_RIGHTS) + size(i32)
                    // — NeonVale: The fd comes via SCM_RIGHTS, not in the payload.
                    // For now, we handle pools when we get the fd separately.
                    if payload.len() >= 8 {
                        let new_id = u32::from_ne_bytes(payload[0..4].try_into().unwrap());
                        let size = i32::from_ne_bytes(payload[4..8].try_into().unwrap());
                        self.objects.insert(new_id, ObjectType::ShmPool);
                        // Pool fd will be received via SCM_RIGHTS — stored separately
                        write_str(1, "[WAYLAND] SHM pool created\n");
                    }
                }
                _ => {}
            },

            Some(ObjectType::Surface) => match opcode {
                WL_SURFACE_ATTACH => {
                    if payload.len() >= 12 {
                        let buffer_id = u32::from_ne_bytes(payload[0..4].try_into().unwrap());
                        let _x = i32::from_ne_bytes(payload[4..8].try_into().unwrap());
                        let _y = i32::from_ne_bytes(payload[8..12].try_into().unwrap());
                        if let Some(surface) = self.surfaces.get_mut(&object_id) {
                            surface.pending_buffer_id = Some(buffer_id);
                        }
                    }
                }
                WL_SURFACE_COMMIT => {
                    if let Some(surface) = self.surfaces.get_mut(&object_id) {
                        surface.committed_buffer_id = surface.pending_buffer_id;
                        write_str(1, "[WAYLAND] Surface committed\n");
                        // Trigger compositor render
                        self.composite();
                    }
                }
                WL_SURFACE_FRAME => {
                    // Frame callback — client wants to know when to draw next frame
                    if payload.len() >= 4 {
                        let callback_id = u32::from_ne_bytes(payload[0..4].try_into().unwrap());
                        self.objects.insert(callback_id, ObjectType::Callback);
                        let serial = self.next_serial();
                        self.send(&build_callback_done(callback_id, serial));
                        self.send(&build_delete_id(callback_id));
                        self.objects.remove(&callback_id);
                    }
                }
                WL_SURFACE_DAMAGE => {} // Noted, recomposite on commit
                WL_SURFACE_DESTROY => {
                    self.surfaces.remove(&object_id);
                    self.objects.remove(&object_id);
                }
                _ => {}
            },

            Some(ObjectType::XdgWmBase) => match opcode {
                XDG_WM_BASE_GET_XDG_SURFACE => {
                    if payload.len() >= 8 {
                        let new_id = u32::from_ne_bytes(payload[0..4].try_into().unwrap());
                        let _surface_id = u32::from_ne_bytes(payload[4..8].try_into().unwrap());
                        self.objects.insert(new_id, ObjectType::XdgSurface);
                    }
                }
                XDG_WM_BASE_PONG => {} // Client responded to ping
                _ => {}
            },

            Some(ObjectType::XdgSurface) => match opcode {
                XDG_SURFACE_GET_TOPLEVEL => {
                    if payload.len() >= 4 {
                        let new_id = u32::from_ne_bytes(payload[0..4].try_into().unwrap());
                        self.objects.insert(new_id, ObjectType::XdgToplevel);

                        // Send xdg_toplevel.configure (width=0, height=0 = client chooses)
                        let mut configure_payload = Vec::new();
                        configure_payload.extend_from_slice(&0i32.to_ne_bytes()); // width
                        configure_payload.extend_from_slice(&0i32.to_ne_bytes()); // height
                        configure_payload.extend_from_slice(&0u32.to_ne_bytes()); // states array length
                        self.send(&build_msg(new_id, XDG_TOPLEVEL_CONFIGURE, &configure_payload));

                        // Send xdg_surface.configure
                        let serial = self.next_serial();
                        self.send(&build_msg(object_id, XDG_SURFACE_CONFIGURE, &serial.to_ne_bytes()));
                    }
                }
                XDG_SURFACE_ACK_CONFIGURE => {} // Client acknowledged configure
                _ => {}
            },

            _ => {
                // Unknown object — ignore
            }
        }
    }

    /// Process incoming data from the client
    fn process_client_data(&mut self, data: &[u8]) {
        self.recv_buf.extend_from_slice(data);

        while self.recv_buf.len() >= 8 {
            let object_id = u32::from_ne_bytes(self.recv_buf[0..4].try_into().unwrap());
            let size_opcode = u32::from_ne_bytes(self.recv_buf[4..8].try_into().unwrap());
            let size = (size_opcode >> 16) as usize;
            let opcode = (size_opcode & 0xFFFF) as u16;

            if size < 8 || size > 4096 {
                // Invalid message size — drop connection
                write_str(1, "[WAYLAND] Invalid message size, dropping\n");
                self.recv_buf.clear();
                return;
            }

            if self.recv_buf.len() < size {
                return; // Incomplete message — wait for more data
            }

            let payload = self.recv_buf[8..size].to_vec();
            // Remove processed message from buffer
            self.recv_buf = self.recv_buf[size..].to_vec();

            self.handle_message(object_id, opcode, &payload);
        }
    }

    /// Composite all surfaces to framebuffer
    fn composite(&self) {
        if self.fb_ptr.is_null() {
            return;
        }

        let pixel_count = (self.fb_stride * self.fb_height) as usize / 4;
        let fb = unsafe { core::slice::from_raw_parts_mut(self.fb_ptr as *mut u32, pixel_count) };

        // — NeonVale: Clear to cyberpunk navy (0x1a1a2e)
        for pixel in fb.iter_mut() {
            *pixel = 0xFF1a1a2e;
        }

        // Blit committed surfaces
        for surface in self.surfaces.values() {
            if let Some(buffer_id) = surface.committed_buffer_id {
                if let Some(buffer) = self.buffers.get(&buffer_id) {
                    if !buffer.pool_data.is_null() {
                        let src_ptr = unsafe { buffer.pool_data.add(buffer.offset as usize) };
                        let src = unsafe {
                            core::slice::from_raw_parts(src_ptr as *const u32,
                                (buffer.stride * buffer.height) as usize / 4)
                        };

                        for row in 0..buffer.height {
                            let dst_y = surface.y + row as i32;
                            if dst_y < 0 || dst_y >= self.fb_height as i32 { continue; }
                            for col in 0..buffer.width {
                                let dst_x = surface.x + col as i32;
                                if dst_x < 0 || dst_x >= self.fb_width as i32 { continue; }

                                let src_idx = (row * buffer.stride / 4 + col) as usize;
                                let dst_idx = (dst_y as u32 * self.fb_width + dst_x as u32) as usize;

                                if src_idx < src.len() && dst_idx < fb.len() {
                                    fb[dst_idx] = src[src_idx];
                                }
                            }
                        }
                    }
                }
            }
        }

        write_str(1, "[WAYLAND] Frame composited\n");
    }

    /// Initialize: open framebuffer, create socket, start event loop
    fn run(&mut self) -> i32 {
        write_str(1, "=== oxide-wayland compositor starting ===\n");

        // Open /dev/fb0
        let fb_path = b"/dev/fb0\0";
        let fb_fd = unsafe { syscall2(2, fb_path.as_ptr() as usize, 2) } as i32; // O_RDWR=2
        if fb_fd < 0 {
            write_str(1, "[WAYLAND] Warning: /dev/fb0 not available, running headless\n");
        } else {
            // mmap the framebuffer
            let fb_size = (self.fb_width * self.fb_height * 4) as usize;
            let ptr = unsafe {
                syscall6(9, 0, fb_size, 3, 1, fb_fd as usize, 0) // mmap(NULL, size, PROT_READ|WRITE, MAP_SHARED, fd, 0)
            };
            if ptr > 0 && (ptr as usize) < 0xFFFF_FFFF_FFFF_F000 {
                self.fb_ptr = ptr as *mut u8;
                write_str(1, "[WAYLAND] Framebuffer mmap'd\n");
            }
        }

        // Create AF_UNIX socket
        let sock_fd = unsafe { syscall3(41, 1, 1, 0) } as i32; // socket(AF_UNIX, SOCK_STREAM, 0)
        if sock_fd < 0 {
            write_str(1, "[WAYLAND] ERROR: Failed to create socket\n");
            return 1;
        }

        // Bind to /run/wayland-0
        let mut addr = [0u8; 110]; // sockaddr_un
        addr[0] = 1; // AF_UNIX
        addr[1] = 0;
        let path = b"/run/wayland-0";
        addr[2..2 + path.len()].copy_from_slice(path);
        let bind_result = unsafe { syscall3(49, sock_fd as usize, addr.as_ptr() as usize, (2 + path.len() + 1) as usize) };
        if bind_result < 0 {
            write_str(1, "[WAYLAND] ERROR: Failed to bind socket (is another compositor running?)\n");
            return 1;
        }

        // Listen
        let listen_result = unsafe { syscall2(50, sock_fd as usize, 4) };
        if listen_result < 0 {
            write_str(1, "[WAYLAND] ERROR: Failed to listen\n");
            return 1;
        }

        self.listen_fd = sock_fd;
        write_str(1, "[WAYLAND] Listening on /run/wayland-0\n");

        // Set WAYLAND_DISPLAY env var (for child processes)
        // This would normally be done by the session manager

        // Event loop: accept one client and process messages
        // — NeonVale: Single-client for now. Multi-client requires epoll.
        write_str(1, "[WAYLAND] Waiting for client connection...\n");

        let client_fd = unsafe { syscall3(43, sock_fd as usize, 0, 0) } as i32; // accept
        if client_fd < 0 {
            write_str(1, "[WAYLAND] ERROR: Failed to accept client\n");
            return 1;
        }

        write_str(1, "[WAYLAND] Client connected!\n");
        self.client_fd = client_fd;

        // Register wl_display as object 1
        self.objects.insert(WL_DISPLAY_ID, ObjectType::Display);

        // Read messages from client
        let mut buf = [0u8; 4096];
        loop {
            let n = unsafe { syscall3(0, client_fd as usize, buf.as_mut_ptr() as usize, buf.len()) };
            if n <= 0 {
                write_str(1, "[WAYLAND] Client disconnected\n");
                break;
            }
            self.process_client_data(&buf[..n as usize]);
        }

        // Cleanup
        unsafe {
            syscall1(3, client_fd as usize); // close
            syscall1(3, sock_fd as usize);
        }

        write_str(1, "[WAYLAND] Compositor shutting down\n");
        0
    }
}

// ============================================================================
// Entry point
// ============================================================================

#[unsafe(no_mangle)]
fn main() -> i32 {
    let mut compositor = Compositor::new();
    compositor.run()
}

// — NeonVale: Allocator and panic handler required for no_std + alloc
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    write_str(2, "[WAYLAND] PANIC!\n");
    loop { unsafe { core::arch::asm!("hlt"); } }
}

#[global_allocator]
static ALLOC: SimpleAlloc = SimpleAlloc;
struct SimpleAlloc;
unsafe impl core::alloc::GlobalAlloc for SimpleAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        unsafe extern "C" { fn malloc(size: usize) -> *mut u8; }
        unsafe { malloc(layout.size()) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: core::alloc::Layout) {
        unsafe extern "C" { fn free(ptr: *mut u8); }
        unsafe { free(ptr) }
    }
}
