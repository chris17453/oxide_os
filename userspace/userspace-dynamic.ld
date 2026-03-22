/* Linker script for dynamically-linked OXIDE userspace programs */
/* — PulseForge: identical to userspace.ld but adds INTERP for PT_INTERP */

ENTRY(_start)

PHDRS
{
    interp PT_INTERP;
    text PT_LOAD FLAGS(5);    /* R-X */
    rodata PT_LOAD FLAGS(4);  /* R-- */
    data PT_LOAD FLAGS(6);    /* RW- */
}

/* User programs start at 0x400000 */
SECTIONS
{
    . = 0x400000;

    .interp : { *(.interp) } :interp :text

    .text : {
        *(.text .text.*)
    } :text

    .rodata : {
        *(.rodata .rodata.*)
    } :rodata

    /* RW sections on their own page boundary — keeps W^X clean */
    . = ALIGN(0x1000);

    .got : {
        *(.got .got.*)
    } :data

    .data : {
        *(.data .data.*)
    } :data

    .bss : {
        *(COMMON)
        *(.bss .bss.*)
    } :data

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
        *(.note*)
    }
}
