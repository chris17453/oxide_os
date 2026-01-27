# OXIDE Boot Specification

**Version:** 0.1.0
**Status:** Draft

---

## 1) Overview

This specification defines the boot sequence for OXIDE across all supported architectures. The boot process is divided into platform-specific early boot and architecture-independent kernel initialization.

### 1.1 Boot Phases

```
┌─────────────────────────────────────────────────────────────────┐
│                      BOOT PHASES                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Phase 0: Firmware (BIOS/UEFI/OpenSBI/ARCS/U-Boot)              │
│     │                                                           │
│     ▼                                                           │
│  Phase 1: Bootloader (optional - GRUB, systemd-boot, etc.)      │
│     │                                                           │
│     ▼                                                           │
│  Phase 2: Kernel Entry (arch-specific assembly)                 │
│     │                                                           │
│     ▼                                                           │
│  Phase 3: Early Init (minimal C/Rust, no allocator)             │
│     │                                                           │
│     ▼                                                           │
│  Phase 4: Memory Init (physical allocator, page tables)         │
│     │                                                           │
│     ▼                                                           │
│  Phase 5: Kernel Init (heap, scheduler, VFS)                    │
│     │                                                           │
│     ▼                                                           │
│  Phase 6: Driver Init (device tree, probe)                      │
│     │                                                           │
│     ▼                                                           │
│  Phase 7: User Init (init process)                              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2) Supported Boot Protocols

### 2.1 Protocol Matrix

| Architecture | Primary Protocol | Alternatives |
|--------------|------------------|--------------|
| x86_64 | UEFI | Multiboot2, Linux Boot |
| i686 | Multiboot2 | BIOS direct, UEFI |
| AArch64 | UEFI + DTB | Linux Boot, bare metal |
| ARM32 | DTB + U-Boot | Linux zImage |
| MIPS64 | ARCS | U-Boot, bare metal |
| MIPS32 | U-Boot | YAMON, bare metal |
| RISC-V 64 | OpenSBI + DTB | U-Boot |
| RISC-V 32 | OpenSBI + DTB | U-Boot |

### 2.2 Boot Protocol Trait

```rust
/// Information passed from bootloader to kernel
pub struct BootInfo {
    /// Physical memory map
    pub memory_map: &'static [MemoryRegion],

    /// Command line arguments
    pub cmdline: Option<&'static str>,

    /// Initrd/initramfs location
    pub initrd: Option<PhysicalRange>,

    /// Framebuffer info (if available)
    pub framebuffer: Option<FramebufferInfo>,

    /// Device tree blob (ARM, RISC-V, MIPS)
    pub dtb: Option<PhysAddr>,

    /// ACPI RSDP (x86, some ARM)
    pub acpi_rsdp: Option<PhysAddr>,

    /// SMBIOS entry (x86)
    pub smbios: Option<PhysAddr>,

    /// EFI system table (UEFI boot)
    pub efi_system_table: Option<PhysAddr>,

    /// Kernel physical/virtual addresses
    pub kernel_phys: PhysAddr,
    pub kernel_virt: VirtAddr,
    pub kernel_size: usize,
}

#[repr(C)]
pub struct MemoryRegion {
    pub start: PhysAddr,
    pub size: usize,
    pub kind: MemoryKind,
}

#[repr(u32)]
pub enum MemoryKind {
    Usable = 0,
    Reserved = 1,
    AcpiReclaimable = 2,
    AcpiNvs = 3,
    BadMemory = 4,
    Bootloader = 5,
    Kernel = 6,
    Framebuffer = 7,
}
```

---

## 3) Kernel Entry Requirements

### 3.1 Entry Point Signature

```rust
/// Architecture-specific entry point
/// Called directly from bootloader or early assembly stub
#[no_mangle]
pub extern "C" fn _start(boot_info: *const BootInfo) -> ! {
    // Phase 2-3: Early initialization
    arch::early_init(boot_info);

    // Phase 4: Memory initialization
    memory::init(boot_info);

    // Phase 5-7: Main kernel initialization
    kernel_main();
}
```

### 3.2 Entry State Requirements

| Requirement | x86_64 | AArch64 | MIPS64 | RISC-V |
|-------------|--------|---------|--------|--------|
| CPU Mode | Long Mode | EL1 | Kernel Mode | S-Mode |
| MMU | Off or Identity | Off or Identity | Off | Off |
| Interrupts | Disabled | Disabled | Disabled | Disabled |
| FPU | Undefined | Undefined | Undefined | Undefined |
| Stack | Provided | Provided | Provided | Provided |
| BSS | May need clear | May need clear | May need clear | May need clear |

---

## 4) Early Initialization

### 4.1 Early Init Trait

```rust
pub trait ArchEarlyInit {
    /// Minimal setup before memory allocator
    /// - Clear BSS
    /// - Set up emergency console
    /// - Parse boot info
    unsafe fn early_init(boot_info: *const BootInfo);

