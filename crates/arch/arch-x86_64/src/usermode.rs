//! User mode transition for x86_64
//!
//! Provides the mechanism to transition from Ring 0 to Ring 3.

use crate::gdt::{USER_CS, USER_DS};
use core::arch::naked_asm;

/// Jump to user mode
///
/// This function sets up the stack for iretq and transitions to Ring 3.
///
/// # Arguments
/// * `entry` - User-mode entry point (RIP)
/// * `user_stack` - User-mode stack pointer (RSP)
///
/// # Safety
/// - The entry point must be valid user-mode code
/// - The user stack must be properly mapped in user space
/// - The address space must have the user mappings set up
/// - This function never returns to the caller
#[unsafe(naked)]
pub unsafe extern "C" fn jump_to_usermode(entry: u64, user_stack: u64) -> ! {
    naked_asm!(
        // Arguments: rdi = entry point, rsi = user stack

        // Set up the stack frame for iretq:
        // [rsp+32] SS
        // [rsp+24] RSP
        // [rsp+16] RFLAGS
        // [rsp+8]  CS
        // [rsp+0]  RIP

        // Push SS (user data segment with RPL=3)
        "mov rax, {user_ds}",
        "push rax",

        // Push RSP (user stack pointer)
        "push rsi",

        // Push RFLAGS with IF set (interrupts enabled)
        // Use known safe value: IF=1, IOPL=0, NT=0, TF=0
        "mov rax, 0x202",
        "push rax",

        // Push CS (user code segment with RPL=3)
        "mov rax, {user_cs}",
        "push rax",

        // Push RIP (entry point)
        "push rdi",

        // Load user data segments BEFORE clearing registers
        "mov ax, {user_ds}",
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",

        // Clear all general purpose registers for security
        // (prevent leaking kernel data to user mode)
        "xor rax, rax",
        "xor rbx, rbx",
        "xor rcx, rcx",
        "xor rdx, rdx",
        "xor rsi, rsi",
        "xor rdi, rdi",
        "xor rbp, rbp",
        "xor r8, r8",
        "xor r9, r9",
        "xor r10, r10",
        "xor r11, r11",
        "xor r12, r12",
        "xor r13, r13",
        "xor r14, r14",
        "xor r15, r15",

        // NOTE: Do NOT swapgs here!
        // KERNEL_GS_BASE contains the kernel stack pointer for syscall handling
        // When user does syscall, syscall_entry will swapgs to get it

        // Jump to user mode
        "iretq",

        user_cs = const USER_CS as u64,
        user_ds = const USER_DS as u64,
    );
}

/// Switch to a new kernel stack, change page tables, and jump to user mode
///
/// This is needed because the initial kernel stack from the bootloader may not
/// be mapped in the user's page tables. This function switches to a kernel stack
/// that IS in the higher half (and thus preserved across page table switches),
/// then switches page tables and jumps to user mode.
///
/// # Arguments
/// * `kernel_stack` - New kernel stack top (must be in higher half)
/// * `pml4_phys` - Physical address of the user's PML4 table
/// * `entry` - User-mode entry point
/// * `user_stack` - User-mode stack pointer
/// * `fs_base` - Value for FS base register (for TLS), 0 if not needed
///
/// # Safety
/// All pointers must be valid. The kernel stack must be in the higher half.
/// This function never returns.
///
/// NOTE: This clears all registers, so it should only be used for fresh process
/// entry, not for returning from fork (use enter_usermode_with_context instead).
#[unsafe(naked)]
pub unsafe extern "C" fn enter_usermode(
    kernel_stack: u64, // rdi
    pml4_phys: u64,    // rsi
    entry: u64,        // rdx
    user_stack: u64,   // rcx
    fs_base: u64,      // r8
) -> ! {
    naked_asm!(
        // Disable interrupts during the transition
        "cli",

        // Save arguments before we modify registers
        // r8 already contains fs_base
        "mov r10, rdx",  // entry -> r10
        "mov r11, rcx",  // user_stack -> r11
        "mov r12, r8",   // fs_base -> r12

        // First, switch to the new kernel stack (which is in higher half)
        "mov rsp, rdi",

        // Now switch page tables - the new stack is still accessible
        // because it's in the higher half which is preserved
        "mov cr3, rsi",

        // Set up the stack frame for iretq:
        // [rsp+32] SS
        // [rsp+24] RSP
        // [rsp+16] RFLAGS
        // [rsp+8]  CS
        // [rsp+0]  RIP

        // Push SS (user data segment with RPL=3)
        "mov rax, {user_ds}",
        "push rax",

        // Push RSP (user stack pointer from r11)
        "push r11",

        // Push RFLAGS with IF set (interrupts enabled)
        // Use known safe value: IF=1, IOPL=0, NT=0, TF=0
        "mov rax, 0x202",
        "push rax",

        // Push CS (user code segment with RPL=3)
        "mov rax, {user_cs}",
        "push rax",

        // Push RIP (entry point from r10)
        "push r10",

        // Set FS base MSR if fs_base is non-zero (IA32_FS_BASE = 0xC0000100)
        "test r12, r12",
        "jz 2f",                       // Skip if fs_base is 0
        "mov rcx, 0xC0000100",         // MSR number for FS_BASE
        "mov rax, r12",                // Low 32 bits of fs_base
        "mov rdx, r12",
        "shr rdx, 32",                 // High 32 bits of fs_base
        "wrmsr",                       // Write MSR
        "2:",

        // Load user data segments
        "mov ax, {user_ds}",
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",

        // Clear all general purpose registers for security
        "xor rax, rax",
        "xor rbx, rbx",
        "xor rcx, rcx",
        "xor rdx, rdx",
        "xor rsi, rsi",
        "xor rdi, rdi",
        "xor rbp, rbp",
        "xor r8, r8",
        "xor r9, r9",
        "xor r10, r10",
        "xor r11, r11",
        "xor r12, r12",
        "xor r13, r13",
        "xor r14, r14",
        "xor r15, r15",

        // NOTE: Do NOT swapgs here!
        // KERNEL_GS_BASE contains the kernel stack pointer
        // GS.base is user's value (will be 0 in user mode)
        // When user does syscall, syscall_entry will swapgs to get kernel stack

        // Jump to user mode
        "iretq",

        user_cs = const USER_CS as u64,
        user_ds = const USER_DS as u64,
    );
}

