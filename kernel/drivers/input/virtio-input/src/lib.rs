//! VirtIO Input Device Driver
//!
//! — InputShade: Rebuilt from the ground up. The old driver confused PCI transport
//! with MMIO transport, passed virtual addresses to hardware, and never wired
//! interrupts. This version uses VirtioPciTransport (same as virtio-gpu) for
//! modern PCI devices, fixes all address translation, and bridges keyboard
//! events to the VT console layer so the shell actually receives keystrokes.

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{fence, Ordering};
use input::{InputDeviceInfo, InputDeviceType, KeyValue};
use pci;
use spin::Mutex;

/// VirtIO input device configuration select values
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioInputConfigSelect {
    IdName = 0x01,
    IdSerial = 0x02,
    IdDevids = 0x03,
    PropBits = 0x10,
    EvBits = 0x11,
    AbsInfo = 0x12,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtioInputDevids {
    pub bustype: u16,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
}

#[repr(C)]
pub struct VirtioInputConfig {
    pub select: u8,
    pub subsel: u8,
    pub size: u8,
    _reserved: [u8; 5],
    pub data: [u8; 128],
}

/// VirtIO input event — 8 bytes, matches Linux evdev
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtioInputEvent {
    pub event_type: u16,
    pub code: u16,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioInputType {
    Keyboard,
    Mouse,
    Tablet,
    Generic,
}

// — InputShade: virtqueue descriptor table, spec §2.6. The unholy trinity of
// desc/avail/used that every virtio driver must get right or hardware goes brrr.

#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 256],
}

#[repr(C)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 256],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

const VIRTQ_DESC_F_WRITE: u16 = 2;
const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;
const EVENT_QUEUE: u16 = 0;
const STATUS_QUEUE: u16 = 1;

const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;

/// — InputShade: physical-map virtual address for a known physical address.
#[inline]
fn phys_to_virt(phys: u64) -> u64 {
    phys + PHYS_MAP_BASE
}

/// VirtIO input device driver
/// — InputShade: one instance per PCI input device (keyboard, mouse, tablet).
/// All DMA buffers are allocated from the physical frame allocator and accessed
/// through the direct physical map — kernel heap addresses are NOT valid for DMA.
pub struct VirtioInput {
    transport: pci::VirtioPciTransport,
    config: *mut VirtioInputConfig,
    event_desc: *mut VirtqDesc,
    event_avail: *mut VirtqAvail,
    event_used: *mut VirtqUsed,
    event_size: u16,
    event_last_used: u16,
    /// — InputShade: base pointer into physical-map region, NOT heap.
    /// Each event buffer is at event_buf_base + i * sizeof(VirtioInputEvent).
    event_buf_base: *mut VirtioInputEvent,
    status_desc: *mut VirtqDesc,
    status_avail: *mut VirtqAvail,
    status_used: *mut VirtqUsed,
    status_size: u16,
    name: String,
    device_type: VirtioInputType,
    devids: VirtioInputDevids,
    device_id: Option<usize>,
}

// ============================================================================
// Public API — matches the interface init.rs and console.rs expect
// ============================================================================

static VIRTIO_INPUT_DEVICES: Mutex<Vec<VirtioInput>> = Mutex::new(Vec::new());

/// Probe all virtio-input devices on the PCI bus
/// — InputShade: called once during boot from init.rs
pub fn probe_all_pci() -> usize {
    // — InputShade: trace PCI scan results for debugging
    let all_devs = pci::devices();
    unsafe {
        let msg = b"[VIRTIO-INPUT] PCI devices total: ";
        for &b in msg.iter() {
            while (core::ptr::read_volatile(&0x3FDu16 as *const _ as *const u8) & 0x20) == 0 {
                core::arch::asm!("in al, dx", out("al") _, in("dx") 0x3FDu16, options(nomem, nostack, preserves_flags));
            }
            core::arch::asm!("out dx, al", in("al") b, in("dx") 0x3F8u16, options(nomem, nostack, preserves_flags));
        }
    }
    serial_write_num(all_devs.len());
    serial_write_crlf();

    // Log virtio devices specifically
    for d in &all_devs {
        if d.vendor_id == 0x1AF4 {
            serial_write_str(b"[VIRTIO-INPUT] VirtIO dev ");
            serial_write_hex16(d.device_id);
            serial_write_str(b" class=");
            serial_write_hex8(d.class_code);
            serial_write_str(b" sub=");
            serial_write_hex8(d.subclass);
            serial_write_str(b" is_input=");
            serial_write_str(if d.is_virtio_input() { b"Y" } else { b"N" });
            serial_write_crlf();
        }
    }

    let pci_devices = pci::find_virtio_input();
    serial_write_str(b"[VIRTIO-INPUT] find_virtio_input returned ");
    serial_write_num(pci_devices.len());
    serial_write_crlf();

    let mut devices = VIRTIO_INPUT_DEVICES.lock();
    let mut count = 0;

    for pci_dev in pci_devices {
        serial_write_str(b"[VIRTIO-INPUT] Probing device 0x");
        serial_write_hex16(pci_dev.device_id);
        serial_write_crlf();
        if let Some(dev) = VirtioInput::from_pci(&pci_dev) {
            serial_write_str(b"[VIRTIO-INPUT] Device initialized OK\r\n");
            devices.push(dev);
            count += 1;
        } else {
            serial_write_str(b"[VIRTIO-INPUT] Device init FAILED\r\n");
        }
    }

    count
}

// — InputShade: raw serial helpers for ISR-safe debugging
fn serial_write_str(s: &[u8]) {
    for &b in s {
        unsafe {
            loop {
                let status: u8;
                core::arch::asm!("in al, dx", out("al") status, in("dx") 0x3FDu16, options(nomem, nostack, preserves_flags));
                if status & 0x20 != 0 { break; }
            }
            core::arch::asm!("out dx, al", in("al") b, in("dx") 0x3F8u16, options(nomem, nostack, preserves_flags));
        }
    }
}

fn serial_write_crlf() { serial_write_str(b"\r\n"); }

fn serial_write_num(n: usize) {
    if n == 0 { serial_write_str(b"0"); return; }
    let mut buf = [0u8; 10];
    let mut i = 0;
    let mut v = n;
    while v > 0 { buf[i] = b'0' + (v % 10) as u8; v /= 10; i += 1; }
    while i > 0 { i -= 1; serial_write_str(&[buf[i]]); }
}

fn serial_write_hex16(v: u16) {
    let nibbles = [(v >> 12) as u8 & 0xF, (v >> 8) as u8 & 0xF, (v >> 4) as u8 & 0xF, v as u8 & 0xF];
    for n in nibbles {
        let c = if n < 10 { b'0' + n } else { b'a' + n - 10 };
        serial_write_str(&[c]);
    }
}

fn serial_write_hex8(v: u8) {
    let nibbles = [(v >> 4) & 0xF, v & 0xF];
    for n in nibbles {
        let c = if n < 10 { b'0' + n } else { b'a' + n - 10 };
        serial_write_str(&[c]);
    }
}

fn serial_write_hex_usize(v: usize) {
    // Print as 16-digit hex for 64-bit addresses
    for i in (0..16).rev() {
        let n = ((v >> (i * 4)) & 0xF) as u8;
        let c = if n < 10 { b'0' + n } else { b'a' + n - 10 };
        serial_write_str(&[c]);
    }
}

/// Poll all VirtIO input devices for pending events
/// — InputShade: called from terminal_tick() at ~30 FPS as interrupt fallback
pub fn poll() {
    if let Some(mut devices) = VIRTIO_INPUT_DEVICES.try_lock() {
        for device in devices.iter_mut() {
            device.process_events();
        }
    }
}

/// Handle interrupt for all VirtIO input devices
pub fn handle_interrupt() {
    if let Some(mut devices) = VIRTIO_INPUT_DEVICES.try_lock() {
        for device in devices.iter_mut() {
            let isr = device.transport.read_isr();
            if isr & 1 != 0 {
                device.process_events();
            }
        }
    }
}

/// Get the number of initialized VirtIO input devices
pub fn device_count() -> usize {
    VIRTIO_INPUT_DEVICES.lock().len()
}

// ============================================================================
// Driver implementation
// ============================================================================

impl VirtioInput {
    /// Create and initialize a VirtIO input device from a PCI device
    /// — InputShade: uses VirtioPciTransport, same pattern as virtio-gpu.
    /// No more MMIO magic probing on PCI bars, no more virtual-as-physical.
    pub fn from_pci(pci_dev: &pci::PciDevice) -> Option<Self> {
        if !pci_dev.is_virtio_input() {
            serial_write_str(b"[VIRTIO-INPUT] not virtio input, skip\r\n");
            return None;
        }

        pci::enable_bus_master(pci_dev.address);
        pci::enable_memory_space(pci_dev.address);

        serial_write_str(b"[VIRTIO-INPUT] finding caps...\r\n");
        let caps = pci::find_virtio_caps(pci_dev);
        serial_write_str(b"[VIRTIO-INPUT] from_caps...\r\n");
        let transport = match pci::VirtioPciTransport::from_caps(pci_dev, &caps) {
            Some(t) => {
                serial_write_str(b"[VIRTIO-INPUT] transport OK\r\n");
                t
            }
            None => {
                serial_write_str(b"[VIRTIO-INPUT] transport FAILED (no caps?)\r\n");
                return None;
            }
        };

        let config = if transport.device_cfg != 0 {
            serial_write_str(b"[VIRTIO-INPUT] device_cfg OK\r\n");
            transport.device_cfg as *mut VirtioInputConfig
        } else {
            serial_write_str(b"[VIRTIO-INPUT] device_cfg is 0 - FAIL\r\n");
            return None;
        };

        let mut dev = VirtioInput {
            transport,
            config,
            event_desc: core::ptr::null_mut(),
            event_avail: core::ptr::null_mut(),
            event_used: core::ptr::null_mut(),
            event_size: 0,
            event_last_used: 0,
            event_buf_base: core::ptr::null_mut(),
            status_desc: core::ptr::null_mut(),
            status_avail: core::ptr::null_mut(),
            status_used: core::ptr::null_mut(),
            status_size: 0,
            name: String::new(),
            device_type: VirtioInputType::Generic,
            devids: VirtioInputDevids::default(),
            device_id: None,
        };

        // VirtIO init sequence (spec 3.1.1)
        serial_write_str(b"[VIRTIO-INPUT] common=0x");
        serial_write_hex_usize(dev.transport.common);
        serial_write_str(b" notify=0x");
        serial_write_hex_usize(dev.transport.notify);
        serial_write_str(b" device_cfg=0x");
        serial_write_hex_usize(dev.transport.device_cfg);
        serial_write_str(b"\r\n");
        serial_write_str(b"[VIRTIO-INPUT] writing status 0 (reset)...\r\n");
        dev.transport.write_status(0);
        dev.transport.write_status(VIRTIO_STATUS_ACKNOWLEDGE);
        dev.transport
            .write_status(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER);

        // Feature negotiation
        let features0 = dev.transport.read_device_features(0);
        let features1 = dev.transport.read_device_features(1);
        dev.transport.write_driver_features(0, features0);
        dev.transport.write_driver_features(1, features1 & 0x1);

        dev.transport.write_status(
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK,
        );
        let status = dev.transport.read_status();
        if status & VIRTIO_STATUS_FEATURES_OK == 0 {
            serial_write_str(b"[VIRTIO-INPUT] FEATURES_OK not set - FAIL\r\n");
            dev.transport.write_status(128);
            return None;
        }
        serial_write_str(b"[VIRTIO-INPUT] features OK\r\n");

        dev.read_device_config();

        serial_write_str(b"[VIRTIO-INPUT] init event queue...\r\n");
        if dev.init_event_queue().is_err() {
            serial_write_str(b"[VIRTIO-INPUT] event queue init FAILED\r\n");
            dev.transport.write_status(128);
            return None;
        }
        serial_write_str(b"[VIRTIO-INPUT] event queue OK, init status queue...\r\n");
        let _ = dev.init_status_queue();

        dev.transport.write_status(
            VIRTIO_STATUS_ACKNOWLEDGE
                | VIRTIO_STATUS_DRIVER
                | VIRTIO_STATUS_FEATURES_OK
                | VIRTIO_STATUS_DRIVER_OK,
        );

        let device_type = match dev.device_type {
            VirtioInputType::Keyboard => InputDeviceType::Keyboard,
            VirtioInputType::Mouse => InputDeviceType::Mouse,
            VirtioInputType::Tablet => InputDeviceType::Tablet,
            VirtioInputType::Generic => InputDeviceType::Unknown,
        };
        let info = InputDeviceInfo {
            name: dev.name.clone(),
            phys: String::from("virtio-pci"),
            uniq: String::new(),
            device_type,
            vendor: dev.devids.vendor,
            product: dev.devids.product,
            version: dev.devids.version,
        };
        dev.device_id = Some(input::register_device_info(info));

        Some(dev)
    }

    fn read_device_config(&mut self) {
        unsafe {
            write_volatile(&mut (*self.config).select, VirtioInputConfigSelect::IdName as u8);
            write_volatile(&mut (*self.config).subsel, 0);
            fence(Ordering::SeqCst);

            let size = read_volatile(&(*self.config).size) as usize;
            if size > 0 && size <= 128 {
                let mut buf = [0u8; 128];
                for i in 0..size {
                    buf[i] = read_volatile(&(*self.config).data[i]);
                }
                if let Ok(name) = core::str::from_utf8(&buf[..size]) {
                    self.name = String::from(name.trim_end_matches('\0'));
                }
            }

            write_volatile(
                &mut (*self.config).select,
                VirtioInputConfigSelect::IdDevids as u8,
            );
            write_volatile(&mut (*self.config).subsel, 0);
            fence(Ordering::SeqCst);

            let size = read_volatile(&(*self.config).size) as usize;
            if size >= core::mem::size_of::<VirtioInputDevids>() {
                self.devids.bustype =
                    read_volatile(&(*self.config).data[0] as *const u8 as *const u16);
                self.devids.vendor =
                    read_volatile(&(*self.config).data[2] as *const u8 as *const u16);
                self.devids.product =
                    read_volatile(&(*self.config).data[4] as *const u8 as *const u16);
                self.devids.version =
                    read_volatile(&(*self.config).data[6] as *const u8 as *const u16);
            }

            write_volatile(
                &mut (*self.config).select,
                VirtioInputConfigSelect::EvBits as u8,
            );
            write_volatile(&mut (*self.config).subsel, 1);
            fence(Ordering::SeqCst);
            let size = read_volatile(&(*self.config).size) as usize;
            if size > 4 {
                let byte4 = read_volatile(&(*self.config).data[4]);
                if byte4 != 0 {
                    self.device_type = VirtioInputType::Keyboard;
                    return;
                }
            }

            write_volatile(
                &mut (*self.config).select,
                VirtioInputConfigSelect::EvBits as u8,
            );
            write_volatile(&mut (*self.config).subsel, 2);
            fence(Ordering::SeqCst);
            if read_volatile(&(*self.config).size) > 0 {
                self.device_type = VirtioInputType::Mouse;
                return;
            }

            write_volatile(
                &mut (*self.config).select,
                VirtioInputConfigSelect::EvBits as u8,
            );
            write_volatile(&mut (*self.config).subsel, 3);
            fence(Ordering::SeqCst);
            if read_volatile(&(*self.config).size) > 0 {
                self.device_type = VirtioInputType::Tablet;
            }
        }
    }

    fn init_event_queue(&mut self) -> Result<(), &'static str> {
        use mm_manager::mm;
        use mm_traits::FrameAllocator;

        self.transport.select_queue(EVENT_QUEUE);
        let max_size = self.transport.queue_max_size();
        if max_size == 0 {
            return Err("Event queue not available");
        }
        let size = max_size.min(64);
        self.event_size = size;
        self.transport.set_queue_size(size);

        // — InputShade: DMA buffers MUST come from the physical frame allocator, not the
        // kernel heap. The heap lives at KERNEL_VIRT_BASE (0xFFFF_FFFF_80xx), and
        // `addr - PHYS_MAP_BASE` gives a bogus ~128TB "physical" address. The frame
        // allocator returns REAL physical addresses; we access them via the direct map.
        let desc_bytes = size as usize * core::mem::size_of::<VirtqDesc>();
        let avail_bytes = core::mem::size_of::<VirtqAvail>();
        let used_bytes = core::mem::size_of::<VirtqUsed>();
        let ring_total = desc_bytes + avail_bytes + used_bytes;
        let ring_pages = (ring_total + 4095) / 4096;

        let ring_phys = mm()
            .alloc_contiguous(ring_pages)
            .map_err(|_| "alloc ring frames")?;
        let ring_phys_base = ring_phys.as_u64();
        let ring_virt_base = (PHYS_MAP_BASE + ring_phys_base) as *mut u8;

        // Zero the DMA region
        unsafe {
            core::ptr::write_bytes(ring_virt_base, 0, ring_pages * 4096);
        }

        // Layout: desc | avail | used (contiguous in physical memory)
        self.event_desc = ring_virt_base as *mut VirtqDesc;
        self.event_avail = unsafe { ring_virt_base.add(desc_bytes) } as *mut VirtqAvail;
        self.event_used = unsafe { ring_virt_base.add(desc_bytes + avail_bytes) } as *mut VirtqUsed;

        self.transport.set_queue_desc(ring_phys_base);
        self.transport
            .set_queue_avail(ring_phys_base + desc_bytes as u64);
        self.transport
            .set_queue_used(ring_phys_base + desc_bytes as u64 + avail_bytes as u64);
        self.transport.enable_queue();

        // Allocate event buffers from physical frames
        let buf_bytes = size as usize * core::mem::size_of::<VirtioInputEvent>();
        let buf_pages = (buf_bytes + 4095) / 4096;
        let buf_phys = mm()
            .alloc_contiguous(buf_pages)
            .map_err(|_| "alloc event buf frames")?;
        let buf_phys_base = buf_phys.as_u64();
        let buf_virt_base = (PHYS_MAP_BASE + buf_phys_base) as *mut VirtioInputEvent;

        unsafe {
            core::ptr::write_bytes(
                buf_virt_base as *mut u8,
                0,
                buf_pages * 4096,
            );
        }

        self.event_buf_base = buf_virt_base;

        // Fill descriptors and avail ring
        let event_sz = core::mem::size_of::<VirtioInputEvent>() as u64;
        for i in 0..size {
            let buf_phys_addr = buf_phys_base + (i as u64) * event_sz;

            unsafe {
                let desc = &mut *self.event_desc.add(i as usize);
                write_volatile(&mut desc.addr, buf_phys_addr);
                write_volatile(&mut desc.len, event_sz as u32);
                write_volatile(&mut desc.flags, VIRTQ_DESC_F_WRITE);
                write_volatile(&mut desc.next, 0);

                write_volatile(&mut (*self.event_avail).ring[i as usize], i);
            }
        }

        unsafe {
            fence(Ordering::Release);
            write_volatile(&mut (*self.event_avail).idx, size);
        }

        self.transport.notify_queue(EVENT_QUEUE);

        Ok(())
    }

    fn init_status_queue(&mut self) -> Result<(), &'static str> {
        use mm_manager::mm;
        use mm_traits::FrameAllocator;

        self.transport.select_queue(STATUS_QUEUE);
        let max_size = self.transport.queue_max_size();
        if max_size == 0 {
            return Err("Status queue not available");
        }
        let size = max_size.min(16);
        self.status_size = size;
        self.transport.set_queue_size(size);

        // — InputShade: same DMA-safe allocation as event queue
        let desc_bytes = size as usize * core::mem::size_of::<VirtqDesc>();
        let avail_bytes = core::mem::size_of::<VirtqAvail>();
        let used_bytes = core::mem::size_of::<VirtqUsed>();
        let ring_total = desc_bytes + avail_bytes + used_bytes;
        let ring_pages = (ring_total + 4095) / 4096;

        let ring_phys = mm()
            .alloc_contiguous(ring_pages)
            .map_err(|_| "alloc status ring frames")?;
        let ring_phys_base = ring_phys.as_u64();
        let ring_virt_base = (PHYS_MAP_BASE + ring_phys_base) as *mut u8;

        unsafe {
            core::ptr::write_bytes(ring_virt_base, 0, ring_pages * 4096);
        }

        self.status_desc = ring_virt_base as *mut VirtqDesc;
        self.status_avail = unsafe { ring_virt_base.add(desc_bytes) } as *mut VirtqAvail;
        self.status_used =
            unsafe { ring_virt_base.add(desc_bytes + avail_bytes) } as *mut VirtqUsed;

        self.transport.set_queue_desc(ring_phys_base);
        self.transport
            .set_queue_avail(ring_phys_base + desc_bytes as u64);
        self.transport
            .set_queue_used(ring_phys_base + desc_bytes as u64 + avail_bytes as u64);
        self.transport.enable_queue();

        Ok(())
    }

    fn process_events(&mut self) {
        let mut had_events = false;

        loop {
            let used_idx = unsafe { read_volatile(&(*self.event_used).idx) };
            if self.event_last_used == used_idx {
                break;
            }

            let ring_idx = (self.event_last_used % self.event_size) as usize;
            let used_elem = unsafe { read_volatile(&(*self.event_used).ring[ring_idx]) };
            let desc_idx = used_elem.id as usize;

            if desc_idx < self.event_size as usize {
                // — InputShade: read event from physical-map-backed buffer
                let event = unsafe { read_volatile(self.event_buf_base.add(desc_idx)) };
                self.dispatch_event(&event);
            }

            let avail_idx = unsafe { read_volatile(&(*self.event_avail).idx) };
            unsafe {
                write_volatile(
                    &mut (*self.event_avail).ring[(avail_idx % self.event_size) as usize],
                    desc_idx as u16,
                );
                fence(Ordering::Release);
                write_volatile(&mut (*self.event_avail).idx, avail_idx.wrapping_add(1));
            }

            self.event_last_used = self.event_last_used.wrapping_add(1);
            had_events = true;
        }

        if had_events {
            self.transport.notify_queue(EVENT_QUEUE);
        }
    }

    /// Dispatch event to input subsystem AND console/VT layer
    /// — InputShade: reports to evdev AND uses shared kbd module for console bridge.
    /// No more inline keyboard conversion — kbd.rs handles that for ALL drivers.
    fn dispatch_event(&self, event: &VirtioInputEvent) {
        let device_id = match self.device_id {
            Some(id) => id,
            None => return,
        };

        match event.event_type {
            0 => {
                input::report_sync(device_id);
            }
            1 => {
                // EV_KEY
                let value = match event.value {
                    0 => KeyValue::Released,
                    1 => KeyValue::Pressed,
                    2 => KeyValue::Repeat,
                    _ => return,
                };
                input::report_key(device_id, event.code, value);

                // — InputShade: shared kbd module does modifier tracking, Ctrl codes,
                // ANSI escapes, keymap lookup, and VT push. Same path PS/2 uses.
                if self.device_type == VirtioInputType::Keyboard {
                    let pressed = event.value == 1 || event.value == 2;
                    input::kbd::process_key_event(event.code, pressed);
                }
            }
            2 => {
                input::report_rel(device_id, event.code, event.value as i32);
            }
            3 => {
                input::report_abs(device_id, event.code, event.value as i32);
            }
            _ => {}
        }
    }

    pub fn device_type(&self) -> VirtioInputType {
        self.device_type
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

unsafe impl Send for VirtioInput {}
unsafe impl Sync for VirtioInput {}

// ============================================================================
// PciDriver Implementation for Dynamic Driver Loading
// ============================================================================
// — InputShade: keyboard and mouse, auto-discovered

use driver_core::{PciDriver, PciDeviceId, DriverError, DriverBindingData};

/// Device ID table for VirtIO input devices
static VIRTIO_INPUT_IDS: &[PciDeviceId] = &[
    PciDeviceId::new(pci::vendor::VIRTIO, pci::virtio_modern::INPUT),   // Modern only
];

/// VirtIO input driver for driver-core system
struct VirtioInputDriver;

impl PciDriver for VirtioInputDriver {
    fn name(&self) -> &'static str {
        "virtio-input"
    }

    fn id_table(&self) -> &'static [PciDeviceId] {
        VIRTIO_INPUT_IDS
    }

    fn probe(&self, dev: &pci::PciDevice, _id: &PciDeviceId) -> Result<DriverBindingData, DriverError> {
        // SAFETY: PCI device is valid and matches our ID table
        let device = unsafe { VirtioInput::from_pci(dev) }
            .ok_or(DriverError::InitFailed)?;

        // Add to internal device list (virtio-input uses its own registry)
        VIRTIO_INPUT_DEVICES.lock().push(device);

        // Return dummy binding data
        Ok(DriverBindingData::new(0))
    }

    unsafe fn remove(&self, _dev: &pci::PciDevice, _binding_data: DriverBindingData) {
        // TODO: Implement proper device removal from VIRTIO_INPUT_DEVICES
    }
}

/// Static driver instance for registration
static VIRTIO_INPUT_DRIVER: VirtioInputDriver = VirtioInputDriver;

// Register driver via compile-time linker section
driver_core::register_pci_driver!(VIRTIO_INPUT_DRIVER);
