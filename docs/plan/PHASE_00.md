# Phase 0: Boot + Serial

**Stage:** 1 - Foundation
**Status:** Not Started
**Dependencies:** None

---

## Goal

Boot to Rust on all architectures with serial output.

---

## Deliverables

| Item | Status |
|------|--------|
| UEFI bootloader (x86_64, aarch64) | [ ] |
| BIOS bootloader (i686) | [ ] |
| OpenSBI payload (riscv64, riscv32) | [ ] |
| ARCS/YAMON loader (mips64, mips32) | [ ] |
| U-Boot support (arm) | [ ] |
| Kernel entry in Rust | [ ] |
| Serial output driver | [ ] |
| Panic handler | [ ] |

---

## Architecture Status

| Arch | Bootloader | Serial | Panic | Done |
|------|------------|--------|-------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] |

---

## Serial Ports

| Arch | Device | Address |
|------|--------|---------|
| x86_64/i686 | COM1 | 0x3F8 |
| aarch64 | PL011 | 0x0900_0000 (QEMU virt) |
| arm | PL011 | 0x0900_0000 (QEMU virt) |
| mips64/mips32 | UART | 0x1F00_0900 (Malta) |
| riscv64/riscv32 | UART | 0x1000_0000 (QEMU virt) |

---

## Key Files to Create

```
kernel/
├── arch/
│   ├── mod.rs                    # Arch trait definitions
│   ├── x86_64/
│   │   ├── mod.rs
│   │   ├── boot.rs               # UEFI entry
│   │   └── serial.rs             # COM1 driver
│   ├── i686/
│   │   ├── mod.rs
│   │   ├── boot.rs               # BIOS/UEFI entry
│   │   └── serial.rs
│   ├── aarch64/
│   │   ├── mod.rs
│   │   ├── boot.rs               # UEFI entry
│   │   └── serial.rs             # PL011 driver
│   └── ... (other arches)
├── core/
│   └── panic.rs                  # Panic handler
└── lib.rs                        # Kernel entry
bootloader/
├── uefi/                         # UEFI bootloader
├── bios/                         # BIOS bootloader
└── ...
```

---

## Exit Criteria

- [ ] "Hello from EFFLUX" prints on x86_64
- [ ] "Hello from EFFLUX" prints on i686
- [ ] "Hello from EFFLUX" prints on aarch64
- [ ] "Hello from EFFLUX" prints on arm
- [ ] "Hello from EFFLUX" prints on mips64
- [ ] "Hello from EFFLUX" prints on mips32
- [ ] "Hello from EFFLUX" prints on riscv64
- [ ] "Hello from EFFLUX" prints on riscv32
- [ ] Panic handler prints message and halts
- [ ] PXE boot works on x86_64 (optional)

---

## Notes

*(Add implementation notes as work progresses)*

---

*Phase 0 of EFFLUX Implementation*