/// User context structure for enter_usermode_with_context
#[repr(C)]
pub struct UserContext {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
}

/// Debug variables to capture what enter_usermode_with_context reads
#[unsafe(no_mangle)]
pub static mut DEBUG_RCX_READ: u64 = 0xDEADDEADDEADDEAD;
#[unsafe(no_mangle)]
pub static mut DEBUG_R15_VALUE: u64 = 0xDEADDEADDEADDEAD;
#[unsafe(no_mangle)]
pub static mut DEBUG_RIP_READ: u64 = 0xDEADDEADDEADDEAD;
#[unsafe(no_mangle)]
pub static mut DEBUG_RSP_READ: u64 = 0xDEADDEADDEADDEAD;
#[unsafe(no_mangle)]
pub static mut DEBUG_RCX_ACTUAL: u64 = 0xDEADDEADDEADDEAD;
#[unsafe(no_mangle)]
pub static mut DEBUG_RAX_ACTUAL: u64 = 0xDEADDEADDEADDEAD;
#[unsafe(no_mangle)]
pub static mut DEBUG_IRETQ_RIP: u64 = 0xDEADDEADDEADDEAD;
#[unsafe(no_mangle)]
pub static mut DEBUG_IRETQ_RSP: u64 = 0xDEADDEADDEADDEAD;
#[unsafe(no_mangle)]
pub static mut DEBUG_IRETQ_CS: u64 = 0xDEADDEADDEADDEAD;

