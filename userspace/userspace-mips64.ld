/* Linker script for OXIDE userspace programs - MIPS64 (SGI) */
/* ⚠️  BIG-ENDIAN: Data is stored in big-endian format */
/* — GraveShift */

OUTPUT_FORMAT("elf64-bigmips")
OUTPUT_ARCH(mips)

ENTRY(_start)

/* User programs start at 0x400000 (standard userspace base) */
/* MIPS64 uses 4KB, 8KB, or 16KB pages depending on platform */
/* SGI systems typically use 4KB or 16KB pages */
SECTIONS
{
    . = 0x400000;

    .text : ALIGN(16384) {
        *(.text .text.*)
    }

    /* MIPS requires GP-relative data section for global pointer */
    .sdata : ALIGN(16384) {
        _gp = . + 0x8000;  /* Global pointer at middle of section */
        *(.sdata .sdata.*)
    }

    .rodata : ALIGN(16384) {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(16384) {
        *(.data .data.*)
    }

    .sbss : ALIGN(16384) {
        *(.sbss .sbss.*)
    }

    .bss : ALIGN(16384) {
        *(COMMON)
        *(.bss .bss.*)
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
        *(.note*)
        *(.MIPS.*)
        *(.pdr)
        *(.mdebug*)
    }
}
