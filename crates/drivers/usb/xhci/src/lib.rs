//! xHCI (USB 3.x) Host Controller Driver
//!
//! Implements the eXtensible Host Controller Interface for USB 3.x.

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::AtomicU8;
use spin::Mutex;
use usb::{
    DeviceSpeed, EndpointDescriptor, PortStatus, SetupPacket, TransferDirection, UsbError,
    UsbHostController, UsbResult,
};

/// xHCI capability registers
#[repr(C)]
struct CapRegs {
    caplength: u8,
    _rsvd: u8,
    hciversion: u16,
    hcsparams1: u32,
    hcsparams2: u32,
    hcsparams3: u32,
    hccparams1: u32,
    dboff: u32,
    rtsoff: u32,
    hccparams2: u32,
}

/// xHCI operational registers
#[repr(C)]
struct OpRegs {
    usbcmd: u32,
    usbsts: u32,
    pagesize: u32,
    _rsvd1: [u32; 2],
    dnctrl: u32,
    crcr: u64,
    _rsvd2: [u32; 4],
    dcbaap: u64,
    config: u32,
}

/// Port register set
#[repr(C)]
struct PortRegs {
    portsc: u32,
    portpmsc: u32,
    portli: u32,
    porthlpmc: u32,
}

/// Transfer Request Block
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Default)]
struct Trb {
    parameter: u64,
    status: u32,
    control: u32,
}

/// TRB types
mod trb_type {
    pub const NORMAL: u32 = 1;
    pub const SETUP_STAGE: u32 = 2;
    pub const DATA_STAGE: u32 = 3;
    pub const STATUS_STAGE: u32 = 4;
    pub const ISOCH: u32 = 5;
    pub const LINK: u32 = 6;
    pub const EVENT_DATA: u32 = 7;
    pub const NOOP: u32 = 8;
    pub const ENABLE_SLOT: u32 = 9;
    pub const DISABLE_SLOT: u32 = 10;
    pub const ADDRESS_DEVICE: u32 = 11;
    pub const CONFIGURE_EP: u32 = 12;
    pub const EVALUATE_CONTEXT: u32 = 13;
    pub const RESET_EP: u32 = 14;
    pub const STOP_EP: u32 = 15;
    pub const SET_TR_DEQUEUE: u32 = 16;
    pub const RESET_DEVICE: u32 = 17;
    pub const FORCE_EVENT: u32 = 18;
    pub const NEGOTIATE_BW: u32 = 19;
    pub const SET_LATENCY: u32 = 20;
    pub const GET_PORT_BW: u32 = 21;
    pub const FORCE_HEADER: u32 = 22;
    pub const NOOP_CMD: u32 = 23;
    pub const TRANSFER_EVENT: u32 = 32;
    pub const COMMAND_COMPLETION: u32 = 33;
    pub const PORT_STATUS_CHANGE: u32 = 34;
}

/// Completion codes
mod completion {
    pub const INVALID: u32 = 0;
    pub const SUCCESS: u32 = 1;
    pub const DATA_BUFFER: u32 = 2;
    pub const BABBLE: u32 = 3;
    pub const USB_TRANSACTION: u32 = 4;
    pub const TRB_ERROR: u32 = 5;
    pub const STALL: u32 = 6;
    pub const SHORT_PACKET: u32 = 13;
}

/// Device context
#[repr(C, align(64))]
struct DeviceContext {
    slot: SlotContext,
    endpoints: [EndpointContext; 31],
}

/// Slot context
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct SlotContext {
    data: [u32; 8],
}

/// Endpoint context
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct EndpointContext {
    data: [u32; 8],
}

/// Input context
#[repr(C, align(64))]
struct InputContext {
    control: InputControlContext,
    slot: SlotContext,
    endpoints: [EndpointContext; 31],
}

/// Input control context
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct InputControlContext {
    drop_flags: u32,
    add_flags: u32,
    _rsvd: [u32; 5],
    config: u32,
}

/// Transfer ring
struct TransferRing {
    trbs: Box<[Trb; 256]>,
    enqueue: usize,
    cycle: bool,
}

impl TransferRing {
    fn new() -> Self {
        TransferRing {
            trbs: Box::new([Trb::default(); 256]),
            enqueue: 0,
            cycle: true,
        }
    }

