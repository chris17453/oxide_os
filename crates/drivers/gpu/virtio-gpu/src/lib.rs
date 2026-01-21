//! VirtIO GPU Driver
//!
//! Implements the VirtIO GPU device specification for graphics
//! in virtualized environments.

#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU16, Ordering};
use fb::{Color, Framebuffer, FramebufferInfo, PixelFormat};
use spin::Mutex;

/// VirtIO GPU control commands
mod cmd {
    pub const GET_DISPLAY_INFO: u32 = 0x100;
    pub const RESOURCE_CREATE_2D: u32 = 0x101;
    pub const RESOURCE_UNREF: u32 = 0x102;
    pub const SET_SCANOUT: u32 = 0x103;
    pub const RESOURCE_FLUSH: u32 = 0x104;
    pub const TRANSFER_TO_HOST_2D: u32 = 0x105;
    pub const RESOURCE_ATTACH_BACKING: u32 = 0x106;
    pub const RESOURCE_DETACH_BACKING: u32 = 0x107;
    pub const GET_CAPSET_INFO: u32 = 0x108;
    pub const GET_CAPSET: u32 = 0x109;
    pub const GET_EDID: u32 = 0x10A;
}

/// VirtIO GPU response types
mod resp {
    pub const OK_NODATA: u32 = 0x1100;
    pub const OK_DISPLAY_INFO: u32 = 0x1101;
    pub const OK_CAPSET_INFO: u32 = 0x1102;
    pub const OK_CAPSET: u32 = 0x1103;
    pub const OK_EDID: u32 = 0x1104;
    pub const ERR_UNSPEC: u32 = 0x1200;
    pub const ERR_OUT_OF_MEMORY: u32 = 0x1201;
    pub const ERR_INVALID_SCANOUT_ID: u32 = 0x1202;
    pub const ERR_INVALID_RESOURCE_ID: u32 = 0x1203;
    pub const ERR_INVALID_CONTEXT_ID: u32 = 0x1204;
    pub const ERR_INVALID_PARAMETER: u32 = 0x1205;
}

/// Resource formats
mod format {
    pub const B8G8R8A8_UNORM: u32 = 1;
    pub const B8G8R8X8_UNORM: u32 = 2;
    pub const A8R8G8B8_UNORM: u32 = 3;
    pub const X8R8G8B8_UNORM: u32 = 4;
    pub const R8G8B8A8_UNORM: u32 = 67;
    pub const X8B8G8R8_UNORM: u32 = 68;
    pub const A8B8G8R8_UNORM: u32 = 121;
    pub const R8G8B8X8_UNORM: u32 = 134;
}

/// VirtIO GPU control header
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CtrlHeader {
    type_: u32,
    flags: u32,
    fence_id: u64,
    ctx_id: u32,
    padding: u32,
}

/// Display info for one scanout
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct DisplayOne {
    r: Rect,
    enabled: u32,
    flags: u32,
}

/// Rectangle
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// Resource create 2D command
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ResourceCreate2d {
    hdr: CtrlHeader,
    resource_id: u32,
    format: u32,
    width: u32,
    height: u32,
}

/// Resource attach backing command
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ResourceAttachBacking {
    hdr: CtrlHeader,
    resource_id: u32,
    nr_entries: u32,
}

/// Memory entry for attach backing
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct MemEntry {
    addr: u64,
    length: u32,
    padding: u32,
}

/// Set scanout command
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct SetScanout {
    hdr: CtrlHeader,
    r: Rect,
    scanout_id: u32,
    resource_id: u32,
}

/// Transfer to host 2D command
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct TransferToHost2d {
    hdr: CtrlHeader,
    r: Rect,
    offset: u64,
    resource_id: u32,
    padding: u32,
}

/// Resource flush command
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ResourceFlush {
    hdr: CtrlHeader,
    r: Rect,
    resource_id: u32,
    padding: u32,
}