/// Enter user mode with a full register context
///
/// This restores all registers to the values in the context, then jumps to
/// user mode. Used for returning to a forked child process.
///
/// # Arguments
/// * `kernel_stack` - Kernel stack top
/// * `pml4_phys` - Physical address of the user's PML4 table
/// * `ctx` - Pointer to UserContext with full register state
/// * `fs_base` - Value for FS base register (for TLS)
///
/// # Safety
/// The context pointer must be valid and all values must be safe for user mode.
#[unsafe(naked)]
pub unsafe extern "C" fn enter_usermode_with_context(
    kernel_stack: u64,       // rdi
    pml4_phys: u64,          // rsi
    ctx: *const UserContext, // rdx
    fs_base: u64,            // rcx
) -> ! {
    naked_asm!(
        // Disable interrupts during the transition
        "cli",

        // IMPORTANT: Copy context to new kernel stack BEFORE switching page tables
        // This avoids issues where the context memory might not be accessible
        // or map to different physical memory after the CR3 switch.
        //
        // Stack layout (growing down):
        //   kernel_stack_top
        //   [kernel_stack_top - 40]  = iretq frame (5 qwords = 40 bytes)
        //   [kernel_stack_top - 184] = context copy (18 qwords = 144 bytes)
        //
        // We need to copy context BELOW where iretq frame will be, then build
        // the iretq frame above it

        // Save arguments
        "mov r15, rdx",           // r15 = context pointer (in current address space)
        "mov r14, rsi",           // r14 = pml4_phys (save for later)
        "mov r13, rdi",           // r13 = kernel_stack_top
        "mov r12, rcx",           // r12 = fs_base (save for later)

        // Switch to the new kernel stack, leaving room for iretq frame (40 bytes)
        // and context copy (144 bytes)
        "mov rsp, r13",
        "sub rsp, 184",           // rsp = kernel_stack_top - 184 (room for iretq + context)

        // Copy context to rsp+0 through rsp+143 (18 qwords = 144 bytes)
        "mov rax, [r15 + 0]",
        "mov [rsp + 0], rax",
        "mov rax, [r15 + 8]",
        "mov [rsp + 8], rax",
        "mov rax, [r15 + 16]",
        "mov [rsp + 16], rax",
        "mov rax, [r15 + 24]",
        "mov [rsp + 24], rax",
        "mov rax, [r15 + 32]",
        "mov [rsp + 32], rax",
        "mov rax, [r15 + 40]",
        "mov [rsp + 40], rax",
        "mov rax, [r15 + 48]",
        "mov [rsp + 48], rax",
        "mov rax, [r15 + 56]",
        "mov [rsp + 56], rax",
        "mov rax, [r15 + 64]",
        "mov [rsp + 64], rax",
        "mov rax, [r15 + 72]",
        "mov [rsp + 72], rax",
        "mov rax, [r15 + 80]",
        "mov [rsp + 80], rax",
        "mov rax, [r15 + 88]",
        "mov [rsp + 88], rax",
        "mov rax, [r15 + 96]",
        "mov [rsp + 96], rax",
        "mov rax, [r15 + 104]",
        "mov [rsp + 104], rax",
        "mov rax, [r15 + 112]",
        "mov [rsp + 112], rax",
        "mov rax, [r15 + 120]",
        "mov [rsp + 120], rax",
        "mov rax, [r15 + 128]",
        "mov [rsp + 128], rax",
        "mov rax, [r15 + 136]",
        "mov [rsp + 136], rax",

        // r15 now points to the copied context on the new kernel stack
        "mov r15, rsp",

        // NOW switch page tables - context is safe on the kernel stack
        "mov cr3, r14",

        // NOTE: Do NOT load user segments yet! Loading GS/FS might affect
        // segment-based addressing. Load them right before iretq.

        // Build iretq stack frame at [kernel_stack_top - 40]
        // The frame goes at rsp+144 through rsp+183 (above the context copy)
        // iretq pops: RIP, CS, RFLAGS, RSP, SS (in that order from low to high addr)
        // So we need:
        //   [rsp+144] = RIP
        //   [rsp+152] = CS
        //   [rsp+160] = RFLAGS
        //   [rsp+168] = RSP
        //   [rsp+176] = SS

        // Store RIP
        "mov rax, [r15 + 128]",   // ctx.rip
        "mov [rsp + 144], rax",

        // Store CS
        "mov rax, {user_cs}",
        "mov [rsp + 152], rax",

        // Store RFLAGS - use safe value with IF=1 (interrupts enabled)
        "mov rax, 0x202",         // Reserved bit 1 + IF, IOPL=0, NT=0, TF=0
        "mov [rsp + 160], rax",

        // Store user RSP
        "mov rax, [r15 + 56]",    // ctx.rsp
        "mov [rsp + 168], rax",

        // Store SS
        "mov rax, {user_ds}",
        "mov [rsp + 176], rax",

        // DEBUG: Capture r15 value and what we read from offsets
        "mov [{debug_r15}], r15",
        "mov rax, [r15 + 16]",
        "mov [{debug_rcx}], rax",
        "mov rax, [r15 + 128]",
        "mov [{debug_rip}], rax",
        "mov rax, [r15 + 56]",
        "mov [{debug_rsp}], rax",

        // Now restore all general purpose registers from copied context
        "mov r14, [r15 + 112]",   // r14
        "mov r13, [r15 + 104]",   // r13
        "mov r12, [r15 + 96]",    // r12
        "mov r11, [r15 + 88]",    // r11
        "mov r10, [r15 + 80]",    // r10
        "mov r9, [r15 + 72]",     // r9
        "mov r8, [r15 + 64]",     // r8
        "mov rbp, [r15 + 48]",    // rbp
        "mov rdi, [r15 + 40]",    // rdi
        "mov rsi, [r15 + 32]",    // rsi
        "mov rdx, [r15 + 24]",    // rdx
        "mov rcx, [r15 + 16]",    // rcx
        "mov [{debug_rcx_actual}], rcx",  // DEBUG: capture actual RCX value after load
        "mov rbx, [r15 + 8]",     // rbx
        "mov rax, [r15 + 0]",     // rax
        "mov [{debug_rax_actual}], rax",  // DEBUG: capture actual RAX value after load
        "mov r15, [r15 + 120]",   // r15 (last)

        // Point RSP to the iretq frame and return to user mode
        // Current rsp = kernel_stack_top - 184 (unchanged)
        // iretq frame = current rsp + 144
        "add rsp, 144",

        // DEBUG: capture iretq frame values before iretq
        // At this point: [rsp+0]=RIP, [rsp+8]=CS, [rsp+16]=RFLAGS, [rsp+24]=RSP, [rsp+32]=SS
        // Use [rsp-8] to save/restore RAX
        "mov [rsp - 8], rax",              // Save user's RAX (which should be 0)
        "mov rax, [rsp]",
        "mov [{debug_iretq_rip}], rax",    // Save iretq RIP
        "mov rax, [rsp + 8]",
        "mov [{debug_iretq_cs}], rax",     // Save iretq CS
        "mov rax, [rsp + 24]",
        "mov [{debug_iretq_rsp}], rax",    // Save iretq RSP
        "mov rax, [rsp - 8]",              // Restore user's RAX

        // Set FS base MSR if fs_base is non-zero (IA32_FS_BASE = 0xC0000100)
        "test r12, r12",
        "jz 2f",                       // Skip if fs_base is 0
        "mov rcx, 0xC0000100",         // MSR number for FS_BASE
        "mov rax, r12",                // Low 32 bits of fs_base
        "mov rdx, r12",
        "shr rdx, 32",                 // High 32 bits of fs_base
        "wrmsr",                       // Write MSR
        "2:",

        // Load user data segments right before iretq
        // We need to preserve RAX. Store it at [rsp-16] (different spot to not conflict with debug)
        "mov [rsp - 16], rax",
        "mov ax, {user_ds}",
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",                  // FS selector (segment base is set by MSR above)
        "mov gs, ax",
        "mov rax, [rsp - 16]",

        // Now RSP points to the iretq frame
        // RAX contains the context's rax value (0 for fork child)
        "iretq",

        user_cs = const USER_CS as u64,
        user_ds = const USER_DS as u64,
        debug_r15 = sym DEBUG_R15_VALUE,
        debug_rcx = sym DEBUG_RCX_READ,
        debug_rip = sym DEBUG_RIP_READ,
        debug_rsp = sym DEBUG_RSP_READ,
        debug_rcx_actual = sym DEBUG_RCX_ACTUAL,
        debug_rax_actual = sym DEBUG_RAX_ACTUAL,
        debug_iretq_rip = sym DEBUG_IRETQ_RIP,
        debug_iretq_cs = sym DEBUG_IRETQ_CS,
        debug_iretq_rsp = sym DEBUG_IRETQ_RSP,
    );
}

