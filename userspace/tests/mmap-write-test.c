/*
 * mmap-write-test.c — Unit tests for mmap + write behavior
 *
 * — CrashBloom: isolates the exact scenario that crashes vim:
 * 1. mmap a region with MAP_FIXED|MAP_ANONYMOUS|MAP_PRIVATE|PROT_WRITE
 * 2. Write to various pages in the region
 * 3. Later, write to a different page in the same region
 * 4. Verify all pages remain writable
 *
 * Uses own _start (no crt0.o) and direct syscalls (no libc dependency).
 * This test ONLY exercises the kernel's mmap and page fault path.
 */

static long sys_write(int fd, const void *buf, unsigned long count) {
    long ret;
    __asm__ volatile ("syscall" : "=a"(ret) : "a"(1), "D"(fd), "S"(buf), "d"(count) : "rcx", "r11", "memory");
    return ret;
}

static void sys_exit(int code) {
    __asm__ volatile ("syscall" :: "a"(60), "D"(code));
    __builtin_unreachable();
}

/* OXIDE sys_mmap: (addr, length, prot, flags, fd, offset) */
static long sys_mmap(unsigned long addr, unsigned long length, int prot, int flags, int fd, long offset) {
    long ret;
    register long r10 __asm__("r10") = flags;
    register long r8  __asm__("r8")  = fd;
    register long r9  __asm__("r9")  = offset;
    __asm__ volatile ("syscall" : "=a"(ret) : "a"(9), "D"(addr), "S"(length), "d"(prot), "r"(r10), "r"(r8), "r"(r9) : "rcx", "r11", "memory");
    return ret;
}

static void print(const char *s) {
    unsigned long len = 0;
    while (s[len]) len++;
    sys_write(1, s, len);
}

static void print_hex(unsigned long v) {
    char buf[19];
    buf[0] = '0'; buf[1] = 'x';
    for (int i = 15; i >= 0; i--) {
        int nibble = (v >> (i * 4)) & 0xF;
        buf[17 - i] = nibble < 10 ? '0' + nibble : 'a' + nibble - 10;
    }
    buf[18] = 0;
    /* Skip leading zeros */
    int start = 2;
    while (start < 17 && buf[start] == '0') start++;
    sys_write(1, buf + start - 2, 18 - start + 2);
}

#define PROT_READ  1
#define PROT_WRITE 2
#define PROT_EXEC  4
#define MAP_PRIVATE   0x02
#define MAP_ANONYMOUS 0x20
#define MAP_FIXED     0x10

__attribute__((naked)) void _start(void) {
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

int main(int argc, char **argv) {
    print("[MMAP-TEST] Starting mmap write tests\n");

    /* Test 1: Simple mmap + write */
    print("[TEST1] mmap 4KB at 0x50000000, write, read back... ");
    long addr1 = sys_mmap(0x50000000, 4096, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_FIXED, -1, 0);
    if (addr1 < 0) { print("FAIL: mmap returned "); print_hex(addr1); print("\n"); return 1; }
    *(volatile char *)0x50000000 = 0x42;
    if (*(volatile char *)0x50000000 != 0x42) { print("FAIL: read back wrong\n"); return 1; }
    print("OK\n");

    /* Test 2: Large mmap (3MB), write first page, then page 735 (the crash page) */
    print("[TEST2] mmap 3MB at 0x30000000, write page 0 and page 735... ");
    long addr2 = sys_mmap(0x30000000, 0x328000, PROT_READ|PROT_WRITE|PROT_EXEC,
                          MAP_PRIVATE|MAP_ANONYMOUS|MAP_FIXED, -1, 0);
    if (addr2 < 0) { print("FAIL: mmap returned "); print_hex(addr2); print("\n"); return 1; }
    /* Write page 0 */
    *(volatile unsigned char *)0x30000000UL = 0x42;
    if (*(volatile unsigned char *)0x30000000UL != 0x42) { print("FAIL: page 0 read back\n"); return 1; }
    print("page0 OK, ");
    /* Write page 735 (the exact crash page: 0x302df000) */
    *(volatile unsigned char *)0x302df000UL = 0x43;
    if (*(volatile unsigned char *)0x302df000UL != 0x43) { print("FAIL: page 735 read back\n"); return 1; }
    print("page735 OK, ");
    /* Write the exact crash address */
    *(volatile unsigned char *)0x302df7a1UL = 0x44;
    if (*(volatile unsigned char *)0x302df7a1UL != 0x44) { print("FAIL: 0x302df7a1 read back\n"); return 1; }
    print("OK\n");

    /* Test 3: Write pages sequentially, checking at what page count it breaks */
    print("[TEST3] Touch pages sequentially, checking each... ");
    int fail_page = -1;
    for (int i = 0; i < 808; i++) {
        unsigned long pg_addr = 0x30000000UL + (unsigned long)i * 4096UL;
        *(volatile unsigned char *)pg_addr = (unsigned char)(i & 0xFF);
        if (*(volatile unsigned char *)pg_addr != (unsigned char)(i & 0xFF)) {
            fail_page = i;
            break;
        }
    }
    if (fail_page >= 0) {
        print("FAIL at page ");
        print_hex(fail_page);
        print(" addr ");
        print_hex(0x30000000UL + (unsigned long)fail_page * 4096UL);
        print("\n");
        return 1;
    }
    print("OK (all 808 pages)\n");

    /* Test 4: Second mmap at different address, then verify first region */
    print("[TEST4] mmap 512KB at 0x30400000, then write page 735 of first region... ");
    long addr3 = sys_mmap(0x30400000, 0x80000, PROT_READ|PROT_WRITE|PROT_EXEC,
                          MAP_PRIVATE|MAP_ANONYMOUS|MAP_FIXED, -1, 0);
    if (addr3 < 0) { print("FAIL: second mmap returned "); print_hex(addr3); print("\n"); return 1; }
    *(volatile unsigned char *)0x30400000UL = 0xEE;
    /* Now try page 735 of the FIRST region — this is where vim crashes */
    *(volatile unsigned char *)0x302df7a1UL = 0x55;
    if (*(volatile unsigned char *)0x302df7a1UL != 0x55) { print("FAIL: first region corrupted after second mmap\n"); return 1; }
    print("OK\n");

    /* Test 5: copy_nonoverlapping pattern (simulates ld-oxide LOAD segment copy) */
    print("[TEST5] Copy data pattern, then write BSS area... ");
    /* Simulate LOAD copy to pages 0-100 */
    for (int i = 0; i < 100; i++) {
        unsigned char *dst = (unsigned char *)(0x30000000UL + (unsigned long)i * 4096UL);
        for (int j = 0; j < 4096; j++) dst[j] = (unsigned char)(i + j);
    }
    /* Now write to BSS page 735 (never copied, should still be demand-paged) */
    *(volatile unsigned char *)0x302df7a1UL = 0x11;
    if (*(volatile unsigned char *)0x302df7a1UL != 0x11) { print("FAIL: BSS page corrupted after LOAD copy\n"); return 1; }
    print("OK\n");

    print("[MMAP-TEST] ALL PASSED\n");
    print("MMAP_WRITE_PASS\n");
    return 0;
}
