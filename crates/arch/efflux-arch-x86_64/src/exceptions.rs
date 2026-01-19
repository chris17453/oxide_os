//! Exception and interrupt handlers for x86_64
//!
//! Provides the low-level handlers that are installed in the IDT.

use core::arch::{asm, naked_asm};

/// Interrupt stack frame pushed by the CPU
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct InterruptFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

/// Extended interrupt frame with error code
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct InterruptFrameError {
    pub error_code: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

/// Saved registers for interrupt handling
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SavedRegisters {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
}

/// Complete interrupt context (saved registers + CPU-pushed frame)
///
/// This is the complete state saved on the stack when an interrupt occurs.
/// The RSP passed to the scheduler callback points to this structure.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct InterruptContext {
    // Registers pushed by our handler (in push order, so reversed in memory)
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    // Pushed by CPU on interrupt
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl InterruptContext {
    /// Create a new interrupt context for a thread
    ///
    /// Sets up the context so that when restored via iretq, the thread
    /// will start executing at `entry` with argument `arg` in rdi.
    pub fn new(entry: fn(usize) -> !, stack_top: usize, arg: usize) -> Self {
        Self {
            // General purpose registers - mostly zero
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            r11: 0,
            r10: 0,
            r9: 0,
            r8: 0,
            rbp: 0,
            rdi: arg as u64,  // First argument
            rsi: 0,
            rdx: 0,
            rcx: 0,
            rbx: 0,
            rax: 0,
            // CPU state for iretq
            rip: entry as *const () as u64,
            cs: crate::gdt::KERNEL_CS as u64,
            rflags: 0x202, // IF=1 (interrupts enabled), reserved bit 1
            rsp: stack_top as u64,
            ss: crate::gdt::KERNEL_DS as u64,
        }
    }
}

// Macro to create exception handler without error code
macro_rules! exception_handler {
    ($name:ident, $handler:ident) => {
        #[unsafe(naked)]
        pub extern "C" fn $name() {
            naked_asm!(
                // Save all registers
                "push rax",
                "push rbx",
                "push rcx",
                "push rdx",
                "push rsi",
                "push rdi",
                "push rbp",
                "push r8",
                "push r9",
                "push r10",
                "push r11",
                "push r12",
                "push r13",
                "push r14",
                "push r15",

                // First argument: pointer to interrupt frame (after saved regs)
                "lea rdi, [rsp + 15*8]",
                // Second argument: 0 (no error code)
                "xor rsi, rsi",
                // Call the handler
                "call {}",

                // Restore all registers
                "pop r15",
                "pop r14",
                "pop r13",
                "pop r12",
                "pop r11",
                "pop r10",
                "pop r9",
                "pop r8",
                "pop rbp",
                "pop rdi",
                "pop rsi",
                "pop rdx",
                "pop rcx",
                "pop rbx",
                "pop rax",

                "iretq",
                sym $handler,
            );
        }
    };
}

// Macro to create exception handler with error code
macro_rules! exception_handler_error {
    ($name:ident, $handler:ident) => {
        #[unsafe(naked)]
        pub extern "C" fn $name() {
            naked_asm!(
                // Save all registers
                "push rax",
                "push rbx",
                "push rcx",
                "push rdx",
                "push rsi",
                "push rdi",
                "push rbp",
                "push r8",
                "push r9",
                "push r10",
                "push r11",
                "push r12",
                "push r13",
                "push r14",
                "push r15",

                // First argument: pointer to interrupt frame (after saved regs + error code)
                "lea rdi, [rsp + 15*8 + 8]",
                // Second argument: error code
                "mov rsi, [rsp + 15*8]",
                // Call the handler
                "call {}",

                // Restore all registers
                "pop r15",
                "pop r14",
                "pop r13",
                "pop r12",
                "pop r11",
                "pop r10",
                "pop r9",
                "pop r8",
                "pop rbp",
                "pop rdi",
                "pop rsi",
                "pop rdx",
                "pop rcx",
                "pop rbx",
                "pop rax",

                // Pop error code
                "add rsp, 8",
                "iretq",
                sym $handler,
            );
        }
    };
}

// Exception handlers
exception_handler!(divide_error, handle_divide_error);
exception_handler!(debug, handle_debug);
exception_handler!(nmi, handle_nmi);
exception_handler!(breakpoint, handle_breakpoint);
exception_handler!(overflow, handle_overflow);
exception_handler!(bound_range, handle_bound_range);
exception_handler!(invalid_opcode, handle_invalid_opcode);
exception_handler!(device_not_available, handle_device_not_available);
exception_handler_error!(double_fault, handle_double_fault);
exception_handler_error!(invalid_tss, handle_invalid_tss);
exception_handler_error!(segment_not_present, handle_segment_not_present);
exception_handler_error!(stack_segment, handle_stack_segment);
exception_handler_error!(general_protection, handle_general_protection);
exception_handler_error!(page_fault, handle_page_fault);
exception_handler!(x87_fpu, handle_x87_fpu);
exception_handler_error!(alignment_check, handle_alignment_check);
exception_handler!(machine_check, handle_machine_check);
exception_handler!(simd, handle_simd);

// Timer interrupt handler with context switch support
#[unsafe(naked)]
pub extern "C" fn timer_interrupt() {
    naked_asm!(
        // Save all registers
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rbp",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // Pass current RSP as argument to handler
        "mov rdi, rsp",
        // Call timer handler - returns new RSP (may be different if context switch)
        "call {}",
        // Use returned RSP (rax contains return value)
        "mov rsp, rax",

        // Restore all registers from (possibly new) stack
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",

        "iretq",
        sym handle_timer,
    );
}

// Spurious interrupt handler
#[unsafe(naked)]
pub extern "C" fn spurious_interrupt() {
    naked_asm!(
        // Spurious interrupts don't need EOI
        "iretq",
    );
}

// Rust handlers

extern "C" fn handle_divide_error(frame: *const InterruptFrame, _error: u64) {
    let frame = unsafe { &*frame };
    panic!("DIVIDE ERROR at {:#x}", frame.rip);
}

extern "C" fn handle_debug(frame: *const InterruptFrame, _error: u64) {
    let frame = unsafe { &*frame };
    crate::serial_println!("[DEBUG] Debug exception at {:#x}", frame.rip);
}

extern "C" fn handle_nmi(_frame: *const InterruptFrame, _error: u64) {
    crate::serial_println!("[NMI] Non-maskable interrupt");
}

extern "C" fn handle_breakpoint(frame: *const InterruptFrame, _error: u64) {
    let frame = unsafe { &*frame };
    crate::serial_println!("[BREAKPOINT] at {:#x}", frame.rip);
}

extern "C" fn handle_overflow(frame: *const InterruptFrame, _error: u64) {
    let frame = unsafe { &*frame };
    panic!("OVERFLOW at {:#x}", frame.rip);
}

extern "C" fn handle_bound_range(frame: *const InterruptFrame, _error: u64) {
    let frame = unsafe { &*frame };
    panic!("BOUND RANGE EXCEEDED at {:#x}", frame.rip);
}

extern "C" fn handle_invalid_opcode(frame: *const InterruptFrame, _error: u64) {
    let frame = unsafe { &*frame };
    panic!("INVALID OPCODE at {:#x}", frame.rip);
}

extern "C" fn handle_device_not_available(frame: *const InterruptFrame, _error: u64) {
    let frame = unsafe { &*frame };
    panic!("DEVICE NOT AVAILABLE at {:#x}", frame.rip);
}

extern "C" fn handle_double_fault(frame: *const InterruptFrame, error: u64) {
    let frame = unsafe { &*frame };
    panic!("DOUBLE FAULT at {:#x}, error: {:#x}", frame.rip, error);
}

extern "C" fn handle_invalid_tss(frame: *const InterruptFrame, error: u64) {
    let frame = unsafe { &*frame };
    panic!("INVALID TSS at {:#x}, error: {:#x}", frame.rip, error);
}

extern "C" fn handle_segment_not_present(frame: *const InterruptFrame, error: u64) {
    let frame = unsafe { &*frame };
    panic!("SEGMENT NOT PRESENT at {:#x}, error: {:#x}", frame.rip, error);
}

extern "C" fn handle_stack_segment(frame: *const InterruptFrame, error: u64) {
    let frame = unsafe { &*frame };
    panic!("STACK SEGMENT FAULT at {:#x}, error: {:#x}", frame.rip, error);
}

extern "C" fn handle_general_protection(frame: *const InterruptFrame, error: u64) {
    let frame = unsafe { &*frame };
    panic!("GENERAL PROTECTION FAULT at {:#x}, error: {:#x}", frame.rip, error);
}

extern "C" fn handle_page_fault(frame: *const InterruptFrame, error: u64) {
    use core::ptr::addr_of;

    let frame = unsafe { &*frame };
    let cr2: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack));
    }

    // Try page fault callback first (for COW handling, etc.)
    let callback = unsafe { *addr_of!(PAGE_FAULT_CALLBACK) };
    if let Some(handler) = callback {
        if handler(cr2, error, frame.rip) {
            // Fault was handled (e.g., COW page copied)
            return;
        }
    }

    // Fault not handled - print debug info and panic
    // The saved registers are on stack before the frame:
    // frame is at: stack + 15*8 (saved regs) + 8 (error code)
    // So saved regs are at frame - 15*8 - 8
    // RCX is at offset 96 from the start of saved regs (see exception_handler_error macro)
    let frame_ptr = frame as *const InterruptFrame as *const u8;
    let saved_regs_ptr = frame_ptr.wrapping_sub(15 * 8 + 8);
    let saved_rcx = unsafe { *(saved_regs_ptr.wrapping_add(96) as *const u64) };
    let saved_rax = unsafe { *(saved_regs_ptr.wrapping_add(112) as *const u64) };

    crate::serial_println!("PAGE FAULT!");
    crate::serial_println!("  Address: {:#x}", cr2);
    crate::serial_println!("  RIP: {:#x}", frame.rip);
    crate::serial_println!("  RSP: {:#x}", frame.rsp);
    crate::serial_println!("  RCX: {:#x}", saved_rcx);
    crate::serial_println!("  RAX: {:#x}", saved_rax);
    crate::serial_println!("  Error: {:#x}", error);
    crate::serial_println!("    Present: {}", error & 1 != 0);
    crate::serial_println!("    Write: {}", error & 2 != 0);
    crate::serial_println!("    User: {}", error & 4 != 0);
    crate::serial_println!("    Reserved: {}", error & 8 != 0);
    crate::serial_println!("    Instruction: {}", error & 16 != 0);

    // Print debug values from enter_usermode_with_context
    unsafe {
        use core::ptr::addr_of;
        use crate::usermode::{DEBUG_R15_VALUE, DEBUG_RCX_READ, DEBUG_RIP_READ, DEBUG_RSP_READ, DEBUG_RCX_ACTUAL, DEBUG_RAX_ACTUAL, DEBUG_IRETQ_RIP, DEBUG_IRETQ_CS, DEBUG_IRETQ_RSP};
        crate::serial_println!("  DEBUG from enter_usermode_with_context:");
        crate::serial_println!("    r15 value: {:#x}", *addr_of!(DEBUG_R15_VALUE));
        crate::serial_println!("    [r15+16] (rcx pre): {:#x}", *addr_of!(DEBUG_RCX_READ));
        crate::serial_println!("    rcx after load: {:#x}", *addr_of!(DEBUG_RCX_ACTUAL));
        crate::serial_println!("    rax after load: {:#x}", *addr_of!(DEBUG_RAX_ACTUAL));
        crate::serial_println!("    iretq frame RIP: {:#x}", *addr_of!(DEBUG_IRETQ_RIP));
        crate::serial_println!("    iretq frame CS: {:#x}", *addr_of!(DEBUG_IRETQ_CS));
        crate::serial_println!("    iretq frame RSP: {:#x}", *addr_of!(DEBUG_IRETQ_RSP));
    }

    panic!("Page fault");
}

