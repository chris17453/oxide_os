//! Exception and interrupt handlers for x86_64
//!
//! Provides the low-level handlers that are installed in the IDT.

use core::arch::{asm, naked_asm};
use core::sync::atomic::{AtomicUsize, Ordering};

extern crate perf;

/// Helper: Write u32 via os_log (ISR-safe)
#[inline]
pub unsafe fn write_u32_via_oslog(n: u32) {
    unsafe {
        if n == 0 {
            os_log::write_byte_raw(b'0');
            return;
        }
        let mut buf = [0u8; 10];
        let mut v = n;
        let mut pos = 0;
        while v > 0 {
            buf[pos] = b'0' + (v % 10) as u8;
            v /= 10;
            pos += 1;
        }
        for i in (0..pos).rev() {
            os_log::write_byte_raw(buf[i]);
        }
    }
}

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
            rdi: arg as u64, // First argument
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

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct FaultRecord {
    pub vec: u8,
    pub _pad: [u8; 7],
    pub error: u64,
    pub rip: u64,
    pub rsp: u64,
    pub rflags: u64,
    pub cr2: u64,
}

const FAULT_LOG_LEN: usize = 128;

#[repr(C, align(64))]
pub struct FaultLog {
    head: AtomicUsize,
    entries: [FaultRecord; FAULT_LOG_LEN],
}

impl FaultLog {
    pub const fn new() -> Self {
        const ZERO: FaultRecord = FaultRecord {
            vec: 0,
            _pad: [0; 7],
            error: 0,
            rip: 0,
            rsp: 0,
            rflags: 0,
            cr2: 0,
        };
        Self {
            head: AtomicUsize::new(0),
            entries: [ZERO; FAULT_LOG_LEN],
        }
    }

    fn push(&self, record: FaultRecord) {
        let idx = self.head.fetch_add(1, Ordering::Relaxed) % FAULT_LOG_LEN;
        unsafe {
            core::ptr::write_volatile(self.entries.as_ptr().add(idx) as *mut FaultRecord, record);
        }
    }

    pub fn entries(&self) -> &[FaultRecord; FAULT_LOG_LEN] {
        &self.entries
    }
}

static FAULT_LOG: FaultLog = FaultLog::new();

pub fn fault_log() -> &'static FaultLog {
    &FAULT_LOG
}

#[inline(always)]
fn dbg_byte(b: u8) {
    unsafe {
        asm!("out dx, al", in("dx") 0xe9u16, in("al") b, options(nomem, nostack, preserves_flags));
    }
}

fn log_fault(vec: u8, error: u64, rip: u64, rsp: u64, rflags: u64, cr2: u64) {
    dbg_byte(vec);
    let record = FaultRecord {
        vec,
        _pad: [0; 7],
        error,
        rip,
        rsp,
        rflags,
        cr2,
    };
    FAULT_LOG.push(record);
}

macro_rules! record_and_halt {
    ($vec:expr, $error:expr, $frame:expr, $cr2:expr) => {{
        let frame = unsafe { &*$frame };
        log_fault($vec, $error, frame.rip, frame.rsp, frame.rflags, $cr2);
        loop {
            unsafe {
                core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
            }
        }
    }};
}

