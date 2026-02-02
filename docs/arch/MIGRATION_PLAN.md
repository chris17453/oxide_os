# OXIDE OS Architecture Abstraction Migration Plan

**Status:** Planning Phase
**Target Architectures:** x86_64 (current), ARM64 (aarch64), SGI MIPS64 (big-endian)
**Timeline:** 22 weeks (5.5 months)
**Last Updated:** 2026-02-02
**SGI Target Platforms:** IP22 (Indy), IP27 (Origin), IP30 (Octane), IP32 (O2)

## Executive Summary

This document outlines the complete migration plan to transform OXIDE OS from a monolithic x86_64-specific codebase into a properly abstracted multi-architecture operating system. The current system contains **8,575+ lines of assembly code** across **47 Rust files** with architecture-specific operations scattered throughout the codebase.

## Current Assembly Inventory

### Native Assembly Files (2 files, 209 lines)
- `crates/arch/arch-x86_64/src/ap_boot.s` (101 lines) - AP boot trampoline
- `userspace/init/init.S` (108 lines) - Userspace initialization test

### Inline Assembly Distribution (47 files, 377+ blocks)

**Core Architecture (9 files):**
- `exceptions.rs` - 28 instances (largest file)
- `syscall.rs` - Exception entry/exit, MSR operations
- `usermode.rs` - Ring transitions
- `apic.rs` - APIC/CPUID operations
- `lib.rs` - Port I/O (inb/outb/etc)
- `gdt.rs`, `idt.rs` - Descriptor table management
- `serial.rs`, `ap_boot.rs` - Minimal assembly

**Key Operations to Abstract:**
- Port I/O (x86-specific)
- MSR access (architecture-specific system registers)
- Control registers (CR0/CR3/CR4 vs TTBR vs CP0)
- TLB operations (invlpg vs tlbi vs tlbwi)
- Syscall mechanisms (syscall/sysret vs svc vs syscall)
- Exception handling (IDT vs vectors vs exception vectors)
- Context switching (register sets differ)

## Architecture Comparison Matrix

| Feature | x86_64 | ARM64 | SGI MIPS64 |
|---------|--------|-------|------------|
| **Endianness** | Little-endian | Little-endian | **Big-endian** ⚠️ |
| **Boot** | Real→Protected→Long | EL2/EL1 | **ARCS/PROM** |
| **Boot Firmware** | UEFI/BIOS | UEFI/U-Boot | **ARCS (SGI)** |
| **Syscalls** | syscall/sysret | svc/eret | syscall/eret |
| **Interrupts** | IDT + APIC | Vectors + GIC | Vectors + **INT2/INT3** |
| **MMU** | CR3 (PML4) | TTBR0/1 | CP0 EntryHi/Lo |
| **TLB** | invlpg/CR3 | tlbi | tlbwi/tlbwr |
| **TLB Size** | Variable (1536+) | Variable (128-1024) | **48-64 entries** |
| **I/O** | Port I/O + MMIO | MMIO only | MMIO only |
| **Timer** | APIC timer | Generic Timer | CP0 Count/Compare |
| **DMA** | Standard PCI DMA | Coherent | **Non-coherent** ⚠️ |
| **Cache** | PIPT | PIPT/VIPT | **VIVT** (manual mgmt) |

**⚠️ Critical Differences:**
- **Big-endian:** All multi-byte values reversed vs x86/ARM
- **ARCS boot:** SGI-specific firmware, different from UEFI
- **Non-coherent DMA:** Manual cache flushing required
- **VIVT cache:** Requires explicit cache management

## Migration Phases

### Phase 1: Trait Expansion (Weeks 1-2)
**Goal:** Define comprehensive trait interfaces

**Key Traits to Create:**
```rust
pub trait ControlRegisters {
    type PageTableRoot;
    fn read_page_table_root() -> Self::PageTableRoot;
    unsafe fn write_page_table_root(root: Self::PageTableRoot);
}

pub trait SystemRegisters {
    unsafe fn read_sys_reg(id: u32) -> u64;
    unsafe fn write_sys_reg(id: u32, value: u64);
}

pub trait SyscallInterface {
    type SyscallFrame;
    unsafe fn init_syscall_mechanism();
}

pub trait ExceptionHandler {
    type ExceptionFrame;
    type ExceptionVector;
    unsafe fn register_exception(vector: Self::ExceptionVector, handler: usize);
}

pub trait CacheOps {
    unsafe fn flush_cache();
    unsafe fn flush_cache_range(start: VirtAddr, len: usize);
    unsafe fn invalidate_cache_range(start: VirtAddr, len: usize);
    fn is_cache_coherent() -> bool; // False for SGI MIPS
}

pub trait BootProtocol {
    type BootInfo;
    unsafe fn early_init(boot_info: &Self::BootInfo);
}

// NEW: Endianness handling for SGI MIPS
pub trait Endianness {
    fn is_big_endian() -> bool;
    fn is_little_endian() -> bool {
        !Self::is_big_endian()
    }

    // Convert between native and specific endianness
    fn to_le16(val: u16) -> u16;
    fn to_le32(val: u32) -> u32;
    fn to_le64(val: u64) -> u64;
    fn from_le16(val: u16) -> u16;
    fn from_le32(val: u32) -> u32;
    fn from_le64(val: u64) -> u64;

    fn to_be16(val: u16) -> u16;
    fn to_be32(val: u32) -> u32;
    fn to_be64(val: u64) -> u64;
    fn from_be16(val: u16) -> u16;
    fn from_be32(val: u32) -> u32;
    fn from_be64(val: u64) -> u64;
}

// NEW: DMA operations for non-coherent systems (SGI)
pub trait DmaOps {
    fn is_dma_coherent() -> bool;
    unsafe fn dma_sync_for_device(addr: PhysAddr, len: usize);
    unsafe fn dma_sync_for_cpu(addr: PhysAddr, len: usize);
    unsafe fn dma_map(addr: VirtAddr, len: usize) -> PhysAddr;
}
```

**Deliverables:**
- Expanded `arch-traits/src/lib.rs`
- `arch-traits/src/context.rs` with common types
- `docs/arch/PORTING_GUIDE.md`
- `docs/arch/ARCH_COMPARISON.md`

### Phase 2: x86_64 Refactor (Weeks 3-5)
**Goal:** Refactor existing x86_64 to use new traits

**Critical Changes:**
- Implement all new traits in arch-x86_64
- Update drivers to use `PortIo` trait
- Make memory management use trait-based operations
- Maintain 100% functional compatibility

**Files to Modify:**
- `crates/arch/arch-x86_64/src/lib.rs` - Trait implementations
- `crates/arch/arch-x86_64/src/exceptions.rs` - Exception handling
- `crates/arch/arch-x86_64/src/syscall.rs` - Syscall mechanism
- `crates/drivers/pci/src/lib.rs` - Port I/O usage
- `crates/mm/mm-paging/src/lib.rs` - MMU operations

**Validation:** `make test` must pass with no regressions

### Phase 3: ARM64 Skeleton (Weeks 6-8)
**Goal:** Create stub ARM64 implementation

**Structure:**
```
crates/arch/arch-aarch64/
├── src/
│   ├── lib.rs           # Trait implementations
│   ├── boot/            # EL2→EL1 boot
│   ├── exceptions/      # Vector table
│   ├── syscalls/        # SVC mechanism
│   ├── interrupts/      # GIC
│   ├── mmu/             # Page tables
│   └── context.rs       # Context switch
```

**Key Implementations:**
- Exception vectors (2048-byte aligned)
- SVC-based syscalls
- TTBR0/1 page table management
- GIC stub for interrupts

### Phase 4: SGI MIPS64 Skeleton (Weeks 9-11) ⚠️ Extended for Big-Endian
**Goal:** Create stub SGI MIPS64 implementation with big-endian support

