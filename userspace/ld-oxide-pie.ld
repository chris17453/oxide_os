/* Linker script for ld-oxide.so.1 as static PIE
 * — PulseForge: vaddr 0 so it can load at any address.
 * The kernel provides AT_BASE with the actual load address.
 * _start self-relocates before calling any Rust code.
 */

ENTRY(_start)

PHDRS
{
    text PT_LOAD FLAGS(5);     /* R-X */
    rodata PT_LOAD FLAGS(4);   /* R-- */
    data PT_LOAD FLAGS(6);     /* RW- */
    dynamic PT_DYNAMIC FLAGS(6); /* RW- */
}

SECTIONS
{
    . = SIZEOF_HEADERS;

    .text : {
        *(.text .text.*)
    } :text

    .rodata : {
        *(.rodata .rodata.*)
    } :rodata

    . = ALIGN(0x1000);

    .dynamic : {
        *(.dynamic)
    } :data :dynamic

    .got : {
        *(.got .got.*)
    } :data

    .rela.dyn : {
        *(.rela.dyn .rela.*)
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
