//! Application Processor Boot Support
//!
//! Provides trampoline code and setup for booting APs from real mode to long mode.

use boot_proto::PHYS_MAP_BASE;
use core::arch::asm;
use os_core::PhysAddr;

/// Physical address where trampoline code is copied
pub const TRAMPOLINE_PHYS: u64 = 0x8000;

/// Trampoline page number for SIPI vector
pub const TRAMPOLINE_PAGE: u8 = 0x08; // 0x8000 / 0x1000 = 0x08

/// Offsets within trampoline for data fields
const OFFSET_CR3: usize = 0; // Will be calculated from symbol
const OFFSET_STACK: usize = 8;
const OFFSET_ENTRY: usize = 16;

// External symbols from assembly
unsafe extern "C" {
    static ap_trampoline_start: u8;
    static ap_trampoline_end: u8;
}

/// Copy trampoline code to low memory and initialize it
///
/// # Safety
/// Must be called before booting any APs.
/// Low memory at TRAMPOLINE_PHYS must be available.
pub unsafe fn setup_trampoline(cr3: PhysAddr, stack: u64, entry: u64) {
    // Calculate trampoline size
    let start = unsafe { &ap_trampoline_start as *const u8 as usize };
    let end = unsafe { &ap_trampoline_end as *const u8 as usize };
    let size = end - start;

    // Get virtual address of trampoline destination
    let dest_virt = PHYS_MAP_BASE + TRAMPOLINE_PHYS;
    let dest = dest_virt as *mut u8;

    // Copy trampoline code
    unsafe {
        core::ptr::copy_nonoverlapping(start as *const u8, dest, size);
    }

    // Find offset of ap_cr3, ap_stack, ap_entry symbols
    // These are at the end of the trampoline, just before ap_trampoline_end
    // The assembly has them in order: cr3 (8 bytes), stack (8 bytes), entry (8 bytes)
    let data_offset = size - 24; // 3 * 8 bytes

    // Fill in CR3 value
    let cr3_ptr = (dest_virt + data_offset as u64) as *mut u64;
    unsafe {
        *cr3_ptr = cr3.as_u64();
    }

    // Fill in stack pointer
    let stack_ptr = (dest_virt + data_offset as u64 + 8) as *mut u64;
    unsafe {
        *stack_ptr = stack;
    }

    // Fill in entry point
    let entry_ptr = (dest_virt + data_offset as u64 + 16) as *mut u64;
    unsafe {
        *entry_ptr = entry;
    }
}

/// AP initialization callback type
pub type ApInitCallback = fn(u8) -> !;

/// Global AP initialization callback
static mut AP_INIT_CALLBACK: Option<ApInitCallback> = None;

/// Register the AP initialization callback
///
/// # Safety
/// Must be called before booting any APs.
pub unsafe fn register_ap_init_callback(callback: ApInitCallback) {
    unsafe {
        AP_INIT_CALLBACK = Some(callback);
    }
}

/// AP entry point in Rust
///
/// This is called by the trampoline after the AP is in long mode.
///
/// — WireSaint: Boot order matters. We must read APIC ID *before* loading
/// the GDT so we know which per-CPU GDT slot to initialize. The BSP already
/// called gdt::register_cpu(apic_id, logical_cpu_id) for each AP before
/// sending the SIPI, so our APIC_TO_CPU map is ready.
#[unsafe(no_mangle)]
pub extern "C" fn ap_entry_rust() -> ! {
    // At this point:
    // - We're in long mode
    // - Paging is enabled with kernel page tables
    // - Stack is set up
    // - Interrupts are disabled

    // Read APIC ID first — we need it to find our logical cpu_id.
    // — WireSaint: APIC ID is available immediately; no GDT required to read it.
    let apic_id = crate::apic::id();

    // Load this AP's own per-CPU GDT and TSS.
    // gdt::register_cpu was called by the BSP before the SIPI, so the
    // APIC_TO_CPU map resolves apic_id → cpu_id correctly here.
    //
    // — WireSaint: DO NOT call gdt::init() here. That resets cpu 0's GDT slot.
    // Each AP must call init_cpu with its own logical cpu_id, derived from the
    // APIC ID we just read and the mapping the BSP pre-registered.
    unsafe {
        // Derive cpu_id from our APIC ID via the pre-registered mapping.
        // register_cpu was called by the BSP's init loop before sending SIPI.
        let cpu_id = crate::gdt::cpu_id_from_apic(apic_id);
        crate::gdt::init_cpu(cpu_id);
    }

    // Load IDT so we can handle interrupts and IPIs
    unsafe {
        crate::idt::init();
    }

    // Initialize this CPU's APIC to receive IPIs
    crate::apic::enable();

    // Call registered callback if set, otherwise just halt
    unsafe {
        if let Some(callback) = AP_INIT_CALLBACK {
            callback(apic_id);
        }
    }

    // Fallback: AP initialization complete - enter idle loop
    loop {
        unsafe {
            asm!("hlt");
        }
    }
}
