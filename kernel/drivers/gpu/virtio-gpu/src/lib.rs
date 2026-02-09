//! VirtIO GPU Driver
//!
//! Implements the VirtIO GPU device specification for graphics
//! in virtualized environments.

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use boot_proto::VideoMode;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU16, Ordering};
use fb::mode::ModeSetter;
use fb::{Color, Framebuffer, FramebufferInfo, PixelFormat};
use pci::{PciDevice, VirtioPciTransport};
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

/// Physical memory map base for converting kernel virtual <-> physical DMA addresses
/// — GlassSignal: the membrane between address spaces
const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;

/// VirtIO GPU device
pub struct VirtioGpu {
    /// MMIO base address (0 when using PCI transport)
    mmio_base: usize,
    /// PCI transport (None for MMIO mode)
    transport: Option<VirtioPciTransport>,
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
    /// Current pixel format
    pixel_format: PixelFormat,
    /// Bytes per pixel
    bytes_per_pixel: u32,
    /// Virtio resource format
    virtio_format: u32,
    /// Framebuffer resource ID
    resource_id: u32,
    /// Framebuffer memory
    framebuffer: Option<Box<[u8]>>,
    /// Display info cache
    displays: [DisplayOne; 16],
    /// Number of displays reported
    display_count: u32,
}

