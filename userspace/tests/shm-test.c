/*
 * shm-test.c — System V shared memory test
 *
 * — ThreadRogue: tests the shared memory syscall path:
 * 1. shmget — create a shared memory segment
 * 2. shmat — attach it to our address space
 * 3. Write data to the shared region
 * 4. Verify data is readable
 * 5. shmdt — detach
 * 6. shmctl IPC_RMID — remove segment
 */

extern int shmget(int key, unsigned long size, int shmflg);
extern void *shmat(int shmid, const void *shmaddr, int shmflg);
extern int shmdt(const void *shmaddr);
extern int shmctl(int shmid, int cmd, void *buf);
extern int fork(void);
extern int wait(int *status);

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

#define IPC_PRIVATE 0
#define IPC_CREAT   01000
#define IPC_RMID    0

int main(int argc, char **argv) {
    print("[SHM-TEST] Starting shared memory tests\n");

    /* Test 1: Create a shared memory segment */
    int shmid = shmget(IPC_PRIVATE, 4096, IPC_CREAT | 0666);
    if (shmid < 0) {
        print("  [FAIL] shmget returned error\n");
        return 1;
    }
    print("  [PASS] shmget created segment\n");

    /* Test 2: Attach the segment */
    void *addr = shmat(shmid, (void *)0, 0);
    if (addr == (void *)-1) {
        print("  [FAIL] shmat returned -1\n");
        return 1;
    }
    print("  [PASS] shmat attached segment\n");

    /* Test 3: Write to shared memory */
    volatile int *shared = (volatile int *)addr;
    *shared = 0x42424242;
    if (*shared != 0x42424242) {
        print("  [FAIL] shared memory write/read mismatch\n");
        return 1;
    }
    print("  [PASS] write + readback OK\n");

    /* Test 4: Write a pattern across multiple pages */
    volatile unsigned char *bytes = (volatile unsigned char *)addr;
    for (int i = 0; i < 4096; i++) {
        bytes[i] = (unsigned char)(i & 0xFF);
    }
    int ok = 1;
    for (int i = 0; i < 4096; i++) {
        if (bytes[i] != (unsigned char)(i & 0xFF)) { ok = 0; break; }
    }
    if (ok) {
        print("  [PASS] full page pattern write/verify\n");
    } else {
        print("  [FAIL] pattern mismatch\n");
        return 1;
    }

    /* Test 5: Detach */
    int dt = shmdt(addr);
    if (dt < 0) {
        print("  [FAIL] shmdt returned error\n");
        return 1;
    }
    print("  [PASS] shmdt detached\n");

    /* Test 6: Remove segment */
    int ctl = shmctl(shmid, IPC_RMID, (void *)0);
    if (ctl < 0) {
        print("  [FAIL] shmctl IPC_RMID returned error\n");
        return 1;
    }
    print("  [PASS] shmctl IPC_RMID removed segment\n");

    print("[SHM-TEST] ALL PASSED\n");
    print("SHM_PASS\n");
    return 0;
}
