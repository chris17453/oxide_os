//! Interrupt Descriptor Table (IDT) for x86_64
//!
//! Sets up the IDT with exception and interrupt handlers.

use core::mem::size_of;

use crate::gdt::KERNEL_CS;

/// Number of IDT entries
pub const IDT_ENTRIES: usize = 256;

/// Exception vectors
pub mod vector {
    pub const DIVIDE_ERROR: u8 = 0;
    pub const DEBUG: u8 = 1;
    pub const NMI: u8 = 2;
    pub const BREAKPOINT: u8 = 3;
    pub const OVERFLOW: u8 = 4;
    pub const BOUND_RANGE: u8 = 5;
    pub const INVALID_OPCODE: u8 = 6;
    pub const DEVICE_NOT_AVAILABLE: u8 = 7;
    pub const DOUBLE_FAULT: u8 = 8;
    pub const INVALID_TSS: u8 = 10;
    pub const SEGMENT_NOT_PRESENT: u8 = 11;
    pub const STACK_SEGMENT: u8 = 12;
    pub const GENERAL_PROTECTION: u8 = 13;
    pub const PAGE_FAULT: u8 = 14;
    pub const X87_FPU: u8 = 16;
    pub const ALIGNMENT_CHECK: u8 = 17;
    pub const MACHINE_CHECK: u8 = 18;
    pub const SIMD: u8 = 19;
    pub const VIRTUALIZATION: u8 = 20;

    /// First hardware interrupt (after PIC remapping)
    pub const IRQ_BASE: u8 = 32;

    /// Timer interrupt
    pub const TIMER: u8 = IRQ_BASE;

    /// Keyboard interrupt (IRQ 1)
    pub const KEYBOARD: u8 = IRQ_BASE + 1;

    /// Spurious interrupt vector for APIC
    pub const SPURIOUS: u8 = 0xFF;

    /// IPI vectors (Inter-Processor Interrupts)
    pub const IPI_RESCHEDULE: u8 = 0xF0;
    pub const IPI_TLB_SHOOTDOWN: u8 = 0xF1;
    pub const IPI_CALL_FUNCTION: u8 = 0xF2;
    pub const IPI_STOP: u8 = 0xF3;
}

/// IDT gate types
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum GateType {
    Interrupt = 0xE, // 64-bit interrupt gate
    Trap = 0xF,      // 64-bit trap gate
}

/// IDT entry (gate descriptor)
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    /// Create an empty/null IDT entry
    pub const fn null() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    /// Create a new IDT entry
    ///
    /// - `handler`: Address of the interrupt handler
    /// - `selector`: Code segment selector (usually KERNEL_CS)
    /// - `ist`: Interrupt Stack Table index (0 = no IST)
    /// - `gate_type`: Interrupt or trap gate
    /// - `dpl`: Descriptor privilege level (0-3)
    pub fn new(handler: u64, selector: u16, ist: u8, gate_type: GateType, dpl: u8) -> Self {
        Self {
            offset_low: handler as u16,
            selector,
            ist: ist & 0x7,
            type_attr: (1 << 7)             // Present
                | ((dpl & 0x3) << 5)        // DPL
                | (gate_type as u8), // Type
            offset_mid: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }

    /// Set the handler address
    pub fn set_handler(&mut self, handler: u64) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
    }
}

/// Interrupt Descriptor Table
#[repr(C, align(16))]
pub struct Idt {
    entries: [IdtEntry; IDT_ENTRIES],
}

impl Idt {
    /// Create a new IDT with all null entries
    pub const fn new() -> Self {
        Self {
            entries: [IdtEntry::null(); IDT_ENTRIES],
        }
    }

    /// Set an interrupt handler
    pub fn set_handler(&mut self, vector: u8, handler: u64, gate_type: GateType) {
        self.entries[vector as usize] = IdtEntry::new(
            handler, KERNEL_CS, 0, gate_type, 0, // Ring 0 only
        );
    }

    /// Set an interrupt handler with IST
    pub fn set_handler_ist(&mut self, vector: u8, handler: u64, gate_type: GateType, ist: u8) {
        self.entries[vector as usize] = IdtEntry::new(handler, KERNEL_CS, ist, gate_type, 0);
    }
}

/// IDT descriptor for LIDT instruction
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct IdtDescriptor {
    limit: u16,
    base: u64,
}

/// Global IDT instance
static mut IDT: Idt = Idt::new();

