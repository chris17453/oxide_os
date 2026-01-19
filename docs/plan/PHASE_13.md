# Phase 13: Input Devices

**Stage:** 3 - Hardware
**Status:** Complete
**Dependencies:** Phase 8 (Libc + Userland)

---

## Goal

Implement input subsystem for keyboard and mouse.

---

## Deliverables

| Item | Status |
|------|--------|
| Input event subsystem | [x] |
| PS/2 keyboard driver (x86) | [x] |
| PS/2 mouse driver (x86) | [x] |
| virtio-input driver | [x] |
| USB HID support (Phase 16) | [ ] |
| Key repeat | [x] |
| Mouse acceleration | [ ] |

---

## Architecture Status

| Arch | Events | Keyboard | Mouse | virtio | Done |
|------|--------|----------|-------|--------|------|
| x86_64 | [x] | [x] | [x] | [x] | [x] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Implementation

### Crates Created

- `input` - Input event subsystem with Linux evdev compatible events
- `ps2` - PS/2 controller and keyboard/mouse drivers
- `virtio-input` - VirtIO input device driver

### Key Features

- **InputEvent** - Linux evdev compatible event structure
- **Event types** - Key, Relative, Absolute, Synchronization
- **Keymap** - Scan code set 1 to keycode mapping
- **Keycode to ASCII** - US keyboard layout conversion
- **PS/2 Controller** - 8042 controller initialization
- **PS/2 Keyboard** - Scan code processing, LED control
- **PS/2 Mouse** - 3/4 byte packet parsing, scroll wheel support
- **VirtIO Input** - MMIO based input device for VMs

---

## Input Event Types

```rust
#[repr(C)]
pub struct InputEvent {
    pub time: Timestamp,
    pub type_: u16,
    pub code: u16,
    pub value: i32,
}

// Event types
pub const EV_SYN: u16 = 0x00;   // Synchronization
pub const EV_KEY: u16 = 0x01;   // Key/button
pub const EV_REL: u16 = 0x02;   // Relative movement
pub const EV_ABS: u16 = 0x03;   // Absolute movement

// Synchronization codes
pub const SYN_REPORT: u16 = 0;  // End of event batch

// Key values
pub const KEY_RELEASED: i32 = 0;
pub const KEY_PRESSED: i32 = 1;
pub const KEY_REPEAT: i32 = 2;
```

---

## Input Subsystem Architecture

```
+-----------------------------+
|      Application            |
|   read(/dev/input/event0)   |
+-------------+---------------+
              |
+-------------v---------------+
|      Input Core             |
|  - Event routing            |
|  - Device registration      |
|  - /dev/input/eventN        |
+-------------+---------------+
              |
    +---------+---------+
    v         v         v
+--------+ +--------+ +--------+
|  PS/2  | |virtio- | |  USB   |
|Keyboard| | input  | |  HID   |
+--------+ +--------+ +--------+
```

---

## PS/2 Controller (x86)

```
+-------------------------------------+
|          8042 Controller            |
|                                     |
|  Port 0x60: Data                    |
|  Port 0x64: Status/Command          |
|                                     |
|  IRQ 1: Keyboard                    |
|  IRQ 12: Mouse                      |
+-------------------------------------+

Status Register (0x64 read):
  Bit 0: Output buffer full (data ready)
  Bit 1: Input buffer full (don't write)
  Bit 5: Mouse data (vs keyboard)
```

---

## Exit Criteria

- [x] Keyboard input reaches userspace
- [x] Mouse movement tracked
- [x] Key repeat works
- [x] /dev/input/eventN devices created
- [x] Works in shell (typing commands)
- [ ] Works on all 8 architectures

---

## Notes

Phase 13 complete with input subsystem, PS/2 drivers, and virtio-input driver.
USB HID support deferred to Phase 16.

---

*Phase 13 of EFFLUX Implementation*
