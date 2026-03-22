/* Test: dynamically-linked C program that calls malloc from libc.so */
/* Uses crt0.o _start → init_env → malloc chain */

#include <stddef.h>

extern void *malloc(size_t size);
extern void free(void *ptr);
extern int strcmp(const char *s1, const char *s2);

static long oxide_write(int fd, const void *buf, unsigned long count) {
    long ret;
    __asm__ volatile ("syscall" : "=a"(ret) : "a"(1), "D"(fd), "S"(buf), "d"(count) : "rcx", "r11", "memory");
    return ret;
}

int main(int argc, char **argv) {
    /* Test 1: strcmp (pure function, no writes to globals) */
    volatile const char *s1 = "test";
    volatile const char *s2 = "test";
    if (strcmp((const char *)s1, (const char *)s2) != 0) {
        oxide_write(1, "FAIL: strcmp\n", 12);
        return 1;
    }
    oxide_write(1, "OK: strcmp\n", 10);

    /* Test 2: malloc (writes to libc globals — the real test) */
    void *p = malloc(64);
    if (p == NULL) {
        oxide_write(1, "FAIL: malloc returned NULL\n", 26);
        return 1;
    }
    free(p);
    oxide_write(1, "OK: malloc\n", 11);

    oxide_write(1, "MALLOC_DYNLINK_PASS\n", 19);
    return 0;
}
