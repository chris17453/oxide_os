# Application Processor Boot Trampoline
#
# This code runs in real mode when an AP starts via SIPI.
# It transitions through protected mode to long mode and jumps to Rust code.
#
# The trampoline is copied to physical address 0x8000 (page 0x08).

.section .ap_trampoline, "awx"
.code16
.global ap_trampoline_start
.global ap_trampoline_end

# Entry point - CPU starts here in real mode after SIPI
ap_trampoline_start:
    cli                     # Disable interrupts
    cld                     # Clear direction flag

    # Load our GDT
    lgdt (ap_gdt_ptr - ap_trampoline_start + 0x8000)

    # Enable protected mode
    mov %cr0, %eax
    or $1, %eax
    mov %eax, %cr0

    # Jump to 32-bit protected mode code
    ljmp $0x08, $(ap_protected_mode - ap_trampoline_start + 0x8000)

.code32
ap_protected_mode:
    # Set up 32-bit data segments
    mov $0x10, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %fs
    mov %ax, %gs
    mov %ax, %ss

    # Load CR3 with kernel page tables (filled in by kernel)
    mov (ap_cr3 - ap_trampoline_start + 0x8000), %eax
    mov %eax, %cr3

    # Enable PAE (Physical Address Extension) - CR4.PAE = 1
    mov %cr4, %eax
    or $0x20, %eax
    mov %eax, %cr4

    # Enable long mode - set IA32_EFER.LME = 1
    mov $0xC0000080, %ecx
    rdmsr
    or $0x100, %eax
    wrmsr

    # Enable paging - CR0.PG = 1
    mov %cr0, %eax
    or $0x80000000, %eax
    mov %eax, %cr0

    # Jump to 64-bit long mode code
    ljmp $0x18, $(ap_long_mode - ap_trampoline_start + 0x8000)

.code64
ap_long_mode:
    # Set up 64-bit data segments
    mov $0x20, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %fs
    mov %ax, %gs
    mov %ax, %ss

    # Load stack pointer (filled in by kernel)
    mov (ap_stack - ap_trampoline_start + 0x8000), %rsp

    # Jump to Rust AP entry point (filled in by kernel)
    mov (ap_entry - ap_trampoline_start + 0x8000), %rax
    jmp *%rax

# Trampoline GDT (32-bit for transition)
.align 8
ap_gdt:
    .quad 0x0000000000000000
    .quad 0x00CF9A000000FFFF
    .quad 0x00CF92000000FFFF
    .quad 0x00AF9A000000FFFF
    .quad 0x00AF92000000FFFF

ap_gdt_ptr:
    .word (ap_gdt_ptr - ap_gdt - 1)
    .long (ap_gdt - ap_trampoline_start + 0x8000)

# Values filled in by kernel before AP boot
.align 8
ap_cr3:
    .quad 0
ap_stack:
    .quad 0
ap_entry:
    .quad 0

ap_trampoline_end:
