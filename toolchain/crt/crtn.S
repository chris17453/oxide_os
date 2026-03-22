/*
 * crtn.S — .init/.fini section epilogues for OXIDE OS (x86_64)
 *
 * — PulseForge: the closing act. Paired with crti.S to complete the
 * _init and _fini function frames. The linker stitches them together:
 *   crti(.init prologue) + constructor calls + crtn(.init epilogue)
 *   crti(.fini prologue) + destructor calls + crtn(.fini epilogue)
 * — PulseForge
 */

.section .init
    popq %rbp
    ret

.section .fini
    popq %rbp
    ret
