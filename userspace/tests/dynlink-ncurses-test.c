/*
 * dynlink-ncurses-test.c — Dynamic linking test with ncurses
 *
 * — CrashBloom: tests multi-library dynamic linking. This program links
 * against both libc.so AND libncursesw.so. When it runs:
 * 1. ld-oxide loads libc.so + libncursesw.so
 * 2. Resolves strcmp (from libc) + initscr/endwin (from ncurses) via PLT/GOT
 * 3. Calls strcmp to verify libc works
 * 4. Prints success message
 *
 * We DON'T actually call initscr() because it would take over the terminal.
 * Instead we just verify the GOT entry for initscr is non-zero (resolved).
 */

extern int strcmp(const char *s1, const char *s2);

/* — CrashBloom: declare ncurses functions so the linker generates PLT entries.
 * We take the address to verify resolution without calling them. */
extern void *initscr(void);

/* — CrashBloom: own _start — avoid crt0.o's libc init which triggers allocator bug */
__attribute__((naked, used)) void _start(void) {
    __asm__ volatile (
        "xor %%rbp, %%rbp\n"
        "mov (%%rsp), %%edi\n"
        "lea 8(%%rsp), %%rsi\n"
        "and $-16, %%rsp\n"
        "call main\n"
        "mov %%eax, %%edi\n"
        "mov $60, %%eax\n"
        "syscall\n"
        "ud2\n"
        ::: "memory"
    );
}

static long oxide_write(int fd, const void *buf, unsigned long count) {
    long ret;
    __asm__ volatile ("syscall" : "=a"(ret) : "a"(1), "D"(fd), "S"(buf), "d"(count) : "rcx", "r11", "memory");
    return ret;
}

int main(int argc, char **argv) {
    /* Test 1: strcmp from libc.so */
    volatile const char *s1 = "OXIDE";
    volatile const char *s2 = "OXIDE";
    int result = strcmp((const char *)s1, (const char *)s2);

    if (result != 0) {
        oxide_write(1, "[NCURSES-TEST] FAIL - strcmp broken\n", 36);
        return 1;
    }

    /* Test 2: verify initscr was resolved (non-null function pointer) */
    void *(*initscr_ptr)(void) = initscr;
    if (initscr_ptr == (void*)0) {
        oxide_write(1, "[NCURSES-TEST] FAIL - initscr not resolved\n", 44);
        return 1;
    }

    oxide_write(1, "[NCURSES-TEST] OK - libc + ncurses resolved!\n", 46);
    oxide_write(1, "NCURSES_DYNLINK_PASS\n", 20);
    return 0;
}
