# OXIDE OS Boot Protocols

**Last Updated:** 2026-02-02
**Status:** Multi-protocol support implemented (skeleton)

## Overview

OXIDE OS supports multiple boot protocols to enable booting on different architectures and platforms. Each architecture has specific boot firmware requirements and memory layout expectations.

## Supported Boot Protocols

### 1. UEFI (Unified Extensible Firmware Interface)

**Architectures:** x86_64, aarch64
**Status:** ✅ Fully implemented (x86_64), 🚧 Skeleton (aarch64)

#### Boot Sequence

1. UEFI firmware loads bootloader EFI application
2. Bootloader allocates memory, loads kernel and initramfs
3. Bootloader sets up page tables and direct physical memory map
4. Bootloader calls `ExitBootServices()` to reclaim boot memory
5. Bootloader jumps to kernel entry point with `BootInfo` structure

#### Memory Layout (x86_64)

```
0xFFFF_FFFF_8000_0000  ← Kernel virtual base (KERNEL_VIRT_BASE)
                         Kernel code and data mapped here

0xFFFF_8000_0000_0000  ← Physical memory map base (PHYS_MAP_BASE)
                         All physical RAM linearly mapped
                         Used to access boot structures, initramfs

Physical addresses:
  0x0000_0000          ← Low memory, BIOS/firmware reserved
  0x0010_0000          ← Typical kernel load address (1MB+)
  varies               ← Initramfs, framebuffer, boot structures
```

#### Boot Information Passed

- `BootInfo` structure at fixed address
- Memory map from UEFI `GetMemoryMap()`
- Framebuffer info from GOP (Graphics Output Protocol)
- Page table root (CR3 for x86_64)
- Initramfs location and size
- Kernel physical and virtual addresses

#### Entry Point Signature

```rust
#[unsafe(no_mangle)]
pub extern "C" fn kernel_main(boot_info: &'static BootInfo) -> !
```

---

### 2. ARCS (Advanced RISC Computing Specification)

**Architectures:** SGI MIPS64
**Status:** 🚧 Skeleton implementation

#### Boot Sequence

1. ARCS PROM initializes hardware
2. PROM loads bootloader from volume header (disk partition 8)
3. Bootloader receives ARCS SPB (System Parameter Block) pointer
4. Bootloader queries firmware for memory map via `GetMemoryDescriptor()`
5. Bootloader loads kernel to KSEG0 (cached, unmapped)
6. Bootloader jumps to kernel with ARCS structures pointer

#### Memory Layout (MIPS64)

```
KSEG0: 0xFFFF_FFFF_8000_0000  ← Cached, unmapped kernel segment
                                 Direct mapping of first 512MB RAM
                                 Kernel code loaded here

KSEG1: 0xFFFF_FFFF_A000_0000  ← Uncached, unmapped segment
                                 Used for device I/O (MMIO)

KSEG2: 0xFFFF_FFFF_C000_0000  ← Mapped kernel segment
                                 Page tables used for heap, etc.

Physical addresses:
  0x0000_0000          ← Physical RAM starts
  varies               ← Kernel, initramfs locations
```

#### Boot Information Passed

ARCS provides:
- SPB (System Parameter Block) with firmware vectors
- Memory descriptors (page-based, big-endian)
- Environment variables (console device, boot path, etc.)
- Firmware callbacks (load, invoke, halt, etc.)

#### Key Differences from UEFI

1. **Big-Endian Data**: All ARCS structures are big-endian
2. **Firmware Persists**: Unlike UEFI, ARCS firmware remains callable
3. **Page-Based Addressing**: Memory descriptors use page numbers, not byte addresses
4. **32-bit Pointers**: Even on 64-bit MIPS, ARCS uses 32-bit pointers
5. **KSEG0 Direct Map**: First 512MB RAM directly accessible without page tables

#### ARCS Structure Example

```c
struct MEMORYDESCRIPTOR {
    MEMORYTYPE MemoryType;  // big-endian u32
    ULONG BasePage;         // big-endian u32
    ULONG PageCount;        // big-endian u32
};

// Memory types:
// 0 = ExceptionBlock  (ROM vectors)
// 1 = SPBPage         (System Parameter Block)
// 2 = FreeMemory      (Available RAM)
// 3 = FirmwareTemporary
// 4 = FirmwarePermanent
// 5 = FreeContiguous
// 6 = BadMemory
// 7 = LoadedProgram   (Kernel)
// 8 = FirmwareCode
```

#### Entry Point Signature

```rust
#[unsafe(no_mangle)]
pub extern "C" fn kernel_main(
    arcs_spb: *const ArcsSpb,
    argc: u32,
    argv: *const *const u8,
    envp: *const *const u8
) -> !
```

---

### 3. Device Tree (FDT/DTB)

**Architectures:** ARM, RISC-V
**Status:** 📋 Planned (not yet implemented)

#### Boot Sequence

