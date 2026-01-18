//! Syscall/sysret mechanism for x86_64
//!
//! Sets up the syscall instruction for fast user-to-kernel transitions.

use core::arch::{asm, naked_asm};

use crate::gdt::{KERNEL_CS, KERNEL_DS};

/// MSR addresses
mod msr {
    pub const EFER: u32 = 0xC000_0080;
    pub const STAR: u32 = 0xC000_0081;
    pub const LSTAR: u32 = 0xC000_0082;
    pub const SFMASK: u32 = 0xC000_0084;
}

/// EFER bits
mod efer {
    pub const SCE: u64 = 1 << 0;  // System Call Extensions
}

/// RFLAGS bits to mask on syscall entry
/// We clear: IF (interrupts), DF (direction), TF (trap), AC (alignment check)
const SFMASK_VALUE: u64 = 0x4700;  // IF=0x200, DF=0x400, TF=0x100, AC=0x4_0000

/// Read a Model Specific Register
#[inline]
unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | (low as u64)
}

/// Write a Model Specific Register
#[inline]
unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") low,
            in("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Syscall handler function type
///
/// Arguments: (syscall_number, arg1, arg2, arg3, arg4, arg5, arg6)
/// Returns: result value (or negative errno)
pub type SyscallHandler = fn(u64, u64, u64, u64, u64, u64, u64) -> i64;

/// Global syscall handler
static mut SYSCALL_HANDLER: Option<SyscallHandler> = None;

/// Register a syscall handler
///
/// # Safety
/// Must only be called once during initialization.
pub unsafe fn set_syscall_handler(handler: SyscallHandler) {
    use core::ptr::addr_of_mut;
    unsafe {
        *addr_of_mut!(SYSCALL_HANDLER) = Some(handler);
    }
}

/// Initialize the syscall mechanism
///
/// Sets up MSRs for syscall/sysret instruction.
///
/// # Safety
/// Must only be called once during kernel initialization, after GDT is set up.
pub unsafe fn init() {
    // Enable System Call Extensions in EFER
    let efer = unsafe { rdmsr(msr::EFER) };
    unsafe { wrmsr(msr::EFER, efer | efer::SCE) };

    // Set up STAR: segment selectors for syscall/sysret
    // Bits 47:32 = kernel CS (for SYSCALL: CS=this, SS=this+8)
    // Bits 63:48 = user CS base (for SYSRET 64-bit: SS=this+8, CS=this+16)
    //
    // With our GDT layout:
    // - SYSCALL: CS=0x08 (KERNEL_CS), SS=0x10 (KERNEL_DS)
    // - SYSRET:  SS=0x10+8=0x18|3=0x1B (USER_DS), CS=0x10+16=0x20|3=0x23 (USER_CS)
    let star = ((KERNEL_DS as u64 - 8) << 48) | ((KERNEL_CS as u64) << 32);
    unsafe { wrmsr(msr::STAR, star) };

    // Set up LSTAR: syscall entry point
    let entry = syscall_entry as *const () as u64;
    unsafe { wrmsr(msr::LSTAR, entry) };

    // Set up SFMASK: RFLAGS bits to clear on syscall
    unsafe { wrmsr(msr::SFMASK, SFMASK_VALUE) };

    crate::serial_println!("[SYSCALL] Initialized syscall mechanism");
}

/// Syscall entry point
///
/// On entry (from syscall instruction):
/// - RCX = user RIP (return address)
/// - R11 = user RFLAGS
/// - RAX = syscall number
/// - RDI, RSI, RDX, R10, R8, R9 = arguments 1-6
/// - RSP = user RSP (unchanged by syscall)
///
/// We need to:
/// 1. Switch to kernel stack
/// 2. Save user state
/// 3. Call the syscall handler
/// 4. Restore and sysret
#[unsafe(naked)]
extern "C" fn syscall_entry() {
    naked_asm!(
        // At this point:
        // - We're in kernel mode (CPL=0)
        // - Interrupts are disabled (IF cleared by SFMASK)
        // - RCX = user RIP, R11 = user RFLAGS
        // - RSP still points to user stack!

        // Swap to kernel stack using swapgs + TSS RSP0
        // First, swap GS base to get kernel per-CPU data
        "swapgs",

        // Save user RSP to scratch register, load kernel RSP from TSS
        // We use GS:0 to store the kernel stack pointer temporarily
        // For now, use a simpler approach: save user RSP in R12, use static kernel stack
        "mov r12, rsp",                    // Save user RSP in r12

        // Load kernel stack from TSS RSP0 (at GS:4, but we don't have per-CPU yet)
        // For now, use a dedicated syscall stack
        "mov rsp, gs:[0]",                 // Load kernel stack from per-CPU area

        // Push user state for later sysret
        "push r11",                        // User RFLAGS
        "push rcx",                        // User RIP
        "push r12",                        // User RSP

        // Save callee-saved registers (we might clobber them)
        "push rbp",
        "push rbx",
        "push r13",
        "push r14",
        "push r15",

        // Re-enable interrupts now that we're on kernel stack
        "sti",

        // Set up arguments for syscall handler:
        // handler(number, arg1, arg2, arg3, arg4, arg5, arg6)
        // Args come in: RAX=number, RDI=arg1, RSI=arg2, RDX=arg3, R10=arg4, R8=arg5, R9=arg6
        // System V ABI wants: RDI, RSI, RDX, RCX, R8, R9
        // So: RDI=number, RSI=arg1, RDX=arg2, RCX=arg3, R8=arg4, R9=arg5, [rsp]=arg6
        "push r9",                         // arg6 on stack
        "mov r9, r8",                      // arg5
        "mov r8, r10",                     // arg4
        "mov rcx, rdx",                    // arg3
        "mov rdx, rsi",                    // arg2
        "mov rsi, rdi",                    // arg1
        "mov rdi, rax",                    // syscall number

        // Call the syscall handler
        "call {handler}",

        // Clean up stack arg
        "add rsp, 8",

        // Disable interrupts before sysret
        "cli",

        // Result is in RAX

        // Restore callee-saved registers
        "pop r15",
        "pop r14",
        "pop r13",
        "pop rbx",
        "pop rbp",

        // Restore user state
        "pop r12",                         // User RSP
        "pop rcx",                         // User RIP (for sysret)
        "pop r11",                         // User RFLAGS (for sysret)

        // Restore user RSP
        "mov rsp, r12",

        // Swap GS back to user
        "swapgs",

        // Return to user mode
        // SYSRETQ will:
        // - Load RIP from RCX
        // - Load RFLAGS from R11
        // - Set CS to (STAR[63:48] + 16) | 3
        // - Set SS to (STAR[63:48] + 8) | 3
        "sysretq",

        handler = sym syscall_dispatch,
    );
}

/// Dispatch syscall to the registered handler
extern "C" fn syscall_dispatch(
    number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    use core::ptr::addr_of;

    unsafe {
        if let Some(handler) = *addr_of!(SYSCALL_HANDLER) {
            handler(number, arg1, arg2, arg3, arg4, arg5, arg6)
        } else {
            // No handler registered, return -ENOSYS
            -38
        }
    }
}

/// Per-CPU syscall data
///
/// This must be set up before syscalls can work.
#[repr(C)]
pub struct SyscallCpuData {
    /// Kernel stack pointer for syscall entry
    pub kernel_rsp: u64,
    /// User stack pointer (saved during syscall)
    pub user_rsp: u64,
}

/// Initialize per-CPU syscall data
///
/// # Safety
/// Must be called with a valid kernel stack pointer.
pub unsafe fn set_kernel_stack(kernel_rsp: u64) {
    // For now, we store the kernel RSP in the GS base
    // In a proper implementation, this would be per-CPU data
    // accessed via GS segment.

    // Write kernel RSP to a known location accessible via GS
    // We'll use KERNEL_GS_BASE MSR to set up a pointer to our data
    const KERNEL_GS_BASE: u32 = 0xC000_0102;

    // Allocate a static for the CPU data
    static mut CPU_DATA: SyscallCpuData = SyscallCpuData {
        kernel_rsp: 0,
        user_rsp: 0,
    };

    use core::ptr::addr_of_mut;
    unsafe {
        let cpu_data = addr_of_mut!(CPU_DATA);
        (*cpu_data).kernel_rsp = kernel_rsp;

        // Set KERNEL_GS_BASE to point to our CPU data
        // This will be swapped in on syscall entry via swapgs
        wrmsr(KERNEL_GS_BASE, cpu_data as u64);
    }
}
