//! Global Descriptor Table (GDT) for x86_64
//!
//! Sets up the GDT with kernel code/data segments and TSS.

use core::mem::size_of;

/// Kernel code segment selector
pub const KERNEL_CS: u16 = 0x08;

/// Kernel data segment selector
pub const KERNEL_DS: u16 = 0x10;

/// User data segment selector (with RPL=3)
pub const USER_DS: u16 = 0x1B; // 0x18 | 3

/// User code segment selector (with RPL=3)
pub const USER_CS: u16 = 0x23; // 0x20 | 3

/// TSS segment selector
pub const TSS_SELECTOR: u16 = 0x28;

/// GDT entry
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GdtEntry {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
}

impl GdtEntry {
    /// Create a null GDT entry
    pub const fn null() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
        }
    }

    /// Create a kernel code segment (64-bit long mode)
    pub const fn kernel_code() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access: 0x9A,      // Present, Ring 0, Code, Execute/Read
            granularity: 0xAF, // Long mode, 4KB granularity
            base_high: 0,
        }
    }

    /// Create a kernel data segment
    pub const fn kernel_data() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access: 0x92,      // Present, Ring 0, Data, Read/Write
            granularity: 0xCF, // 4KB granularity, 32-bit (ignored in long mode)
            base_high: 0,
        }
    }

    /// Create a user data segment
    pub const fn user_data() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access: 0xF2,      // Present, Ring 3, Data, Read/Write
            granularity: 0xCF, // 4KB granularity
            base_high: 0,
        }
    }

    /// Create a user code segment (64-bit long mode)
    pub const fn user_code() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access: 0xFA,      // Present, Ring 3, Code, Execute/Read
            granularity: 0xAF, // Long mode, 4KB granularity
            base_high: 0,
        }
    }
}

/// TSS entry (16 bytes in 64-bit mode)
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct TssEntry {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
    base_upper: u32,
    reserved: u32,
}

impl TssEntry {
    /// Create a null TSS entry
    pub const fn null() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
            base_upper: 0,
            reserved: 0,
        }
    }

    /// Create a TSS entry from a TSS pointer
    pub fn new(tss: &'static TaskStateSegment) -> Self {
        let base = tss as *const _ as u64;
        let limit = (size_of::<TaskStateSegment>() - 1) as u16;

        Self {
            limit_low: limit,
            base_low: base as u16,
            base_middle: (base >> 16) as u8,
            access: 0x89, // Present, 64-bit TSS (available)
            granularity: 0,
            base_high: (base >> 24) as u8,
            base_upper: (base >> 32) as u32,
            reserved: 0,
        }
    }
}

/// Task State Segment (TSS)
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct TaskStateSegment {
    reserved0: u32,
    /// Privilege stack table (RSP for each privilege level)
    pub rsp: [u64; 3],
    reserved1: u64,
    /// Interrupt stack table
    pub ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    /// I/O map base address
    pub iomap_base: u16,
}

impl TaskStateSegment {
    /// Create a new empty TSS
    pub const fn new() -> Self {
        Self {
            reserved0: 0,
            rsp: [0; 3],
            reserved1: 0,
            ist: [0; 7],
            reserved2: 0,
            reserved3: 0,
            iomap_base: size_of::<TaskStateSegment>() as u16,
        }
    }
}

/// GDT structure with all entries
///
/// Layout:
/// - 0x00: Null
/// - 0x08: Kernel Code (KERNEL_CS)
/// - 0x10: Kernel Data (KERNEL_DS)
/// - 0x18: User Data (USER_DS = 0x1B with RPL=3)
/// - 0x20: User Code (USER_CS = 0x23 with RPL=3)
/// - 0x28: TSS (16 bytes)
#[repr(C, packed)]
pub struct Gdt {
    null: GdtEntry,
    kernel_code: GdtEntry,
    kernel_data: GdtEntry,
    user_data: GdtEntry,
    user_code: GdtEntry,
    tss: TssEntry,
}

impl Gdt {
    /// Create a new GDT with kernel and user segments
    pub const fn new() -> Self {
        Self {
            null: GdtEntry::null(),
            kernel_code: GdtEntry::kernel_code(),
            kernel_data: GdtEntry::kernel_data(),
            user_data: GdtEntry::user_data(),
            user_code: GdtEntry::user_code(),
            tss: TssEntry::null(),
        }
    }

    /// Set the TSS entry
    pub fn set_tss(&mut self, tss: &'static TaskStateSegment) {
        self.tss = TssEntry::new(tss);
    }
}

/// GDT descriptor for LGDT instruction
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GdtDescriptor {
    limit: u16,
    base: u64,
}

/// Global GDT instance
static mut GDT: Gdt = Gdt::new();

/// Global TSS instance
static mut TSS: TaskStateSegment = TaskStateSegment::new();

/// Initialize the GDT
///
/// # Safety
/// Must only be called once during boot.
pub unsafe fn init() {
    use core::ptr::addr_of_mut;

    unsafe {
        // Set up TSS
        let tss_ptr = addr_of_mut!(TSS);
        (*tss_ptr) = TaskStateSegment::new();

        // Set TSS in GDT - we need to pass a 'static reference
        // This is safe because TSS is static and lives forever
        let gdt_ptr = addr_of_mut!(GDT);
        (*gdt_ptr).set_tss(&*tss_ptr);

        // Load GDT
        let descriptor = GdtDescriptor {
            limit: (size_of::<Gdt>() - 1) as u16,
            base: gdt_ptr as u64,
        };

        core::arch::asm!(
            "lgdt [{}]",
            in(reg) &descriptor,
            options(nostack)
        );

        // Reload code segment
        core::arch::asm!(
            "push {sel}",
            "lea {tmp}, [rip + 2f]",
            "push {tmp}",
            "retfq",
            "2:",
            sel = in(reg) KERNEL_CS as u64,
            tmp = lateout(reg) _,
            options(preserves_flags)
        );

        // Reload data segments
        core::arch::asm!(
            "mov ds, {0:x}",
            "mov es, {0:x}",
            "mov fs, {0:x}",
            "mov gs, {0:x}",
            "mov ss, {0:x}",
            in(reg) KERNEL_DS,
            options(nostack, preserves_flags)
        );

        // Load TSS
        core::arch::asm!(
            "ltr {0:x}",
            in(reg) TSS_SELECTOR,
            options(nostack, preserves_flags)
        );
    }
}

/// Set the kernel stack pointer in the TSS
///
/// This is called on context switch to set RSP0 for the new thread.
pub fn set_kernel_stack(stack_top: u64) {
    use core::ptr::addr_of_mut;
    unsafe {
        let tss_ptr = addr_of_mut!(TSS);
        (*tss_ptr).rsp[0] = stack_top;
    }
}

/// Set an IST entry
///
/// IST entries are used for handling specific exceptions on a known-good stack.
pub fn set_ist(index: usize, stack_top: u64) {
    use core::ptr::addr_of_mut;
    if index < 7 {
        unsafe {
            let tss_ptr = addr_of_mut!(TSS);
            (*tss_ptr).ist[index] = stack_top;
        }
    }
}
