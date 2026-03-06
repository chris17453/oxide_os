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
use core::cell::UnsafeCell;
use core::ptr::{read_volatile, write_volatile};
use fb::{Color, Framebuffer, FramebufferInfo, PixelFormat};
use pci::{PciDevice, VirtioPciTransport};
use spin::Mutex;

// — GlassSignal: shared virtio plumbing — one ring crate to stop the copy-paste bleeding
use virtio_core::status as dev_status;
use virtio_core::virtqueue::desc_flags;
use virtio_core::{phys_to_virt, virt_to_phys, Virtqueue};

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

/// — GlassSignal: queue size cap — virtio-core defines the ceiling, we take what the device gives
const QUEUE_SIZE: usize = virtio_core::virtqueue::MAX_QUEUE_SIZE;

/// Control virtqueue
const VIRTIO_GPU_CONTROLQ: u32 = 0;
/// Cursor virtqueue
const VIRTIO_GPU_CURSORQ: u32 = 1;

// — GlassSignal: descriptor structs, status bits, phys_to_virt — all live in virtio-core now
// no more copy-paste virtqueue boilerplate in every driver

/// Framebuffer backing memory — may be DMA-allocated (PCI) or heap-allocated (MMIO).
/// — GlassSignal: DMA memory from the frame allocator must NOT be freed by the heap allocator.
/// This wrapper prevents Box<[u8]> from calling dealloc on physical-map pointers.
struct FbBacking {
    /// Virtual address of the framebuffer data
    ptr: *mut u8,
    /// Size in bytes
    len: usize,
    /// Physical address (valid for PCI DMA, 0 for MMIO)
    phys: u64,
}

/// SAFETY: protected by outer VIRTIO_GPU mutex — GlassSignal
unsafe impl Send for FbBacking {}
unsafe impl Sync for FbBacking {}

/// VirtIO GPU device
/// — GlassSignal: pixel pipeline from PCI bus to scanout — now powered by shared virtqueue core
pub struct VirtioGpu {
    /// MMIO base address (0 when using PCI transport)
    mmio_base: usize,
    /// PCI transport (None for MMIO mode)
    transport: Option<VirtioPciTransport>,
    /// Control virtqueue — shared implementation from virtio-core
    /// UnsafeCell for interior mutability: Framebuffer trait needs &self for flush,
    /// but Virtqueue operations need &mut. The outer Mutex<Option<VirtioGpu>> ensures
    /// exclusive access at runtime — this just makes the borrow checker stop screaming.
    /// — GlassSignal: the cell is unsafe, but the lock is not
    controlq: UnsafeCell<Option<Virtqueue>>,
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
    /// Framebuffer backing memory — DMA-safe for PCI, heap for MMIO
    /// — GlassSignal: intentionally leaked on destroy — GPU framebuffer outlives mode switches
    framebuffer: Option<FbBacking>,
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
            controlq: UnsafeCell::new(None),
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
            controlq: UnsafeCell::new(None),
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
    /// — GlassSignal: reset, acknowledge, negotiate, ready — the sacred four-step
    fn init_pci_handshake(&mut self) -> Result<(), &'static str> {
        let t = self.transport.as_ref().ok_or("No PCI transport")?;

        // Reset
        t.write_status(0);
        t.write_status(dev_status::ACKNOWLEDGE);
        t.write_status(dev_status::ACKNOWLEDGE | dev_status::DRIVER);

        // Read features, accept none for basic 2D
        let _features = t.read_device_features(0);
        t.write_driver_features(0, 0);

        // Features OK
        t.write_status(
            dev_status::ACKNOWLEDGE | dev_status::DRIVER | dev_status::FEATURES_OK,
        );

        let status = t.read_status();
        if status & dev_status::FEATURES_OK == 0 {
            t.write_status(dev_status::FAILED);
            return Err("Features not accepted");
        }

        // Release the immutable borrow before calling &mut self method
        let _ = t;

        // Initialize control queue via PCI transport
        self.init_controlq_pci()?;