// Macro to create exception handler without error code
macro_rules! exception_handler {
    ($name:ident, $handler:ident) => {
        #[unsafe(naked)]
        pub extern "C" fn $name() {
            naked_asm!(
                // Check if from user mode (CS & 3 != 0), swap GS if so
                // Stack at entry: RIP, CS, RFLAGS, RSP, SS
                "test qword ptr [rsp + 8], 3",
                "jz 2f",
                "swapgs",
                "2:",

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

                // Check if returning to user mode, swap GS if so
                "test qword ptr [rsp + 8], 3",
                "jz 3f",
                "swapgs",
                "3:",
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
                // Check if from user mode (CS & 3 != 0), swap GS if so
                // Stack at entry: error_code, RIP, CS, RFLAGS, RSP, SS
                // CS is at [rsp + 16]
                "test qword ptr [rsp + 16], 3",
                "jz 2f",
                "swapgs",
                "2:",

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
                // Check if returning to user mode, swap GS if so
                // Stack now: RIP, CS, RFLAGS, RSP, SS
                "test qword ptr [rsp + 8], 3",
                "jz 3f",
                "swapgs",
                "3:",
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
        // Check if we came from user mode (CS & 3 != 0)
        // Stack at entry: RIP, CS, RFLAGS, RSP, SS
        // CS is at [rsp + 8]
        "test qword ptr [rsp + 8], 3",
        "jz 2f",
        "swapgs",
        "2:",

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

        // Check if returning to user mode (CS & 3 != 0)
        // After pops, stack is: RIP, CS, RFLAGS, RSP, SS
        "test qword ptr [rsp + 8], 3",
        "jz 3f",
        "swapgs",
        "3:",
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

/// Keyboard IRQ callback type
pub type KeyboardCallback = fn();

/// Keyboard callback
static mut KEYBOARD_CALLBACK: Option<KeyboardCallback> = None;

/// Set keyboard callback
///
/// # Safety
/// Must be called during initialization.
pub unsafe fn set_keyboard_callback(callback: KeyboardCallback) {
    unsafe {
        use core::ptr::addr_of_mut;
        *addr_of_mut!(KEYBOARD_CALLBACK) = Some(callback);
    }
}

// Keyboard interrupt handler (IRQ 1, vector 33)
#[unsafe(naked)]
pub extern "C" fn keyboard_interrupt() {
    naked_asm!(
        // Check if from user mode (CS & 3 != 0), swap GS if so
        "test qword ptr [rsp + 8], 3",
        "jz 2f",
        "swapgs",
        "2:",

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

        // Call keyboard handler
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

        // Check if returning to user mode, swap GS if so
        "test qword ptr [rsp + 8], 3",
        "jz 3f",
        "swapgs",
        "3:",
        "iretq",
        sym handle_keyboard,
    );
}

/// Mouse IRQ callback type
pub type MouseCallback = fn();

/// Mouse callback
static mut MOUSE_CALLBACK: Option<MouseCallback> = None;

/// Set mouse callback
///
/// # Safety
/// Must be called during initialization.
pub unsafe fn set_mouse_callback(callback: MouseCallback) {
    unsafe {
        use core::ptr::addr_of_mut;
        *addr_of_mut!(MOUSE_CALLBACK) = Some(callback);
    }
}

// Mouse interrupt handler (IRQ 12, vector 44)
#[unsafe(naked)]
pub extern "C" fn mouse_interrupt() {
    naked_asm!(
        // Check if from user mode (CS & 3 != 0), swap GS if so
        "test qword ptr [rsp + 8], 3",
        "jz 2f",
        "swapgs",
        "2:",

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

        // Call mouse handler
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

        // Check if returning to user mode, swap GS if so
        "test qword ptr [rsp + 8], 3",
        "jz 3f",
        "swapgs",
        "3:",
        "iretq",
        sym handle_mouse,
    );
}

/// Mouse handler
/// -- TorqueJax: IRQ 12 fires on mouse data. If nobody's listening, drain the byte
/// or the level-triggered interrupt storms and locks up the CPU forever.
extern "C" fn handle_mouse() {
    let start_cycles = perf::rdtsc();

    // Forward to the registered mouse callback (ps2::handle_mouse_irq).
    // The callback itself reads port 0x60 — we must NOT read it here
    // or the byte will be consumed before the driver sees it.
    unsafe {
        if let Some(callback) = MOUSE_CALLBACK {
            callback();
        } else {
            // No driver registered yet — drain the byte from port 0x60
            // to clear the output buffer. Without this, the level-triggered
            // IRQ 12 re-fires immediately after EOI, creating an interrupt
            // storm that starves the init code trying to set up the mouse.
            core::arch::asm!("in al, 0x60", out("al") _, options(nomem, nostack, preserves_flags));
        }
    }

    // Send EOI to APIC
    crate::apic::end_of_interrupt();

    // Record mouse IRQ execution time
    let end_cycles = perf::rdtsc();
    let elapsed = end_cycles.saturating_sub(start_cycles);
    perf::counters().record_mouse_irq(elapsed);
}

/// Atomic counter of keyboard interrupts (for debugging)
static KEYBOARD_IRQ_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Get the keyboard interrupt count (for debugging)
pub fn keyboard_irq_count() -> u64 {
    KEYBOARD_IRQ_COUNT.load(core::sync::atomic::Ordering::Relaxed)
}

/// Keyboard handler - simplified non-naked version
extern "C" fn handle_keyboard() {
    let start_cycles = perf::rdtsc();
    KEYBOARD_IRQ_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

    #[cfg(feature = "debug-input")]
    unsafe {
        os_log::write_str_raw("[KB_IRQ] ");
    }

    // If a platform keyboard handler is registered, delegate to it (it reads the scancode)
    unsafe {
        if let Some(callback) = KEYBOARD_CALLBACK {
            #[cfg(feature = "debug-input")]
            os_log::write_str_raw("CB ");
            callback();
            // Send EOI to APIC
            crate::apic::end_of_interrupt();
            return;
        } else {
            #[cfg(feature = "debug-input")]
            os_log::write_str_raw("NO_CB ");
        }
    }

    // Read scancode ourselves (fallback path)
    let scancode = unsafe {
        let mut value: u8;
        core::arch::asm!("in al, 0x60", out("al") value, options(nomem, nostack, preserves_flags));
        value
    };

    // Store in buffer (WATOS-style but safe)
    unsafe {
        let write_pos = KEY_WRITE_POS;
        let next_pos = (write_pos + 1) & 31;

        // Check if buffer is full
        if next_pos != KEY_READ_POS {
            KEY_BUFFER[write_pos] = scancode;
            KEY_WRITE_POS = next_pos;
        }
    }

    // Send EOI to APIC
    crate::apic::end_of_interrupt();

    // Record keyboard IRQ execution time
    let end_cycles = perf::rdtsc();
    let elapsed = end_cycles.saturating_sub(start_cycles);
    perf::counters().record_keyboard_irq(elapsed);
}

// ============================================================================
// IPI Handlers (Inter-Processor Interrupts)
// ============================================================================

/// IPI callback type
pub type IpiCallback = fn();

/// TLB shootdown IPI callback
static mut TLB_SHOOTDOWN_CALLBACK: Option<IpiCallback> = None;

/// Register TLB shootdown IPI callback
///
/// # Safety
/// Must be called during initialization.
pub unsafe fn set_tlb_shootdown_callback(callback: IpiCallback) {
    unsafe {
        use core::ptr::addr_of_mut;
        *addr_of_mut!(TLB_SHOOTDOWN_CALLBACK) = Some(callback);
    }
}

/// Reschedule IPI handler (vector 0xF0)
///
/// — GraveShift: This IPI is a kick — its only job is to break a CPU out
/// of `hlt` so the scheduler can pick up newly-runnable tasks. No state
/// mutation needed; just ACK the APIC and return.
#[unsafe(naked)]
pub extern "C" fn ipi_reschedule() {
    naked_asm!(
        // Save minimal registers (handler is trivial)
        "push rax",
        "push rcx",
        "push rdx",

        // ACK the APIC and return
        "call {}",

        "pop rdx",
        "pop rcx",
        "pop rax",

        "iretq",
        sym handle_ipi_reschedule,
    );
}

/// Handle reschedule IPI — just send EOI
/// — GraveShift: The wake already set need_resched on the target RQ.
/// The next scheduler_tick (or the interrupted hlt return path) will
/// call pick_next_task and context switch.
extern "C" fn handle_ipi_reschedule() {
    crate::apic::end_of_interrupt();
}

/// TLB shootdown IPI handler (vector 0xF1)
#[unsafe(naked)]
pub extern "C" fn ipi_tlb_shootdown() {
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

        // Call TLB shootdown handler
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
        sym handle_ipi_tlb_shootdown,
    );
}

/// Handle TLB shootdown IPI
extern "C" fn handle_ipi_tlb_shootdown() {
    // Call the registered TLB shootdown callback
    unsafe {
        if let Some(callback) = TLB_SHOOTDOWN_CALLBACK {
            callback();
        }
    }

    // Send EOI to APIC
    crate::apic::end_of_interrupt();
}

// Rust handlers

extern "C" fn handle_divide_error(frame: *const InterruptFrame, _error: u64) {
    let frame = unsafe { &*frame };
    // -- BlackLatch: User divides by zero, user dies. Not the kernel.
    if (frame.cs & 3) == 3 {
        crate::serial_println!("[SIGFPE] User divide error at RIP {:#x}", frame.rip);
        let kill_cb = unsafe { *core::ptr::addr_of!(USER_FAULT_KILL_CALLBACK) };
        if let Some(kill) = kill_cb {
            kill(0, frame.rip, 8); // 8 = SIGFPE
            return; // -- BlackLatch: Process neutralized, kernel carries on
        }
    }
    // -- GraveShift: Kernel-mode divide error is fatal. No coming back.
    panic!("DIVIDE ERROR at {:#x}", frame.rip);
}

extern "C" fn handle_debug(_frame: *const InterruptFrame, _error: u64) {
    // Debug exceptions can occur from hardware debug registers or TF flag
    // Silently ignore for now - the user's process can handle these if needed
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

    // Read saved registers from stack (they were pushed before the frame pointer)
    // Stack layout: [r15][r14][r13][r12][r11][r10][r9][r8][rbp][rdi][rsi][rdx][rcx][rbx][rax][rip][cs][rflags][rsp][ss]
    let regs_ptr = unsafe { (frame as *const InterruptFrame).cast::<u64>().sub(15) };
    let regs = unsafe { core::slice::from_raw_parts(regs_ptr, 15) };

    // Print debug info
    crate::serial_println!("\n=== INVALID OPCODE ===");
    crate::serial_println!("RIP: {:#x}  CS: {:#x}", frame.rip, frame.cs);
    crate::serial_println!("RSP: {:#x}  SS: {:#x}", frame.rsp, frame.ss);
    crate::serial_println!("RFLAGS: {:#x}", frame.rflags);
    crate::serial_println!(
        "RAX: {:#x}  RBX: {:#x}  RCX: {:#x}",
        regs[14],
        regs[13],
        regs[12]
    );
    crate::serial_println!(
        "RDX: {:#x}  RSI: {:#x}  RDI: {:#x}",
        regs[11],
        regs[10],
        regs[9]
    );
    crate::serial_println!(
        "RBP: {:#x}  R8:  {:#x}  R9:  {:#x}",
        regs[8],
        regs[7],
        regs[6]
    );
    crate::serial_println!(
        "R10: {:#x}  R11: {:#x}  R12: {:#x}",
        regs[5],
        regs[4],
        regs[3]
    );
    crate::serial_println!(
        "R13: {:#x}  R14: {:#x}  R15: {:#x}",
        regs[2],
        regs[1],
        regs[0]
    );

    // — GraveShift: User-mode #UD → deliver SIGILL, don't nuke the system.
    // Rust's panic_abort uses ud2 as its abort instruction, so every unwinding
    // panic in a userspace std binary lands here. The process dies; life goes on.
    if (frame.cs & 3) != 0 {
        crate::serial_println!(
            "[SIGILL] User invalid opcode at RIP {:#x}",
            frame.rip
        );
        let kill_cb = unsafe { *core::ptr::addr_of!(USER_FAULT_KILL_CALLBACK) };
        if let Some(kill) = kill_cb {
            kill(0, frame.rip, 4); // 4 = SIGILL
            return;
        }
        // — GraveShift: No kill callback (early boot). Fall through to panic.
    }

    panic!("INVALID OPCODE at {:#x}", frame.rip);
}

extern "C" fn handle_device_not_available(frame: *const InterruptFrame, _error: u64) {
    let frame = unsafe { &*frame };
    panic!("DEVICE NOT AVAILABLE at {:#x}", frame.rip);
}

extern "C" fn handle_double_fault(frame: *const InterruptFrame, error: u64) {
    let frame = unsafe { &*frame };

    // — NeonRoot: Double fault = the kernel already tried to handle an exception
    // and failed. Stack is likely corrupt. fmt is dead. Use raw serial writes only.
    // This is the last breath before the machine goes dark.
    unsafe {
        crate::serial::write_str_unsafe("\r\n[KERNEL DOUBLE FAULT] The kernel exception handler faulted.\r\n");
        crate::serial::write_str_unsafe("  RIP: ");
        crate::serial::write_u64_hex_unsafe(frame.rip);
        crate::serial::write_str_unsafe("\r\n");
        crate::serial::write_str_unsafe("  RSP: ");
        crate::serial::write_u64_hex_unsafe(frame.rsp);
        crate::serial::write_str_unsafe("\r\n");
        crate::serial::write_str_unsafe("  RFLAGS: ");
        crate::serial::write_u64_hex_unsafe(frame.rflags);
        crate::serial::write_str_unsafe("\r\n");
        crate::serial::write_str_unsafe("  Error code: ");
        crate::serial::write_u64_hex_unsafe(error);
        crate::serial::write_str_unsafe("\r\n");
        crate::serial::write_str_unsafe("  CS: ");
        crate::serial::write_u64_hex_unsafe(frame.cs);
        crate::serial::write_str_unsafe("\r\n");
        crate::serial::write_str_unsafe("  Halting.\r\n");
    }

    log_fault(8, error, frame.rip, frame.rsp, frame.rflags, 0);
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

extern "C" fn handle_invalid_tss(frame: *const InterruptFrame, error: u64) {
    let frame = unsafe { &*frame };
    panic!("INVALID TSS at {:#x}, error: {:#x}", frame.rip, error);
}

extern "C" fn handle_segment_not_present(frame: *const InterruptFrame, error: u64) {
    let frame = unsafe { &*frame };
    panic!(
        "SEGMENT NOT PRESENT at {:#x}, error: {:#x}",
        frame.rip, error
    );
}

extern "C" fn handle_stack_segment(frame: *const InterruptFrame, error: u64) {
    let frame = unsafe { &*frame };
    panic!(
        "STACK SEGMENT FAULT at {:#x}, error: {:#x}",
        frame.rip, error
    );
}

extern "C" fn handle_general_protection(frame: *const InterruptFrame, error: u64) {
    let frame = unsafe { &*frame };

    // — NeonRoot: CPL check. Ring 3 bit set means a user process committed
    // a hardware crime. The process dies; the kernel survives. Elegant.
    if (frame.cs & 3) != 0 {
        // User-mode GPF — send SIGSEGV and return. Do NOT halt the system.
        crate::serial_println!(
            "[SIGSEGV] User GPF at RIP {:#x}, error code {:#x}",
            frame.rip,
            error
        );
        let kill_cb = unsafe { *core::ptr::addr_of!(USER_FAULT_KILL_CALLBACK) };
        if let Some(kill) = kill_cb {
            kill(0, frame.rip, 11); // 11 = SIGSEGV
            return; // — NeonRoot: Process flatlined; kernel lives to fight another day
        }
        // — NeonRoot: No kill callback yet (early boot). Fall through to halt.
    }

    // Kernel-mode GPF — the kernel itself broke the rules. Print everything,
    // then die gracefully (halt, don't corrupt further state).
    // — NeonRoot: We use lock-free serial writes because fmt machinery may
    // itself be corrupted. Trust no heap. Trust no locks.
    unsafe {
        crate::serial::write_str_unsafe("\r\n[KERNEL GPF] General Protection Fault!\r\n");
        crate::serial::write_str_unsafe("  RIP: ");
        crate::serial::write_u64_hex_unsafe(frame.rip);
        crate::serial::write_str_unsafe("\r\n");
        crate::serial::write_str_unsafe("  RSP: ");
        crate::serial::write_u64_hex_unsafe(frame.rsp);
        crate::serial::write_str_unsafe("\r\n");
        crate::serial::write_str_unsafe("  RFLAGS: ");
        crate::serial::write_u64_hex_unsafe(frame.rflags);
        crate::serial::write_str_unsafe("\r\n");
        crate::serial::write_str_unsafe("  Error code: ");
        crate::serial::write_u64_hex_unsafe(error);
        crate::serial::write_str_unsafe("\r\n");
        crate::serial::write_str_unsafe("  CS: ");
        crate::serial::write_u64_hex_unsafe(frame.cs);
        crate::serial::write_str_unsafe("\r\n");
    }

    // Log to fault record ring buffer for GDB post-mortem
    log_fault(13, error, frame.rip, frame.rsp, frame.rflags, 0);

    // — NeonRoot: Halt. Not panic. Panic allocates; we don't trust the heap now.
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

extern "C" fn handle_page_fault(frame: *const InterruptFrame, error: u64) {
    use core::ptr::addr_of;
    use core::sync::atomic::{AtomicU32, Ordering};

    /// — CrashBloom: Unconditional page fault trace counter. First 1000 user-mode
    /// faults get printed so we can see COW resolution through login fork.
    static PF_DIAG_COUNT: AtomicU32 = AtomicU32::new(0);

    let frame = unsafe { &*frame };
    let cr2: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack));
    }

    // — CrashBloom: Unconditional trace for user-mode write faults (COW candidates).
    // This fires regardless of debug-pagefault feature to diagnose fork/COW hangs.
    let is_user_write = (error & 0x6) == 0x6; // user=1, write=1
    if is_user_write {
        let n = PF_DIAG_COUNT.fetch_add(1, Ordering::Relaxed);
        if n < 1000 {
            let cr3: u64;
            unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3); }
            unsafe {
                os_log::write_str_raw("[PF-DIAG] user-write addr=0x");
                os_log::write_u64_hex_raw(cr2);
                os_log::write_str_raw(" rip=0x");
                os_log::write_u64_hex_raw(frame.rip);
                os_log::write_str_raw(" err=0x");
                os_log::write_u64_hex_raw(error);
                os_log::write_str_raw(" cr3=0x");
                os_log::write_u64_hex_raw(cr3);
                os_log::write_str_raw("\n");
            }
        }
    }

    #[cfg(feature = "debug-pagefault")]
    {
        let cr3: u64;
        unsafe {
            core::arch::asm!("mov {}, cr3", out(reg) cr3);
        }
        crate::serial_println!(
            "[PF] addr={:#x} rip={:#x} err={:#x} cr3={:#x}",
            cr2,
            frame.rip,
            error,
            cr3
        );
    }

    // — CrashBloom: Also trace non-present user faults (demand page / stack growth)
    let is_user_nonpresent = (error & 0x5) == 0x4; // user=1, present=0
    if is_user_nonpresent {
        let n = PF_DIAG_COUNT.fetch_add(1, Ordering::Relaxed);
        if n < 1000 {
            unsafe {
                os_log::write_str_raw("[PF-DIAG] user-nonpresent addr=0x");
                os_log::write_u64_hex_raw(cr2);
                os_log::write_str_raw(" rip=0x");
                os_log::write_u64_hex_raw(frame.rip);
                os_log::write_str_raw("\n");
            }
        }
    }

    // Try page fault callback first (for COW handling, etc.)
    let callback = unsafe { *addr_of!(PAGE_FAULT_CALLBACK) };
    if let Some(handler) = callback {
        if handler(cr2, error, frame.rip) {
            // — CrashBloom: Trace successful COW/demand resolution
            if is_user_write || is_user_nonpresent {
                let n = PF_DIAG_COUNT.load(Ordering::Relaxed);
                if n <= 1001 {
                    unsafe {
                        os_log::write_str_raw("[PF-DIAG] HANDLED OK\n");
                    }
                }
            }
            #[cfg(feature = "debug-pagefault")]
            {
                crate::serial_println!("[PF] Handled by callback");
            }
            // Fault was handled (e.g., COW page copied)
            return;
        }
    }

    // — CrashBloom: Trace unhandled user faults — these lead to SIGSEGV or panic
    if (error & 4) != 0 {
        unsafe {
            os_log::write_str_raw("[PF-DIAG] UNHANDLED user fault addr=0x");
            os_log::write_u64_hex_raw(cr2);
            os_log::write_str_raw(" rip=0x");
            os_log::write_u64_hex_raw(frame.rip);
            os_log::write_str_raw(" err=0x");
            os_log::write_u64_hex_raw(error);
            os_log::write_str_raw("\n");
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

    // Get CR3 for debugging (identifies the process)
    let cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) cr3);
    }

    crate::serial_println!("PAGE FAULT!");
    crate::serial_println!("  CR3 (PML4): {:#x}", cr3);
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
    crate::serial_println!("    SMAP violation: {}", error & 32 != 0);

    crate::serial_println!("  === RFLAGS CHECK ===");

    // Check RFLAGS AC bit to verify STAC/CLAC state
    let rflags: u64;
    unsafe {
        core::arch::asm!("pushfq; pop {}", out(reg) rflags, options(nomem, nostack));
    }
    crate::serial_println!("  RFLAGS: {:#x}", rflags);
    crate::serial_println!(
        "    AC flag: {}",
        if rflags & (1 << 18) != 0 {
            "SET"
        } else {
            "CLEAR"
        }
    );

    // Print debug values from enter_usermode_with_context
    unsafe {
        use crate::usermode::{
            DEBUG_IRETQ_CS, DEBUG_IRETQ_RIP, DEBUG_IRETQ_RSP, DEBUG_R15_VALUE, DEBUG_RAX_ACTUAL,
            DEBUG_RCX_ACTUAL, DEBUG_RCX_READ,
        };
        use core::ptr::addr_of;
        crate::serial_println!("  DEBUG from enter_usermode_with_context:");
        crate::serial_println!("    r15 value: {:#x}", *addr_of!(DEBUG_R15_VALUE));
        crate::serial_println!("    [r15+16] (rcx pre): {:#x}", *addr_of!(DEBUG_RCX_READ));
        crate::serial_println!("    rcx after load: {:#x}", *addr_of!(DEBUG_RCX_ACTUAL));
        crate::serial_println!("    rax after load: {:#x}", *addr_of!(DEBUG_RAX_ACTUAL));
        crate::serial_println!("    iretq frame RIP: {:#x}", *addr_of!(DEBUG_IRETQ_RIP));
        crate::serial_println!("    iretq frame CS: {:#x}", *addr_of!(DEBUG_IRETQ_CS));
        crate::serial_println!("    iretq frame RSP: {:#x}", *addr_of!(DEBUG_IRETQ_RSP));
    }

    // Print debug values from syscall sysretq path and AC tracking.
    // — NeonRoot: Old code imported racy single-global statics; now we read
    // THIS CPU's per-CPU slot. Same information, zero cross-CPU contamination.
    //
    // — GraveShift: Guard against GS_BASE=0. If GS_BASE is null (uninitialized CPU,
    // corrupted state, or nested fault during early boot), reading gs:[40] triggers
    // page fault at 0x28 → cascading fault → double fault → dead. Read GS_BASE
    // via rdmsr first. If it's 0, skip the per-CPU debug dump entirely.
    unsafe {
        let gs_base: u64;
        core::arch::asm!(
            "mov ecx, 0xC0000101",  // MSR IA32_GS_BASE
            "rdmsr",
            "shl rdx, 32",
            "or rax, rdx",
            out("ecx") _,
            out("rax") gs_base,
            out("rdx") _,
            options(nomem, nostack),
        );
        if gs_base == 0 {
            // — CrashBloom: Which set_kernel_stack call site was last hit before the crash?
            // 0=none, 1=scheduler, 2=fork, 3=run_child_process
            let site = crate::syscall::LAST_SET_KSTACK_SITE.load(core::sync::atomic::Ordering::Relaxed);
            crate::serial_println!("  [SKIP] GS_BASE is NULL — per-CPU debug slot unavailable (last_kstack_site={})", site);

            // — CrashBloom: Read KERNEL_GS_BASE to see if swapgs state is inverted
            let kgs: u64;
            core::arch::asm!(
                "mov ecx, 0xC0000102",
                "rdmsr",
                "shl rdx, 32",
                "or rax, rdx",
                out("ecx") _,
                out("rax") kgs,
                out("rdx") _,
                options(nomem, nostack),
            );
            crate::serial_println!("  [DIAG] KERNEL_GS_BASE=0x{:016x}", kgs);
        }
    }
    // — GraveShift: Only access per-CPU data if GS_BASE is valid (non-zero).
    let gs_base_valid = unsafe {
        let gs_base: u64;
        core::arch::asm!(
            "mov ecx, 0xC0000101",
            "rdmsr",
            "shl rdx, 32",
            "or rax, rdx",
            out("ecx") _,
            out("rax") gs_base,
            out("rdx") _,
            options(nomem, nostack),
        );
        gs_base != 0
    };
    if gs_base_valid {
    unsafe {
        let dbg = crate::syscall::get_current_cpu_debug_slot();
        crate::serial_println!("  DEBUG from syscall sysretq (this CPU's slot):");
        crate::serial_println!("    kernel stack ptr: {:#x}", dbg.debug_sysret_stack);
        crate::serial_println!("    loaded RSP: {:#x}", dbg.debug_sysret_rsp);
        crate::serial_println!("    loaded RCX (user RIP): {:#x}", dbg.debug_sysret_rcx);
        crate::serial_println!("    loaded R11 (user RFLAGS): {:#x}", dbg.debug_sysret_r11);
        crate::serial_println!("    RAX (return value): {:#x}", dbg.debug_sysret_rax);

        crate::serial_println!("  DEBUG from syscall AC tracking (this CPU's slot):");

        let at_entry = dbg.debug_ac_at_entry;
        crate::serial_println!("    RFLAGS at entry: {:#x}", at_entry);
        crate::serial_println!(
            "    AC flag at entry: {}",
            if (at_entry >> 18) & 1 != 0 { "SET" } else { "CLEAR" }
        );

        let stac_rflags = dbg.debug_stac_rflags;
        crate::serial_println!("    RFLAGS after STAC: {:#x}", stac_rflags);
        crate::serial_println!(
            "    AC flag after STAC: {}",
            if (stac_rflags >> 18) & 1 != 0 { "SET" } else { "CLEAR" }
        );

        let before_call = dbg.debug_ac_before_call;
        crate::serial_println!("    RFLAGS before handler call: {:#x}", before_call);
        crate::serial_println!(
            "    AC flag before call: {}",
            if (before_call >> 18) & 1 != 0 { "SET" } else { "CLEAR" }
        );

        let after_call = dbg.debug_ac_after_call;
        crate::serial_println!("    RFLAGS after handler call: {:#x}", after_call);
        crate::serial_println!(
            "    AC flag after call: {}",
            if (after_call >> 18) & 1 != 0 { "SET" } else { "CLEAR" }
        );
    }
    } // — GraveShift: end of gs_base_valid guard

    // Walk the page tables to see what's mapped
    crate::serial_println!("  === PAGE TABLE WALK ===");
    unsafe {
        let vaddr = cr2;

        // Extract page table indices
        let pml4_idx = (vaddr >> 39) & 0x1FF;
        let pdpt_idx = (vaddr >> 30) & 0x1FF;
        let pd_idx = (vaddr >> 21) & 0x1FF;
        let pt_idx = (vaddr >> 12) & 0x1FF;

        crate::serial_println!("  Virtual address: {:#x}", vaddr);
        crate::serial_println!(
            "  PML4 index: {}, PDPT index: {}, PD index: {}, PT index: {}",
            pml4_idx,
            pdpt_idx,
            pd_idx,
            pt_idx
        );

        // Walk PML4
        let pml4_phys = cr3 & 0xFFFF_FFFF_F000;
        let pml4_virt = 0xFFFF_8000_0000_0000 | pml4_phys;
        let pml4_entry = *((pml4_virt + pml4_idx * 8) as *const u64);
        crate::serial_println!(
            "  PML4[{}] = {:#x} (P={} W={} U={} A={} PWT={} PCD={} PS={})",
            pml4_idx,
            pml4_entry,
            pml4_entry & 1,
            (pml4_entry >> 1) & 1,
            (pml4_entry >> 2) & 1,
            (pml4_entry >> 5) & 1,
            (pml4_entry >> 3) & 1,
            (pml4_entry >> 4) & 1,
            (pml4_entry >> 7) & 1
        );

        if pml4_entry & 1 == 0 {
            crate::serial_println!("  PML4 entry not present!");
        } else {
            // Walk PDPT
            let pdpt_phys = pml4_entry & 0xFFFF_FFFF_F000;
            let pdpt_virt = 0xFFFF_8000_0000_0000 | pdpt_phys;
            let pdpt_entry = *((pdpt_virt + pdpt_idx * 8) as *const u64);
            crate::serial_println!(
                "  PDPT[{}] = {:#x} (P={} W={} U={} A={} PWT={} PCD={} PS={})",
                pdpt_idx,
                pdpt_entry,
                pdpt_entry & 1,
                (pdpt_entry >> 1) & 1,
                (pdpt_entry >> 2) & 1,
                (pdpt_entry >> 5) & 1,
                (pdpt_entry >> 3) & 1,
                (pdpt_entry >> 4) & 1,
                (pdpt_entry >> 7) & 1
            );

            if pdpt_entry & 1 == 0 {
                crate::serial_println!("  PDPT entry not present!");
            } else if (pdpt_entry >> 7) & 1 != 0 {
                crate::serial_println!("  PDPT entry is 1GB page!");
            } else {
                // Walk PD
                let pd_phys = pdpt_entry & 0xFFFF_FFFF_F000;
                let pd_virt = 0xFFFF_8000_0000_0000 | pd_phys;
                let pd_entry = *((pd_virt + pd_idx * 8) as *const u64);
                crate::serial_println!(
                    "  PD[{}] = {:#x} (P={} W={} U={} A={} PWT={} PCD={} PS={})",
                    pd_idx,
                    pd_entry,
                    pd_entry & 1,
                    (pd_entry >> 1) & 1,
                    (pd_entry >> 2) & 1,
                    (pd_entry >> 5) & 1,
                    (pd_entry >> 3) & 1,
                    (pd_entry >> 4) & 1,
                    (pd_entry >> 7) & 1
                );

                if pd_entry & 1 == 0 {
                    crate::serial_println!("  PD entry not present!");
                } else if (pd_entry >> 7) & 1 != 0 {
                    crate::serial_println!("  PD entry is 2MB page!");
                } else {
                    // Walk PT
                    let pt_phys = pd_entry & 0xFFFF_FFFF_F000;
                    let pt_virt = 0xFFFF_8000_0000_0000 | pt_phys;
                    let pt_entry = *((pt_virt + pt_idx * 8) as *const u64);
                    crate::serial_println!(
                        "  PT[{}] = {:#x} (P={} W={} U={} A={} D={} PAT={} G={})",
                        pt_idx,
                        pt_entry,
                        pt_entry & 1,
                        (pt_entry >> 1) & 1,
                        (pt_entry >> 2) & 1,
                        (pt_entry >> 5) & 1,
                        (pt_entry >> 6) & 1,
                        (pt_entry >> 7) & 1,
                        (pt_entry >> 8) & 1
                    );

                    if pt_entry & 1 == 0 {
                        crate::serial_println!("  PT entry not present!");
                    } else {
                        let final_phys = (pt_entry & 0xFFFF_FFFF_F000) | (vaddr & 0xFFF);
                        crate::serial_println!("  Final physical address: {:#x}", final_phys);
                    }
                }
            }
        }
    }

    // — BlackLatch: User-mode page faults kill the process, not the kernel.
    // Only kernel faults are truly fatal. Enhanced diagnostics so we stop
    // guessing whether it's COW, corruption, or a ghost in the machine.
    if error & 4 != 0 {
        crate::serial_println!(
            "[SIGSEGV] User page fault at addr {:#x}, RIP {:#x}, error={:#x}",
            cr2,
            frame.rip,
            error
        );
        // — GraveShift: Decode the error code bits. Every bit tells a story,
        // usually a horror story.
        crate::serial_println!(
            "  error bits: present={} write={} user={} rsvd={} ifetch={}",
            error & 1,
            (error >> 1) & 1,
            (error >> 2) & 1,
            (error >> 3) & 1,
            (error >> 4) & 1
        );
        // — GraveShift: PTE walk for the faulting address — check COW bit (bit 9).
        // The generic walk above already ran, but this summary is what matters
        // for the SIGSEGV triage.
        unsafe {
            let cr3_val: u64;
            core::arch::asm!("mov {}, cr3", out(reg) cr3_val, options(nomem, nostack));
            let pml4_phys = cr3_val & 0xFFFF_FFFF_F000;
            let pml4_virt = 0xFFFF_8000_0000_0000 | pml4_phys;
            let pml4_idx = (cr2 >> 39) & 0x1FF;
            let pml4e = *((pml4_virt + pml4_idx * 8) as *const u64);
            if pml4e & 1 != 0 {
                let pdpt_virt = 0xFFFF_8000_0000_0000 | (pml4e & 0xFFFF_FFFF_F000);
                let pdpt_idx = (cr2 >> 30) & 0x1FF;
                let pdpte = *((pdpt_virt + pdpt_idx * 8) as *const u64);
                if pdpte & 1 != 0 && (pdpte >> 7) & 1 == 0 {
                    let pd_virt = 0xFFFF_8000_0000_0000 | (pdpte & 0xFFFF_FFFF_F000);
                    let pd_idx = (cr2 >> 21) & 0x1FF;
                    let pde = *((pd_virt + pd_idx * 8) as *const u64);
                    if pde & 1 != 0 && (pde >> 7) & 1 == 0 {
                        let pt_virt = 0xFFFF_8000_0000_0000 | (pde & 0xFFFF_FFFF_F000);
                        let pt_idx = (cr2 >> 12) & 0x1FF;
                        let pte = *((pt_virt + pt_idx * 8) as *const u64);
                        crate::serial_println!(
                            "  PTE={:#x} P={} W={} U={} COW={} phys={:#x}",
                            pte,
                            pte & 1,
                            (pte >> 1) & 1,
                            (pte >> 2) & 1,
                            (pte >> 9) & 1,
                            pte & 0xFFFF_FFFF_F000
                        );
                    }
                }
            }
        }
        let kill_cb = unsafe { *core::ptr::addr_of!(USER_FAULT_KILL_CALLBACK) };
        if let Some(kill) = kill_cb {
            kill(0, frame.rip, 11); // 11 = SIGSEGV
            return; // -- BlackLatch: Process flatlined, kernel survives
        }
        // -- GraveShift: No kill callback registered yet, fall through to panic
        crate::serial_println!("[WARN] No user fault kill callback, kernel panic on user fault");
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

/// Timer tick counter — SableWire: AtomicU64 for SMP safety.
/// Only BSP (APIC ID 0) increments; APs read-only. Prevents the
/// data-race where 4 CPUs stomp a bare `static mut` and corrupt
/// the counter or inflate the tick rate 4×.
static TIMER_TICKS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Terminal tick callback (called at ~30 FPS)
static mut TERMINAL_TICK_CALLBACK: Option<fn()> = None;

/// Ticks between terminal updates (for 30 FPS at 100 Hz timer = 3 ticks)
/// — NeonVale: Back to 3 now that chunked writes fixed the lock contention.
/// 30 FPS rendering is plenty when the app can actually get the lock between chunks.
const TERMINAL_TICK_INTERVAL: u64 = 3;

/// Last tick when terminal was updated
/// — SableWire: Only BSP touches this (gated in handle_timer), so no SMP race.
static mut LAST_TERMINAL_TICK: u64 = 0;

/// -- BlackLatch: User fault kill callback --
/// Signature: (pid, faulting_rip, signal_number)
/// Kills the offending process so the kernel lives on.
pub type UserFaultKillCallback = fn(u64, u64, u64);

/// Global user fault kill callback
static mut USER_FAULT_KILL_CALLBACK: Option<UserFaultKillCallback> = None;

/// Register a callback to kill user processes that cause hardware faults.
///
/// # Safety
/// Callback must be safe to call from exception context.
pub unsafe fn set_user_fault_kill_callback(callback: UserFaultKillCallback) {
    use core::ptr::addr_of_mut;
    unsafe {
        *addr_of_mut!(USER_FAULT_KILL_CALLBACK) = Some(callback);
    }
}

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

/// Register a terminal tick callback (called at ~30 FPS)
///
/// # Safety
/// The callback must be valid and thread-safe.
pub unsafe fn set_terminal_tick_callback(callback: fn()) {
    use core::ptr::addr_of_mut;
    unsafe {
        let cb_ptr = addr_of_mut!(TERMINAL_TICK_CALLBACK);
        *cb_ptr = Some(callback);
    }
}

/// Timer interrupt handler
///
/// Takes current RSP, returns RSP to restore from (may be different for context switch)
extern "C" fn handle_timer(current_rsp: u64) -> u64 {
    use core::ptr::addr_of;
    use core::sync::atomic::Ordering;

    // — PatchBay: Start cycle counter for ISR profiling
    let start_cycles = perf::rdtsc();

    // — SableWire: SMP-safe tick handling. Only BSP increments the global
    // counter; APs just read it. This keeps the tick rate at the intended
    // 100 Hz regardless of CPU count and eliminates the data-race on the
    // old `static mut` counter.
    let is_bsp = crate::apic::id() == 0;

    let current_tick = if is_bsp {
        TIMER_TICKS.fetch_add(1, Ordering::Relaxed) + 1
    } else {
        TIMER_TICKS.load(Ordering::Relaxed)
    };

    #[cfg(feature = "debug-timer")]
    {
        if is_bsp && current_tick % 100 == 0 {
            crate::serial_println!("[TIMER] tick={:#x} rsp={:#x}", current_tick, current_rsp);
        }
    }

    // Send EOI to APIC first (before potentially long scheduler work)
    crate::apic::end_of_interrupt();

    // — PatchBay: Print performance statistics every 500 ticks (~5 seconds @ 100Hz)
    // Only on BSP to avoid interleaved output from multiple CPUs.
    if is_bsp && current_tick % 500 == 0 {
        perf::stats::print_perf_stats(perf::counters(), current_tick);
    }

    // — SableWire: Terminal tick only on BSP. Console I/O is single-
    // threaded; running the callback from 4 CPUs races on VT state
    // and quadruples serial output in ISR context.
    if is_bsp {
        unsafe {
            use core::ptr::addr_of_mut;
            let last_tick_ptr = addr_of_mut!(LAST_TERMINAL_TICK);
            if current_tick.saturating_sub(*last_tick_ptr) >= TERMINAL_TICK_INTERVAL {
                *last_tick_ptr = current_tick;
                let cb_ptr = addr_of!(TERMINAL_TICK_CALLBACK);
                if let Some(callback) = *cb_ptr {
                    #[cfg(feature = "debug-timer")]
                    {
                        if current_tick % 100 == 0 {
                            crate::serial_println!("[TIMER] Terminal tick callback");
                        }
                    }
                    callback();
                }
            }
        }
    }

    // Scheduler callback runs on ALL CPUs — each CPU manages its own
    // run queue and needs preemption ticks.
    let new_rsp = unsafe {
        let cb_ptr = addr_of!(SCHEDULER_CALLBACK);
        if let Some(callback) = *cb_ptr {
            #[cfg(feature = "debug-timer")]
            {
                if is_bsp && current_tick % 100 == 0 {
                    crate::serial_println!("[TIMER] Calling scheduler");
                }
            }
            callback(current_rsp)
        } else {
            current_rsp
        }
    };

    #[cfg(feature = "debug-timer")]
    {
        if new_rsp != current_rsp {
            crate::serial_println!(
                "[TIMER] Context switch: {:#x} -> {:#x}",
                current_rsp,
                new_rsp
            );
        }
    }

    // — PatchBay: Record ISR execution time
    let end_cycles = perf::rdtsc();
    let elapsed = end_cycles.saturating_sub(start_cycles);
    perf::counters().record_timer_irq(elapsed);

    // — PatchBay: PERF-WARN gated behind debug-perf. Each message = 53 bytes serial.
    // Under load, fires hundreds/sec → exceeds baud rate → creates feedback loop
    // where serial I/O causes next ISR to exceed threshold too. See perf-warn-feedback-loop.md.
    #[cfg(feature = "debug-perf")]
    if elapsed > 1_000_000 {
        unsafe {
            os_log::write_str_raw("\n[PERF-WARN] Timer ISR took ");
            write_u32_via_oslog(elapsed as u32);
            os_log::write_str_raw(" cycles (");
            write_u32_via_oslog((elapsed / 1000) as u32);
            os_log::write_str_raw("K)\n");
        }
    }

    new_rsp
}

/// Get current timer tick count
pub fn ticks() -> u64 {
    TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed)
}

