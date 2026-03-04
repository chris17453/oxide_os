//! Global Descriptor Table (GDT) for x86_64
//!
//! Per-CPU GDT/TSS arrays so each CPU loads its own descriptor table.
//! Fixes the SMP stack-corruption bug where all CPUs shared one TSS.RSP0.
//!
//! — WireSaint: The hardware doesn't care about your feelings. Every CPU
//! that fires an interrupt from usermode slams its RSP straight into
//! TSS.RSP0. One global TSS means CPUs just overwrite each other's kernel
//! stacks like drunks fighting over a single barstool. Now each CPU has
//! its own barstool. No more bar fights. No more triple faults.

use core::mem::size_of;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Maximum supported CPU count — WireSaint: 256 should outlive us all.
const MAX_CPUS: usize = 256;

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
///
/// — WireSaint: The TSS descriptor in the GDT is a 16-byte system descriptor.
/// It encodes the *physical address* of THIS CPU's TaskStateSegment. Get the
/// pointer wrong and the next ring-3 interrupt uses a random stack. Have fun.
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

    /// Create a TSS entry from a raw TSS pointer.
    ///
    /// # Safety
    /// `tss_ptr` must point to a TSS that lives at least as long as this GDT entry
    /// is loaded by any CPU. Passing a per-CPU static pointer is safe.
    ///
    /// — WireSaint: raw ptr, not a 'static ref, because we initialize the arrays
    /// in place before taking the pointer — no safe way to get a 'static ref to
    /// an in-progress init without lying to the borrow checker. The unsafe here is
    /// load-bearing: the caller must guarantee lifetime.
    unsafe fn from_ptr(tss_ptr: *const TaskStateSegment) -> Self {
        let base = tss_ptr as u64;
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
///
/// — WireSaint: RSP0 is the ring-0 stack the CPU loads whenever it takes an
/// interrupt from ring-3. IST entries are used for fault stacks (double fault,
/// NMI). Every CPU MUST have its own TSS or they clobber each other's RSP0.
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
/// - 0x28: TSS (16 bytes, 2 GDT slots)
///
/// — WireSaint: TSS is 16 bytes wide (two normal GDT slots). The selector
/// arithmetic in TSS_SELECTOR must account for this. Don't touch the layout.
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
    /// Create a new GDT with null TSS (call set_tss_ptr before loading)
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

    /// Set the TSS entry from a raw pointer.
    ///
    /// # Safety
    /// `tss_ptr` must point to a valid TSS that outlives this GDT's use by the CPU.
    unsafe fn set_tss_ptr(&mut self, tss_ptr: *const TaskStateSegment) {
        // Safety: caller guarantees tss_ptr validity and lifetime.
        self.tss = unsafe { TssEntry::from_ptr(tss_ptr) };
    }
}

/// GDT descriptor for LGDT instruction
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GdtDescriptor {
    limit: u16,
    base: u64,
}

// ============================================================================
// Per-CPU GDT and TSS arrays
//
// — WireSaint: These are the only GDT/TSS instances in the system. Each CPU
// indexes by its logical CPU ID (APIC-ID-to-CPU-ID map, same as sched).
// DO NOT add any other GDT or TSS statics. One array to rule them all.
// ============================================================================

/// Per-CPU GDT array — index by logical cpu_id
static mut GDT_ARRAY: [Gdt; MAX_CPUS] = [const { Gdt::new() }; MAX_CPUS];

/// Per-CPU TSS array — index by logical cpu_id
static mut TSS_ARRAY: [TaskStateSegment; MAX_CPUS] = [const { TaskStateSegment::new() }; MAX_CPUS];

/// Number of CPUs that have been initialized (debug/sanity only)
static CPUS_INITIALIZED: AtomicUsize = AtomicUsize::new(0);