/// Return to user mode from a syscall
///
/// This is called after a syscall to return to user mode.
/// The user RIP and RSP are already set up in RCX and R11
/// from the original syscall entry.
///
/// Note: This is typically done via sysretq in the syscall handler,
/// but this function can be used for returning after creating
/// a new user thread context.
#[unsafe(naked)]
pub unsafe extern "C" fn return_to_usermode(entry: u64, user_stack: u64, rflags: u64) -> ! {
    naked_asm!(
        // Arguments: rdi = entry (RIP), rsi = user_stack (RSP), rdx = rflags

        // Set up for sysretq:
        // RCX = user RIP
        // R11 = user RFLAGS

        "mov rcx, rdi",           // User RIP
        "mov r11, rdx",           // User RFLAGS
        "mov rsp, rsi",           // User RSP

        // Clear registers for security
        "xor rax, rax",
        "xor rbx, rbx",
        "xor rdx, rdx",
        "xor rsi, rsi",
        "xor rdi, rdi",
        "xor rbp, rbp",
        "xor r8, r8",
        "xor r9, r9",
        "xor r10, r10",
        "xor r12, r12",
        "xor r13, r13",
        "xor r14, r14",
        "xor r15, r15",

        // Load user data segments
        "mov ax, {user_ds}",
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",
        "xor rax, rax",

        // Swap to user GS
        "swapgs",

        // Return to user mode
        "sysretq",

        user_ds = const USER_DS,
    );
}