extern "C" fn handle_x87_fpu(frame: *const InterruptFrame, _error: u64) {
    let frame = unsafe { &*frame };
    panic!("x87 FPU ERROR at {:#x}", frame.rip);
}

extern "C" fn handle_alignment_check(frame: *const InterruptFrame, error: u64) {
    let frame = unsafe { &*frame };
    panic!("ALIGNMENT CHECK at {:#x}, error: {:#x}", frame.rip, error);
}

extern "C" fn handle_machine_check(_frame: *const InterruptFrame, _error: u64) {
    panic!("MACHINE CHECK EXCEPTION");
}

extern "C" fn handle_simd(frame: *const InterruptFrame, _error: u64) {
    let frame = unsafe { &*frame };
    panic!("SIMD EXCEPTION at {:#x}", frame.rip);
}

/// Timer tick counter
static mut TIMER_TICKS: u64 = 0;

/// Page fault callback type
///
/// Takes the faulting address, error code, and instruction pointer.
/// Returns true if the fault was handled (e.g., COW), false to panic.
pub type PageFaultCallback = fn(fault_addr: u64, error_code: u64, rip: u64) -> bool;

/// Global page fault callback
static mut PAGE_FAULT_CALLBACK: Option<PageFaultCallback> = None;

/// Register a page fault callback (for COW handling, etc.)
///
/// # Safety
/// The callback must be valid and handle page faults correctly.
pub unsafe fn set_page_fault_callback(callback: PageFaultCallback) {
    use core::ptr::addr_of_mut;
    unsafe {
        *addr_of_mut!(PAGE_FAULT_CALLBACK) = Some(callback);
    }
}