/// VirtIO MMIO register offsets
const VIRTIO_MMIO_MAGIC: usize = 0x000;
const VIRTIO_MMIO_VERSION: usize = 0x004;
const VIRTIO_MMIO_DEVICE_ID: usize = 0x008;
const VIRTIO_MMIO_DEVICE_FEATURES: usize = 0x010;
const VIRTIO_MMIO_DEVICE_FEATURES_SEL: usize = 0x014;
const VIRTIO_MMIO_DRIVER_FEATURES: usize = 0x020;
const VIRTIO_MMIO_DRIVER_FEATURES_SEL: usize = 0x024;
const VIRTIO_MMIO_QUEUE_SEL: usize = 0x030;
const VIRTIO_MMIO_QUEUE_NUM_MAX: usize = 0x034;
const VIRTIO_MMIO_QUEUE_NUM: usize = 0x038;
const VIRTIO_MMIO_QUEUE_READY: usize = 0x044;
const VIRTIO_MMIO_QUEUE_NOTIFY: usize = 0x050;
const VIRTIO_MMIO_INTERRUPT_STATUS: usize = 0x060;
const VIRTIO_MMIO_INTERRUPT_ACK: usize = 0x064;
const VIRTIO_MMIO_STATUS: usize = 0x070;
const VIRTIO_MMIO_QUEUE_DESC_LOW: usize = 0x080;
const VIRTIO_MMIO_QUEUE_DESC_HIGH: usize = 0x084;
const VIRTIO_MMIO_QUEUE_AVAIL_LOW: usize = 0x090;
const VIRTIO_MMIO_QUEUE_AVAIL_HIGH: usize = 0x094;
const VIRTIO_MMIO_QUEUE_USED_LOW: usize = 0x0a0;
const VIRTIO_MMIO_QUEUE_USED_HIGH: usize = 0x0a4;

/// VirtIO status bits
const VIRTIO_STATUS_ACKNOWLEDGE: u32 = 1;
const VIRTIO_STATUS_DRIVER: u32 = 2;
const VIRTIO_STATUS_FEATURES_OK: u32 = 8;
const VIRTIO_STATUS_DRIVER_OK: u32 = 4;
const VIRTIO_STATUS_FAILED: u32 = 128;

/// Control virtqueue
const VIRTIO_GPU_CONTROLQ: u32 = 0;
/// Cursor virtqueue
const VIRTIO_GPU_CURSORQ: u32 = 1;

/// Descriptor flags
const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;

/// Virtqueue descriptor
#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

/// Virtqueue available ring
#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 256],
}

/// Virtqueue used ring element
#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

/// Virtqueue used ring
#[repr(C)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 256],
}

/// VirtIO GPU device
pub struct VirtioGpu {
    /// MMIO base address
    mmio_base: usize,
    /// Descriptor table
    descriptors: *mut VirtqDesc,
    /// Available ring
    available: *mut VirtqAvail,
    /// Used ring
    used: *mut VirtqUsed,
    /// Queue size
    queue_size: u16,
    /// Next descriptor (uses interior mutability for flush)
    next_desc: AtomicU16,
    /// Last used index (uses interior mutability for flush)
    last_used: AtomicU16,
    /// Display width
    width: u32,
    /// Display height
    height: u32,
    /// Framebuffer resource ID
    resource_id: u32,
    /// Framebuffer memory
    framebuffer: Option<Box<[u8]>>,
}

impl VirtioGpu {
    /// Probe for VirtIO GPU at MMIO address
    pub fn probe(mmio_base: usize) -> Option<Self> {
        let magic = unsafe { read_volatile((mmio_base + VIRTIO_MMIO_MAGIC) as *const u32) };
        if magic != 0x74726976 {
            return None;
        }

        let version = unsafe { read_volatile((mmio_base + VIRTIO_MMIO_VERSION) as *const u32) };
        if version != 2 {
            return None;
        }

        let device_id = unsafe { read_volatile((mmio_base + VIRTIO_MMIO_DEVICE_ID) as *const u32) };
        if device_id != 16 {
            // Not a GPU device
            return None;
        }

        Some(VirtioGpu {
            mmio_base,
            descriptors: core::ptr::null_mut(),
            available: core::ptr::null_mut(),
            used: core::ptr::null_mut(),
            queue_size: 0,
            next_desc: AtomicU16::new(0),
            last_used: AtomicU16::new(0),
            width: 0,
            height: 0,
            resource_id: 1,
            framebuffer: None,
        })
    }

