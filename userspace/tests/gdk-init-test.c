/*
 * gdk-init-test.c — Test GDK initialization step by step
 */
#include <gdk/gdk.h>

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

int main(int argc, char *argv[]) {
    print("=== GDK Init Test ===\n");

    print("1. Calling gdk_init_check...\n");
    gboolean ok = gdk_init_check(&argc, &argv);
    if (ok) {
        print("   gdk_init_check: OK\n");
    } else {
        print("   gdk_init_check: FAILED (no display)\n");
        print("   This is expected without a running Wayland compositor.\n");
        return 1;
    }

    print("2. Getting default display...\n");
    GdkDisplay *display = gdk_display_get_default();
    if (display) {
        print("   display: non-NULL\n");
        const char *name = gdk_display_get_name(display);
        print("   name: ");
        print(name ? name : "(null)");
        print("\n");
    } else {
        print("   display: NULL\n");
    }

    print("=== GDK test done ===\n");
    return 0;
}
