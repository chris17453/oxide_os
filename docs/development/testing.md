# OXIDE Multi-Architecture Testing Guide

**Last Updated:** 2026-02-02

## Overview

This guide provides instructions for testing OXIDE OS on multiple architectures:
- x86_64 (Intel/AMD 64-bit)
- aarch64 (ARM 64-bit)
- mips64 (SGI MIPS 64-bit, big-endian)

---

## Testing Userspace Programs

### Simple Test Program

Here's a minimal test program to verify syscalls and entry point:

```rust
// test_arch.rs
#![no_std]
#![no_main]

extern crate libc;

use libc::*;

#[no_mangle]
fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    // Test write syscall
    let msg = b"Architecture test passed!\n";
    sys_write(1, msg);

    // Test getpid syscall
    let pid = sys_getpid();

    // Test exit
    0
}
```

### Compile for Different Architectures

**x86_64:**
```bash
cargo build -p libc --target x86_64-unknown-linux-gnu
```

**ARM64:**
```bash
# Install toolchain first
rustup target add aarch64-unknown-linux-gnu

# Configure linker in .cargo/config.toml:
# [target.aarch64-unknown-linux-gnu]
# linker = "aarch64-linux-gnu-gcc"

cargo build -p libc --target aarch64-unknown-linux-gnu
```

**MIPS64:**
```bash
# Install toolchain first
rustup target add mips64-unknown-linux-gnu

# Configure linker in .cargo/config.toml:
# [target.mips64-unknown-linux-gnu]
# linker = "mips64-linux-gnuabi64-gcc"

cargo build -p libc --target mips64-unknown-linux-gnu
```

---

## Syscall Testing Matrix

Test each syscall works correctly on each architecture:

| Syscall | x86_64 | ARM64 | MIPS64 | Notes |
|---------|--------|-------|--------|-------|
| sys_write | ✅ | 🧪 | 🧪 | Basic I/O |
| sys_read | ✅ | 🧪 | 🧪 | Basic I/O |
| sys_exit | ✅ | 🧪 | 🧪 | Process termination |
| sys_fork | ✅ | 🧪 | 🧪 | Process creation |
| sys_getpid | ✅ | 🧪 | 🧪 | Process info |
| sys_open | ✅ | 🧪 | 🧪 | File operations |
| sys_close | ✅ | 🧪 | 🧪 | File operations |
| sys_mmap | ✅ | 🧪 | 🧪 | Memory management |

Legend:
- ✅ Tested and working
- 🧪 Awaiting cross-compile/hardware test
- ❌ Known issue

---

## Architecture-Specific Tests

### Endianness Test

Test that endianness conversions work correctly:

```rust
fn test_endianness() {
    // x86_64 and ARM64: little-endian
    // MIPS64: big-endian

    let value: u32 = 0x12345678;

    // To little-endian (for disk I/O)
    let le = to_le32(value);

    // On x86_64/ARM: le == 0x12345678 (no change)
    // On MIPS64: le == 0x78563412 (swapped)

    // Write to disk, read back
    // Should get original value on all architectures
}
```

### Cache Coherency Test (MIPS64)

**⚠️ CRITICAL for MIPS64:**

```rust
fn test_dma_coherency() {
    // Allocate DMA buffer
    let mut buffer = [0u8; 4096];

    // Write data
    buffer[0] = 0x42;

    // On MIPS64: MUST flush cache before DMA
    unsafe {
        dma_sync_for_device(buffer.as_ptr() as PhysAddr, 4096);
    }

    // Device reads from buffer via DMA
    // ...

    // Device writes to buffer via DMA
    // ...

    // On MIPS64: MUST invalidate cache after DMA
    unsafe {
        dma_sync_for_cpu(buffer.as_ptr() as PhysAddr, 4096);
    }

    // Now read buffer - should see device's writes
    assert_eq!(buffer[0], expected_value);
}
```

**On x86_64/ARM64:** These functions are no-ops (coherent)
**On MIPS64:** These functions issue CACHE instructions

---

## QEMU Testing

### x86_64 (UEFI)

```bash
# Run OXIDE in QEMU with UEFI firmware
qemu-system-x86_64 \
    -bios /usr/share/edk2/ovmf/OVMF_CODE.fd \
    -drive format=raw,file=oxide.img \
    -m 512M \
    -serial stdio \
    -nographic
```

### ARM64 (UEFI)

```bash
# Run OXIDE in QEMU with ARM64 UEFI firmware
qemu-system-aarch64 \
    -M virt \
    -cpu cortex-a57 \
    -bios /usr/share/edk2/aarch64/QEMU_EFI.fd \
    -drive format=raw,file=oxide.img \
    -m 512M \
    -serial stdio \
    -nographic
```

### MIPS64 (ARCS or direct kernel)

```bash
# Note: QEMU Malta doesn't have ARCS firmware
# Can test with direct kernel loading

qemu-system-mips64 \
    -M malta \
    -cpu MIPS64R2-generic \
    -kernel kernel.elf \
    -m 256M \
    -serial stdio \
    -nographic
```

**For real ARCS testing:** Use actual SGI hardware (Indy, Indigo2, Octane)

---

## Real Hardware Testing

### x86_64

**Recommended:**
- Any modern x86_64 PC with UEFI firmware
- Intel NUC, Dell OptiPlex, custom build

**Minimum Requirements:**
- 64-bit x86 processor (Intel Core 2 or newer, AMD Athlon 64 or newer)
- UEFI firmware (CSM/legacy mode also works)
- 512MB RAM minimum, 2GB recommended
- USB or SATA for boot device

### ARM64

