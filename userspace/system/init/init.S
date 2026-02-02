# Phase 4 test: fork and wait
# Tests the fork-wait cycle

.section .text
.global _start

# Syscall numbers
.equ SYS_EXIT, 0
.equ SYS_WRITE, 1
.equ SYS_FORK, 3
.equ SYS_WAIT, 5

_start:
    # Print startup message
    mov $SYS_WRITE, %rax
    mov $1, %rdi            # stdout
    lea msg_start(%rip), %rsi
    mov $25, %rdx           # length
    syscall

    # Call fork
    mov $SYS_FORK, %rax
    syscall

    # Check return value
    test %rax, %rax
    jz child_process        # rax == 0: we're the child
    js fork_failed          # rax < 0: fork failed
    # rax > 0: we're the parent, rax = child PID

parent_process:
    # Save child PID
    push %rax

    # Print parent message
    mov $SYS_WRITE, %rax
    mov $1, %rdi
    lea msg_parent(%rip), %rsi
    mov $27, %rdx
    syscall

    # Wait for child (pid=-1, status_ptr=0, options=0)
    mov $SYS_WAIT, %rax
    mov $0, %rdi            # status_ptr (NULL)
    syscall

    # Print wait done message
    push %rax               # save wait result
    mov $SYS_WRITE, %rax
    mov $1, %rdi
    lea msg_reaped(%rip), %rsi
    mov $30, %rdx
    syscall

    # Print success message
    mov $SYS_WRITE, %rax
    mov $1, %rdi
    lea msg_success(%rip), %rsi
    mov $35, %rdx
    syscall

    # Exit with status 0
    mov $SYS_EXIT, %rax
    mov $0, %rdi
    syscall

child_process:
    # Print child message
    mov $SYS_WRITE, %rax
    mov $1, %rdi
    lea msg_child(%rip), %rsi
    mov $26, %rdx
    syscall

    # Exit with status 42 (so parent can verify)
    mov $SYS_EXIT, %rax
    mov $42, %rdi
    syscall

fork_failed:
    # Print error message
    mov $SYS_WRITE, %rax
    mov $1, %rdi
    lea msg_error(%rip), %rsi
    mov $13, %rdx
    syscall

    # Exit with error
    mov $SYS_EXIT, %rax
    mov $1, %rdi
    syscall

    # Should never reach here
1:  jmp 1b

.section .rodata
msg_start:
    .ascii "Phase 4: Fork-Wait Test\n\0"
msg_parent:
    .ascii "Parent: forked, waiting...\n"
msg_child:
    .ascii "Child: running, exiting\n\0\0"
msg_reaped:
    .ascii "Parent: child reaped, done!\n\0\0"
msg_success:
    .ascii "\nSUCCESS: Fork-wait cycle works!\n\0\0"
msg_error:
    .ascii "Fork failed!\n"
