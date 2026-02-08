// Test program to trigger syscall 999 (screen dump)
#include <stdio.h>

int main() {
    printf("Calling syscall 999 to dump screen...\n");
    fflush(stdout);

    // Call syscall 999
    long result;
    asm volatile (
        "mov $999, %%rax\n"
        "syscall\n"
        : "=a"(result)
        :
        : "rcx", "r11", "memory"
    );

    printf("Syscall 999 returned: %ld\n", result);
    return 0;
}
