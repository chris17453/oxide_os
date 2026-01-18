# Phase 16: USB

**Stage:** 3 - Hardware
**Status:** Not Started
**Dependencies:** Phase 10 (Modules)

---

## Goal

Implement USB host controller with mass storage and HID class support.

---

## Deliverables

| Item | Status |
|------|--------|
| xHCI host controller driver | [ ] |
| USB device enumeration | [ ] |
| USB hub support | [ ] |
| Mass storage class (MSC) | [ ] |
| HID class (keyboard, mouse) | [ ] |
| USB core framework | [ ] |

---

## Architecture Status

| Arch | xHCI | Enumeration | MSC | HID | Done |
|------|------|-------------|-----|-----|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## USB Stack Architecture

```
┌─────────────────────────────┐
│      Class Drivers          │
│  (MSC, HID, Audio, etc.)    │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│        USB Core             │
│  - Device enumeration       │
│  - Configuration            │
│  - URB management           │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│   Host Controller Driver    │
│        (xHCI)               │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│     xHCI Hardware           │
└─────────────────────────────┘
```

---

## xHCI Overview

```
xHCI Memory Structures:
┌────────────────────────────────────────────────┐
│ Device Context Base Address Array (DCBAA)      │
│ - Array of pointers to Device Contexts         │
├────────────────────────────────────────────────┤
│ Device Context                                 │
│ - Slot Context (device info)                   │
│ - Endpoint Contexts (31 endpoints)             │
├────────────────────────────────────────────────┤
│ Transfer Ring (per endpoint)                   │
│ - Circular buffer of TRBs                      │
├────────────────────────────────────────────────┤
│ Event Ring                                     │
│ - Transfer completion events                   │
│ - Command completion events                    │
├────────────────────────────────────────────────┤
│ Command Ring                                   │
│ - Host controller commands                     │
└────────────────────────────────────────────────┘
```

---

## Transfer Request Block (TRB)

```rust
#[repr(C)]
struct Trb {
    parameter: u64,     // Data pointer or immediate data
    status: u32,        // Status/transfer length
    control: u32,       // TRB type, flags, cycle bit
}

// TRB Types
const TRB_NORMAL: u32 = 1;      // Normal data transfer
const TRB_SETUP: u32 = 2;       // Setup stage
const TRB_DATA: u32 = 3;        // Data stage
const TRB_STATUS: u32 = 4;      // Status stage
const TRB_LINK: u32 = 6;        // Link to next ring segment
const TRB_EVENT_DATA: u32 = 7;  // Event data
const TRB_ENABLE_SLOT: u32 = 9; // Enable slot command
const TRB_ADDRESS: u32 = 11;    // Address device command
const TRB_CONFIGURE: u32 = 12;  // Configure endpoint command
```

---

## USB Enumeration Process

```
1. Device connected (port status change event)
        │
        ▼
2. Reset port
        │
        ▼
3. Enable Slot (get slot ID)
        │
        ▼
4. Address Device (set USB address)
        │
        ▼
5. Get Device Descriptor (8 bytes)
        │
        ▼
6. Get full Device Descriptor
        │
        ▼
7. Get Configuration Descriptor
        │
        ▼
8. Set Configuration
        │
        ▼
9. Load class driver
```

---

## USB Descriptors

```rust
#[repr(C, packed)]
struct DeviceDescriptor {
    length: u8,             // 18
    descriptor_type: u8,    // 1
    bcd_usb: u16,           // USB version (0x0200 = USB 2.0)
    device_class: u8,
    device_subclass: u8,
    device_protocol: u8,
    max_packet_size0: u8,   // Max packet size for EP0
    vendor_id: u16,
    product_id: u16,
    bcd_device: u16,
    manufacturer: u8,       // String index
    product: u8,            // String index
    serial_number: u8,      // String index
    num_configurations: u8,
}

#[repr(C, packed)]
struct ConfigDescriptor {
    length: u8,             // 9
    descriptor_type: u8,    // 2
    total_length: u16,      // Total length including interfaces
    num_interfaces: u8,
    configuration_value: u8,
    configuration: u8,      // String index
    attributes: u8,
    max_power: u8,          // In 2mA units
}
```

---

## Mass Storage Class

```rust
// SCSI commands over USB Bulk-Only Transport
const SCSI_TEST_UNIT_READY: u8 = 0x00;
const SCSI_REQUEST_SENSE: u8 = 0x03;
const SCSI_INQUIRY: u8 = 0x12;
const SCSI_READ_CAPACITY: u8 = 0x25;
const SCSI_READ_10: u8 = 0x28;
const SCSI_WRITE_10: u8 = 0x2A;

#[repr(C, packed)]
struct CommandBlockWrapper {
    signature: u32,         // 0x43425355 "USBC"
    tag: u32,               // Unique command ID
    data_length: u32,       // Expected data length
    flags: u8,              // Direction (0x80 = IN)
    lun: u8,                // Logical unit number
    cb_length: u8,          // Command block length (1-16)
    cb: [u8; 16],           // Command block (SCSI CDB)
}

#[repr(C, packed)]
struct CommandStatusWrapper {
    signature: u32,         // 0x53425355 "USBS"
    tag: u32,               // Matches CBW tag
    data_residue: u32,      // Difference from expected
    status: u8,             // 0=success, 1=fail, 2=phase error
}
```

---

## HID Class

```rust
// HID Report Descriptor parsing
// Describes the format of input/output reports

// Boot protocol (simple keyboard/mouse)
#[repr(C, packed)]
struct BootKeyboardReport {
    modifiers: u8,          // Ctrl, Shift, Alt, GUI
    reserved: u8,
    keycodes: [u8; 6],      // Up to 6 simultaneous keys
}

#[repr(C, packed)]
struct BootMouseReport {
    buttons: u8,            // Button state
    x: i8,                  // X movement
    y: i8,                  // Y movement
}
```

---

## Key Files

```
crates/usb/efflux-usb/src/
├── lib.rs
├── device.rs          # USB device abstraction
├── descriptor.rs      # Descriptor parsing
├── transfer.rs        # URB/transfer management
└── hub.rs             # Hub driver

crates/drivers/usb/efflux-xhci/src/
├── lib.rs
├── registers.rs       # xHCI registers
├── ring.rs            # Transfer/event rings
├── context.rs         # Device contexts
└── commands.rs        # Command handling

crates/drivers/usb/efflux-usb-msc/src/
├── lib.rs
├── scsi.rs            # SCSI commands
└── transport.rs       # Bulk-only transport

crates/drivers/usb/efflux-usb-hid/src/
├── lib.rs
├── keyboard.rs        # Keyboard driver
├── mouse.rs           # Mouse driver
└── report.rs          # Report parsing
```

---

## Exit Criteria

- [ ] xHCI controller initialized
- [ ] USB devices detected and enumerated
- [ ] Device descriptors read correctly
- [ ] USB keyboard input works
- [ ] USB mouse input works
- [ ] USB mass storage mounts
- [ ] Hub devices work
- [ ] Works on all 8 architectures

---

## Test

```bash
# List USB devices
$ lsusb
Bus 001 Device 001: ID 1d6b:0002 Linux Foundation 2.0 root hub
Bus 001 Device 002: ID 046d:c52b Logitech USB Receiver
Bus 001 Device 003: ID 0781:5567 SanDisk Cruzer Blade

# Mount USB drive
$ mount /dev/sda1 /mnt/usb

# Test USB keyboard
$ cat /dev/input/event1
(type on USB keyboard, see events)
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 16 of EFFLUX Implementation*
