/* Common linker script for OXIDE userspace programs */
/* -- PulseForge: Page-align sections with different permissions so the  */
/*    ELF loader can map them with correct R/W/X flags per page.        */

ENTRY(_start)

/* User programs start at 0x400000 */
SECTIONS
{
    . = 0x400000;

    .text : {
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    /* RW sections on their own page boundary — keeps W^X clean */
    . = ALIGN(0x1000);

    .got : {
        *(.got .got.*)
    }

    .data : {
        *(.data .data.*)
    }

    .bss : {
        *(COMMON)
        *(.bss .bss.*)
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
        *(.note*)
    }
}
