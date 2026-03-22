/*
 * wayland-connect-test.c — Test AF_UNIX connect to /run/wayland-0
 */

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

static void print_num(long n) {
    char buf[20]; int i = 0;
    if (n < 0) { print("-"); n = -n; }
    if (n == 0) { print("0"); return; }
    while (n > 0) { buf[i++] = '0' + (n % 10); n /= 10; }
    while (i > 0) { char c[2] = {buf[--i], 0}; print(c); }
}

extern int socket(int domain, int type, int protocol);
extern int connect(int sockfd, const void *addr, unsigned int addrlen);
extern int close(int fd);
extern char *getenv(const char *name);

#define AF_UNIX 1
#define SOCK_STREAM 1

int main(void) {
    print("=== Wayland Connect Test ===\n");

    /* Check env vars */
    char *display = getenv("WAYLAND_DISPLAY");
    print("WAYLAND_DISPLAY=");
    print(display ? display : "(null)");
    print("\n");

    char *xdg = getenv("XDG_RUNTIME_DIR");
    print("XDG_RUNTIME_DIR=");
    print(xdg ? xdg : "(null)");
    print("\n");

    /* Create AF_UNIX socket */
    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    print("socket() = ");
    print_num(fd);
    print("\n");

    if (fd < 0) {
        print("FAIL: socket() failed\n");
        return 1;
    }

    /* Build sockaddr_un for /run/wayland-0 */
    char addr[110];
    for (int i = 0; i < 110; i++) addr[i] = 0;
    addr[0] = AF_UNIX; /* sun_family low byte */
    addr[1] = 0;       /* sun_family high byte */
    /* sun_path starts at offset 2 */
    const char *path = "/run/wayland-0";
    int pi = 0;
    while (path[pi]) { addr[2 + pi] = path[pi]; pi++; }

    int ret = connect(fd, addr, 2 + pi + 1);
    print("connect() = ");
    print_num(ret);
    print("\n");

    if (ret < 0) {
        print("FAIL: connect() failed\n");
    } else {
        print("SUCCESS: connected to wayland compositor!\n");
    }

    close(fd);
    return ret < 0 ? 1 : 0;
}