    /// Initialize the GPU device
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Reset device
        self.write_reg(VIRTIO_MMIO_STATUS, 0);

        // Acknowledge device
        self.write_reg(VIRTIO_MMIO_STATUS, VIRTIO_STATUS_ACKNOWLEDGE);

        // Driver loaded
        self.write_reg(
            VIRTIO_MMIO_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER,
        );

        // Read features
        self.write_reg(VIRTIO_MMIO_DEVICE_FEATURES_SEL, 0);
        let _features = self.read_reg(VIRTIO_MMIO_DEVICE_FEATURES);

        // Accept features
        self.write_reg(VIRTIO_MMIO_DRIVER_FEATURES_SEL, 0);
        self.write_reg(VIRTIO_MMIO_DRIVER_FEATURES, 0);

        // Features OK
        self.write_reg(
            VIRTIO_MMIO_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK,
        );

        // Verify features accepted
        let status = self.read_reg(VIRTIO_MMIO_STATUS);
        if status & VIRTIO_STATUS_FEATURES_OK == 0 {
            self.write_reg(VIRTIO_MMIO_STATUS, VIRTIO_STATUS_FAILED);
            return Err("Features not accepted");
        }

        // Initialize control queue
        self.init_controlq()?;

        // Driver ready
        self.write_reg(
            VIRTIO_MMIO_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE
                | VIRTIO_STATUS_DRIVER
                | VIRTIO_STATUS_FEATURES_OK
                | VIRTIO_STATUS_DRIVER_OK,
        );

        // Get display info
        self.get_display_info()?;

        // Create resources
        self.setup_framebuffer()?;