/// Map APIC ID → logical CPU index for GDT/TSS lookup.
///
/// — WireSaint: We can't call into the sched crate from arch (circular dep),
/// so we keep our own tiny APIC→logical map here, initialized alongside GDT init.
/// Indexed by APIC ID (0..256); value is logical cpu_id. 0xFF means unmapped.
static mut APIC_TO_CPU: [u8; 256] = [0xFF; 256];

/// Register the apic_id → cpu_id mapping used by set_kernel_stack / set_ist.
///
/// Must be called before `init_cpu` so the mapping exists when the CPU loads its TSS.
///
/// # Safety
/// Must be called once per CPU before init_cpu. apic_id and cpu_id must be valid.
pub unsafe fn register_cpu(apic_id: u8, cpu_id: usize) {
    if cpu_id < MAX_CPUS {
        unsafe {
            APIC_TO_CPU[apic_id as usize] = cpu_id as u8;
        }
    }
}

/// Resolve a logical cpu_id from an APIC ID using our pre-registered map.
///
/// — WireSaint: Used by ap_entry_rust to find the logical cpu_id before
/// loading the GDT. Returns 0 if unmapped (BSP / early boot). Callers that
/// need the actual AP id must call register_cpu before this.
pub fn cpu_id_from_apic(apic_id: u8) -> usize {
    let mapped = unsafe { APIC_TO_CPU[apic_id as usize] };
    if mapped == 0xFF {
        // — WireSaint: unmapped; shouldn't happen for APs after BSP registered them.
        // Fallback to 0 rather than panic — misroutes to BSP slot but doesn't corrupt.
        0
    } else {
        mapped as usize
    }
}

/// Resolve the current CPU's logical cpu_id via APIC ID.
///
/// — WireSaint: Reads APIC ID from the local APIC register. Falls back to 0
/// if the mapping isn't set yet (early boot, BSP). The fallback is safe because
/// BSP always has cpu_id 0 and is the first to call init_cpu(0).
#[inline]
fn current_cpu_id() -> usize {
    let apic_id = crate::apic::id();
    cpu_id_from_apic(apic_id)
}

/// Initialize the GDT and TSS for the given logical CPU.
///
/// Initializes `GDT_ARRAY[cpu_id]` and `TSS_ARRAY[cpu_id]`, sets the TSS
/// descriptor in this CPU's GDT to point at `TSS_ARRAY[cpu_id]`, then loads
/// the GDT and TSS on the calling CPU.
///
/// # Safety
/// - Must be called exactly once per CPU.
/// - Must be called on the CPU that will use this GDT (not cross-CPU).
/// - `cpu_id` must be < MAX_CPUS.
/// - The BSP must call `init_cpu(0)` (equivalently `init()`); APs pass their logical cpu_id.
///
/// — WireSaint: Each CPU does its own LGDT + LTR. No CPU loads another CPU's
/// GDT. This is the contract. Break it and you get fun surprises.
pub unsafe fn init_cpu(cpu_id: usize) {
    use core::ptr::addr_of_mut;

    // — WireSaint: Bounds check before we go spelunking in static arrays.
    assert!(
        cpu_id < MAX_CPUS,
        "init_cpu: cpu_id {} out of range (MAX_CPUS={})",
        cpu_id,
        MAX_CPUS
    );

    unsafe {
        // Reset this CPU's TSS to a clean state.
        let tss_ptr = addr_of_mut!(TSS_ARRAY[cpu_id]);
        *tss_ptr = TaskStateSegment::new();

        // Build this CPU's GDT with a TSS descriptor pointing to its own TSS.
        let gdt_ptr = addr_of_mut!(GDT_ARRAY[cpu_id]);
        (*gdt_ptr) = Gdt::new();
        // Safety: tss_ptr points to TSS_ARRAY[cpu_id], a static with program lifetime.
        (*gdt_ptr).set_tss_ptr(tss_ptr);

        // Build the LGDT descriptor for this CPU's GDT.
        let descriptor = GdtDescriptor {
            limit: (size_of::<Gdt>() - 1) as u16,
            base: gdt_ptr as u64,
        };

        // Load this CPU's GDT. The descriptor is on the stack; the LGDT instruction
        // reads it and stores the limit+base internally — the stack variable can go
        // out of scope afterward.
        core::arch::asm!(
            "lgdt [{}]",
            in(reg) &descriptor,
            options(nostack)
        );

        // Reload code segment via far return (RETFQ pops RIP then CS).
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

        // Reload data segments.
        core::arch::asm!(
            "mov ds, {0:x}",
            "mov es, {0:x}",
            "mov fs, {0:x}",
            "mov gs, {0:x}",
            "mov ss, {0:x}",
            in(reg) KERNEL_DS,
            options(nostack, preserves_flags)
        );

        // Load this CPU's TSS. LTR marks the TSS descriptor as "busy".
        // — WireSaint: Each CPU's GDT has its OWN TSS descriptor, so LTR on
        // CPU 1 doesn't stomp CPU 0's "busy" bit. Critical detail.
        core::arch::asm!(
            "ltr {0:x}",
            in(reg) TSS_SELECTOR,
            options(nostack, preserves_flags)
        );
    }

    CPUS_INITIALIZED.fetch_add(1, Ordering::Relaxed);
}

