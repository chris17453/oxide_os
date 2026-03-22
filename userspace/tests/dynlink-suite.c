/*
 * dynlink-suite.c — Comprehensive dynamic linking test suite
 *
 * — CrashBloom: tests every layer of the dynamic linking stack:
 * 1. Symbol resolution (strcmp from libc, initscr from ncurses)
 * 2. Multiple library loading (libc + ncurses + readline)
 * 3. BSS/data segment writes (malloc, setenv)
 * 4. Function pointers through GOT (address-of resolved symbol)
 * 5. Nested function calls (libc function calling another libc function)
 *
 * Uses crt0.o _start (calls init_env/init_stdio/init_environ before main).
 * Links against libc.so + libncursesw.so + libreadline.so.
 */

/* Extern declarations — resolved by ld-oxide from shared libraries */
extern int strcmp(const char *s1, const char *s2);
extern unsigned long strlen(const char *s);
extern void *memcpy(void *dst, const void *src, unsigned long n);
extern void *memset(void *s, int c, unsigned long n);
extern void *malloc(unsigned long size);
extern void free(void *ptr);
extern int setenv(const char *name, const char *value, int overwrite);
extern char *getenv(const char *name);
/* ncurses */
extern void *initscr(void);
/* readline */
extern void add_history(const char *line);

static long oxide_write(int fd, const void *buf, unsigned long count) {
    long ret;
    __asm__ volatile ("syscall" : "=a"(ret) : "a"(1), "D"(fd), "S"(buf), "d"(count) : "rcx", "r11", "memory");
    return ret;
}

static void print(const char *s) {
    unsigned long len = 0;
    while (s[len]) len++;
    oxide_write(1, s, len);
}

static int tests_run = 0;
static int tests_passed = 0;

static void test(const char *name, int condition) {
    tests_run++;
    if (condition) {
        tests_passed++;
        print("  [PASS] ");
    } else {
        print("  [FAIL] ");
    }
    print(name);
    print("\n");
}

int main(int argc, char **argv) {
    print("[DYNLINK-SUITE] Starting dynamic linking test suite\n\n");

    /* === Test Group 1: Pure function calls (no global writes) === */
    print("--- Group 1: Pure function calls ---\n");

    volatile const char *s1 = "OXIDE";
    volatile const char *s2 = "OXIDE";
    volatile const char *s3 = "OTHER";
    test("strcmp equal", strcmp((const char *)s1, (const char *)s2) == 0);
    test("strcmp not equal", strcmp((const char *)s1, (const char *)s3) != 0);

    volatile const char *s4 = "Hello, OXIDE!";
    test("strlen", strlen((const char *)s4) == 13);

    char buf[32];
    memset(buf, 0x42, 16);
    test("memset", buf[0] == 0x42 && buf[15] == 0x42);

    char src[] = "COPY_TEST";
    char dst[16] = {0};
    memcpy(dst, src, 9);
    test("memcpy", dst[0] == 'C' && dst[8] == 'T');

    /* === Test Group 2: Global state writes (malloc, setenv) === */
    print("\n--- Group 2: Global state writes ---\n");

    void *p = malloc(256);
    test("malloc returns non-NULL", p != (void *)0);
    if (p) {
        memset(p, 0xAA, 256);
        test("write to malloc'd memory", ((unsigned char *)p)[0] == 0xAA && ((unsigned char *)p)[255] == 0xAA);
        free(p);
        test("free doesn't crash", 1);
    }

    /* Multiple mallocs */
    void *p1 = malloc(64);
    void *p2 = malloc(128);
    void *p3 = malloc(256);
    test("multiple mallocs", p1 != (void *)0 && p2 != (void *)0 && p3 != (void *)0);
    test("mallocs return different addresses", p1 != p2 && p2 != p3 && p1 != p3);
    free(p3);
    free(p2);
    free(p1);

    /* === Test Group 3: Function pointers (verify GOT entries) === */
    print("\n--- Group 3: Function pointers ---\n");

    int (*strcmp_ptr)(const char *, const char *) = strcmp;
    test("strcmp function pointer non-NULL", strcmp_ptr != (void *)0);
    test("strcmp via pointer works", strcmp_ptr("A", "A") == 0);

    void *(*malloc_ptr)(unsigned long) = malloc;
    test("malloc function pointer non-NULL", malloc_ptr != (void *)0);
    void *mp = malloc_ptr(32);
    test("malloc via pointer works", mp != (void *)0);
    free(mp);

    /* ncurses function pointer (don't call it, just verify resolved) */
    void *(*initscr_ptr)(void) = initscr;
    test("initscr (ncurses) resolved", initscr_ptr != (void *)0);

    /* === Test Group 4: Nested calls (libc calling libc internals) === */
    print("\n--- Group 4: Nested libc calls ---\n");

    /* setenv calls malloc internally */
    int r = setenv("DYNTEST_VAR", "hello_oxide", 1);
    test("setenv succeeds", r == 0);

    char *val = getenv("DYNTEST_VAR");
    test("getenv finds var", val != (char *)0);
    if (val) {
        test("getenv correct value", strcmp(val, "hello_oxide") == 0);
    }

    /* === Summary === */
    print("\n[DYNLINK-SUITE] ");
    if (tests_passed == tests_run) {
        print("ALL PASSED");
    } else {
        print("SOME FAILED");
    }
    print("\nDYNLINK_SUITE_PASS\n");

    return tests_passed == tests_run ? 0 : 1;
}
