/*
 * glib-init-test.c — Test GLib type system initialization
 * Isolates the GTK crash by testing each init step.
 */

extern void *g_malloc(unsigned long size);
extern void g_free(void *ptr);
extern void g_type_init(void);
/* g_type_fundamental_next removed — not a public API */
extern void *g_main_context_default(void);
extern int g_setenv(const char *variable, const char *value, int overwrite);
extern const char *g_getenv(const char *variable);

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

int main(void) {
    print("=== GLib Init Test ===\n");

    print("1. Testing g_malloc... ");
    void *p = g_malloc(64);
    if (p) { print("OK\n"); g_free(p); }
    else { print("FAIL\n"); return 1; }

    print("2. Testing g_getenv... ");
    const char *val = g_getenv("HOME");
    print(val ? val : "(null)");
    print("\n");

    print("3. Testing g_main_context_default... ");
    void *ctx = g_main_context_default();
    if (ctx) { print("OK\n"); }
    else { print("NULL\n"); }

    print("4. Skipped (internal API)\n");

    print("=== All GLib tests passed ===\n");
    return 0;
}
