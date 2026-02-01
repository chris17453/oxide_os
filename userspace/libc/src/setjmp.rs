//! setjmp/longjmp implementation for x86_64
//!
//! jmp_buf layout (8 * i64):
//!   [0] = rbx
//!   [1] = rbp
//!   [2] = r12
//!   [3] = r13
//!   [4] = r14
//!   [5] = r15
//!   [6] = rsp (after setjmp returns)
//!   [7] = rip (return address)

use core::arch::global_asm;

global_asm!(
    ".global setjmp",
    ".global _setjmp",
    ".global longjmp",
    ".global _longjmp",
    ".global sigsetjmp",
    ".global siglongjmp",
    "",
    "setjmp:",
    "_setjmp:",
    "sigsetjmp:",
    "    mov [rdi],    rbx",
    "    mov [rdi+8],  rbp",
    "    mov [rdi+16], r12",
    "    mov [rdi+24], r13",
    "    mov [rdi+32], r14",
    "    mov [rdi+40], r15",
    "    lea rax, [rsp+8]", // rsp after return
    "    mov [rdi+48], rax",
    "    mov rax, [rsp]", // return address
    "    mov [rdi+56], rax",
    "    xor eax, eax", // return 0
    "    ret",
    "",
    "longjmp:",
    "_longjmp:",
    "siglongjmp:",
    "    mov rbx, [rdi]",
    "    mov rbp, [rdi+8]",
    "    mov r12, [rdi+16]",
    "    mov r13, [rdi+24]",
    "    mov r14, [rdi+32]",
    "    mov r15, [rdi+40]",
    "    mov rsp, [rdi+48]",
    "    mov eax, esi", // return value (zero-extends to rax)
    "    test eax, eax",
    "    jnz 1f",
    "    inc eax", // if val==0, return 1
    "1:",
    "    jmp [rdi+56]", // jump to saved rip
);