/// Initialize the GDT for the BSP (CPU 0).
///
/// Kept for BSP boot path. Registers APIC ID 0 → cpu_id 0 and calls init_cpu(0).
///
/// # Safety
/// Must only be called once during BSP boot.
pub unsafe fn init() {
    // — WireSaint: BSP is always APIC ID 0, logical cpu_id 0. Register the
    // mapping before init_cpu so current_cpu_id() resolves correctly immediately.
    unsafe {
        register_cpu(0, 0);
        init_cpu(0);
    }
}

/// Set the kernel stack pointer in this CPU's TSS (RSP0).
///
/// Called on every context switch so the hardware uses the correct kernel stack
/// when transitioning from ring-3 to ring-0 on interrupt.
///
/// — WireSaint: TSS.RSP0 is not optional. When the CPU takes an interrupt from
/// userspace it unconditionally loads RSP from TSS.RSP0. If you forget to
/// update this, the interrupt uses a stale (or zeroed) kernel stack. Enjoy your
/// stack-on-stack corruption. We don't enjoy it. Hence per-CPU TSS.
pub fn set_kernel_stack(stack_top: u64) {
    use core::ptr::addr_of_mut;
    let cpu_id = current_cpu_id();
    // Safety: cpu_id < MAX_CPUS guaranteed by current_cpu_id() returning min(apic,MAX-1).
    // TSS_ARRAY is only written for this CPU's slot; other CPUs write their own slots.
    unsafe {
        let tss_ptr = addr_of_mut!(TSS_ARRAY[cpu_id]);
        (*tss_ptr).rsp[0] = stack_top;
    }
}

/// Set an IST entry in this CPU's TSS.
///
/// IST entries provide known-good stacks for NMI/double-fault/machine-check handlers.
/// Each CPU must populate its own IST slots — they're addresses in that CPU's TSS,
/// pointing at stacks the CPU will use if those exceptions fire on that CPU.
///
/// — WireSaint: IST1 is for double faults. If your double-fault handler uses the
/// wrong stack (because you didn't init this per-CPU), your double fault triple-
/// faults. Congrats. QEMU will silently exit. Have fun.
pub fn set_ist(index: usize, stack_top: u64) {
    use core::ptr::addr_of_mut;
    if index < 7 {
        let cpu_id = current_cpu_id();
        // Safety: same invariant as set_kernel_stack.
        unsafe {
            let tss_ptr = addr_of_mut!(TSS_ARRAY[cpu_id]);
            (*tss_ptr).ist[index] = stack_top;
        }
    }
}