    fn enqueue_trb(&mut self, mut trb: Trb) -> *const Trb {
        if self.cycle {
            trb.control |= 1; // Cycle bit
        }

        self.trbs[self.enqueue] = trb;
        let ptr = &self.trbs[self.enqueue] as *const Trb;

        self.enqueue = (self.enqueue + 1) % 255;

        // Add link TRB at end
        if self.enqueue == 254 {
            let mut link = Trb::default();
            link.parameter = self.trbs.as_ptr() as u64;
            link.control = (trb_type::LINK << 10) | (1 << 1); // Toggle cycle
            if self.cycle {
                link.control |= 1;
            }
            self.trbs[254] = link;
            self.enqueue = 0;
            self.cycle = !self.cycle;
        }

        ptr
    }

    fn base(&self) -> u64 {
        self.trbs.as_ptr() as u64
    }
}

/// Event ring segment
#[repr(C)]
struct EventRingSegment {
    base: u64,
    size: u32,
    _rsvd: u32,
}

/// Event ring
struct EventRing {
    trbs: Box<[Trb; 256]>,
    segment: Box<EventRingSegment>,
    dequeue: usize,
    cycle: bool,
}

impl EventRing {
    fn new() -> Self {
        let trbs = Box::new([Trb::default(); 256]);
        let mut segment = Box::new(EventRingSegment {
            base: 0,
            size: 256,
            _rsvd: 0,
        });
        segment.base = trbs.as_ptr() as u64;

        EventRing {
            trbs,
            segment,
            dequeue: 0,
            cycle: true,
        }
    }

    fn dequeue_event(&mut self) -> Option<Trb> {
        let trb = self.trbs[self.dequeue];
        let cycle = (trb.control & 1) != 0;

        if cycle != self.cycle {
            return None;
        }

        self.dequeue = (self.dequeue + 1) % 256;
        if self.dequeue == 0 {
            self.cycle = !self.cycle;
        }

        Some(trb)
    }

    fn segment_table(&self) -> u64 {
        self.segment.as_ref() as *const EventRingSegment as u64
    }
}

/// xHCI controller
pub struct XhciController {
    /// MMIO base address
    base: usize,
    /// Capability registers
    cap_regs: *const CapRegs,
    /// Operational registers
    op_regs: *mut OpRegs,
    /// Port registers
    port_regs: *mut PortRegs,
    /// Runtime registers offset
    runtime_offset: usize,
    /// Doorbell offset
    doorbell_offset: usize,
    /// Max device slots
    max_slots: u8,
    /// Number of ports
    num_ports: u8,
    /// Device context base address array (with interior mutability)
    dcbaa: Mutex<Box<[u64; 256]>>,
    /// Device contexts
    contexts: Mutex<Vec<Option<Box<DeviceContext>>>>,
    /// Command ring
    command_ring: Mutex<TransferRing>,
    /// Event ring
    event_ring: Mutex<EventRing>,
    /// Transfer rings per slot/endpoint
    transfer_rings: Mutex<Vec<Vec<Option<TransferRing>>>>,
    /// Next slot ID
    next_slot: AtomicU8,
}

impl XhciController {
    /// Probe for xHCI controller at PCI address
    pub fn probe(base: usize) -> Option<Self> {
        let cap_regs = base as *const CapRegs;

        unsafe {
            let caplength = read_volatile(&(*cap_regs).caplength) as usize;
            let hcsparams1 = read_volatile(&(*cap_regs).hcsparams1);
            let dboff = read_volatile(&(*cap_regs).dboff) as usize;
            let rtsoff = read_volatile(&(*cap_regs).rtsoff) as usize;

            let max_slots = (hcsparams1 & 0xFF) as u8;
            let num_ports = ((hcsparams1 >> 24) & 0xFF) as u8;

            let op_regs = (base + caplength) as *mut OpRegs;
            let port_regs = (base + caplength + 0x400) as *mut PortRegs;

            Some(XhciController {
                base,
                cap_regs,
                op_regs,
                port_regs,
                runtime_offset: rtsoff,
                doorbell_offset: dboff,
                max_slots,
                num_ports,
                dcbaa: Mutex::new(Box::new([0u64; 256])),
                contexts: Mutex::new(Vec::new()),
                command_ring: Mutex::new(TransferRing::new()),
                event_ring: Mutex::new(EventRing::new()),
                transfer_rings: Mutex::new(Vec::new()),
                next_slot: AtomicU8::new(1),
            })
        }
    }

