# Phase 13: Input Devices

**Stage:** 3 - Hardware
**Status:** Not Started
**Dependencies:** Phase 8 (Libc + Userland)

---

## Goal

Implement input subsystem for keyboard and mouse.

---

## Deliverables

| Item | Status |
|------|--------|
| Input event subsystem | [ ] |
| PS/2 keyboard driver (x86) | [ ] |
| PS/2 mouse driver (x86) | [ ] |
| virtio-input driver | [ ] |
| USB HID support (Phase 16) | [ ] |
| Key repeat | [ ] |
| Mouse acceleration | [ ] |

---

## Architecture Status

| Arch | Events | Keyboard | Mouse | virtio | Done |
|------|--------|----------|-------|--------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

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
┌─────────────────────────────┐
│      Application            │
│   read(/dev/input/event0)   │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│      Input Core             │
│  - Event routing            │
│  - Device registration      │
│  - /dev/input/eventN        │
└──────────────┬──────────────┘
               │
    ┌──────────┼──────────┐
    ▼          ▼          ▼
┌────────┐ ┌────────┐ ┌────────┐
│  PS/2  │ │virtio- │ │  USB   │
│Keyboard│ │ input  │ │  HID   │
└────────┘ └────────┘ └────────┘
```

---

## PS/2 Controller (x86)

```
┌─────────────────────────────────────┐
│          8042 Controller            │
│                                     │
│  Port 0x60: Data                    │
│  Port 0x64: Status/Command          │
│                                     │
│  IRQ 1: Keyboard                    │
│  IRQ 12: Mouse                      │
└─────────────────────────────────────┘

Status Register (0x64 read):
  Bit 0: Output buffer full (data ready)
  Bit 1: Input buffer full (don't write)
  Bit 5: Mouse data (vs keyboard)
```

---

## Keyboard Scan Codes

```
Set 1 (XT) - Most common:
  0x01 = Escape
  0x02-0x0B = 1-0
  0x0E = Backspace
  0x0F = Tab
  0x10-0x19 = Q-P
  0x1C = Enter
  0x1D = Left Ctrl
  ...

Extended codes start with 0xE0:
  0xE0 0x48 = Up arrow
  0xE0 0x4B = Left arrow
  0xE0 0x4D = Right arrow
  0xE0 0x50 = Down arrow

Release codes: scan code | 0x80
```

---

## Mouse Protocol

```
PS/2 Mouse packet (3-4 bytes):
┌─────────────────────────────────────┐
│ Byte 0: Status                      │
│   Bit 0: Left button                │
│   Bit 1: Right button               │
│   Bit 2: Middle button              │
│   Bit 4: X sign                     │
│   Bit 5: Y sign                     │
│   Bit 6: X overflow                 │
│   Bit 7: Y overflow                 │
├─────────────────────────────────────┤
│ Byte 1: X movement (signed)         │
├─────────────────────────────────────┤
│ Byte 2: Y movement (signed)         │
├─────────────────────────────────────┤
│ Byte 3: Scroll wheel (if enabled)   │
└─────────────────────────────────────┘
```

---

## virtio-input

```rust
#[repr(C)]
struct VirtioInputEvent {
    type_: u16,
    code: u16,
    value: u32,
}

// Configuration:
// - Select subsel to query device capabilities
// - Read device name, serial, supported events
```

---

## Key Files

```
crates/input/efflux-input/src/
├── lib.rs
├── event.rs           # Input event types
├── device.rs          # Input device trait
├── keymap.rs          # Scancode to keycode
└── repeat.rs          # Key repeat handling

crates/drivers/input/efflux-ps2/src/
├── lib.rs
├── controller.rs      # 8042 controller
├── keyboard.rs        # Keyboard driver
└── mouse.rs           # Mouse driver

crates/drivers/input/efflux-virtio-input/src/
└── lib.rs
```

---

## Key Repeat

```
Key pressed
    │
    ▼
Send KEY_PRESSED event
    │
    ▼
Start delay timer (250ms default)
    │
    ▼
Timer fires ──► Send KEY_REPEAT event
    │           │
    │           └── Restart repeat timer (33ms = 30Hz)
    │
Key released
    │
    ▼
Send KEY_RELEASED event
Cancel timers
```

---

## Exit Criteria

- [ ] Keyboard input reaches userspace
- [ ] Mouse movement tracked
- [ ] Key repeat works
- [ ] /dev/input/eventN devices created
- [ ] Works in shell (typing commands)
- [ ] Works on all 8 architectures

---

## Test Program

```c
int main() {
    int fd = open("/dev/input/event0", O_RDONLY);
    if (fd < 0) {
        perror("open");
        return 1;
    }

    printf("Reading input events (Ctrl+C to exit):\n");

    struct input_event ev;
    while (read(fd, &ev, sizeof(ev)) == sizeof(ev)) {
        if (ev.type == EV_KEY) {
            printf("Key %d %s\n", ev.code,
                   ev.value ? "pressed" : "released");
        } else if (ev.type == EV_REL) {
            printf("Mouse %s: %d\n",
                   ev.code == REL_X ? "X" : "Y", ev.value);
        }
    }

    close(fd);
    return 0;
}
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 13 of EFFLUX Implementation*