**Structure:**
```
crates/arch/arch-sgi-mips64/
├── src/
│   ├── lib.rs           # Trait implementations (big-endian)
│   ├── boot/
│   │   ├── mod.rs
│   │   └── arcs.rs      # ARCS firmware interface
│   ├── exceptions/
│   │   ├── mod.rs
│   │   └── vectors.rs   # Exception vectors
│   ├── syscalls/
│   │   ├── mod.rs
│   │   └── syscall.rs   # syscall/eret mechanism
│   ├── interrupts/
│   │   ├── mod.rs
│   │   ├── int2.rs      # INT2 controller (IP22/IP32)
│   │   └── int3.rs      # INT3 controller (IP27/IP30)
│   ├── mmu/
│   │   ├── mod.rs
│   │   ├── tlb.rs       # TLB management (48-64 entries)
│   │   └── kseg.rs      # KSEG0/1 addressing
│   ├── cache/
│   │   ├── mod.rs
│   │   └── vivt.rs      # VIVT cache management
│   ├── dma.rs           # Non-coherent DMA operations
│   ├── endian.rs        # Big-endian conversions
│   ├── context.rs       # Context switch
│   └── serial.rs        # NS16550 UART (Zilog for IP22)
```

**SGI-Specific Implementations:**

1. **Big-Endian Support** (`endian.rs`):
```rust
impl Endianness for SgiMips64 {
    fn is_big_endian() -> bool {
        true  // SGI systems are big-endian
    }

    fn to_le32(val: u32) -> u32 {
        val.swap_bytes()  // Convert BE→LE
    }

    fn from_le32(val: u32) -> u32 {
        val.swap_bytes()  // Convert LE→BE
    }

    // Native is big-endian, so no conversion needed
    fn to_be32(val: u32) -> u32 {
        val
    }

    fn from_be32(val: u32) -> u32 {
        val
    }
}
```

2. **ARCS Boot Protocol** (`boot/arcs.rs`):
```rust
// ARCS (Advanced RISC Computing Specification)
// Used by SGI PROM firmware

#[repr(C)]
pub struct ArcsBootInfo {
    pub mem_descriptors: *const ArcsMemDescriptor,
    pub mem_count: u32,
    pub firmware_vector: *const ArcsFirmwareVector,
    pub environment: *const *const u8,
}

#[repr(C)]
pub struct ArcsMemDescriptor {
    pub mem_type: u32,      // Free, BadMemory, LoadedProgram, etc.
    pub base_page: u32,     // Physical page number
    pub page_count: u32,    // Number of pages
}

pub unsafe fn parse_arcs_memory(info: &ArcsBootInfo) -> Vec<MemoryRegion> {
    // Parse ARCS memory descriptors
    // Convert to platform-independent format
    // Handle big-endian values
}
```

3. **Non-Coherent DMA** (`dma.rs`):
```rust
impl DmaOps for SgiMips64 {
    fn is_dma_coherent() -> bool {
        false  // SGI systems have non-coherent DMA
    }

    unsafe fn dma_sync_for_device(addr: PhysAddr, len: usize) {
        // Write-back cache before DMA write
        cache::writeback_range(addr, len);
    }

    unsafe fn dma_sync_for_cpu(addr: PhysAddr, len: usize) {
        // Invalidate cache after DMA read
        cache::invalidate_range(addr, len);
    }
}
```

4. **VIVT Cache Management** (`cache/vivt.rs`):
```rust
// Virtually Indexed, Virtually Tagged cache
// Requires manual management and alias handling

pub unsafe fn flush_cache_page(vaddr: VirtAddr) {
    asm!(
        ".set push",
        ".set noreorder",
        "cache 0x15, 0({0})",  // D-cache Hit Writeback Invalidate
        "cache 0x10, 0({0})",  // I-cache Hit Invalidate
        ".set pop",
        in(reg) vaddr.as_u64(),
        options(nostack)
    );
}

pub unsafe fn writeback_dcache_range(start: VirtAddr, len: usize) {
    let cache_line_size = 32;  // Typical for SGI systems
    let mut addr = start.align_down(cache_line_size);
    let end = (start + len).align_up(cache_line_size);

    while addr < end {
        asm!(
            "cache 0x15, 0({0})",  // Hit Writeback Invalidate
            in(reg) addr.as_u64()
        );
        addr += cache_line_size;
    }
}
```

