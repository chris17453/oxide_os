//! User mode transition for x86_64
//!
//! Provides the mechanism to transition from Ring 0 to Ring 3.

use core::arch::naked_asm;
use crate::gdt::{USER_CS, USER_DS};

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
        "pushfq",
        "pop rax",
        "or rax, 0x200",          // Set IF (interrupt flag)
        "and rax, 0xFFFFFFFFFFFFF6FF",  // Clear IOPL, NT, TF
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
///
/// # Safety
/// All pointers must be valid. The kernel stack must be in the higher half.
/// This function never returns.
#[unsafe(naked)]
pub unsafe extern "C" fn enter_usermode(
    kernel_stack: u64,  // rdi
    pml4_phys: u64,     // rsi
    entry: u64,         // rdx
    user_stack: u64,    // rcx
) -> ! {
    naked_asm!(
        // Disable interrupts during the transition
        "cli",

        // Save rdx and rcx before we use rsp
        "mov r8, rdx",  // entry -> r8
        "mov r9, rcx",  // user_stack -> r9

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

        // Push RSP (user stack pointer from r9)
        "push r9",

        // Push RFLAGS with IF set (interrupts enabled)
        "pushfq",
        "pop rax",
        "or rax, 0x200",          // Set IF (interrupt flag)
        "and rax, 0xFFFFFFFFFFFFF6FF",  // Clear IOPL, NT, TF
        "push rax",

        // Push CS (user code segment with RPL=3)
        "mov rax, {user_cs}",
        "push rax",

        // Push RIP (entry point from r8)
        "push r8",

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
