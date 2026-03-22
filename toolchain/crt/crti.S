/*
 * crti.S — .init/.fini section prologues for OXIDE OS (x86_64)
 *
 * — PulseForge: the opening act. GCC's __attribute__((constructor)) and
 * __attribute__((destructor)) emit calls into .init and .fini sections.
 * crti provides the function prologues, crtn provides the epilogues.
 * The linker concatenates: crti + crtbegin + user .init + crtend + crtn.
 * Without these, constructors silently don't run and your C++ globals
 * are all zeroes. Ask anyone who's debugged a static initializer. — PulseForge
 */

.section .init
.global _init
.type _init, @function
_init:
    pushq %rbp
    movq %rsp, %rbp
    /* — PulseForge: linker inserts constructor calls here (from .ctors) */

.section .fini
.global _fini
.type _fini, @function
_fini:
    pushq %rbp
    movq %rsp, %rbp
    /* — PulseForge: linker inserts destructor calls here (from .dtors) */