5. **INT2/INT3 Interrupt Controllers**:
```rust
// INT2: Used on IP22 (Indy), IP26, IP28, IP32 (O2)
// INT3: Used on IP27 (Origin), IP30 (Octane)

pub struct Int2Controller {
    base: VirtAddr,
}

impl Int2Controller {
    const LOCAL0_STATUS: usize = 0x00;
    const LOCAL0_MASK: usize = 0x04;
    const LOCAL1_STATUS: usize = 0x08;
    const LOCAL1_MASK: usize = 0x0C;

    pub unsafe fn init(&mut self) {
        // Initialize INT2 controller
        // Big-endian MMIO access
    }

    pub unsafe fn enable_irq(&mut self, irq: u8) {
        let offset = if irq < 8 { Self::LOCAL0_MASK } else { Self::LOCAL1_MASK };
        let mut mask = self.read_be_u8(offset);
        mask |= 1 << (irq % 8);
        self.write_be_u8(offset, mask);
    }
}
```

6. **CP0 Register Access**:
```rust
impl SystemRegisters for SgiMips64 {
    unsafe fn read_sys_reg(cp0_reg: u32) -> u64 {
        let value: u64;
        match cp0_reg {
            0 => asm!("dmfc0 {}, $0", out(reg) value),   // Index
            2 => asm!("dmfc0 {}, $2", out(reg) value),   // EntryLo0
            3 => asm!("dmfc0 {}, $3", out(reg) value),   // EntryLo1
            4 => asm!("dmfc0 {}, $4", out(reg) value),   // Context
            5 => asm!("dmfc0 {}, $5", out(reg) value),   // PageMask
            8 => asm!("dmfc0 {}, $8", out(reg) value),   // BadVAddr
            9 => asm!("dmfc0 {}, $9", out(reg) value),   // Count
            10 => asm!("dmfc0 {}, $10", out(reg) value), // EntryHi
            11 => asm!("dmfc0 {}, $11", out(reg) value), // Compare
            12 => asm!("dmfc0 {}, $12", out(reg) value), // Status
            13 => asm!("dmfc0 {}, $13", out(reg) value), // Cause
            14 => asm!("dmfc0 {}, $14", out(reg) value), // EPC
            _ => panic!("Invalid CP0 register"),
        }
        value
    }

    unsafe fn write_sys_reg(cp0_reg: u32, value: u64) {
        match cp0_reg {
            0 => asm!("dmtc0 {}, $0", in(reg) value),   // Index
            2 => asm!("dmtc0 {}, $2", in(reg) value),   // EntryLo0
            3 => asm!("dmtc0 {}, $3", in(reg) value),   // EntryLo1
            // ... similar for other registers
            _ => panic!("Invalid CP0 register"),
        }
    }
}
```

**SGI Platform Detection:**
```rust
pub enum SgiPlatform {
    Ip22Indy,      // Indy, Indigo2
    Ip27Origin,    // Origin 200/2000, Onyx2
    Ip30Octane,    // Octane, Octane2
    Ip32O2,        // O2, O2+
}

pub fn detect_sgi_platform() -> SgiPlatform {
    // Read from ARCS firmware environment
    // Or detect via hardware registers
}
```

**Critical SGI-Specific Considerations:**

⚠️ **Endianness Everywhere:**
- Network protocols assume big-endian (network byte order = native)
- File formats (ELF, ext4) need byte swapping when reading/writing
- PCI configuration space access needs endian conversion
- Framebuffer pixel formats may differ

⚠️ **Cache Management:**
- Manual D-cache writeback before DMA
- Manual I-cache invalidation after code modification
- Handle cache aliases (VIVT can have multiple virtual addresses for same physical)
- Cache line size typically 32 bytes

