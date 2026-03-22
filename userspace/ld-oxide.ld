/* Linker script for ld-oxide.so.1 (OXIDE dynamic linker)
 * — PulseForge: static PIE — starts at vaddr 0, loadable at any address.
 * The kernel provides AT_BASE with the actual load address.
 * _start self-relocates (applies own RELATIVE entries) before calling Rust code.
 * If loaded at 0x200000, all RELATIVE relocs add 0x200000 to their addends.
 */

ENTRY(_start)

PHDRS
{
    text PT_LOAD FLAGS(5);       /* R-X */
    rodata PT_LOAD FLAGS(4);     /* R-- */
    data PT_LOAD FLAGS(6);       /* RW- */
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
        _DYNAMIC = .;
        *(.dynamic)
    } :data :dynamic

    .rela.dyn : {
        *(.rela.dyn .rela.*)
    } :data

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