        Ok(())
    }

    /// Initialize control queue
    fn init_controlq(&mut self) -> Result<(), &'static str> {
        self.write_reg(VIRTIO_MMIO_QUEUE_SEL, VIRTIO_GPU_CONTROLQ);

        let max_size = self.read_reg(VIRTIO_MMIO_QUEUE_NUM_MAX) as u16;
        if max_size == 0 {
            return Err("Queue not available");
        }

        let size = max_size.min(64);
        self.queue_size = size;
        self.write_reg(VIRTIO_MMIO_QUEUE_NUM, size as u32);

        // Allocate queue structures
        use alloc::alloc::{alloc_zeroed, Layout};

        let desc_size = size as usize * core::mem::size_of::<VirtqDesc>();
        let desc_layout = Layout::from_size_align(desc_size, 16).unwrap();
        self.descriptors = unsafe { alloc_zeroed(desc_layout) } as *mut VirtqDesc;

        let avail_layout = Layout::from_size_align(core::mem::size_of::<VirtqAvail>(), 2).unwrap();
        self.available = unsafe { alloc_zeroed(avail_layout) } as *mut VirtqAvail;

        let used_layout = Layout::from_size_align(core::mem::size_of::<VirtqUsed>(), 4).unwrap();
        self.used = unsafe { alloc_zeroed(used_layout) } as *mut VirtqUsed;

        // Set queue addresses
        let desc_addr = self.descriptors as u64;
        let avail_addr = self.available as u64;
        let used_addr = self.used as u64;

        self.write_reg(VIRTIO_MMIO_QUEUE_DESC_LOW, desc_addr as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_DESC_HIGH, (desc_addr >> 32) as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_AVAIL_LOW, avail_addr as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_AVAIL_HIGH, (avail_addr >> 32) as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_USED_LOW, used_addr as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_USED_HIGH, (used_addr >> 32) as u32);

        self.write_reg(VIRTIO_MMIO_QUEUE_READY, 1);

        Ok(())
    }

    /// Get display info
    fn get_display_info(&mut self) -> Result<(), &'static str> {
        #[repr(C)]
        struct GetDisplayInfoResp {
            hdr: CtrlHeader,
            displays: [DisplayOne; 16],
        }

        let cmd = CtrlHeader {
            type_: cmd::GET_DISPLAY_INFO,
            ..Default::default()
        };

        let mut resp = GetDisplayInfoResp {
            hdr: CtrlHeader::default(),
            displays: [DisplayOne::default(); 16],
        };

        self.send_command(&cmd, &mut resp)?;

        if resp.hdr.type_ != resp::OK_DISPLAY_INFO {
            return Err("Failed to get display info");
        }

        // Find first enabled display
        for display in &resp.displays {
            if display.enabled != 0 {
                self.width = display.r.width;
                self.height = display.r.height;
                return Ok(());
            }
        }

        // Default resolution
        self.width = 800;
        self.height = 600;
        Ok(())
    }

    /// Setup framebuffer
    fn setup_framebuffer(&mut self) -> Result<(), &'static str> {
        // Create resource
        let create_cmd = ResourceCreate2d {
            hdr: CtrlHeader {
                type_: cmd::RESOURCE_CREATE_2D,
                ..Default::default()
            },
            resource_id: self.resource_id,
            format: format::B8G8R8A8_UNORM,
            width: self.width,
            height: self.height,
        };

        let mut resp = CtrlHeader::default();
        self.send_command(&create_cmd, &mut resp)?;

        if resp.type_ != resp::OK_NODATA {
            return Err("Failed to create resource");
        }

        // Allocate framebuffer
        let fb_size = (self.width * self.height * 4) as usize;
        let fb = alloc::vec![0u8; fb_size].into_boxed_slice();
        let fb_addr = fb.as_ptr() as u64;

        // Attach backing
        #[repr(C)]
        struct AttachCmd {
            hdr: ResourceAttachBacking,
            entry: MemEntry,
        }

        let attach_cmd = AttachCmd {
            hdr: ResourceAttachBacking {
                hdr: CtrlHeader {
                    type_: cmd::RESOURCE_ATTACH_BACKING,
                    ..Default::default()
                },
                resource_id: self.resource_id,
                nr_entries: 1,
            },
            entry: MemEntry {
                addr: fb_addr,
                length: fb_size as u32,
                padding: 0,
            },
        };

        self.send_command(&attach_cmd, &mut resp)?;

        if resp.type_ != resp::OK_NODATA {
            return Err("Failed to attach backing");
        }

        // Set scanout
        let scanout_cmd = SetScanout {
            hdr: CtrlHeader {
                type_: cmd::SET_SCANOUT,
                ..Default::default()
            },
            r: Rect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            },
            scanout_id: 0,
            resource_id: self.resource_id,
        };

        self.send_command(&scanout_cmd, &mut resp)?;

        if resp.type_ != resp::OK_NODATA {
            return Err("Failed to set scanout");
        }

        self.framebuffer = Some(fb);
        Ok(())
    }

    /// Send a command and wait for response
    fn send_command<C, R>(&self, cmd: &C, resp: &mut R) -> Result<(), &'static str> {
        let cmd_size = core::mem::size_of::<C>();
        let resp_size = core::mem::size_of::<R>();

        // Setup descriptors (use atomic operations for thread-safety)
        let desc_idx = self.next_desc.fetch_add(2, Ordering::SeqCst) % self.queue_size;

        unsafe {
            // Command descriptor
            let desc0 = &mut *self.descriptors.add(desc_idx as usize);
            desc0.addr = cmd as *const C as u64;
            desc0.len = cmd_size as u32;
            desc0.flags = VIRTQ_DESC_F_NEXT;
            desc0.next = (desc_idx + 1) % self.queue_size;

            // Response descriptor
            let desc1 = &mut *self.descriptors.add(((desc_idx + 1) % self.queue_size) as usize);
            desc1.addr = resp as *mut R as u64;
            desc1.len = resp_size as u32;
            desc1.flags = VIRTQ_DESC_F_WRITE;
            desc1.next = 0;

            // Add to available ring
            let avail = &mut *self.available;
            let avail_idx = avail.idx;
            avail.ring[(avail_idx % self.queue_size) as usize] = desc_idx;
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            write_volatile(&mut avail.idx, avail_idx.wrapping_add(1));
        }

        // Notify device
        self.write_reg(VIRTIO_MMIO_QUEUE_NOTIFY, VIRTIO_GPU_CONTROLQ);

        // Wait for completion
        let last_used = self.last_used.load(Ordering::Acquire);
        loop {
            let used_idx = unsafe { read_volatile(&(*self.used).idx) };
            if used_idx != last_used {
                self.last_used.store(used_idx, Ordering::Release);
                break;
            }
            core::hint::spin_loop();
        }

        Ok(())
    }

    /// Flush framebuffer to display (full screen)
    pub fn flush(&self) {
        self.flush_region(0, 0, self.width, self.height);
    }
    
    /// Flush a specific region to display (optimized for partial updates)
    pub fn flush_region(&self, x: u32, y: u32, width: u32, height: u32) {
        if self.framebuffer.is_none() {
            return;
        }

        // Transfer to host
        let transfer_cmd = TransferToHost2d {
            hdr: CtrlHeader {
                type_: cmd::TRANSFER_TO_HOST_2D,
                ..Default::default()
            },
            r: Rect {
                x,
                y,
                width,
                height,
            },
            offset: 0,
            resource_id: self.resource_id,
            padding: 0,
        };

        let mut resp = CtrlHeader::default();
        let _ = self.send_command(&transfer_cmd, &mut resp);

        // Flush resource
        let flush_cmd = ResourceFlush {
            hdr: CtrlHeader {
                type_: cmd::RESOURCE_FLUSH,
                ..Default::default()
            },
            r: Rect {
                x,
                y,
                width,
                height,
            },
            resource_id: self.resource_id,
            padding: 0,
        };

        let _ = self.send_command(&flush_cmd, &mut resp);
    }

    /// Get framebuffer info
    pub fn framebuffer_info(&self) -> Option<FramebufferInfo> {
        self.framebuffer.as_ref().map(|fb| FramebufferInfo {
            base: fb.as_ptr() as usize,
            size: fb.len(),
            width: self.width,
            height: self.height,
            stride: self.width * 4,
            format: PixelFormat::BGRA8888,
        })
    }

    fn read_reg(&self, offset: usize) -> u32 {
        unsafe { read_volatile((self.mmio_base + offset) as *const u32) }
    }

    fn write_reg(&self, offset: usize, value: u32) {
        unsafe { write_volatile((self.mmio_base + offset) as *mut u32, value) }
    }
}