⚠️ **TLB Management:**
- Only 48-64 TLB entries (vs 1536+ on x86)
- Requires careful TLB entry management
- Wired entries for kernel mappings
- ASID (Address Space ID) support for process isolation

⚠️ **Memory Layout:**
- KSEG0 (0x8000_0000_xxxx_xxxx): Unmapped cached
- KSEG1 (0xA000_0000_xxxx_xxxx): Unmapped uncached (for I/O)
- XKPHYS (0x8xxx_xxxx_xxxx_xxxx - 0xBxxx_xxxx_xxxx_xxxx): Mapped regions

**Build System Configuration for SGI:**

**Cargo.toml Feature Flags:**
```toml
[features]
default = []

# Architecture selection
arch-x86_64 = ["arch-x86_64"]
arch-aarch64 = ["arch-aarch64"]
arch-sgi-mips64 = ["arch-sgi-mips64"]

# SGI platform variants
sgi-ip22 = ["arch-sgi-mips64"]  # Indy, Indigo2
sgi-ip27 = ["arch-sgi-mips64"]  # Origin, Onyx2
sgi-ip30 = ["arch-sgi-mips64"]  # Octane
sgi-ip32 = ["arch-sgi-mips64"]  # O2

[dependencies]
arch-x86_64 = { path = "crates/arch/arch-x86_64", optional = true }
arch-aarch64 = { path = "crates/arch/arch-aarch64", optional = true }
arch-sgi-mips64 = { path = "crates/arch/arch-sgi-mips64", optional = true }
```

**Conditional Compilation:**
```rust
// kernel/src/main.rs
#[cfg(all(target_arch = "mips64", target_endian = "big"))]
use arch_sgi_mips64::SgiMips64 as ArchImpl;

#[cfg(all(target_arch = "mips64", target_endian = "big"))]
const ARCH_NAME: &str = "SGI MIPS64 (big-endian)";

// Conditional endian-aware code
#[cfg(target_endian = "little")]
fn read_u32_native(ptr: *const u32) -> u32 {
    unsafe { ptr.read() }
}

#[cfg(target_endian = "big")]
fn read_u32_native(ptr: *const u32) -> u32 {
    unsafe { ptr.read() }
}

// Reading little-endian data (e.g., from disk)
fn read_u32_le(ptr: *const u32) -> u32 {
    let val = unsafe { ptr.read() };
    #[cfg(target_endian = "big")]
    return u32::from_le(val);  // Swap on big-endian
    #[cfg(target_endian = "little")]
    return val;                // No-op on little-endian
}
```

**Rust Target Triples:**
```bash
# x86_64
x86_64-unknown-none

# ARM64 (little-endian)
aarch64-unknown-none

# MIPS64 big-endian (SGI)
mips64-unknown-linux-gnu
# or custom target:
mips64-sgi-none.json
```

**Custom Target Specification (`mips64-sgi-none.json`):**
```json
{
  "llvm-target": "mips64-unknown-none",
  "data-layout": "E-m:e-i8:8:32-i16:16:32-i64:64-n32:64-S128",
  "arch": "mips64",
  "target-endian": "big",
  "target-pointer-width": "64",
  "target-c-int-width": "32",
  "os": "none",
  "executables": true,
  "linker-flavor": "ld.lld",
  "linker": "rust-lld",
  "panic-strategy": "abort",
  "disable-redzone": true,
  "features": "+mips64r2,+soft-float",
  "relocation-model": "static",
  "code-model": "medium"
}
```