    /// Validate CPU features
    fn check_cpu_features() -> Result<(), &'static str>;

    /// Set up minimal exception handling
    unsafe fn setup_early_exceptions();
}
```

### 4.2 BSS Clearing

```rust
/// Clear BSS section
/// Must be called before any static variables are used
pub unsafe fn clear_bss() {
    extern "C" {
        static mut __bss_start: u8;
        static mut __bss_end: u8;
    }

    let start = &mut __bss_start as *mut u8;
    let end = &mut __bss_end as *mut u8;
    let size = end as usize - start as usize;

    core::ptr::write_bytes(start, 0, size);
}
```

### 4.3 Early Console

```rust
/// Emergency console for early boot debugging
pub trait EarlyConsole {
    /// Write a single character
    fn putc(&mut self, c: u8);

    /// Write a string
    fn puts(&mut self, s: &str) {
        for b in s.bytes() {
            self.putc(b);
        }
    }
}

/// Platform-specific early console
/// - x86: Serial port (COM1) or VGA text
/// - ARM: UART (from DTB or hardcoded)
/// - MIPS: ARCS console or serial
/// - RISC-V: SBI console or UART
pub static EARLY_CONSOLE: SpinLock<Option<&'static mut dyn EarlyConsole>> =
    SpinLock::new(None);

#[macro_export]
macro_rules! early_println {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        if let Some(console) = EARLY_CONSOLE.lock().as_mut() {
            let _ = writeln!(console, $($arg)*);
        }
    }};
}
```

---

## 5) Memory Map Parsing

### 5.1 Memory Map Sources

| Platform | Source | Format |
|----------|--------|--------|
| UEFI | GetMemoryMap() | EFI_MEMORY_DESCRIPTOR |
| Multiboot2 | Memory map tag | multiboot_mmap_entry |
| Device Tree | /memory nodes | DTB property |
| ARCS | GetMemoryDescriptor() | MEMORYDESCRIPTOR |
| E820 (BIOS) | INT 15h E820 | e820_entry |

### 5.2 Unified Memory Map

```rust
/// Convert platform memory map to unified format
pub trait MemoryMapParser {
    fn parse(raw: *const u8, size: usize) -> Vec<MemoryRegion>;
}

/// E820 to unified
impl MemoryMapParser for E820Parser {
    fn parse(raw: *const u8, size: usize) -> Vec<MemoryRegion> {
        let entries = unsafe {
            core::slice::from_raw_parts(
                raw as *const E820Entry,
                size / core::mem::size_of::<E820Entry>()
            )
        };

        entries.iter().map(|e| MemoryRegion {
            start: PhysAddr::new(e.base),
            size: e.length as usize,
            kind: match e.kind {
                1 => MemoryKind::Usable,
                2 => MemoryKind::Reserved,
                3 => MemoryKind::AcpiReclaimable,
                4 => MemoryKind::AcpiNvs,
                5 => MemoryKind::BadMemory,
                _ => MemoryKind::Reserved,
            },
        }).collect()
    }
}
```

---

## 6) Page Table Setup

### 6.1 Initial Mapping Requirements

```rust
/// Minimum mappings needed for kernel startup
pub struct InitialMappings {
    /// Identity map for transition (temporary)
    pub identity: bool,

    /// Kernel text/data in higher half
    pub kernel_higher_half: VirtAddr,