    /// Initialize controller
    pub fn init(&mut self) -> Result<(), &'static str> {
        unsafe {
            // Stop controller
            let cmd = read_volatile(&(*self.op_regs).usbcmd);
            write_volatile(&mut (*self.op_regs).usbcmd, cmd & !1);

            // Wait for halt
            while read_volatile(&(*self.op_regs).usbsts) & 1 == 0 {
                core::hint::spin_loop();
            }

            // Reset
            write_volatile(&mut (*self.op_regs).usbcmd, 2);
            while read_volatile(&(*self.op_regs).usbcmd) & 2 != 0 {
                core::hint::spin_loop();
            }

            // Set max device slots
            write_volatile(&mut (*self.op_regs).config, self.max_slots as u32);

            // Set DCBAA
            write_volatile(
                &mut (*self.op_regs).dcbaap,
                self.dcbaa.lock().as_ptr() as u64,
            );

            // Set command ring
            let cmd_ring = self.command_ring.lock();
            write_volatile(&mut (*self.op_regs).crcr, cmd_ring.base() | 1);
            drop(cmd_ring);

            // Set event ring (via runtime registers)
            let event_ring = self.event_ring.lock();
            let irs0 = (self.base + self.runtime_offset + 0x20) as *mut u64;

            // Event ring segment table size
            write_volatile((irs0 as *mut u32).add(2), 1);
            // Event ring dequeue pointer
            write_volatile(irs0.add(1), event_ring.trbs.as_ptr() as u64 | (1 << 3));
            // Event ring segment table base
            write_volatile(irs0.add(2), event_ring.segment_table());
            drop(event_ring);

            // Initialize contexts
            let mut contexts = self.contexts.lock();
            for _ in 0..=self.max_slots {
                contexts.push(None);
            }

            // Initialize transfer rings
            let mut rings = self.transfer_rings.lock();
            for _ in 0..=self.max_slots {
                let mut slot_rings = Vec::new();
                for _ in 0..32 {
                    slot_rings.push(None);
                }
                rings.push(slot_rings);
            }

            // Start controller
            let cmd = read_volatile(&(*self.op_regs).usbcmd);
            write_volatile(&mut (*self.op_regs).usbcmd, cmd | 1);

            // Wait for running
            while read_volatile(&(*self.op_regs).usbsts) & 1 != 0 {
                core::hint::spin_loop();
            }
        }

        Ok(())
    }

    /// Ring doorbell
    fn ring_doorbell(&self, slot: u8, endpoint: u8) {
        let doorbell = (self.base + self.doorbell_offset + (slot as usize * 4)) as *mut u32;
        unsafe {
            write_volatile(doorbell, endpoint as u32);
        }
    }

    /// Wait for command completion
    fn wait_command(&self) -> UsbResult<Trb> {
        let mut event_ring = self.event_ring.lock();

        for _ in 0..1000000 {
            if let Some(event) = event_ring.dequeue_event() {
                let trb_type = (event.control >> 10) & 0x3F;
                if trb_type == trb_type::COMMAND_COMPLETION {
                    let code = (event.status >> 24) & 0xFF;
                    if code == completion::SUCCESS {
                        return Ok(event);
                    } else {
                        return Err(UsbError::TransferError);
                    }
                }
            }
            core::hint::spin_loop();
        }

        Err(UsbError::Timeout)
    }

    /// Wait for transfer completion
    fn wait_transfer(&self, slot: u8) -> UsbResult<Trb> {
        let mut event_ring = self.event_ring.lock();

        for _ in 0..1000000 {
            if let Some(event) = event_ring.dequeue_event() {
                let trb_type = (event.control >> 10) & 0x3F;
                if trb_type == trb_type::TRANSFER_EVENT {
                    let event_slot = ((event.control >> 24) & 0xFF) as u8;
                    if event_slot == slot {
                        let code = (event.status >> 24) & 0xFF;
                        return match code {
                            completion::SUCCESS | completion::SHORT_PACKET => Ok(event),
                            completion::STALL => Err(UsbError::Stall),
                            completion::BABBLE => Err(UsbError::Babble),
                            completion::DATA_BUFFER => Err(UsbError::DataBuffer),
                            _ => Err(UsbError::TransferError),
                        };
                    }
                }
            }
            core::hint::spin_loop();
        }

        Err(UsbError::Timeout)
    }
}

impl UsbHostController for XhciController {
    fn name(&self) -> &str {
        "xHCI"
    }