/// Initialize the IDT with exception handlers
///
/// # Safety
/// Must only be called once during boot.
pub unsafe fn init() {
    use crate::exceptions;
    use core::ptr::addr_of_mut;

    unsafe {
        let idt_ptr = addr_of_mut!(IDT);

        // Set up exception handlers
        (*idt_ptr).set_handler(
            vector::DIVIDE_ERROR,
            exceptions::divide_error as *const () as u64,
            GateType::Trap,
        );
        (*idt_ptr).set_handler(
            vector::DEBUG,
            exceptions::debug as *const () as u64,
            GateType::Trap,
        );
        (*idt_ptr).set_handler(
            vector::NMI,
            exceptions::nmi as *const () as u64,
            GateType::Interrupt,
        );
        (*idt_ptr).set_handler(
            vector::BREAKPOINT,
            exceptions::breakpoint as *const () as u64,
            GateType::Trap,
        );
        (*idt_ptr).set_handler(
            vector::OVERFLOW,
            exceptions::overflow as *const () as u64,
            GateType::Trap,
        );
        (*idt_ptr).set_handler(
            vector::BOUND_RANGE,
            exceptions::bound_range as *const () as u64,
            GateType::Trap,
        );
        (*idt_ptr).set_handler(
            vector::INVALID_OPCODE,
            exceptions::invalid_opcode as *const () as u64,
            GateType::Trap,
        );
        (*idt_ptr).set_handler(
            vector::DEVICE_NOT_AVAILABLE,
            exceptions::device_not_available as *const () as u64,
            GateType::Trap,
        );

        // Double fault uses IST1 for a known-good stack
        (*idt_ptr).set_handler_ist(
            vector::DOUBLE_FAULT,
            exceptions::double_fault as *const () as u64,
            GateType::Trap,
            1,
        );

        (*idt_ptr).set_handler(
            vector::INVALID_TSS,
            exceptions::invalid_tss as *const () as u64,
            GateType::Trap,
        );
        (*idt_ptr).set_handler(
            vector::SEGMENT_NOT_PRESENT,
            exceptions::segment_not_present as *const () as u64,
            GateType::Trap,
        );
        (*idt_ptr).set_handler(
            vector::STACK_SEGMENT,
            exceptions::stack_segment as *const () as u64,
            GateType::Trap,
        );
        (*idt_ptr).set_handler(
            vector::GENERAL_PROTECTION,
            exceptions::general_protection as *const () as u64,
            GateType::Trap,
        );
        (*idt_ptr).set_handler(
            vector::PAGE_FAULT,
            exceptions::page_fault as *const () as u64,
            GateType::Trap,
        );
        (*idt_ptr).set_handler(
            vector::X87_FPU,
            exceptions::x87_fpu as *const () as u64,
            GateType::Trap,
        );
        (*idt_ptr).set_handler(
            vector::ALIGNMENT_CHECK,
            exceptions::alignment_check as *const () as u64,
            GateType::Trap,
        );
        (*idt_ptr).set_handler(
            vector::MACHINE_CHECK,
            exceptions::machine_check as *const () as u64,
            GateType::Interrupt,
        );
        (*idt_ptr).set_handler(
            vector::SIMD,
            exceptions::simd as *const () as u64,
            GateType::Trap,
        );

        // Timer interrupt
        (*idt_ptr).set_handler(
            vector::TIMER,
            exceptions::timer_interrupt as *const () as u64,
            GateType::Interrupt,
        );

        // Keyboard interrupt (IRQ 1)
        (*idt_ptr).set_handler(
            vector::KEYBOARD,
            exceptions::keyboard_interrupt as *const () as u64,
            GateType::Interrupt,
        );

        // Spurious interrupt
        (*idt_ptr).set_handler(
            vector::SPURIOUS,
            exceptions::spurious_interrupt as *const () as u64,
            GateType::Interrupt,
        );

        // IPI handlers (Inter-Processor Interrupts)
        (*idt_ptr).set_handler(
            vector::IPI_TLB_SHOOTDOWN,
            exceptions::ipi_tlb_shootdown as *const () as u64,
            GateType::Interrupt,
        );

        // Load the IDT
        let descriptor = IdtDescriptor {
            limit: (size_of::<Idt>() - 1) as u16,
            base: idt_ptr as u64,
        };

        core::arch::asm!(
            "lidt [{}]",
            in(reg) &descriptor,
            options(nostack)
        );
    }
}

/// Set a custom interrupt handler
///
/// # Safety
/// The handler must be a valid function with the correct signature.
pub unsafe fn set_handler(vector: u8, handler: u64) {
    use core::ptr::addr_of_mut;
    unsafe {
        let idt_ptr = addr_of_mut!(IDT);
        (*idt_ptr).set_handler(vector, handler, GateType::Interrupt);
    }
}