impl VirtioGpu {
    /// Construct from a PCI device — walks capabilities, builds transport
    /// — GlassSignal: PCI is just another wire carrying pixel dreams
    pub fn from_pci(pci_dev: &PciDevice) -> Option<Self> {
        if !pci_dev.is_virtio_gpu() {
            return None;
        }

        // Enable device for DMA and MMIO access
        pci::enable_bus_master(pci_dev.address);
        pci::enable_memory_space(pci_dev.address);

        // Walk PCI capabilities to locate VirtIO config regions
        let caps = pci::find_virtio_caps(pci_dev);
        let transport = VirtioPciTransport::from_caps(pci_dev, &caps)?;

        Some(VirtioGpu {
            mmio_base: 0,
            transport: Some(transport),
            descriptors: core::ptr::null_mut(),
            available: core::ptr::null_mut(),
            used: core::ptr::null_mut(),
            queue_size: 0,
            next_desc: AtomicU16::new(0),
            last_used: AtomicU16::new(0),
            width: 0,
            height: 0,
            pixel_format: PixelFormat::BGRA8888,
            bytes_per_pixel: 4,
            virtio_format: format::B8G8R8A8_UNORM,
            resource_id: 1,
            framebuffer: None,
            displays: [DisplayOne::default(); 16],
            display_count: 0,
        })
    }

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
            transport: None,
            descriptors: core::ptr::null_mut(),
            available: core::ptr::null_mut(),
            used: core::ptr::null_mut(),
            queue_size: 0,
            next_desc: AtomicU16::new(0),
            last_used: AtomicU16::new(0),
            width: 0,
            height: 0,
            pixel_format: PixelFormat::BGRA8888,
            bytes_per_pixel: 4,
            virtio_format: format::B8G8R8A8_UNORM,
            resource_id: 1,
            framebuffer: None,
            displays: [DisplayOne::default(); 16],
            display_count: 0,
        })
    }

    /// Initialize the GPU device (works for both MMIO and PCI transport)
    /// — GlassSignal: same handshake, different wires
    pub fn init(&mut self) -> Result<(), &'static str> {
        let has_pci = self.transport.is_some();

        if has_pci {
            self.init_pci_handshake()?;
        } else {
            self.init_mmio_handshake()?;
        }

        // Get display info
        self.get_display_info()?;

        // Create resources
        self.setup_framebuffer()?;

        Ok(())
    }

    /// PCI transport VirtIO handshake
    fn init_pci_handshake(&mut self) -> Result<(), &'static str> {
        let t = self.transport.as_ref().ok_or("No PCI transport")?;

        // Reset
        t.write_status(0);
        t.write_status(VIRTIO_STATUS_ACKNOWLEDGE as u8);
        t.write_status((VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER) as u8);

        // Read features, accept none for basic 2D
        let _features = t.read_device_features(0);
        t.write_driver_features(0, 0);

        // Features OK
        t.write_status(
            (VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK) as u8,
        );

        let status = t.read_status();
        if status & VIRTIO_STATUS_FEATURES_OK as u8 == 0 {
            t.write_status(VIRTIO_STATUS_FAILED as u8);
            return Err("Features not accepted");
        }

        // Release the immutable borrow before calling &mut self method
        let _ = t;

        // Initialize control queue via PCI transport
        self.init_controlq_pci()?;

        // Driver ready
        let t = self.transport.as_ref().ok_or("No PCI transport")?;
        t.write_status(
            (VIRTIO_STATUS_ACKNOWLEDGE
                | VIRTIO_STATUS_DRIVER
                | VIRTIO_STATUS_FEATURES_OK
                | VIRTIO_STATUS_DRIVER_OK) as u8,
        );

        Ok(())
    }

    /// MMIO transport VirtIO handshake (legacy path)
    fn init_mmio_handshake(&mut self) -> Result<(), &'static str> {
        self.write_reg(VIRTIO_MMIO_STATUS, 0);
        self.write_reg(VIRTIO_MMIO_STATUS, VIRTIO_STATUS_ACKNOWLEDGE);
        self.write_reg(
            VIRTIO_MMIO_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER,
        );

        self.write_reg(VIRTIO_MMIO_DEVICE_FEATURES_SEL, 0);
        let _features = self.read_reg(VIRTIO_MMIO_DEVICE_FEATURES);

        self.write_reg(VIRTIO_MMIO_DRIVER_FEATURES_SEL, 0);
        self.write_reg(VIRTIO_MMIO_DRIVER_FEATURES, 0);

        self.write_reg(
            VIRTIO_MMIO_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK,
        );

        let status = self.read_reg(VIRTIO_MMIO_STATUS);
        if status & VIRTIO_STATUS_FEATURES_OK == 0 {
            self.write_reg(VIRTIO_MMIO_STATUS, VIRTIO_STATUS_FAILED);
            return Err("Features not accepted");
        }

        self.init_controlq()?;

        self.write_reg(
            VIRTIO_MMIO_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE
                | VIRTIO_STATUS_DRIVER
                | VIRTIO_STATUS_FEATURES_OK
                | VIRTIO_STATUS_DRIVER_OK,
        );

        Ok(())
    }

    fn destroy_resource(&mut self) {
        // Drop framebuffer backing
        self.framebuffer = None;
    }

    /// Recreate framebuffer/resource for a specific display index and mode
    fn set_mode(
        &mut self,
        display_index: usize,
        width: u32,
        height: u32,
    ) -> Result<FramebufferInfo, &'static str> {
        if display_index >= self.display_count as usize || display_index >= self.displays.len() {
            return Err("invalid display index");
        }

        // Clean existing resource/backing
        self.destroy_resource();
        self.resource_id = self.resource_id.wrapping_add(1).max(1);
        self.width = width;
        self.height = height;
        self.pixel_format = PixelFormat::BGRA8888;
        self.bytes_per_pixel = 4;
        self.virtio_format = format::B8G8R8A8_UNORM;

        self.setup_framebuffer()?;

        // Bind scanout
        let scanout_cmd = SetScanout {
            hdr: CtrlHeader {
                type_: cmd::SET_SCANOUT,
                ..Default::default()
            },
            r: Rect {
                x: 0,
                y: 0,
                width,
                height,
            },
            scanout_id: display_index as u32,
            resource_id: self.resource_id,
        };

        let mut resp = CtrlHeader::default();
        self.send_command(&scanout_cmd, &mut resp)?;
        if resp.type_ != resp::OK_NODATA {
            return Err("Failed to set scanout");
        }

        self.framebuffer_info().ok_or("no framebuffer")
    }

    /// Initialize control queue via PCI transport (physical DMA addresses)
    /// — GlassSignal: virtqueues need real silicon addresses, not kernel illusions
    fn init_controlq_pci(&mut self) -> Result<(), &'static str> {
        let t = self.transport.as_ref().ok_or("No PCI transport")?;

        t.select_queue(VIRTIO_GPU_CONTROLQ as u16);

        let max_size = t.queue_max_size();
        if max_size == 0 {
            return Err("Queue not available");
        }

        let size = max_size.min(64);
        self.queue_size = size;
        t.set_queue_size(size);

        use alloc::alloc::{Layout, alloc_zeroed};

        let desc_size = size as usize * core::mem::size_of::<VirtqDesc>();
        let desc_layout = Layout::from_size_align(desc_size, 16).unwrap();
        self.descriptors = unsafe { alloc_zeroed(desc_layout) } as *mut VirtqDesc;

        let avail_layout = Layout::from_size_align(core::mem::size_of::<VirtqAvail>(), 2).unwrap();
        self.available = unsafe { alloc_zeroed(avail_layout) } as *mut VirtqAvail;

        let used_layout = Layout::from_size_align(core::mem::size_of::<VirtqUsed>(), 4).unwrap();
        self.used = unsafe { alloc_zeroed(used_layout) } as *mut VirtqUsed;

        // PCI transport needs physical addresses for virtqueue rings
        let desc_phys = self.descriptors as u64 - PHYS_MAP_BASE;
        let avail_phys = self.available as u64 - PHYS_MAP_BASE;
        let used_phys = self.used as u64 - PHYS_MAP_BASE;

        t.set_queue_desc(desc_phys);
        t.set_queue_avail(avail_phys);
        t.set_queue_used(used_phys);
        t.enable_queue();

        Ok(())
    }

    /// Initialize control queue (MMIO path)
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
        use alloc::alloc::{Layout, alloc_zeroed};

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

        self.displays = resp.displays;

        // Count enabled displays and pick first enabled for dimensions
        self.display_count = 0;
        for display in &self.displays {
            if display.enabled != 0 {
                self.display_count += 1;
                if self.display_count == 1 {
                    self.width = display.r.width;
                    self.height = display.r.height;
                }
            }
        }
        if self.display_count > 0 {
            return Ok(());
        }

        // Default resolution
        self.width = 800;
        self.height = 600;
        Ok(())
    }

    /// Setup framebuffer for current width/height
    fn setup_framebuffer(&mut self) -> Result<(), &'static str> {
        // Create resource
        let create_cmd = ResourceCreate2d {
            hdr: CtrlHeader {
                type_: cmd::RESOURCE_CREATE_2D,
                ..Default::default()
            },
            resource_id: self.resource_id,
            format: self.virtio_format,
            width: self.width,
            height: self.height,
        };

        let mut resp = CtrlHeader::default();
        self.send_command(&create_cmd, &mut resp)?;

        if resp.type_ != resp::OK_NODATA {
            return Err("Failed to create resource");
        }

        // Allocate framebuffer
        let fb_size = (self.width * self.height * self.bytes_per_pixel) as usize;
        let fb = alloc::vec![0u8; fb_size].into_boxed_slice();
        let fb_virt = fb.as_ptr() as u64;

        // DMA backing address: PCI needs physical, MMIO uses virtual
        // — GlassSignal: the device sees silicon, not kernel address space
        let fb_dma = if self.transport.is_some() {
            fb_virt - PHYS_MAP_BASE
        } else {
            fb_virt
        };

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
                addr: fb_dma,
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
    /// — GlassSignal: push bytes through the virtqueue pipeline, wait for the echo
    fn send_command<C, R>(&self, cmd: &C, resp: &mut R) -> Result<(), &'static str> {
        let cmd_size = core::mem::size_of::<C>();
        let resp_size = core::mem::size_of::<R>();
        let is_pci = self.transport.is_some();

        // Setup descriptors (use atomic operations for thread-safety)
        let desc_idx = (self.next_desc.fetch_add(2, Ordering::SeqCst) % self.queue_size).into();

        unsafe {
            // For PCI transport, descriptor addresses must be physical
            let cmd_addr = if is_pci {
                cmd as *const C as u64 - PHYS_MAP_BASE
            } else {
                cmd as *const C as u64
            };
            let resp_addr = if is_pci {
                resp as *mut R as u64 - PHYS_MAP_BASE
            } else {
                resp as *mut R as u64
            };

            // Command descriptor
            let desc0 = &mut *self.descriptors.add(desc_idx);
            desc0.addr = cmd_addr;
            desc0.len = cmd_size as u32;
            desc0.flags = VIRTQ_DESC_F_NEXT;
            desc0.next = ((desc_idx as u16 + 1) % self.queue_size) as u16;

            // Response descriptor
            let desc1 = &mut *self
                .descriptors
                .add(((desc_idx as u16 + 1) % self.queue_size) as usize);
            desc1.addr = resp_addr;
            desc1.len = resp_size as u32;
            desc1.flags = VIRTQ_DESC_F_WRITE;
            desc1.next = 0;

            // Add to available ring
            let avail = &mut *self.available;
            let avail_idx = avail.idx;
            avail.ring[(avail_idx % self.queue_size) as usize] = desc_idx as u16;
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            write_volatile(&mut avail.idx, avail_idx.wrapping_add(1));
        }

        // Notify device — PCI uses transport, MMIO uses register write
        if let Some(ref t) = self.transport {
            t.notify_queue(VIRTIO_GPU_CONTROLQ as u16);
        } else {
            self.write_reg(VIRTIO_MMIO_QUEUE_NOTIFY, VIRTIO_GPU_CONTROLQ);
        }

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

    /// Get framebuffer info
    pub fn framebuffer_info(&self) -> Option<FramebufferInfo> {
        self.framebuffer.as_ref().map(|fb| FramebufferInfo {
            base: fb.as_ptr() as usize,
            size: fb.len(),
            width: self.width,
            height: self.height,
            stride: self.width * self.bytes_per_pixel,
            format: self.pixel_format,
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
        self.pixel_format
    }

    fn stride(&self) -> u32 {
        self.width * self.bytes_per_pixel
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
        // — GlassSignal: full-screen fallback — callers should prefer flush_region
        self.flush_region(0, 0, self.width, self.height);
    }

    fn flush_region(&self, x: u32, y: u32, w: u32, h: u32) {
        // — GlassSignal: surgical transfer — only the pixels that actually changed
        if self.framebuffer.is_none() {
            return;
        }

        // Transfer dirty region to host
        let transfer_cmd = TransferToHost2d {
            hdr: CtrlHeader {
                type_: cmd::TRANSFER_TO_HOST_2D,
                ..Default::default()
            },
            r: Rect {
                x,
                y,
                width: w,
                height: h,
            },
            offset: 0,
            resource_id: self.resource_id,
            padding: 0,
        };

        let mut resp = CtrlHeader::default();
        let _ = self.send_command(&transfer_cmd, &mut resp);

        // Flush that region to display
        let flush_cmd = ResourceFlush {
            hdr: CtrlHeader {
                type_: cmd::RESOURCE_FLUSH,
                ..Default::default()
            },
            r: Rect {
                x,
                y,
                width: w,
                height: h,
            },
            resource_id: self.resource_id,
            padding: 0,
        };

        let _ = self.send_command(&flush_cmd, &mut resp);
    }
}

unsafe impl Send for VirtioGpu {}
unsafe impl Sync for VirtioGpu {}

/// Global VirtIO GPU instance
static VIRTIO_GPU: Mutex<Option<VirtioGpu>> = Mutex::new(None);

/// Initialize VirtIO GPU from PCI device
/// — GlassSignal: from PCI bus to framebuffer in one function call
pub fn init_from_pci(pci_dev: &PciDevice) -> Result<(), &'static str> {
    let mut gpu = VirtioGpu::from_pci(pci_dev).ok_or("VirtIO GPU PCI probe failed")?;
    gpu.init()?;

    // Initialize framebuffer subsystem
    if let Some(info) = gpu.framebuffer_info() {
        fb::init(info);
    }

    // Register mode setter so fb userspace can switch
    fb::mode::set_mode_setter(set_mode_from_fb);

    *VIRTIO_GPU.lock() = Some(gpu);
    Ok(())
}

