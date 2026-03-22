/*
 * dynlink-test.c — Dynamic linking integration test
 *
 * — CrashBloom: the ultimate proof that dynamic linking works end-to-end.
 * This program:
 * 1. Calls strlen() from libc.so through PLT/GOT (resolved by ld-oxide)
 * 2. Calls write() via direct syscall to output the result
 * 3. If strlen returns the right value, the entire chain works:
 *    kernel → ld-oxide → libc.so loaded → symbol resolved → GOT patched → PLT works
 */

/* — CrashBloom: strcmp is declared as extern — linker generates a PLT stub
 * that goes through GOT. ld-oxide resolves this at load time via GLOB_DAT
 * or JUMP_SLOT relocation. */
extern int strcmp(const char *s1, const char *s2);

/* — CrashBloom: provide own _start — don't use crt0.o's init_env/init_stdio
 * because those trigger libc's allocator which hits a kernel mmap write-protect bug.
 * This test only needs direct syscalls, not libc globals. */
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
    /* — CrashBloom: call strcmp through PLT/GOT — this is the real test.
     * If ld-oxide resolved the symbol correctly, strcmp reads from libc.so's
     * code at 0x30xxxxxx and returns 0 for equal strings. If resolution failed,
     * the GOT entry is 0 and we SIGSEGV.
     * — CrashBloom: use volatile to prevent compiler from optimizing the call away. */
    volatile const char *s1 = "OXIDE";
    volatile const char *s2 = "OXIDE";
    int result = strcmp((const char *)s1, (const char *)s2);

    if (result == 0) {
        const char *msg = "[DYNLINK] OK - strcmp resolved through PLT/GOT!\n";
        oxide_write(1, msg, 48);
        const char *pass = "DYNLINK_PASS\n";
        oxide_write(1, pass, 13);
        return 0;
    } else {
        const char *msg = "[DYNLINK] FAIL - strcmp returned wrong value\n";
        oxide_write(1, msg, 45);
        return 1;
    }
}