        // Driver ready
        let t = self.transport.as_ref().ok_or("No PCI transport")?;
        t.write_status(
            dev_status::ACKNOWLEDGE
                | dev_status::DRIVER
                | dev_status::FEATURES_OK
                | dev_status::DRIVER_OK,
        );

        Ok(())
    }

    /// MMIO transport VirtIO handshake (legacy path)
    /// — GlassSignal: same dance, MMIO flavor — registers instead of PCI config space
    fn init_mmio_handshake(&mut self) -> Result<(), &'static str> {
        self.write_reg(VIRTIO_MMIO_STATUS, 0);
        self.write_reg(VIRTIO_MMIO_STATUS, dev_status::ACKNOWLEDGE as u32);
        self.write_reg(
            VIRTIO_MMIO_STATUS,
            (dev_status::ACKNOWLEDGE | dev_status::DRIVER) as u32,
        );

        self.write_reg(VIRTIO_MMIO_DEVICE_FEATURES_SEL, 0);
        let _features = self.read_reg(VIRTIO_MMIO_DEVICE_FEATURES);

        self.write_reg(VIRTIO_MMIO_DRIVER_FEATURES_SEL, 0);
        self.write_reg(VIRTIO_MMIO_DRIVER_FEATURES, 0);

        self.write_reg(
            VIRTIO_MMIO_STATUS,
            (dev_status::ACKNOWLEDGE | dev_status::DRIVER | dev_status::FEATURES_OK) as u32,
        );

        let status = self.read_reg(VIRTIO_MMIO_STATUS);
        if status & dev_status::FEATURES_OK as u32 == 0 {
            self.write_reg(VIRTIO_MMIO_STATUS, dev_status::FAILED as u32);
            return Err("Features not accepted");
        }

        self.init_controlq()?;

        self.write_reg(
            VIRTIO_MMIO_STATUS,
            (dev_status::ACKNOWLEDGE
                | dev_status::DRIVER
                | dev_status::FEATURES_OK
                | dev_status::DRIVER_OK) as u32,
        );

        Ok(())
    }

    fn destroy_resource(&mut self) {
        // — GlassSignal: intentionally leak old backing memory on mode switch.
        // DMA-allocated frames can't be freed through the heap allocator,
        // and we'd need RESOURCE_DETACH_BACKING + frame dealloc for proper cleanup.
        // GPU framebuffer memory is tiny vs total RAM — acceptable leak.
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
        t.set_queue_size(size);

        // — GlassSignal: let virtio-core handle the ring allocation — no more artisanal malloc
        let queue = unsafe { Virtqueue::new(size) }
            .ok_or("Failed to allocate control virtqueue")?;

        // PCI transport needs physical addresses for virtqueue rings
        let (desc_phys, avail_phys, used_phys) = queue.physical_addresses();

        t.set_queue_desc(desc_phys);
        t.set_queue_avail(avail_phys);
        t.set_queue_used(used_phys);
        t.enable_queue();

        // SAFETY: we have &mut self so exclusive access is guaranteed
        unsafe { *self.controlq.get() = Some(queue); }

        Ok(())
    }

    /// Initialize control queue (MMIO path)
    /// — GlassSignal: MMIO rings — same virtqueue core, different address plumbing
    fn init_controlq(&mut self) -> Result<(), &'static str> {
        self.write_reg(VIRTIO_MMIO_QUEUE_SEL, VIRTIO_GPU_CONTROLQ);

        let max_size = self.read_reg(VIRTIO_MMIO_QUEUE_NUM_MAX) as u16;
        if max_size == 0 {
            return Err("Queue not available");
        }

        let size = max_size.min(64);
        self.write_reg(VIRTIO_MMIO_QUEUE_NUM, size as u32);

        // — GlassSignal: let virtio-core handle the ring allocation
        let queue = unsafe { Virtqueue::new(size) }
            .ok_or("Failed to allocate control virtqueue")?;

        let (desc_phys, avail_phys, used_phys) = queue.physical_addresses();

        self.write_reg(VIRTIO_MMIO_QUEUE_DESC_LOW, desc_phys as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_DESC_HIGH, (desc_phys >> 32) as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_AVAIL_LOW, avail_phys as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_AVAIL_HIGH, (avail_phys >> 32) as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_USED_LOW, used_phys as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_USED_HIGH, (used_phys >> 32) as u32);

        self.write_reg(VIRTIO_MMIO_QUEUE_READY, 1);

        // SAFETY: we have &mut self so exclusive access is guaranteed
        unsafe { *self.controlq.get() = Some(queue); }

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

        // Allocate framebuffer from physical frame allocator for DMA safety
        // — GlassSignal: heap addresses live at KERNEL_VIRT_BASE (0xFFFF_FFFF_8000_0000),
        // virt_to_phys on them gives ~128TB bogus addresses. Frame allocator gives
        // real physical addresses accessible through PHYS_MAP_BASE — hardware can DMA to them.
        let fb_size = (self.width * self.height * self.bytes_per_pixel) as usize;
        let num_pages = (fb_size + 4095) / 4096;

        let backing = if self.transport.is_some() {
            // PCI path: allocate from physical frame allocator for DMA
            let phys_addr = mm_manager::mm()
                .alloc_contiguous(num_pages)
                .map_err(|_| "Failed to alloc DMA frames for GPU framebuffer")?;
            let phys = phys_addr.as_u64();
            let virt = phys_to_virt(phys) as *mut u8;
            // Zero the memory
            unsafe { core::ptr::write_bytes(virt, 0, num_pages * 4096); }
            FbBacking { ptr: virt, len: fb_size, phys }
        } else {
            // MMIO path: heap allocation is fine (no DMA translation needed)
            let fb = alloc::vec![0u8; fb_size].into_boxed_slice();
            let ptr = fb.as_ptr() as *mut u8;
            let virt_addr = ptr as u64;
            // Leak the box — MMIO framebuffer outlives everything
            core::mem::forget(fb);
            FbBacking { ptr, len: fb_size, phys: virt_addr }
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
                addr: backing.phys,
                length: fb_size as u32,
                padding: 0,
            },
        };

        self.send_command(&attach_cmd, &mut resp)?;

        if resp.type_ != resp::OK_NODATA {
            return Err("Failed to attach backing");
        }

        // Set scanout
        // — GlassSignal: this is the moment of truth — does SET_SCANOUT steal VGA display?
        unsafe { os_log::write_str_raw("[VGPU] sending SET_SCANOUT scanout_id=0...\n"); }
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
        unsafe { os_log::write_str_raw("[VGPU] SET_SCANOUT response received\n"); }

        if resp.type_ != resp::OK_NODATA {
            return Err("Failed to set scanout");
        }

        self.framebuffer = Some(backing);
        Ok(())
    }

    /// Send a command and wait for response
    /// — GlassSignal: push bytes through the virtqueue pipeline, wait for the echo
    ///
    /// Takes &self (not &mut self) so Framebuffer trait flush methods can call it.
    /// SAFETY: the outer VIRTIO_GPU Mutex guarantees exclusive access at runtime;
    /// we use UnsafeCell to get the mutable Virtqueue reference the borrow checker won't give us.
    fn send_command<C, R>(&self, cmd: &C, resp: &mut R) -> Result<(), &'static str> {
        let cmd_size = core::mem::size_of::<C>();
        let resp_size = core::mem::size_of::<R>();
        let is_pci = self.transport.is_some();

        // SAFETY: caller holds VIRTIO_GPU mutex — no concurrent access possible
        let queue = unsafe { &mut *self.controlq.get() }
            .as_mut()
            .ok_or("Control queue not initialized")?;

        // — GlassSignal: allocate two descriptors — command out, response back
        let desc0 = queue.alloc_desc().ok_or("No free descriptors for cmd")?;
        let desc1 = queue.alloc_desc().ok_or("No free descriptors for resp")?;

        // For PCI transport, descriptor addresses must be physical
        let cmd_addr = if is_pci {
            virt_to_phys(cmd as *const C as u64)
        } else {
            cmd as *const C as u64
        };
        let resp_addr = if is_pci {
            virt_to_phys(resp as *mut R as u64)
        } else {
            resp as *mut R as u64
        };

        unsafe {
            // Command descriptor — device reads this
            queue.write_desc(desc0, cmd_addr, cmd_size as u32, desc_flags::NEXT, desc1);

            // Response descriptor — device writes here
            queue.write_desc(desc1, resp_addr, resp_size as u32, desc_flags::WRITE, 0);
        }

        // Submit chain to available ring
        queue.add_available(desc0);

        // Notify device — PCI uses transport, MMIO uses register write
        if let Some(ref t) = self.transport {
            t.notify_queue(VIRTIO_GPU_CONTROLQ as u16);
        } else {
            self.write_reg(VIRTIO_MMIO_QUEUE_NOTIFY, VIRTIO_GPU_CONTROLQ);
        }

        // — GlassSignal: spin until the device responds, but with a hard ceiling.
        // We're often called from terminal::write() with the TERMINAL spinlock held.
        // An unbounded spin here deadlocks every future console write. The GOP
        // framebuffer is memory-mapped and visible without GPU commands, so a
        // dropped flush is cosmetic only — not a hang. — NeonRoot
        let mut spins = 0u32;
        const MAX_SPINS: u32 = 100_000;
        loop {
            if queue.has_completed() {
                if let Some((head, _len)) = queue.pop_used() {
                    queue.free_chain(head);
                }
                break;
            }
            spins += 1;
            if spins >= MAX_SPINS {
                // — GlassSignal: GPU didn't respond in time. Free the descriptor
                // chain to avoid leaking them, and bail. GOP framebuffer still
                // shows the data since blit_to_fb already did the memcpy. — NeonRoot
                queue.free_chain(desc0);
                return Err("GPU command timed out");
            }
            core::hint::spin_loop();
        }

        Ok(())
    }

    /// Get framebuffer info
    pub fn framebuffer_info(&self) -> Option<FramebufferInfo> {
        self.framebuffer.as_ref().map(|fb| FramebufferInfo {
            base: fb.ptr as usize,
            size: fb.len,
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
            .map(|fb| fb.ptr)
            .unwrap_or(core::ptr::null_mut())
    }

    fn size(&self) -> usize {
        self.framebuffer.as_ref().map(|fb| fb.len).unwrap_or(0)
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
/// — GlassSignal: probe the device but DON'T replace the framebuffer.
/// UEFI's GOP framebuffer is memory-mapped and works without TRANSFER_TO_HOST_2D.
/// Our SET_SCANOUT to a new resource doesn't take effect in QEMU — the display
/// continues showing UEFI's GOP memory. So we keep using the UEFI GOP buffer
/// and just store the GPU device for future mode-switching capability.
pub fn init_from_pci(pci_dev: &PciDevice) -> Result<(), &'static str> {
    // — GlassShift: If a working GOP framebuffer already exists (set up by OVMF),
    // skip VirtIO-GPU init entirely. The problem: setup_framebuffer() sends
    // SET_SCANOUT which binds a NEW blank resource to scanout 0, replacing
    // whatever OVMF was displaying. If OVMF used VirtIO-GPU for GOP, our
    // SET_SCANOUT steals the display and shows a black screen. The kernel
    // keeps writing to the old GOP address but QEMU displays our empty resource.
    // This is the root cause of the "256M blank screen" — at lower RAM,
    // OVMF picks VirtIO-GPU for GOP instead of VGA std.
    if fb::framebuffer().is_some() {
        unsafe { os_log::write_str_raw("[VGPU] GOP framebuffer already active, skipping init (SET_SCANOUT would steal display)\n"); }
        return Ok(());
    }

    unsafe { os_log::write_str_raw("[VGPU] no GOP fb, initializing VirtIO-GPU...\n"); }
    let mut gpu = VirtioGpu::from_pci(pci_dev).ok_or("VirtIO GPU PCI probe failed")?;
    gpu.init()?;
    unsafe { os_log::write_str_raw("[VGPU] init done, SET_SCANOUT sent\n"); }

    // — GlassSignal: DON'T call fb::init() — the UEFI GOP framebuffer is the one
    // QEMU actually displays. Our VirtIO-GPU resource lives in a DMA buffer that
    // QEMU ignores. The UEFI GOP fb is memory-mapped: writes appear immediately,
    // no TRANSFER_TO_HOST_2D or RESOURCE_FLUSH needed.

    // Register mode setter for future use
    fb::mode::set_mode_setter(set_mode_from_fb);

    // Store the GPU for future use (mode switching, acceleration)
    *VIRTIO_GPU.lock() = Some(gpu);
    unsafe { os_log::write_str_raw("[VGPU] stored, init complete\n"); }

    Ok(())
}

/// — GlassSignal: Take over the display from GOP. Called from kernel init when
/// we explicitly want VirtIO-GPU to own the display pipeline. Unlike init_from_pci
/// (which skips if GOP exists), this FORCES initialization and replaces the
/// framebuffer. Only call this when Bochs display isn't available.
pub fn take_over_display() -> Result<FramebufferInfo, &'static str> {
    // — GlassSignal: Need a VirtIO-GPU device on the PCI bus.
    // If init_from_pci already ran (and skipped), the device is enumerated
    // but VIRTIO_GPU is empty. We need to find it again.
    let devices = pci::devices();
    let gpu_dev = devices
        .iter()
        .find(|d| d.is_virtio_gpu())
        .ok_or("No VirtIO-GPU device on PCI bus")?;

    unsafe {
        os_log::write_str_raw("[VGPU] Takeover: initializing VirtIO-GPU for display ownership\n");
    }

    let mut gpu = VirtioGpu::from_pci(gpu_dev).ok_or("VirtIO GPU PCI probe failed")?;
    gpu.init()?;

    let info = gpu
        .framebuffer_info()
        .ok_or("VirtIO-GPU framebuffer setup failed")?;

    // Register flush callback for TRANSFER_TO_HOST_2D
    fb::set_flush_callback(gpu_flush_region);

    *VIRTIO_GPU.lock() = Some(gpu);

    unsafe {
        os_log::write_str_raw("[VGPU] Takeover complete: ");
        os_log::write_u32_raw(info.width);
        os_log::write_str_raw("x");
        os_log::write_u32_raw(info.height);
        os_log::write_str_raw("\n");
    }

    Ok(info)
}

/// Flush callback — bridges LinearFramebuffer::flush_region() to VirtIO-GPU commands.
/// — GlassSignal: the pixel pipeline's last mile — guest memory to host scanout.
/// Uses try_lock because this may fire from ISR context (terminal tick).
fn gpu_flush_region(x: u32, y: u32, w: u32, h: u32) {
    if let Some(guard) = VIRTIO_GPU.try_lock() {
        if let Some(ref gpu) = *guard {
            <VirtioGpu as Framebuffer>::flush_region(gpu, x, y, w, h);
        }
    }
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
        // — GlassSignal: ALWAYS initialize VirtIO-GPU, even if UEFI GOP set up a boot framebuffer.
        // GOP provides early boot display; we take over with proper flush support.
        // Without our driver, QEMU's VirtIO-GPU display freezes at UEFI's last frame.
        init_from_pci(dev)
            .map_err(|_| DriverError::InitFailed)?;

        Ok(DriverBindingData::new(0))
    }

    unsafe fn remove(&self, _dev: &pci::PciDevice, _binding_data: DriverBindingData) {
        // — GlassSignal: GPU teardown — reset the device and release the controlq.
        // Framebuffer DMA pages are intentionally NOT freed — the display subsystem
        // may still reference them, and we can't unmap a live scanout safely.

        let mut guard = VIRTIO_GPU.lock();
        if let Some(ref mut gpu) = *guard {
            // Reset VirtIO device status — stops all DMA
            if let Some(ref transport) = gpu.transport {
                transport.write_status(0);
            }

            // Drop the controlq (Virtqueue::drop frees DMA ring pages)
            unsafe { *gpu.controlq.get() = None; }
        }

        // Drop the device struct
        *guard = None;
    }
}

/// Static driver instance for registration
static VIRTIO_GPU_DRIVER: VirtioGpuDriver = VirtioGpuDriver;

// Register driver via compile-time linker section
driver_core::register_pci_driver!(VIRTIO_GPU_DRIVER);