/// Initialize VirtIO GPU from MMIO address
pub fn init(mmio_base: usize) -> Result<(), &'static str> {
    let mut gpu = VirtioGpu::probe(mmio_base).ok_or("VirtIO GPU not found")?;
    gpu.init()?;

    // Initialize framebuffer subsystem
    if let Some(info) = gpu.framebuffer_info() {
        fb::init(info);
    }

    // Register mode setter so fb userspace can switch
    fb::mode::set_mode_setter(set_mode_from_fb);

    *VIRTIO_GPU.lock() = Some(gpu);
    Ok(())
}

fn set_mode_from_fb(mode: &VideoMode) -> Option<FramebufferInfo> {
    let mut guard = VIRTIO_GPU.lock();
    let gpu = guard.as_mut()?;
    if gpu.display_count == 0 {
        let _ = gpu.get_display_info();
    }
    if gpu.display_count == 0 {
        return None;
    }
    gpu.set_mode(0, mode.width, mode.height).ok()
}

/// Flush the display
pub fn flush() {
    if let Some(ref gpu) = *VIRTIO_GPU.lock() {
        gpu.flush();
    }
}

/// Get display dimensions
pub fn dimensions() -> Option<(u32, u32)> {
    VIRTIO_GPU
        .lock()
        .as_ref()
        .map(|gpu| (gpu.width, gpu.height))
}