// ============================================================================
// PS/2 Keyboard Controller (i8042) Initialization
// ============================================================================

/// Wait for i8042 input buffer to be empty (ready to accept commands)
fn i8042_wait_input() {
    for _ in 0..10000 {
        let status: u8;
        unsafe {
            core::arch::asm!("in al, 0x64", out("al") status, options(nomem, nostack, preserves_flags));
        }
        if status & 0x02 == 0 {
            return;
        }
    }
}

/// Wait for i8042 output buffer to have data
fn i8042_wait_output() -> bool {
    for _ in 0..10000 {
        let status: u8;
        unsafe {
            core::arch::asm!("in al, 0x64", out("al") status, options(nomem, nostack, preserves_flags));
        }
        if status & 0x01 != 0 {
            return true;
        }
    }
    false
}

/// Flush i8042 output buffer
fn i8042_flush() {
    for _ in 0..64 {
        let status: u8;
        unsafe {
            core::arch::asm!("in al, 0x64", out("al") status, options(nomem, nostack, preserves_flags));
        }
        if status & 0x01 == 0 {
            break;
        }
        // Read and discard
        unsafe {
            core::arch::asm!("in al, 0x60", out("al") _, options(nomem, nostack, preserves_flags));
        }
    }
}

/// Initialize the PS/2 keyboard controller (i8042)
///
/// After UEFI ExitBootServices, the PS/2 controller may be in an unknown state.
/// This initializes it to generate IRQ 1 on keyboard input.
pub fn init_ps2_keyboard() {
    // Disable both PS/2 ports during init
    i8042_wait_input();
    unsafe {
        core::arch::asm!("out 0x64, al", in("al") 0xADu8, options(nomem, nostack, preserves_flags));
    }
    i8042_wait_input();
    unsafe {
        core::arch::asm!("out 0x64, al", in("al") 0xA7u8, options(nomem, nostack, preserves_flags));
    }

    // Flush output buffer
    i8042_flush();

    // Read current configuration byte
    i8042_wait_input();
    unsafe {
        core::arch::asm!("out 0x64, al", in("al") 0x20u8, options(nomem, nostack, preserves_flags));
    }
    let mut config: u8 = 0;
    if i8042_wait_output() {
        unsafe {
            core::arch::asm!("in al, 0x60", out("al") config, options(nomem, nostack, preserves_flags));
        }
    }

    // Enable port 1 IRQ (bit 0) and port 2 IRQ (bit 1)
    // Clear port 1 clock disable (bit 4) and port 2 clock disable (bit 5)
    config |= 0x01; // Enable first port interrupt (IRQ 1)
    config |= 0x02; // Enable second port interrupt (IRQ 12)
    config &= !0x10; // Enable first port clock (clear disable bit)
    config &= !0x20; // Enable second port clock (clear disable bit)

    // Write configuration byte back
    i8042_wait_input();
    unsafe {
        core::arch::asm!("out 0x64, al", in("al") 0x60u8, options(nomem, nostack, preserves_flags));
    }
    i8042_wait_input();
    unsafe {
        core::arch::asm!("out 0x60, al", in("al") config, options(nomem, nostack, preserves_flags));
    }

    // Enable first PS/2 port (keyboard)
    i8042_wait_input();
    unsafe {
        core::arch::asm!("out 0x64, al", in("al") 0xAEu8, options(nomem, nostack, preserves_flags));
    }

    // Enable second PS/2 port (mouse)
    i8042_wait_input();
    unsafe {
        core::arch::asm!("out 0x64, al", in("al") 0xA8u8, options(nomem, nostack, preserves_flags));
    }

    // Reset keyboard device (send 0xFF)
    i8042_wait_input();
    unsafe {
        core::arch::asm!("out 0x60, al", in("al") 0xFFu8, options(nomem, nostack, preserves_flags));
    }

    // Wait for ACK (0xFA) and self-test pass (0xAA)
    if i8042_wait_output() {
        let _ack: u8;
        unsafe {
            core::arch::asm!("in al, 0x60", out("al") _ack, options(nomem, nostack, preserves_flags));
        }
    }
    if i8042_wait_output() {
        let _result: u8;
        unsafe {
            core::arch::asm!("in al, 0x60", out("al") _result, options(nomem, nostack, preserves_flags));
        }
    }

    // Flush any remaining data
    i8042_flush();

    crate::serial_println!("[PS/2] Keyboard controller initialized");
}

