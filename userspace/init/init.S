# Minimal init process for EFFLUX
# Writes "Hello from userspace!" and exits

.section .text
.global _start

_start:
    # sys_write(1, msg, len)
    mov $1, %rax        # syscall number: write
    mov $1, %rdi        # fd: stdout
    lea msg(%rip), %rsi # buf: message address
    mov $22, %rdx       # count: message length
    syscall

    # sys_exit(0)
    mov $0, %rax        # syscall number: exit
    mov $0, %rdi        # status: 0
    syscall

    # Should never reach here
1:  jmp 1b

.section .rodata
msg:
    .ascii "Hello from userspace!\n"