// ============================================================================
// PciDriver Implementation for Dynamic Driver Loading
// ============================================================================
// — EchoFrame: display driver, auto-probed

use driver_core::{PciDriver, PciDeviceId, DriverError, DriverBindingData};

/// Device ID table for VirtIO GPU devices
static VIRTIO_GPU_IDS: &[PciDeviceId] = &[
    PciDeviceId::new(pci::vendor::VIRTIO, pci::virtio_modern::GPU),   // Modern only
];

/// VirtIO GPU driver for driver-core system
struct VirtioGpuDriver;

impl PciDriver for VirtioGpuDriver {
    fn name(&self) -> &'static str {
        "virtio-gpu"
    }

    fn id_table(&self) -> &'static [PciDeviceId] {
        VIRTIO_GPU_IDS
    }

    fn probe(&self, dev: &pci::PciDevice, _id: &PciDeviceId) -> Result<DriverBindingData, DriverError> {
        // SAFETY: PCI device is valid and matches our ID table
        // Only initialize if no framebuffer is active
        if fb::is_initialized() {
            return Err(DriverError::AlreadyBound);
        }

        let _ = unsafe { init_from_pci(dev) }
            .map_err(|_| DriverError::InitFailed)?;

        Ok(DriverBindingData::new(0))
    }

    unsafe fn remove(&self, _dev: &pci::PciDevice, _binding_data: DriverBindingData) {
        // TODO: Implement proper GPU device removal
    }
}

/// Static driver instance for registration
static VIRTIO_GPU_DRIVER: VirtioGpuDriver = VirtioGpuDriver;

// Register driver via compile-time linker section
driver_core::register_pci_driver!(VIRTIO_GPU_DRIVER);