**Recommended:**
- Raspberry Pi 4 (8GB model)
- NVIDIA Jetson Nano
- ARM development boards (96Boards, etc.)

**Requirements:**
- ARMv8-A processor (Cortex-A53 or better)
- UEFI firmware (may need custom UEFI for RPi)
- 1GB RAM minimum, 4GB recommended
- SD card or USB for boot device

### MIPS64 (SGI)

**Recommended Hardware:**
- **SGI Indy** (IP22) - Entry-level workstation, R4000/R5000
- **SGI Indigo2** (IP22) - R4400/R10000
- **SGI Octane** (IP30) - R10000/R12000/R14000, most powerful
- **SGI O2** (IP32) - Compact workstation, R5000/R10000/R12000

**Requirements:**
- MIPS64 R4000 or better
- ARCS firmware (built into SGI hardware)
- 64MB RAM minimum, 256MB+ recommended
- SCSI hard drive or network boot

**Notes:**
- ⚠️ SGI hardware is vintage (1990s-early 2000s)
- May require special SCSI cables, monitors
- ARCS firmware provides boot environment
- Serial console recommended for debugging

---

## Automated Testing

### Build Validation

```bash
# Run multi-architecture build validation
./scripts/validate-arch-simple.sh
```

Expected output:
```
[1/7] Building arch-x86_64...     ✓ PASSED
[2/7] Building arch-aarch64...    ⊘ SKIPPED or ✓ PASSED
[3/7] Building arch-mips64...     ⊘ SKIPPED or ✓ PASSED
[4/7] Building boot-proto...      ✓ PASSED
[5/7] Building libc for x86_64... ✓ PASSED
[6/7] Building libc for aarch64...⊘ SKIPPED or ✓ PASSED
[7/7] Building libc for mips64... ⊘ SKIPPED or ✓ PASSED
```

### Kernel Build Test

```bash
# Test kernel compiles
cargo build -p kernel --target x86_64-unknown-none
```

Should complete without errors.

---

## Debugging

### Serial Console Output

All architectures support serial console for early debugging:

**x86_64:** COM1 (0x3F8)
**ARM64:** PL011 UART
**MIPS64:** SGI ARCS serial console

### QEMU GDB Debugging

```bash
# Start QEMU with GDB server
qemu-system-x86_64 -s -S ... (other args)

# In another terminal
gdb target/x86_64-unknown-none/debug/kernel
(gdb) target remote :1234
(gdb) break kernel_main
(gdb) continue
```

### Architecture-Specific Debugging

**x86_64:**
- Use QEMU monitor (`Ctrl-A c`)
- `info registers` shows CPU state
- `info mem` shows page tables

**ARM64:**
- JTAG debugging on real hardware
- QEMU monitor similar to x86_64

**MIPS64:**
- ARCS firmware debugger on SGI hardware
- Serial console for kernel messages
- No QEMU ARCS, so limited emulator debugging

---

## Performance Testing

### Benchmark Suite

Create benchmarks for:
- Syscall overhead
- Context switch time
- TLB miss handling
- Cache flush performance (MIPS64)

### Expected Performance

| Operation | x86_64 | ARM64 | MIPS64 |
|-----------|--------|-------|--------|
| Syscall | ~50-100ns | ~50-100ns | ~100-200ns |
| Context switch | ~1-2μs | ~1-2μs | ~2-5μs |
| TLB miss | HW handled | HW/SW mix | SW handled |

**Note:** MIPS64 may be slower due to:
- Software TLB management
- Non-coherent cache operations
- Smaller TLB (48-64 entries vs 1536+)

---

## Continuous Integration

### CI Pipeline

```yaml
# .github/workflows/multi-arch.yml
name: Multi-Architecture Build

on: [push, pull_request]

jobs:
  build-x86_64:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Build x86_64
        run: |
          cargo build -p kernel --target x86_64-unknown-none
          cargo build -p libc --target x86_64-unknown-linux-gnu

  build-aarch64:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Install ARM64 toolchain
        run: rustup target add aarch64-unknown-linux-gnu
      - name: Build aarch64
        run: cargo build -p libc --target aarch64-unknown-linux-gnu

  build-mips64:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Install MIPS64 toolchain
        run: rustup target add mips64-unknown-linux-gnu
      - name: Build mips64
        run: cargo build -p libc --target mips64-unknown-linux-gnu
```

---

## Troubleshooting

### Common Issues

**1. Linker not found**
```
error: linker `aarch64-linux-gnu-gcc` not found
```
**Solution:** Install cross-compilation toolchain:
```bash
sudo apt install gcc-aarch64-linux-gnu  # ARM64
sudo apt install gcc-mips64-linux-gnuabi64  # MIPS64
```

**2. Target not installed**
```
error: toolchain 'stable-x86_64-unknown-linux-gnu' does not support target 'aarch64-unknown-linux-gnu'
```
**Solution:** Install target:
```bash
rustup target add aarch64-unknown-linux-gnu
```

**3. MIPS64 big-endian errors**
```
Disk read returned garbage data
```
**Solution:** Ensure all disk I/O uses `Endianness::to_le*()` conversions

**4. MIPS64 DMA corruption**
```
Network packets corrupted
```
**Solution:** Add proper cache synchronization:
```rust
unsafe {
    dma_sync_for_device(...);  // Before device reads
    dma_sync_for_cpu(...);     // After device writes
}
```

---

## Next Steps

1. Set up cross-compilation toolchains
2. Install QEMU for all architectures
3. Run validation script
4. Test in QEMU
5. Test on real hardware (if available)
6. Report issues via GitHub

---

**Maintainer:** OXIDE OS Development Team
**Contact:** See CONTRIBUTING.md

— NeonRoot