// ============================================================================
// WATOS-Style Keyboard Buffer
// ============================================================================

/// Keyboard buffer (exactly like WATOS)
pub static mut KEY_BUFFER: [u8; 32] = [0; 32];
pub static mut KEY_READ_POS: usize = 0;
pub static mut KEY_WRITE_POS: usize = 0;

/// Get a scancode from keyboard buffer (exactly like WATOS)
pub fn get_scancode() -> Option<u8> {
    unsafe {
        if KEY_READ_POS != KEY_WRITE_POS {
            let scancode = KEY_BUFFER[KEY_READ_POS];
            KEY_READ_POS = (KEY_READ_POS + 1) & 31;
            Some(scancode)
        } else {
            None
        }
    }
}

/// Poll i8042 directly for a scancode (fallback when IRQ1 doesn't fire)
///
/// Reads the i8042 status register (port 0x64) and if data is available,
/// reads the scancode from port 0x60. This bypasses the IRQ1 path entirely.
///
/// # Safety
/// Must only be called from interrupt context (e.g., terminal_tick in timer ISR)
/// where we know no other code is accessing the i8042 ports concurrently.
pub unsafe fn poll_keyboard() -> Option<u8> {
    let status: u8;
    // SAFETY: Reading from i8042 status port in ISR context; caller ensures no concurrent access
    // — GraveShift
    unsafe {
        core::arch::asm!("in al, 0x64", out("al") status, options(nomem, nostack, preserves_flags));
    }
    if status & 0x01 != 0 {
        // Check bit 5 — if set, this is mouse data, not keyboard
        if status & 0x20 != 0 {
            return None; // Mouse data — leave it for the mouse IRQ handler
        }
        // Data available - read scancode from port 0x60
        let scancode: u8;
        // SAFETY: Reading from i8042 data port; status bit confirmed data is ready
        // — GraveShift
        unsafe {
            core::arch::asm!("in al, 0x60", out("al") scancode, options(nomem, nostack, preserves_flags));
        }
        Some(scancode)
    } else {
        None
    }
}