impl Framebuffer for VirtioGpu {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn format(&self) -> PixelFormat {
        PixelFormat::BGRA8888
    }

    fn stride(&self) -> u32 {
        self.width * 4
    }

    fn buffer(&self) -> *mut u8 {
        self.framebuffer
            .as_ref()
            .map(|fb| fb.as_ptr() as *mut u8)
            .unwrap_or(core::ptr::null_mut())
    }

    fn size(&self) -> usize {
        self.framebuffer.as_ref().map(|fb| fb.len()).unwrap_or(0)
    }

    fn flush(&self) {
        // Now callable with immutable reference thanks to interior mutability
        self.flush();
    }
}

unsafe impl Send for VirtioGpu {}
unsafe impl Sync for VirtioGpu {}

/// Global VirtIO GPU instance
static VIRTIO_GPU: Mutex<Option<VirtioGpu>> = Mutex::new(None);

/// Initialize VirtIO GPU from MMIO address
pub fn init(mmio_base: usize) -> Result<(), &'static str> {
    let mut gpu = VirtioGpu::probe(mmio_base).ok_or("VirtIO GPU not found")?;
    gpu.init()?;

    // Initialize framebuffer subsystem
    if let Some(info) = gpu.framebuffer_info() {
        fb::init(info);
    }

    *VIRTIO_GPU.lock() = Some(gpu);
    Ok(())
}

/// Flush the display
pub fn flush() {
    if let Some(ref gpu) = *VIRTIO_GPU.lock() {
        gpu.flush();
    }
}

/// Get display dimensions
pub fn dimensions() -> Option<(u32, u32)> {
    VIRTIO_GPU.lock().as_ref().map(|gpu| (gpu.width, gpu.height))
}