    /// Direct physical memory map (optional)
    pub phys_map: Option<VirtAddr>,

    /// Stack
    pub stack_top: VirtAddr,
    pub stack_size: usize,
}

/// Architecture-specific initial page table setup
pub trait InitialPageTables {
    /// Create minimal page tables for kernel
    unsafe fn setup_initial_tables(
        mappings: &InitialMappings,
        memory_map: &[MemoryRegion],
    ) -> *mut PageTable;

    /// Activate page tables and jump to higher half
    unsafe fn activate_and_jump(
        tables: *mut PageTable,
        entry: VirtAddr,
    ) -> !;
}
```

### 6.2 Higher Half Kernel

```rust
/// Standard virtual memory layout
pub const KERNEL_VIRT_BASE: VirtAddr = VirtAddr::new(0xFFFF_8000_0000_0000);  // x86_64
pub const PHYS_MAP_BASE: VirtAddr = VirtAddr::new(0xFFFF_8880_0000_0000);     // Direct map

/// Kernel linker script regions
extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
    static __data_start: u8;
    static __data_end: u8;
    static __bss_start: u8;
    static __bss_end: u8;
}
```

---

## 7) Device Discovery

### 7.1 Discovery Methods

| Method | Platforms | Description |
|--------|-----------|-------------|
| ACPI | x86, ARM server | Hardware enumeration tables |
| Device Tree | ARM, RISC-V, MIPS | Flattened device tree blob |
| PCI Enumeration | x86, ARM, RISC-V | Bus scanning |
| Platform Data | Embedded | Compile-time device list |

### 7.2 Device Tree Parsing

```rust
pub struct DeviceTree<'a> {
    blob: &'a [u8],
}

impl<'a> DeviceTree<'a> {
    /// Parse DTB from boot info
    pub fn from_boot_info(boot_info: &BootInfo) -> Option<Self> {
        let dtb_addr = boot_info.dtb?;
        // Map DTB, validate header, return parser
        todo!()
    }

    /// Find node by path
    pub fn find_node(&self, path: &str) -> Option<DeviceNode<'a>>;

    /// Get memory regions
    pub fn memory_regions(&self) -> impl Iterator<Item = MemoryRegion>;

    /// Get chosen node (cmdline, initrd)
    pub fn chosen(&self) -> Option<ChosenNode<'a>>;

    /// Iterate all nodes
    pub fn nodes(&self) -> impl Iterator<Item = DeviceNode<'a>>;
}
```

### 7.3 ACPI Parsing

```rust
pub struct AcpiTables {
    rsdp: PhysAddr,
}

impl AcpiTables {
    pub fn from_boot_info(boot_info: &BootInfo) -> Option<Self> {
        Some(Self { rsdp: boot_info.acpi_rsdp? })
    }

    /// Get MADT (interrupt controller info)
    pub fn madt(&self) -> Option<&Madt>;

    /// Get FADT (power management)
    pub fn fadt(&self) -> Option<&Fadt>;

    /// Get HPET table
    pub fn hpet(&self) -> Option<&Hpet>;

    /// Get MCFG (PCIe configuration)
    pub fn mcfg(&self) -> Option<&Mcfg>;
}
```

---

## 8) SMP Initialization

### 8.1 AP (Application Processor) Boot

```rust
pub trait SmpBoot {
    /// Get number of CPUs from firmware
    fn cpu_count() -> usize;

    /// Boot a single AP
    unsafe fn boot_ap(cpu_id: usize, entry: VirtAddr, stack: VirtAddr) -> Result<()>;

