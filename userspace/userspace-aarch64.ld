/* Linker script for OXIDE userspace programs - ARM64 (aarch64) */
/* — NeonRoot */

ENTRY(_start)

/* User programs start at 0x400000 (standard userspace base) */
/* ARM64 uses 4KB or 64KB pages, 0x400000 is 4MB-aligned */
SECTIONS
{
    . = 0x400000;

    .text : ALIGN(4096) {
        *(.text .text.*)
    }

    .rodata : ALIGN(4096) {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(4096) {
        *(.data .data.*)
    }

    .bss : ALIGN(4096) {
        *(COMMON)
        *(.bss .bss.*)
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
        *(.note*)
        *(.ARM.*)
    }
}
