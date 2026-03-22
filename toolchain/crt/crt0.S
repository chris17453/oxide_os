/*
 * crt0.S — C runtime startup for OXIDE OS (x86_64)
 *
 * — GraveShift: this is the first thing that runs in every dynamically-linked
 * C program. Calls libc initialization (env, stdio, environ) before main().
 * These init functions are resolved from libc.so by ld-oxide at load time.
 */

.text
.global _start
.type _start, @function
_start:
    xorl %ebp, %ebp          /* Clear frame pointer (ABI requirement) */

    /* Save argc/argv in callee-saved regs across init calls */
    movl (%rsp), %r12d        /* argc */
    leaq 8(%rsp), %r13        /* argv */

    andq $-16, %rsp           /* 16-byte align stack */

    /* — GraveShift: call libc init functions (resolved from libc.so).
     * These set up environment variables, stdio streams, and the
     * environ pointer. Without them, printf/malloc/getenv all crash. */
    call init_env
    call init_stdio
    call init_environ

    /* Call main(argc, argv) */
    movl %r12d, %edi          /* argc */
    movq %r13, %rsi           /* argv */
    call main

    /* Exit with return code */
    movl %eax, %edi
    movl $60, %eax            /* SYS_EXIT */
    syscall
    ud2                       /* unreachable */
.size _start, .-_start
