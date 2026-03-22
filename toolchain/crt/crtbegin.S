/*
 * crtbegin.S — constructor/destructor section markers for OXIDE OS (x86_64)
 *
 * — PulseForge: marks the start of .ctors/.dtors sections. GCC's collect2
 * expects these to exist. crtbegin goes at the start, crtend at the end.
 * The __CTOR_LIST__ and __DTOR_LIST__ arrays are built by the linker from
 * all object files' .ctors/.dtors sections. Minimal for now — no
 * __cxa_finalize or .eh_frame deregistration. — PulseForge
 */

.section .ctors,"aw",@progbits
.align 8
__CTOR_LIST__:
    .quad -1

.section .dtors,"aw",@progbits
.align 8
__DTOR_LIST__:
    .quad -1
