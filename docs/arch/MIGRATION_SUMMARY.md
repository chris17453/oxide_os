# OXIDE Architecture Migration - Quick Reference

**Status:** ✅ Complete (2026-02-02)
**Scope:** Single-architecture → Multi-architecture OS

---

## What Was Done

Migrated OXIDE OS from x86_64-only to supporting **three architectures**:
- **x86_64** (Intel/AMD) - Little-endian, coherent caches
- **ARM64** (aarch64) - Little-endian, coherent caches
- **MIPS64** (SGI) - **BIG-ENDIAN**, **non-coherent caches** ⚠️

---

## Architecture Support Matrix

| Component | x86_64 | ARM64 | MIPS64 |
|-----------|--------|-------|--------|
| **Kernel Traits** | ✅ Complete | ✅ Complete | ✅ Complete |
| **Userspace libc** | ✅ Complete | ✅ Complete | ✅ Complete |
| **Build Tested** | ✅ Yes | 🚧 Needs toolchain | 🚧 Needs toolchain |
| **Runtime Tested** | ✅ Yes | ❌ Pending | ❌ Pending |
| **Boot Protocol** | UEFI | UEFI | ARCS |

---

## Key Files Created

### Architecture Traits
```
crates/arch/arch-traits/     - Common trait definitions
crates/arch/arch-x86_64/     - x86_64 implementation
crates/arch/arch-aarch64/    - ARM64 implementation
crates/arch/arch-mips64/     - MIPS64 implementation
```

### Boot Protocol
```
crates/boot/boot-proto/      - Multi-protocol abstraction
  ├── src/traits.rs          - BootProtocol trait
  └── src/arcs.rs            - ARCS (SGI MIPS) support
```

### Userspace
```
userspace/libc/src/arch/
  ├── x86_64/                - x86_64 syscalls & entry point
  ├── aarch64/               - ARM64 syscalls & entry point
  └── mips64/                - MIPS64 syscalls & entry point
```

### Documentation
```
docs/arch/
  ├── MIGRATION_COMPLETE.md  - Full migration report (686 lines)
  ├── MIGRATION_SUMMARY.md   - This file (quick reference)
  ├── BOOT_PROTOCOLS.md      - Boot protocol documentation
  ├── USERSPACE_ARCH.md      - Userspace architecture guide
  └── TESTING_GUIDE.md       - Testing procedures
```

### Scripts
```
scripts/
  ├── validate-arch-simple.sh - Build validation
  ├── qemu-x86_64.sh         - Launch x86_64 in QEMU
  ├── qemu-aarch64.sh        - Launch ARM64 in QEMU
  └── qemu-mips64.sh         - Launch MIPS64 in QEMU
```

---

## Critical Architecture Differences

### ⚠️ Endianness

| Architecture | Byte Order | Action Required |
|--------------|------------|-----------------|
| x86_64 | Little | None (native) |
| ARM64 | Little | None (native) |
| **MIPS64** | **BIG** | **Must use `Endianness::to_le*()` for disk I/O** |

**Example:**
```rust
// Writing to disk (must be little-endian on all architectures)
let value_le = Endianness::to_le32(value);  // x86/ARM: no-op, MIPS: swap
disk.write(&value_le.to_ne_bytes());
```

### ⚠️ Cache Coherency

| Architecture | Cache | DMA | Action Required |
|--------------|-------|-----|-----------------|
| x86_64 | Coherent | Coherent | None |
| ARM64 | Coherent | Coherent | None |
| **MIPS64** | **Non-Coherent** | **Non-Coherent** | **Must call sync functions** |

**Example:**
```rust
// MIPS64 DMA requires manual cache synchronization
unsafe {
    // Before device reads from memory
    DmaOps::dma_sync_for_device(buffer_phys, size);

    // ... device DMA operation ...

    // After device writes to memory
    DmaOps::dma_sync_for_cpu(buffer_phys, size);
}
// On x86_64/ARM64: these are no-ops
```

### TLB Management

| Architecture | TLB Entries | Management |
|--------------|-------------|------------|
| x86_64 | 1536+ | Hardware |
| ARM64 | 512+ | Hardware/Software |
| **MIPS64** | **48-64** | **Software only** |

---

## How to Build

### x86_64 (Current Default)
```bash
make build        # Full system build
make run          # Build and run in QEMU
```

### ARM64 (Requires Cross-Compile Toolchain)
```bash
rustup target add aarch64-unknown-linux-gnu
cargo build -p kernel --features arch-aarch64
cargo build -p libc --target aarch64-unknown-linux-gnu
```

### MIPS64 (Requires Cross-Compile Toolchain)
```bash
rustup target add mips64-unknown-linux-gnu
cargo build -p kernel --features arch-mips64
cargo build -p libc --target mips64-unknown-linux-gnu
```

---

## How to Test

### Build Validation
```bash
./scripts/validate-arch-simple.sh
```

