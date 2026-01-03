# Phase 0: Boot + Serial

**Stage:** 1 - Foundation
**Status:** Complete
**Target:** x86_64 only

---

## Goal

Boot to Rust on x86_64 with serial output.

---

## Deliverables

| Item | Status |
|------|--------|
| UEFI bootloader (x86_64) | [x] |
| Kernel entry in Rust | [x] |
| Serial output driver (COM1) | [x] |
| Panic handler | [x] |
| Makefile build system | [x] |
| Automated QEMU testing | [x] |

---

## Exit Criteria

- [x] "EFFLUX" prints on serial for x86_64
- [x] Panic handler prints message and halts
- [x] `make test` passes (automated boot verification)

---

## Implementation Notes

### Bootloader
- Uses `uefi` crate v0.32
- Prints banner via UEFI stdout
- Located at `bootloader/efflux-boot-uefi/`
- Does not yet load kernel (placeholder)

### Serial Driver
- COM1 at 0x3F8, 115200 baud, 8N1
- Uses spin::Mutex for thread-safe access
- Located at `crates/drivers/serial/efflux-driver-uart-8250/`
- Also exposed via `crates/arch/efflux-arch-x86_64/src/serial.rs`

### Kernel
- Entry point: `kernel_main()` in `kernel/src/main.rs`
- Prints banner and "Hello from EFFLUX!" to serial
- Panic handler outputs location and message

### Build System
- `make build` - build kernel and bootloader
- `make run` - run in QEMU with display
- `make test` - automated headless test (checks for "EFFLUX" in serial output)

---

## Files Created

```
Cargo.toml                              # Workspace root
Makefile                                # Build and test system
kernel/
├── Cargo.toml
├── src/main.rs                         # Kernel entry
├── targets/x86_64-unknown-none.json    # Custom target
crates/
├── core/
│   ├── efflux-core/                    # VirtAddr, PhysAddr
│   └── efflux-log/                     # Logging (stub)
├── arch/
│   ├── efflux-arch-traits/             # Arch trait
│   └── efflux-arch-x86_64/             # x86_64 impl + serial
├── mm/
│   └── efflux-mm-traits/               # MM traits (stub)
└── drivers/
    ├── efflux-driver-traits/           # Driver traits
    └── serial/efflux-driver-uart-8250/ # 8250 UART driver
bootloader/
└── efflux-boot-uefi/                   # UEFI bootloader
```

---

## Next Phase

Phase 1: Memory - Frame allocator, page tables, kernel heap

---

*Phase 0 Complete - 2025-01-03*