/// Scheduler callback type
///
/// Takes the current stack pointer and returns the new stack pointer.
/// If no context switch is needed, return the same value.
pub type SchedulerCallback = fn(current_rsp: u64) -> u64;

/// Global scheduler callback
static mut SCHEDULER_CALLBACK: Option<SchedulerCallback> = None;

/// Register a scheduler callback to be called on each timer tick
///
/// The callback receives the current RSP (pointing to saved registers)
/// and should return the RSP to restore from (same or different thread).
///
/// # Safety
/// The callback must be valid and thread-safe.
pub unsafe fn set_scheduler_callback(callback: SchedulerCallback) {
    use core::ptr::addr_of_mut;
    unsafe {
        let cb_ptr = addr_of_mut!(SCHEDULER_CALLBACK);
        *cb_ptr = Some(callback);
    }
}

/// Timer interrupt handler
///
/// Takes current RSP, returns RSP to restore from (may be different for context switch)
extern "C" fn handle_timer(current_rsp: u64) -> u64 {
    use core::ptr::{addr_of, addr_of_mut};

    // Increment tick counter
    unsafe {
        let ticks_ptr = addr_of_mut!(TIMER_TICKS);
        *ticks_ptr += 1;
    }

    // Send EOI to APIC first (before potentially long scheduler work)
    crate::apic::end_of_interrupt();

    // Call scheduler callback if registered
    let new_rsp = unsafe {
        let cb_ptr = addr_of!(SCHEDULER_CALLBACK);
        if let Some(callback) = *cb_ptr {
            callback(current_rsp)
        } else {
            current_rsp
        }
    };

    new_rsp
}

/// Get current timer tick count
pub fn ticks() -> u64 {
    use core::ptr::addr_of;
    unsafe { *addr_of!(TIMER_TICKS) }
}