1. Platform-specific bootloader (U-Boot, etc.)
2. Bootloader loads kernel and DTB blob
3. Bootloader jumps to kernel with DTB address in register
4. Kernel parses DTB for hardware configuration

#### Entry Point Signature (ARM64)

```
Registers on entry:
  x0 = DTB physical address
  x1-x3 = 0 (reserved)
```

---

## Architecture-Specific Requirements

### x86_64

**Boot Protocol:** UEFI
**Page Table:** 4-level paging (PML4)
**Virtual Base:** `0xFFFF_FFFF_8000_0000`
**Physical Map:** `0xFFFF_8000_0000_0000`

**Bootloader Must:**
- Set up identity mapping for kernel physical address
- Set up higher-half mapping at KERNEL_VIRT_BASE
- Set up direct physical memory map at PHYS_MAP_BASE
- Enable paging (CR0.PG = 1)
- Set CR3 to PML4 physical address
- Enable NX bit (EFER.NXE = 1)
- Jump to kernel in long mode, CPL=0

### aarch64

**Boot Protocol:** UEFI or Device Tree
**Page Table:** 4-level paging (TTBR0/TTBR1)
**Virtual Base:** `0xFFFF_8000_0000_0000`
**Physical Map:** TBD

**Bootloader Must:**
- Configure TTBR0_EL1 (user page table - empty initially)
- Configure TTBR1_EL1 (kernel page table)
- Enable MMU (SCTLR_EL1.M = 1)
- Set up exception vectors (VBAR_EL1)
- Jump to kernel at EL1

### mips64 (SGI)

**Boot Protocol:** ARCS
**TLB:** Software-managed, 48-64 entries
**Virtual Base:** `0xFFFF_FFFF_8000_0000` (KSEG0)
**Physical Map:** Direct via KSEG0 (first 512MB)

**Bootloader Must:**
- Load kernel to KSEG0 physical mapping
- Pass ARCS SPB pointer in register
- Set CP0 Status register (kernel mode)
- Initialize TLB (optional, can be done by kernel)
- Jump to kernel in 64-bit mode

---

## Boot Protocol Trait

The `boot-proto` crate provides a trait-based abstraction:

```rust
pub trait BootProtocol {
    fn protocol_name(&self) -> &'static str;
    fn memory_map(&self) -> &[MemoryRegion];
    fn framebuffer(&self) -> Option<FramebufferInfo>;
    fn kernel_phys_base(&self) -> u64;
    fn kernel_virt_base(&self) -> u64;
    fn page_table_root(&self) -> u64;
    fn phys_map_base(&self) -> u64;
    fn initramfs(&self) -> Option<InitramfsInfo>;
    fn command_line(&self) -> Option<&str>;
    fn arch_data(&self) -> Option<&dyn core::any::Any>;
}
```

Each boot protocol implements this trait to provide a unified interface to the kernel.

---

## Testing Boot Protocols

### QEMU Testing

**x86_64 (UEFI):**
```bash
qemu-system-x86_64 -bios /usr/share/edk2/ovmf/OVMF_CODE.fd \
    -drive format=raw,file=disk.img \
    -serial stdio -m 512M
```

**aarch64 (UEFI):**
```bash
qemu-system-aarch64 -M virt -cpu cortex-a57 \
    -bios /usr/share/edk2/aarch64/QEMU_EFI.fd \
    -drive format=raw,file=disk.img \
    -serial stdio -m 512M
```

**mips64 (ARCS):**
```bash
# QEMU Malta doesn't have ARCS, but can test bootloader concepts
qemu-system-mips64 -M malta -cpu MIPS64R2-generic \
    -kernel kernel.elf -serial stdio -m 256M
```

### Real Hardware Testing

- **x86_64**: Standard PC with UEFI firmware
- **aarch64**: Raspberry Pi 4, ARM development boards
- **mips64**: SGI Indy, Indigo2, Octane (vintage workstations)

---

## Implementation Status

| Protocol | Trait | Bootloader | Kernel | Tested |
|----------|-------|------------|--------|--------|
| UEFI x86_64 | ✅ | ✅ | ✅ | ✅ |
| UEFI aarch64 | ✅ | 🚧 | 🚧 | ❌ |
| ARCS mips64 | ✅ | 📋 | 🚧 | ❌ |
| Device Tree | 🚧 | ❌ | ❌ | ❌ |

**Legend:**
- ✅ Complete and tested
- 🚧 Skeleton/partial implementation
- 📋 Planned but not started
- ❌ Not implemented

---

## References

- UEFI Specification 2.10
- ARCS Specification (SGI, 1990s)
- ARM Boot Requirements (Linux kernel Documentation)
- Device Tree Specification v0.4

---

## Future Work

1. **Complete UEFI aarch64 bootloader**
2. **Implement ARCS bootloader for SGI MIPS64**
3. **Add Device Tree support for ARM/RISC-V**
4. **Test on real SGI hardware**
5. **Multiboot2 support for legacy x86**