    fn reset_port(&self, port: u8) -> UsbResult<()> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidEndpoint);
        }

        unsafe {
            let port_regs = self.port_regs.add(port as usize);
            let portsc = read_volatile(&(*port_regs).portsc);

            // Set port reset
            write_volatile(&mut (*port_regs).portsc, portsc | (1 << 4));

            // Wait for reset complete
            for _ in 0..100000 {
                let portsc = read_volatile(&(*port_regs).portsc);
                if portsc & (1 << 21) != 0 {
                    // Clear reset change
                    write_volatile(&mut (*port_regs).portsc, portsc | (1 << 21));
                    return Ok(());
                }
                core::hint::spin_loop();
            }
        }

        Err(UsbError::Timeout)
    }

    fn port_status(&self, port: u8) -> UsbResult<PortStatus> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidEndpoint);
        }

        unsafe {
            let port_regs = self.port_regs.add(port as usize);
            let portsc = read_volatile(&(*port_regs).portsc);

            let speed = match (portsc >> 10) & 0xF {
                1 => DeviceSpeed::Full,
                2 => DeviceSpeed::Low,
                3 => DeviceSpeed::High,
                4 => DeviceSpeed::Super,
                5 => DeviceSpeed::SuperPlus,
                _ => DeviceSpeed::Full,
            };

            Ok(PortStatus {
                connected: portsc & 1 != 0,
                enabled: portsc & (1 << 1) != 0,
                reset: portsc & (1 << 4) != 0,
                speed,
                power: portsc & (1 << 9) != 0,
            })
        }
    }

    fn enable_slot(&self) -> UsbResult<u8> {
        let trb = Trb {
            parameter: 0,
            status: 0,
            control: (trb_type::ENABLE_SLOT << 10),
        };

        let mut cmd_ring = self.command_ring.lock();
        cmd_ring.enqueue_trb(trb);
        drop(cmd_ring);

        self.ring_doorbell(0, 0);

        let event = self.wait_command()?;
        let slot = ((event.control >> 24) & 0xFF) as u8;

        Ok(slot)
    }

    fn disable_slot(&self, slot: u8) -> UsbResult<()> {
        let trb = Trb {
            parameter: 0,
            status: 0,
            control: (trb_type::DISABLE_SLOT << 10) | ((slot as u32) << 24),
        };

        let mut cmd_ring = self.command_ring.lock();
        cmd_ring.enqueue_trb(trb);
        drop(cmd_ring);

        self.ring_doorbell(0, 0);
        self.wait_command()?;

        Ok(())
    }

    fn address_device(&self, slot: u8, port: u8, speed: DeviceSpeed) -> UsbResult<u8> {
        // Create device context
        let context = Box::new(DeviceContext {
            slot: SlotContext::default(),
            endpoints: [EndpointContext::default(); 31],
        });

        // Create input context
        let mut input = Box::new(InputContext {
            control: InputControlContext {
                drop_flags: 0,
                add_flags: 3, // Slot + EP0
                _rsvd: [0; 5],
                config: 0,
            },
            slot: SlotContext::default(),
            endpoints: [EndpointContext::default(); 31],
        });

        // Set slot context
        let speed_val = match speed {
            DeviceSpeed::Low => 2,
            DeviceSpeed::Full => 1,
            DeviceSpeed::High => 3,
            DeviceSpeed::Super => 4,
            DeviceSpeed::SuperPlus => 5,
        };

        input.slot.data[0] = (1 << 27) | (speed_val << 20); // Entries = 1, speed
        input.slot.data[1] = (port as u32 + 1) << 16; // Root hub port

        // Set EP0 context
        let max_packet = speed.control_max_packet() as u32;
        input.endpoints[0].data[1] = (4 << 3) | (3 << 1); // EP type = control, cerr = 3

        // Create transfer ring for EP0
        let ep0_ring = TransferRing::new();
        input.endpoints[0].data[2] = (ep0_ring.base() & !0xF) as u32 | 1; // DCS = 1
        input.endpoints[0].data[3] = ((ep0_ring.base() >> 32) & 0xFFFFFFFF) as u32;
        input.endpoints[0].data[1] |= max_packet << 16;

        // Store context and ring
        self.dcbaa.lock()[slot as usize] = context.as_ref() as *const DeviceContext as u64;
        self.contexts.lock()[slot as usize] = Some(context);
        self.transfer_rings.lock()[slot as usize][0] = Some(ep0_ring);

        // Address device command
        let trb = Trb {
            parameter: input.as_ref() as *const InputContext as u64,
            status: 0,
            control: (trb_type::ADDRESS_DEVICE << 10) | ((slot as u32) << 24),
        };

        let mut cmd_ring = self.command_ring.lock();
        cmd_ring.enqueue_trb(trb);
        drop(cmd_ring);

        self.ring_doorbell(0, 0);
        self.wait_command()?;

        Ok(slot)
    }

    fn configure_endpoint(&self, _slot: u8, _endpoint: &EndpointDescriptor) -> UsbResult<()> {
        // Simplified - would need full endpoint configuration
        Ok(())
    }

    fn control_transfer(
        &self,
        slot: u8,
        request: SetupPacket,
        data: Option<&mut [u8]>,
    ) -> UsbResult<usize> {
        let mut rings = self.transfer_rings.lock();
        let ring = rings[slot as usize][0]
            .as_mut()
            .ok_or(UsbError::InvalidEndpoint)?;

        let direction = if request.request_type & 0x80 != 0 {
            1
        } else {
            0
        };

        // Setup stage TRB
        let setup_trb = Trb {
            parameter: unsafe { core::mem::transmute::<SetupPacket, u64>(request) },
            status: 8, // TRB transfer length = 8
            control: (trb_type::SETUP_STAGE << 10) | (1 << 6) | (direction << 16), // IDT, TRT
        };
        ring.enqueue_trb(setup_trb);

        // Data stage TRB (if any)
        let data_len = data.as_ref().map(|d| d.len()).unwrap_or(0);
        if data_len > 0 {
            let data_ptr = data.as_ref().map(|d| d.as_ptr() as u64).unwrap_or(0);
            let data_trb = Trb {
                parameter: data_ptr,
                status: data_len as u32,
                control: (trb_type::DATA_STAGE << 10) | (direction << 16),
            };
            ring.enqueue_trb(data_trb);
        }

        // Status stage TRB
        let status_direction = if data_len > 0 && direction == 1 { 0 } else { 1 };
        let status_trb = Trb {
            parameter: 0,
            status: 0,
            control: (trb_type::STATUS_STAGE << 10) | (1 << 5) | (status_direction << 16), // IOC
        };
        ring.enqueue_trb(status_trb);

        drop(rings);

        // Ring doorbell for EP0
        self.ring_doorbell(slot, 1);

        // Wait for completion
        let event = self.wait_transfer(slot)?;
        let transferred = (event.status & 0xFFFFFF) as usize;

        Ok(data_len - transferred)
    }

    fn bulk_transfer(
        &self,
        slot: u8,
        endpoint: u8,
        data: &mut [u8],
        direction: TransferDirection,
    ) -> UsbResult<usize> {
        let ep_idx = if matches!(direction, TransferDirection::In) {
            (endpoint & 0x0F) * 2 + 1
        } else {
            (endpoint & 0x0F) * 2
        } as usize;

        let mut rings = self.transfer_rings.lock();
        let ring = rings[slot as usize]
            .get_mut(ep_idx)
            .and_then(|r| r.as_mut())
            .ok_or(UsbError::InvalidEndpoint)?;

        let trb = Trb {
            parameter: data.as_ptr() as u64,
            status: data.len() as u32,
            control: (trb_type::NORMAL << 10) | (1 << 5), // IOC
        };
        ring.enqueue_trb(trb);
        drop(rings);

        self.ring_doorbell(slot, ep_idx as u8 + 1);

        let event = self.wait_transfer(slot)?;
        let residue = (event.status & 0xFFFFFF) as usize;

        Ok(data.len() - residue)
    }

    fn interrupt_transfer(
        &self,
        slot: u8,
        endpoint: u8,
        data: &mut [u8],
        direction: TransferDirection,
    ) -> UsbResult<usize> {
        // Same as bulk for basic operation
        self.bulk_transfer(slot, endpoint, data, direction)
    }

    fn max_slots(&self) -> u8 {
        self.max_slots
    }

    fn num_ports(&self) -> u8 {
        self.num_ports
    }
}

unsafe impl Send for XhciController {}
unsafe impl Sync for XhciController {}

/// Global xHCI controller
static XHCI: Mutex<Option<Arc<XhciController>>> = Mutex::new(None);

/// Initialize xHCI from MMIO base
pub fn init(base: usize) -> Result<(), &'static str> {
    let mut controller = XhciController::probe(base).ok_or("xHCI controller not found")?;
    controller.init()?;

    let arc_controller = Arc::new(controller);
    usb::register_controller(arc_controller.clone());

    *XHCI.lock() = Some(arc_controller);
    Ok(())
}