**Makefile Enhancements:**
```makefile
# Makefile
ARCH ?= x86_64
SGI_PLATFORM ?= ip22

# Target triple selection
ifeq ($(ARCH),x86_64)
    TARGET_TRIPLE = x86_64-unknown-none
    ARCH_FEATURES = --features arch-x86_64
    QEMU = qemu-system-x86_64
endif

ifeq ($(ARCH),aarch64)
    TARGET_TRIPLE = aarch64-unknown-none
    ARCH_FEATURES = --features arch-aarch64
    QEMU = qemu-system-aarch64 -M virt -cpu cortex-a57
endif

ifeq ($(ARCH),sgi-mips64)
    TARGET_TRIPLE = mips64-sgi-none
    ARCH_FEATURES = --features arch-sgi-mips64,sgi-$(SGI_PLATFORM)
    QEMU = qemu-system-mips64 -M malta
    ENDIAN_CHECK = @echo "Building for BIG-ENDIAN SGI MIPS64"
endif

build:
	$(ENDIAN_CHECK)
	cargo build --target $(TARGET_TRIPLE) $(ARCH_FEATURES)

# SGI-specific targets
build-sgi-ip22:
	$(MAKE) build ARCH=sgi-mips64 SGI_PLATFORM=ip22

build-sgi-octane:
	$(MAKE) build ARCH=sgi-mips64 SGI_PLATFORM=ip30

test-sgi:
	$(QEMU) -kernel target/$(TARGET_TRIPLE)/debug/kernel \
	    -serial stdio -display none
```

**Endian-Aware Data Structures:**
```rust
// Use byteorder crate or custom macros
use core::mem;

#[cfg(target_endian = "big")]
#[repr(C, packed)]
struct LittleEndianU32 {
    bytes: [u8; 4],
}

#[cfg(target_endian = "big")]
impl LittleEndianU32 {
    fn get(&self) -> u32 {
        u32::from_le_bytes(self.bytes)
    }

    fn set(&mut self, val: u32) {
        self.bytes = val.to_le_bytes();
    }
}

#[cfg(target_endian = "little")]
type LittleEndianU32 = u32;  // No conversion needed
```

**Driver Endian Handling:**
```rust
// Example: PCI configuration space
impl<E: Endianness> PciDevice<E> {
    pub fn read_config_u32(&self, offset: u8) -> u32 {
        let val = unsafe {
            let addr = self.config_base + offset as usize;
            core::ptr::read_volatile(addr as *const u32)
        };

        // PCI is always little-endian, convert if needed
        E::from_le32(val)
    }

    pub fn write_config_u32(&self, offset: u8, value: u32) {
        let val = E::to_le32(value);
        unsafe {
            let addr = self.config_base + offset as usize;
            core::ptr::write_volatile(addr as *mut u32, val);
        }
    }
}
```

**Deliverables:**
- Target specification file for SGI MIPS64
- Updated Cargo.toml with feature flags
- Makefile with multi-arch build support
- Endian-aware abstractions in core types
- Conditional compilation guards throughout codebase

---

### Phase 5: Kernel Abstraction (Weeks 12-14)
**Goal:** Make kernel generic over architecture

**Approach:**
```rust
pub fn kernel_main<A: Arch + ControlRegisters + ...>() {
    unsafe { A::init() }
    memory::init::<A>();
    scheduler::init::<A>();
    syscall::init::<A>();
    A::enable_interrupts();
    scheduler::run::<A>();
}

#[cfg(target_arch = "x86_64")]
use arch_x86_64::X86_64 as ArchImpl;

#[cfg(target_arch = "aarch64")]
use arch_aarch64::AArch64 as ArchImpl;

#[cfg(all(target_arch = "mips64", target_endian = "big"))]
use arch_sgi_mips64::SgiMips64 as ArchImpl;

#[no_mangle]
pub extern "C" fn kernel_entry() -> ! {
    kernel_main::<ArchImpl>()
}
```