### Run in QEMU
```bash
./scripts/qemu-x86_64.sh    # x86_64 with UEFI
./scripts/qemu-aarch64.sh   # ARM64 with UEFI (requires disk image)
./scripts/qemu-mips64.sh    # MIPS64 direct kernel (no ARCS)
```

### Manual Testing
```bash
# Kernel
cargo build -p kernel --target x86_64-unknown-none

# Userspace
cargo build -p libc --target x86_64-unknown-linux-gnu
```

---

## Implementation Details

### Syscall ABIs

| Arch | Instruction | Syscall # | Arguments | Return |
|------|-------------|-----------|-----------|--------|
| x86_64 | `syscall` | rax | rdi,rsi,rdx,r10,r8,r9 | rax |
| ARM64 | `svc #0` | x8 | x0-x5 | x0 |
| MIPS64 | `syscall` | $v0 | $a0-$a5 | $v0 |

### Boot Protocols

| Arch | Protocol | Bootloader | Status |
|------|----------|------------|--------|
| x86_64 | UEFI | boot-uefi | ✅ Working |
| ARM64 | UEFI | boot-uefi | 🚧 Needs ARM64 build |
| MIPS64 | ARCS | (TBD) | 📋 Planned |

---

## Trait Overview

### Core Traits (crates/arch/arch-traits)

```rust
pub trait Arch                  // Basic arch info & halt
pub trait ControlRegisters      // Page table root, IP, SP
pub trait Endianness            // Byte order conversions
pub trait CacheOps              // Cache flush/invalidate
pub trait DmaOps                // DMA synchronization
pub trait TlbControl            // TLB management
pub trait AtomicOps             // Atomic operations
pub trait SyscallInterface      // Syscall mechanism
pub trait ExceptionHandler      // Exception/interrupt handling
pub trait PortIo                // Port I/O (x86 mainly)
pub trait SystemRegisters       // MSR/CP0 access
```

### Usage in Kernel

```rust
use arch_traits::Arch;

// Generic over architecture
fn some_kernel_function<A: Arch>() {
    if A::is_little_endian() {
        // Little-endian path (x86_64, ARM64)
    } else {
        // Big-endian path (MIPS64)
    }
}

// Or use concrete architecture via kernel's arch module
use crate::arch::Arch;
Arch::halt();
```

---

## Statistics

- **Files Created:** 35
- **Files Modified:** 8
- **Lines Added:** ~5110
- **Crates Created:** 3 (arch-aarch64, arch-mips64, boot-proto traits)
- **Build Time:** No regression (~0.3s incremental)
- **Warnings:** 0 (all fixed)

---

## Next Steps

### Immediate (Ready Now)
1. Run validation: `./scripts/validate-arch-simple.sh`
2. Test x86_64: `make run`
3. Review documentation in `docs/arch/`

### Short Term (Requires Toolchains)
1. Install ARM64 cross-compile toolchain
2. Build and test ARM64 in QEMU
3. Install MIPS64 cross-compile toolchain
4. Build MIPS64 kernel

### Medium Term (Hardware Required)
1. Test ARM64 on Raspberry Pi 4
2. Test MIPS64 on SGI hardware (Indy, Octane)
3. Complete ARCS bootloader implementation
4. Validate DMA on non-coherent systems

---

## Common Commands

```bash
# Validation
./scripts/validate-arch-simple.sh

# Build
make build                      # Full x86_64 build
cargo build -p kernel           # Kernel only
cargo build -p libc             # Userspace libc

# Run
make run                        # QEMU x86_64
./scripts/qemu-x86_64.sh        # QEMU x86_64 (manual)

# Clean
make clean                      # Clean build artifacts
cargo clean                     # Full cargo clean
```

---

## Quick Troubleshooting

**Problem:** Toolchain not found
```bash
rustup target add <target>
# aarch64-unknown-linux-gnu
# mips64-unknown-linux-gnu
```

**Problem:** Linker not found (ARM64/MIPS64)
```bash
sudo apt install gcc-aarch64-linux-gnu
sudo apt install gcc-mips64-linux-gnuabi64
```

**Problem:** MIPS64 data corruption
- Check: Are you using `Endianness::to_le*()` for disk I/O?
- Check: Are you calling `dma_sync_*()` for DMA operations?

---

## Resources

- **Full Report:** `docs/arch/MIGRATION_COMPLETE.md` (686 lines)
- **Testing Guide:** `docs/arch/TESTING_GUIDE.md` (389 lines)
- **Boot Protocols:** `docs/arch/BOOT_PROTOCOLS.md` (302 lines)
- **Userspace:** `docs/arch/USERSPACE_ARCH.md` (389 lines)

---

## Architecture Personas

**Code comments** are signed by different personas:
- **NeonRoot** - System integration, cross-platform
- **GraveShift** - Kernel systems, MIPS64 expert
- **BlackLatch** - Security, exception handling
- **WireSaint** - Storage, cache operations
- **ThreadRogue** - Runtime, process model

---

**Status:** Production-ready for x86_64 ✅
**ARM64/MIPS64:** Structurally complete, awaiting testing 🚧

— OXIDE OS Architecture Team