    /// Boot all APs
    unsafe fn boot_all_aps() -> Result<usize> {
        let count = Self::cpu_count();
        let mut booted = 0;

        for cpu in 1..count {  // Skip BSP (0)
            let stack = allocate_ap_stack(cpu);
            if Self::boot_ap(cpu, ap_entry as VirtAddr, stack).is_ok() {
                booted += 1;
            }
        }

        Ok(booted)
    }
}
```

### 8.2 Platform-Specific AP Boot

| Platform | Method |
|----------|--------|
| x86 | INIT-SIPI-SIPI sequence |
| ARM | PSCI or spin-table |
| MIPS | Platform-specific |
| RISC-V | HSM SBI extension |

---

## 9) Init Process Launch

### 9.1 Transition to User Mode

```rust
/// Final kernel initialization before init
pub fn start_init() -> ! {
    // Load init executable
    let init_path = boot_cmdline_get("init").unwrap_or("/sbin/init");
    let init_binary = vfs::read(init_path).expect("Failed to load init");

    // Parse ELF
    let elf = Elf::parse(&init_binary).expect("Invalid init ELF");

    // Create init process
    let init_proc = Process::new_init();

    // Set up address space
    for segment in elf.segments() {
        init_proc.map_segment(segment);
    }

    // Set up stack with argv, envp, auxv
    let user_stack = init_proc.setup_user_stack(&["init"], &[], &elf);

    // Jump to user mode
    arch::enter_usermode(elf.entry_point(), user_stack);
}
```

### 9.2 Auxiliary Vector

```rust
/// Auxiliary vector entries passed to init
#[repr(C)]
pub struct AuxEntry {
    pub typ: usize,
    pub val: usize,
}

pub const AT_NULL: usize = 0;
pub const AT_PHDR: usize = 3;      // Program headers address
pub const AT_PHENT: usize = 4;     // Size of program header entry
pub const AT_PHNUM: usize = 5;     // Number of program headers
pub const AT_PAGESZ: usize = 6;    // Page size
pub const AT_BASE: usize = 7;      // Interpreter base address
pub const AT_ENTRY: usize = 9;     // Entry point
pub const AT_UID: usize = 11;      // Real UID
pub const AT_EUID: usize = 12;     // Effective UID
pub const AT_GID: usize = 13;      // Real GID
pub const AT_EGID: usize = 14;     // Effective GID
pub const AT_RANDOM: usize = 25;   // Address of 16 random bytes
pub const AT_EXECFN: usize = 31;   // Filename of program
```

---

## 10) Architecture-Specific Boot

See architecture-specific boot documentation:

- [x86_64 Boot](arch/x86_64/BOOT.md)
- [i686 Boot](arch/i686/BOOT.md)
- [AArch64 Boot](arch/aarch64/BOOT.md)
- [ARM Boot](arch/arm/BOOT.md)
- [MIPS64 Boot](arch/mips64/BOOT.md)
- [MIPS32 Boot](arch/mips32/BOOT.md)
- [RISC-V 64 Boot](arch/riscv64/BOOT.md)
- [RISC-V 32 Boot](arch/riscv32/BOOT.md)

---

## 11) Boot Command Line

### 11.1 Supported Parameters

| Parameter | Description | Example |
|-----------|-------------|---------|
| init= | Init program path | init=/sbin/init |
| root= | Root filesystem | root=/dev/sda1 |
| rootfstype= | Root FS type | rootfstype=ext4 |
| console= | Console device | console=ttyS0,115200 |
| mem= | Memory limit | mem=512M |
| debug | Enable debug output | debug |
| quiet | Suppress boot messages | quiet |
| nosmp | Disable SMP | nosmp |
| noacpi | Disable ACPI | noacpi |

### 11.2 Parsing

```rust
pub struct CmdLine<'a> {
    raw: &'a str,
}

impl<'a> CmdLine<'a> {
    pub fn get(&self, key: &str) -> Option<&str> {
        for part in self.raw.split_whitespace() {
            if let Some((k, v)) = part.split_once('=') {
                if k == key {
                    return Some(v);
                }
            }
        }
        None
    }

    pub fn has_flag(&self, flag: &str) -> bool {
        self.raw.split_whitespace().any(|p| p == flag)
    }
}
```

---

## 12) Exit Criteria

- [ ] UEFI boot working on x86_64
- [ ] Multiboot2 boot working on i686
- [ ] DTB parsing working on ARM/RISC-V
- [ ] ARCS boot working on MIPS
- [ ] Memory map correctly parsed on all platforms
- [ ] Higher-half kernel mapping working
- [ ] SMP boot working on all multi-core platforms
- [ ] Init process successfully launched

---

*End of Boot Specification*
