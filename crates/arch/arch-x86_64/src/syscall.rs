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
    pub const SCE: u64 = 1 << 0; // System Call Extensions
    pub const NXE: u64 = 1 << 11; // No-Execute Enable
}

/// RFLAGS bits to mask on syscall entry
/// We clear: IF (interrupts), DF (direction), TF (trap), AC (alignment check)
const SFMASK_VALUE: u64 = 0x4700; // IF=0x200, DF=0x400, TF=0x100, AC=0x4_0000

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
    // Enable System Call Extensions and NX (No-Execute) in EFER
    let efer = unsafe { rdmsr(msr::EFER) };
    unsafe { wrmsr(msr::EFER, efer | efer::SCE | efer::NXE) };

    // Set up STAR: segment selectors for syscall/sysret
    // Bits 47:32 = kernel CS (for SYSCALL: CS=this, SS=this+8)
    // Bits 63:48 = user base (for SYSRET 64-bit: SS=this+8|3, CS=this+16|3)
    //
    // With our GDT layout:
    // - SYSCALL: CS=0x08 (KERNEL_CS), SS=0x08+8=0x10 (KERNEL_DS)
    // - SYSRET:  SS=0x10+8=0x18|3=0x1B (USER_DS), CS=0x10+16=0x20|3=0x23 (USER_CS)
    //
    // So bits 47:32 = KERNEL_CS = 0x08
    //    bits 63:48 = KERNEL_DS = 0x10 (NOT KERNEL_DS-8!)
    let star = ((KERNEL_DS as u64) << 48) | ((KERNEL_CS as u64) << 32);
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
/// 2. Save user state (including to SYSCALL_USER_CONTEXT for fork)
/// 3. Call the syscall handler
/// 4. Restore and sysret
#[unsafe(naked)]
pub extern "C" fn syscall_entry() {
    naked_asm!(
        // === PROLOGUE: Save critical registers to per-CPU scratch space ===
        // At this point: CPL=0, interrupts disabled, RSP = user stack
        // RCX = user RIP, R11 = user RFLAGS, RAX = syscall number
        // ALL other registers contain user values

        // Swap GS to get kernel per-CPU data
        "swapgs",

        // CRITICAL: Save user values BEFORE using any register as scratch
        // GS layout: [0]=kernel_rsp, [8]=scratch_rsp, [16]=scratch_rax,
        //             [24]=scratch_r12, [32]=scratch_rcx (unused now)
        // Note: RCX contains user RIP (set by syscall instruction)
        //       R11 contains user RFLAGS (set by syscall instruction)
        //       User's original RCX/R11 are NOT recoverable - this is the syscall ABI
        "mov gs:[8], rsp",                 // Save user RSP
        "mov gs:[16], rax",                // Save syscall number (user RAX)
        "mov gs:[24], r12",                // Save user R12
        // gs:[32] is unused - we no longer read from user stack

        // Now switch to kernel stack
        "mov rsp, gs:[0]",

        // DEBUG: Save RFLAGS at syscall entry (before STAC)
        "pushfq",
        "pop r12",
        "mov [{ac_at_entry}], r12",

        // === Push user state for sysret ===
        // Stack layout (growing down):
        // [rsp+72] = user RFLAGS
        // [rsp+64] = user RIP
        // [rsp+56] = user RSP
        // [rsp+48] = RBP
        // [rsp+40] = RBX
        // [rsp+32] = R13
        // [rsp+24] = R14
        // [rsp+16] = R15
        // [rsp+8]  = user's original R12
        // [rsp+0]  = (stack arg for call, if needed)

        "push r11",                        // User RFLAGS (from R11)
        "push rcx",                        // User RIP (from RCX)
        "mov r12, gs:[8]",                 // Get user RSP from scratch
        "push r12",                        // User RSP

        // Push callee-saved registers
        "push rbp",
        "push rbx",
        "push r13",
        "push r14",
        "push r15",

        // Push user's original R12 (from GS scratch)
        "mov r12, gs:[24]",
        "push r12",

        // === Save to SYSCALL_USER_CONTEXT for fork() ===
        // Stack layout after 9 pushes:
        // [rsp+0]  = user R12 (original)
        // [rsp+8]  = R15
        // [rsp+16] = R14
        // [rsp+24] = R13
        // [rsp+32] = RBX
        // [rsp+40] = RBP
        // [rsp+48] = user RSP
        // [rsp+56] = user RIP
        // [rsp+64] = user RFLAGS

        "lea r13, [{user_ctx}]",

        // RIP, RSP, RFLAGS from stack
        "mov r12, [rsp + 56]",             // User RIP
        "mov [r13 + 0], r12",
        "mov r12, [rsp + 48]",             // User RSP
        "mov [r13 + 8], r12",
        "mov r12, [rsp + 64]",             // User RFLAGS
        "mov [r13 + 16], r12",

        // Syscall number from scratch space
        "mov r12, gs:[16]",
        "mov [r13 + 24], r12",             // ctx.rax = syscall number

        // Other registers (still have user values)
        "mov [r13 + 32], rbx",
        // Note: RCX was clobbered by syscall instruction (now contains user RIP)
        // Store user RIP as RCX - this is what sysret will restore anyway
        "mov r12, [rsp + 56]",             // User RIP (same as what was in RCX)
        "mov [r13 + 40], r12",
        "mov [r13 + 48], rdx",             // arg3
        "mov [r13 + 56], rsi",             // arg2
        "mov [r13 + 64], rdi",             // arg1
        "mov [r13 + 72], rbp",
        "mov [r13 + 80], r8",              // arg5
        "mov [r13 + 88], r9",              // arg6
        "mov [r13 + 96], r10",             // arg4
        "mov [r13 + 104], r11",            // R11 has user RFLAGS

        // Callee-saved from stack
        "mov r12, [rsp + 0]",              // Original user R12
        "mov [r13 + 112], r12",
        "mov r12, [rsp + 8]",              // R15
        "mov [r13 + 136], r12",
        "mov r12, [rsp + 16]",             // R14
        "mov [r13 + 128], r12",
        "mov r12, [rsp + 24]",             // R13
        "mov [r13 + 120], r12",

        // === Call syscall handler ===
        // Enable interrupts
        "sti",

        // Enable user memory access for SMAP
        "stac",

        // DEBUG: Check if AC flag is actually set after STAC
        "pushfq",
        "pop r12",
        "mov [{stac_debug_rflags}], r12",

        // Save caller-saved registers that must be preserved for the user
        // (syscall only clobbers RCX and R11 according to ABI)
        "push r8",
        "push r9",
        "push r10",
        "push rdi",                        // Save user RDI
        "push rsi",                        // Save user RSI
        "push rdx",                        // Save user RDX

        // Set up arguments: handler(number, arg1, arg2, arg3, arg4, arg5, arg6)
        // First save arg6 (r9) before we clobber it
        "mov rax, r9",                     // Save arg6 (r9) to rax temporarily
        "push rax",                        // arg6 on stack

        "mov r9, r8",                      // arg5 (was in r8)
        "mov r8, r10",                     // arg4 (was in r10)
        "mov rcx, rdx",                    // arg3 (was in rdx)
        "mov rdx, rsi",                    // arg2 (was in rsi)
        "mov rsi, rdi",                    // arg1 (was in rdi)
        "mov rdi, gs:[16]",                // syscall number from scratch

        // DEBUG: Save AC flag before call
        "pushfq",
        "pop r12",
        "mov [{ac_before_call}], r12",

        "call {handler}",

        // DEBUG: Save AC flag after call
        "pushfq",
        "pop r12",
        "mov [{ac_after_call}], r12",

        // Clean up stack arg (the pushed arg6)
        "add rsp, 8",

        // Restore caller-saved registers
        "pop rdx",                         // Restore user RDX
        "pop rsi",                         // Restore user RSI
        "pop rdi",                         // Restore user RDI
        "pop r10",
        "pop r9",
        "pop r8",

        // === EPILOGUE: Restore and sysret ===
        // RAX = return value (preserve it!)

        // Disable user memory access (restore SMAP protection)
        "clac",

        // Disable interrupts
        "cli",

        // Restore callee-saved registers
        // Stack: [R12, R15, R14, R13, RBX, RBP, RSP, RIP, RFLAGS]
        "pop r12",                         // User's original R12
        "pop r15",
        "pop r14",
        "pop r13",
        "pop rbx",
        "pop rbp",

        // Now stack has: [user RSP, user RIP, user RFLAGS]
        // We need: RSP = user RSP, RCX = user RIP, R11 = user RFLAGS
        // RAX has return value - keep it there!
        // IMPORTANT: Don't use R10 as scratch - it must be preserved for user!

        // DEBUG: Save stack pointer before reads
        "mov [{sysret_stack_ptr}], rsp",

        // Load sysret values - load RCX and R11 first (they're clobbered by sysret anyway)
        "mov rcx, [rsp + 8]",              // User RIP -> RCX (for sysret)
        "mov r11, [rsp + 16]",             // User RFLAGS -> R11 (for sysret)

        // DEBUG: Save loaded values (use rcx value we just loaded for rsp debug)
        "push rax",                        // Save return value temporarily
        "mov rax, [rsp + 8]",              // Get user RSP (offset +8 because we pushed rax)
        "mov [{sysret_rsp}], rax",
        "pop rax",                         // Restore return value
        "mov [{sysret_rcx}], rcx",
        "mov [{sysret_r11}], r11",
        "mov [{sysret_rax}], rax",

        // Swap GS back to user mode (BEFORE switching RSP!)
        "swapgs",

        // Switch to user stack - read [rsp] and write to rsp in one instruction
        // This works because x86 evaluates the source before writing the destination
        "mov rsp, [rsp]",

        // Return to user mode
        // RAX = return value, RCX = user RIP, R11 = user RFLAGS
        "sysretq",

        handler = sym syscall_dispatch,
        user_ctx = sym SYSCALL_USER_CONTEXT,
        sysret_stack_ptr = sym SYSRET_DEBUG_STACK_PTR,
        sysret_rsp = sym SYSRET_DEBUG_RSP,
        sysret_rcx = sym SYSRET_DEBUG_RCX,
        sysret_r11 = sym SYSRET_DEBUG_R11,
        sysret_rax = sym SYSRET_DEBUG_RAX,
        stac_debug_rflags = sym STAC_DEBUG_RFLAGS,
        ac_before_call = sym AC_BEFORE_CALL,
        ac_after_call = sym AC_AFTER_CALL,
        ac_at_entry = sym AC_AT_ENTRY,
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
/// Fields are accessed at fixed offsets from GS base in assembly:
/// - offset 0:  kernel_rsp
/// - offset 8:  scratch_rsp (for saving user RSP)
/// - offset 16: scratch_rax (for saving syscall number)
/// - offset 24: scratch_r12 (for saving user R12)
/// - offset 32: scratch_rcx (for saving user RCX prior to syscall)
#[repr(C)]
pub struct SyscallCpuData {
    /// Kernel stack pointer for syscall entry
    pub kernel_rsp: u64, // offset 0
    /// Scratch space for user RSP
    pub scratch_rsp: u64, // offset 8
    /// Scratch space for syscall number (RAX)
    pub scratch_rax: u64, // offset 16
    /// Scratch space for user R12
    pub scratch_r12: u64, // offset 24
    /// Scratch space for original user RCX (saved by libc before syscall)
    pub scratch_rcx: u64, // offset 32
}

/// User context at syscall entry
///
/// This is populated by syscall_entry before dispatching to the handler.
/// Used by fork() to capture the parent's context for the child.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct SyscallUserContext {
    pub rip: u64,
    pub rsp: u64,
    pub rflags: u64,
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
}

/// Global syscall user context (populated on each syscall entry)
static mut SYSCALL_USER_CONTEXT: SyscallUserContext = SyscallUserContext {
    rip: 0,
    rsp: 0,
    rflags: 0,
    rax: 0,
    rbx: 0,
    rcx: 0,
    rdx: 0,
    rsi: 0,
    rdi: 0,
    rbp: 0,
    r8: 0,
    r9: 0,
    r10: 0,
    r11: 0,
    r12: 0,
    r13: 0,
    r14: 0,
    r15: 0,
};

/// Debug: capture values before sysretq
#[unsafe(no_mangle)]
pub static mut SYSRET_DEBUG_RSP: u64 = 0xDEAD;
#[unsafe(no_mangle)]
pub static mut SYSRET_DEBUG_RCX: u64 = 0xDEAD;
#[unsafe(no_mangle)]
pub static mut SYSRET_DEBUG_R11: u64 = 0xDEAD;
#[unsafe(no_mangle)]
pub static mut SYSRET_DEBUG_RAX: u64 = 0xDEAD;
#[unsafe(no_mangle)]
pub static mut SYSRET_DEBUG_STACK_PTR: u64 = 0xDEAD;

#[unsafe(no_mangle)]
pub static mut STAC_DEBUG_RFLAGS: u64 = 0xDEAD;

#[unsafe(no_mangle)]
pub static mut AC_BEFORE_CALL: u64 = 0xDEAD;

#[unsafe(no_mangle)]
pub static mut AC_AFTER_CALL: u64 = 0xDEAD;

#[unsafe(no_mangle)]
pub static mut AC_AT_ENTRY: u64 = 0xDEAD;

/// Get the current syscall user context
///
/// This returns the user context at the point of syscall entry.
/// Only valid when called from within a syscall handler.
pub fn get_user_context() -> SyscallUserContext {
    use core::ptr::addr_of;
    unsafe { *addr_of!(SYSCALL_USER_CONTEXT) }
}

/// Save user context (called from syscall entry asm)
///
/// # Safety
/// Only called from syscall_entry assembly.
#[unsafe(no_mangle)]
unsafe extern "C" fn save_syscall_context(
    user_rip: u64,
    user_rsp: u64,
    user_rflags: u64,
    syscall_num: u64,
    arg1: u64, // rdi
    arg2: u64, // rsi
    arg3: u64, // rdx
    arg4: u64, // r10
    arg5: u64, // r8
    arg6: u64, // r9
    rbx: u64,
    rbp: u64,
    r12: u64, // Note: this is caller-saved R12, not user RSP
    r13: u64,
    r14: u64,
    r15: u64,
) {
    use core::ptr::addr_of_mut;
    unsafe {
        let ctx = addr_of_mut!(SYSCALL_USER_CONTEXT);
        (*ctx).rip = user_rip;
        (*ctx).rsp = user_rsp;
        (*ctx).rflags = user_rflags;
        (*ctx).rax = syscall_num;
        (*ctx).rdi = arg1;
        (*ctx).rsi = arg2;
        (*ctx).rdx = arg3;
        (*ctx).r10 = arg4;
        (*ctx).r8 = arg5;
        (*ctx).r9 = arg6;
        (*ctx).rbx = rbx;
        (*ctx).rbp = rbp;
        // RCX and R11 are clobbered by syscall, so use user values
        (*ctx).rcx = user_rip; // RCX had user RIP
        (*ctx).r11 = user_rflags; // R11 had user RFLAGS
        (*ctx).r12 = r12;
        (*ctx).r13 = r13;
        (*ctx).r14 = r14;
        (*ctx).r15 = r15;
    }
}

/// Per-CPU syscall data (static for single-CPU; would be per-CPU TLS on SMP)
static mut CPU_DATA: SyscallCpuData = SyscallCpuData {
    kernel_rsp: 0,
    scratch_rsp: 0,
    scratch_rax: 0,
    scratch_r12: 0,
    scratch_rcx: 0,
};

/// Initialize per-CPU syscall data and set up KERNEL_GS_BASE MSR.
///
/// This must be called ONCE during kernel initialization (before any syscalls).
/// It sets the KERNEL_GS_BASE MSR to point to CPU_DATA so that swapgs at
/// syscall entry loads the correct GS base.
///
/// # Safety
/// Must be called with a valid kernel stack pointer, before syscalls are enabled.
pub unsafe fn init_kernel_stack(kernel_rsp: u64) {
    const KERNEL_GS_BASE_MSR: u32 = 0xC000_0102;

    use core::ptr::addr_of_mut;
    unsafe {
        let cpu_data = addr_of_mut!(CPU_DATA);
        (*cpu_data).kernel_rsp = kernel_rsp;

        // Set KERNEL_GS_BASE to point to our CPU data
        // This will be swapped in on syscall entry via swapgs
        wrmsr(KERNEL_GS_BASE_MSR, cpu_data as u64);
    }
}

/// Update the kernel stack pointer for the current task.
///
/// Called during context switches to ensure syscall_entry uses the correct
/// kernel stack for the newly-scheduled task. Only updates CPU_DATA.kernel_rsp;
/// does NOT rewrite the KERNEL_GS_BASE MSR (which would destroy the saved
/// user GS base if called while in a syscall context where swapgs has already
/// been performed).
///
/// # Safety
/// Must be called with a valid kernel stack pointer.
pub unsafe fn set_kernel_stack(kernel_rsp: u64) {
    use core::ptr::addr_of_mut;
    unsafe {
        let cpu_data = addr_of_mut!(CPU_DATA);
        (*cpu_data).kernel_rsp = kernel_rsp;
    }
}
