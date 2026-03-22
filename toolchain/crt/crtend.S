/*
 * crtend.S — constructor/destructor section end markers for OXIDE OS (x86_64)
 *
 * — PulseForge: marks the end of .ctors/.dtors sections. The sentinel
 * value 0 tells the runtime "stop iterating here". Without this,
 * the constructor loop walks off the end of the array and calls random
 * addresses until it segfaults. Classic. — PulseForge
 */

.section .ctors,"aw",@progbits
.align 8
__CTOR_END__:
    .quad 0

.section .dtors,"aw",@progbits
.align 8
__DTOR_END__:
    .quad 0
