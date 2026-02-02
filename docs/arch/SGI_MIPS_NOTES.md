# SGI MIPS64 Architecture Notes

**Platform:** Silicon Graphics (SGI) Workstations
**Architecture:** MIPS64 (R4000, R5000, R10000, R12000, R14000)
**Endianness:** **Big-endian** ⚠️
**Boot Firmware:** ARCS (Advanced RISC Computing Specification)
**Last Updated:** 2026-02-02

## Target SGI Platforms

| Platform | CPU | Description | Notable Hardware |
|----------|-----|-------------|------------------|
| **IP22** | R4000-R5000 | Indy, Indigo2 | INT2, Zilog UART, Newport/XL graphics |
| **IP27** | R10000-R14000 | Origin 200/2000, Onyx2 | INT3, NUMA, HUB/BEDROCK |
| **IP30** | R10000-R14000 | Octane, Octane2 | INT3, HEART, V6/V8/V10/V12 graphics |
| **IP32** | R5000-R12000 | O2, O2+ | INT2/CRM, 1394, unified memory |

## Critical Architectural Differences vs x86/ARM

### 1. Big-Endian ⚠️

**Impact:** EVERYTHING that deals with multi-byte values

**Affected Areas:**
- Disk I/O (filesystems, partition tables)
- Network protocols (fortunately network byte order = big-endian)
- Binary file parsing (ELF, executables)
- Structure packing/unpacking
- Device MMIO (most hardware is little-endian)
- Framebuffer pixel formats

**Example:**
```rust
// Reading a little-endian u32 from disk (ext4, ELF, etc.)
#[cfg(target_endian = "big")]
fn read_u32_le(data: &[u8]) -> u32 {
    u32::from_le_bytes([data[0], data[1], data[2], data[3]])
}

#[cfg(target_endian = "little")]
fn read_u32_le(data: &[u8]) -> u32 {
    unsafe { *(data.as_ptr() as *const u32) }  // Direct cast
}
```

**Network Advantage:**
Network byte order IS big-endian, so:
- `htonl()` / `ntohl()` are no-ops on SGI
- TCP/IP checksums can be computed natively
- Protocol headers don't need byte swapping

### 2. Non-Coherent DMA ⚠️

**Cache Model:** VIVT (Virtually Indexed, Virtually Tagged)

**Requirements:**
- Manual cache writeback before DMA write
- Manual cache invalidation after DMA read
- Handle cache aliases (same physical page at multiple virtual addresses)

**Code Pattern:**
```rust
// Before DMA write (device reads from memory)
unsafe {
    cache::writeback_dcache_range(vaddr, len);
}
dma_controller.start_write(phys_addr, len);

// After DMA read (device writes to memory)
dma_controller.wait_complete();
unsafe {
    cache::invalidate_dcache_range(vaddr, len);
}
```

**Cache Operations:**
```rust
pub unsafe fn writeback_dcache_range(start: VirtAddr, len: usize) {
    let line_size = 32;  // SGI typical
    let mut addr = start.align_down(line_size);
    let end = (start + len).align_up(line_size);

    while addr < end {
        asm!("cache 0x15, 0({0})", in(reg) addr.as_u64());  // Hit WB Inv
        addr += line_size;
    }
}

pub unsafe fn invalidate_icache_range(start: VirtAddr, len: usize) {
    let line_size = 32;
    let mut addr = start.align_down(line_size);
    let end = (start + len).align_up(line_size);

    while addr < end {
        asm!("cache 0x10, 0({0})", in(reg) addr.as_u64());  // Hit Invalidate
        addr += line_size;
    }
}
```

### 3. ARCS Boot Firmware

**Not UEFI, Not Device Tree, Not traditional BIOS**

**ARCS Provides:**
- Memory map (ARCS memory descriptors)
- Firmware callbacks (console I/O, disk access)
- Environment variables (console device, boot path)
- Boot parameters

**Memory Descriptor Types:**
```c
typedef enum {
    ExceptionBlock,      // ROM exception vectors
    SPBPage,             // System Parameter Block
    FreeMemory,          // Available RAM
    FirmwareTemporary,   // PROM temporary storage
    FirmwarePermanent,   // PROM data
    FreeContiguous,      // Contiguous free memory
    BadMemory,           // Defective memory
    LoadedProgram,       // Loaded program/kernel
    FirmwareCode         // PROM code
} MEMORYTYPE;

struct MEMORYDESCRIPTOR {
    MEMORYTYPE MemoryType;
    ULONG BasePage;      // Physical page number
    ULONG PageCount;     // Number of pages
};
```