**Driver Abstraction with Endianness:**
```rust
pub enum IoAccess {
    PortBased(u16),      // x86 only
    MemoryMapped(usize), // All architectures
}

pub trait DeviceIo<E: Endianness> {
    fn read_u32(&self, offset: IoAccess) -> u32;
    fn write_u32(&self, offset: IoAccess, value: u32);

    // Endian-aware reads (most hardware is little-endian)
    fn read_u32_le(&self, offset: IoAccess) -> u32 {
        E::from_le32(self.read_u32(offset))
    }

    fn write_u32_le(&self, offset: IoAccess, value: u32) {
        self.write_u32(offset, E::to_le32(value))
    }

    // Native endian reads
    fn read_u32_native(&self, offset: IoAccess) -> u32 {
        self.read_u32(offset)
    }
}

// Example usage:
let val = device.read_u32_le(IoAccess::MemoryMapped(0x1000));
// On x86/ARM: no conversion
// On SGI MIPS: byte swap
```

### Phase 6: Bootloader (Weeks 14-15)
**Goal:** Multi-protocol boot support

- UEFI for x86_64 (existing)
- UEFI for ARM64 (new)
- Device Tree parsing for both
- Unified boot info handoff

### Phase 7: Userspace (Weeks 16-17)
**Goal:** Architecture-specific userspace

- Syscall wrappers per arch
- Architecture-specific libc initialization
- Cross-compilation toolchain
- Updated build system

### Phase 8: Testing (Weeks 18-20)
**Goal:** Validation and benchmarking

- Comprehensive test suite
- QEMU testing for all architectures
- Performance benchmarks
- Integration testing

## Implementation Guidelines

### DO:
✅ Keep assembly localized to arch crates
✅ Use traits for all arch-specific operations
✅ Maintain zero-cost abstractions (inline trait methods)
✅ Test incrementally after each change
✅ Document architecture differences
✅ Use conditional compilation (`#[cfg(target_arch)]`)

### DON'T:
❌ Leak arch-specific code into kernel core
❌ Use runtime dispatch for performance-critical paths
❌ Assume x86_64 semantics (e.g., port I/O everywhere)
❌ Break existing x86_64 functionality
❌ Create arch-specific duplicates of common code

## Success Metrics

| Metric | Target |
|--------|--------|
| Architecture-agnostic kernel code | >80% |
| x86_64 functionality preserved | 100% |
| Assembly code in arch crates only | 100% |
| Multi-arch build (single command) | ✓ |
| QEMU boot all architectures | ✓ |
| Syscall overhead vs baseline | <5% |
| New arch LOC requirement | <1000 |

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Trait design inadequate | Validate with ARM64/MIPS64 early |
| x86_64 refactor breaks code | Incremental changes, full testing |
| Port I/O abstraction overhead | Inline methods, static dispatch |
| Cross-compilation complexity | Use LLVM, proven toolchain |

## Post-Migration Roadmap

**Short-term (6-9 months):**
- ARM64 on real hardware (Raspberry Pi 4)
- MIPS64 on Malta/Octeon
- RISC-V support

**Medium-term (10-18 months):**
- ARM32 for embedded
- Architecture-specific optimizations
- SIMD per architecture

**Long-term (Year 2+):**
- eBPF JIT per arch
- Userspace emulation
- Cross-arch virtualization

## References

- **Assembly Analysis:** See explore agent output (agent ID: a372255)
- **Detailed Plan:** See plan agent output (agent ID: a5ff926)
- **Current Code:** `crates/arch/arch-x86_64/src/`
- **Trait Definitions:** `crates/arch/arch-traits/src/lib.rs`

---

**Next Steps:**
1. Review this plan with team
2. Begin Phase 1: Trait expansion
3. Set up tracking for each phase milestone
4. Establish testing baseline for x86_64

**Estimated Completion:** 22 weeks from start date (extended for SGI big-endian support)
**Priority:** Medium (architectural foundation for future growth)

**Timeline Breakdown:**
- Trait design & x86_64 refactor: 5 weeks
- ARM64 skeleton: 3 weeks
- SGI MIPS64 skeleton (with endianness): 3 weeks (+1 week for big-endian)
- Kernel/driver/bootloader abstraction: 7 weeks
- Testing & validation: 3 weeks
- Buffer: 1 week

---

*This plan was generated by analyzing 8,575+ lines of assembly code across 47 files in the OXIDE OS codebase. All assembly has been inventoried and categorized for systematic migration.*