**Boot Sequence:**
```
1. SGI PROM initializes hardware
2. PROM loads bootloader from disk (volume header)
3. Bootloader receives ARCS memory map
4. Bootloader loads kernel
5. Bootloader jumps to kernel with ARCS info pointer
6. Kernel parses ARCS structures
7. Kernel switches to native memory management
```

**Parsing ARCS in Rust:**
```rust
#[repr(C)]
struct ArcsMemDescriptor {
    mem_type: u32,
    base_page: u32,
    page_count: u32,
}

pub unsafe fn parse_arcs_memory(arcs_ptr: *const u8) -> Vec<MemoryRegion> {
    // Read ARCS structures
    // Convert to platform-independent MemoryRegion
    // Handle big-endian values
}
```

### 4. Memory Layout (MIPS64)

**Address Space Segments:**

| Segment | Range | Properties | Use |
|---------|-------|------------|-----|
| **KUSEG** | 0x0000_0000_0000_0000 - 0x0000_00FF_FFFF_FFFF | User, mapped | User processes |
| **KSEG0** | 0xFFFF_FFFF_8000_0000 - 0xFFFF_FFFF_9FFF_FFFF | Kernel, unmapped, cached | Kernel code/data |
| **KSEG1** | 0xFFFF_FFFF_A000_0000 - 0xFFFF_FFFF_BFFF_FFFF | Kernel, unmapped, uncached | Device I/O |
| **KSSEG** | 0xFFFF_FFFF_C000_0000 - 0xFFFF_FFFF_DFFF_FFFF | Kernel, mapped | Kernel heap |
| **KSEG3** | 0xFFFF_FFFF_E000_0000 - 0xFFFF_FFFF_FFFF_FFFF | Kernel, mapped | Per-CPU data |
| **XKPHYS** | 0x8000_0000_0000_0000 - 0xBFFF_FFFF_FFFF_FFFF | Kernel, unmapped, cache-attr | Large physical map |

**XKPHYS Cache Attributes:**
```
0x8xxx_xxxx_xxxx_xxxx = Cached (coherent)
0x9xxx_xxxx_xxxx_xxxx = Uncached
0xAxxx_xxxx_xxxx_xxxx = Uncached accelerated
0xBxxx_xxxx_xxxx_xxxx = Cached non-coherent
```

**Typical Kernel Layout:**
```
KSEG0: 0xFFFF_FFFF_8000_0000
  - Kernel code (loaded by bootloader)
  - Kernel data
  - Initial stack
  - Direct-mapped RAM (512MB typically)

KSEG1: 0xFFFF_FFFF_A000_0000
  - MMIO device registers
  - No caching, ensures writes are immediate
```

### 5. TLB Characteristics

**Size:** 48-64 entries (much smaller than x86's 1536+)

**Entry Structure:**
```
EntryHi:  VPN2 (Virtual Page Number), ASID
EntryLo0: PFN (Physical Frame Number), Coherency, Dirty, Valid, Global
EntryLo1: PFN, Coherency, Dirty, Valid, Global (even/odd pages)
PageMask: Page size (4K, 16K, 64K, 256K, 1M, 4M, 16M)
```

**Operations:**
- `tlbwi` - Write indexed (specific entry)
- `tlbwr` - Write random (random entry, not wired)
- `tlbr` - Read TLB entry
- `tlbp` - Probe for entry

**ASID (Address Space ID):**
- 8-bit value to avoid TLB flush on context switch
- 256 possible ASIDs
- Kernel uses ASID 0 (global bit set)
- User processes get ASID 1-255

**Code Example:**
```rust
pub unsafe fn write_tlb_entry(index: u8, entry: TlbEntry) {
    asm!(
        "dmtc0 {index}, $0",      // Index register
        "dmtc0 {hi}, $10",        // EntryHi
        "dmtc0 {lo0}, $2",        // EntryLo0
        "dmtc0 {lo1}, $3",        // EntryLo1
        "dmtc0 {mask}, $5",       // PageMask
        "tlbwi",                  // Write indexed
        index = in(reg) index,
        hi = in(reg) entry.hi,
        lo0 = in(reg) entry.lo0,
        lo1 = in(reg) entry.lo1,
        mask = in(reg) entry.page_mask,
    );
}
```

### 6. Interrupt Controllers

**IP22/IP32: INT2**
- Local interrupts (2 banks)
- 16 interrupt sources
- Memory-mapped registers

**IP27/IP30: INT3 (HUB/HEART)**
- More sophisticated
- NUMA-aware on IP27
- PCI interrupt routing

**Common Pattern:**
```rust
pub struct Int2 {
    base: VirtAddr,  // KSEG1 address (uncached)
}

impl Int2 {
    pub unsafe fn enable_irq(&mut self, irq: u8) {
        let reg = if irq < 8 { LOCAL0_MASK } else { LOCAL1_MASK };
        let mut mask = self.read_u8(reg);
        mask |= 1 << (irq & 7);
        self.write_u8(reg, mask);
    }

    fn read_u8(&self, offset: usize) -> u8 {
        let addr = (self.base.as_u64() + offset as u64) as *const u8;
        unsafe { core::ptr::read_volatile(addr) }
    }

    fn write_u8(&self, offset: usize, val: u8) {
        let addr = (self.base.as_u64() + offset as u64) as *mut u8;
        unsafe { core::ptr::write_volatile(addr, val) }
    }
}
```

### 7. Serial Console

**IP22:** Zilog Z85C30 SCC (not 16550!)
**IP27/IP30/IP32:** NS16550-compatible UART

**Zilog SCC is WEIRD:**
- Two channels (A and B)
- Register access via command/data ports
- Big-endian
- Different initialization sequence

### 8. Build Configuration

**Target Triple:** `mips64-unknown-linux-gnu` or custom `mips64-sgi-none`

**Rustc Flags:**
```bash
RUSTFLAGS="-C target-cpu=mips64r2 \
           -C target-feature=+mips64r2,-mips64r6 \
           -C relocation-model=static \
           -C code-model=medium"
```

**Required Features:**
- `+mips64r2` - MIPS64 Release 2 ISA
- `+soft-float` - No hardware floating point in kernel
- `-mips64r6` - Explicitly disable R6 (different ISA)

**Linker Script Considerations:**
```ld
OUTPUT_ARCH(mips)
ENTRY(_start)

SECTIONS {
    . = 0xFFFFFFFF80000000;  /* KSEG0 base */

    .text : {
        *(.text.boot)
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .data : {
        *(.data .data.*)
    }

    .bss : {
        *(.bss .bss.*)
    }
}
```

## Testing

**QEMU:**
```bash
qemu-system-mips64 -M malta \
    -cpu MIPS64R2-generic \
    -kernel kernel.elf \
    -serial stdio \
    -nographic
```

**Note:** Malta is NOT an SGI system, but useful for initial testing.

**Real Hardware Testing:**
- IP22 Indy/Indigo2 (easiest to obtain)
- Serial console required
- NetBSD or Linux for comparison testing

## Common Gotchas

1. **Forgetting byte swaps** when reading disk structures
2. **Cache not flushed** before/after DMA
3. **TLB too small** - need careful management
4. **KSEG1 vs KSEG0** confusion - wrong caching
5. **Delay slots** after branches (assembler handles this)
6. **ARCS parsing** - structures are complex and pointer-heavy

## References

- MIPS64 Architecture For Programmers (Imagination Technologies)
- SGI Technical Publications Library
- Linux/MIPS source code (arch/mips/sgi-ip22, ip27, ip30, ip32)
- NetBSD/sgimips source code
- ARCS Specification (available from archive.org)

## TODO for Implementation

- [ ] Create `mips64-sgi-none.json` target spec
- [ ] Implement `Endianness` trait
- [ ] ARCS boot info parser
- [ ] INT2/INT3 interrupt controller drivers
- [ ] TLB manager (handle 48-64 entry limit)
- [ ] Cache operation wrappers
- [ ] Zilog SCC driver (IP22)
- [ ] DMA helper functions with cache sync
- [ ] Test on QEMU Malta
- [ ] Test on real SGI hardware

---

**Critical:** Big-endian support affects EVERY part of the OS that deals with structured data. Plan for comprehensive testing of endian conversions.
