/*
 * shm-fork-test.c — Cross-process shared memory test
 *
 * — ThreadRogue: the REAL shared memory test. Single-process shmat is easy —
 * this one forks and verifies the parent's writes are visible in the child's
 * mapping of the SAME shared segment. If this passes, IPC over shared memory works.
 *
 * Flow:
 * 1. Parent: shmget + shmat
 * 2. Parent: write magic value to shared region
 * 3. Parent: fork
 * 4. Child: shmat the SAME shmid
 * 5. Child: read and verify magic value
 * 6. Child: write different value
 * 7. Parent: wait for child, verify child's write is visible
 */

extern int shmget(int key, unsigned long size, int shmflg);
extern void *shmat(int shmid, const void *shmaddr, int shmflg);
extern int shmdt(const void *shmaddr);
extern int shmctl(int shmid, int cmd, void *buf);

#define IPC_PRIVATE 0
#define IPC_CREAT   01000
#define IPC_RMID    0

static long oxide_write(int fd, const void *buf, unsigned long count) {
    long ret;
    __asm__ volatile ("syscall" : "=a"(ret) : "a"(1), "D"(fd), "S"(buf), "d"(count) : "rcx", "r11", "memory");
    return ret;
}

static int oxide_fork(void) {
    long ret;
    __asm__ volatile ("syscall" : "=a"(ret) : "a"(57) : "rcx", "r11", "memory");
    return (int)ret;
}

static int oxide_waitpid(int pid, int *status, int options) {
    long ret;
    __asm__ volatile ("syscall" : "=a"(ret) : "a"(61), "D"(pid), "S"(status), "d"(options) : "rcx", "r11", "memory");
    return (int)ret;
}

static void oxide_exit(int code) {
    __asm__ volatile ("syscall" :: "a"(60), "D"(code));
    __builtin_unreachable();
}

static void oxide_usleep(unsigned long us) {
    /* nanosleep via syscall 35 */
    struct { long sec; long nsec; } ts = { 0, us * 1000 };
    __asm__ volatile ("syscall" :: "a"(35), "D"(&ts), "S"(0) : "rcx", "r11", "memory");
}

static void print(const char *s) {
    unsigned long len = 0;
    while (s[len]) len++;
    oxide_write(1, s, len);
}

int main(int argc, char **argv) {
    print("[SHM-FORK] Starting cross-process shared memory test\n");

    /* Step 1: Create shared memory segment */
    int shmid = shmget(IPC_PRIVATE, 4096, IPC_CREAT | 0666);
    if (shmid < 0) {
        print("  [FAIL] shmget failed\n");
        return 1;
    }
    print("  [PASS] shmget OK\n");

    /* Step 2: Attach in parent */
    volatile int *shared = (volatile int *)shmat(shmid, (void *)0, 0);
    if ((long)shared == -1) {
        print("  [FAIL] parent shmat failed\n");
        return 1;
    }
    print("  [PASS] parent shmat OK\n");

    /* Step 3: Parent writes magic value */
    shared[0] = 0xDEADBEEF;
    shared[1] = 0xCAFEBABE;
    print("  [INFO] parent wrote 0xDEADBEEF + 0xCAFEBABE\n");

    /* Step 4: Fork */
    int pid = oxide_fork();
    if (pid < 0) {
        print("  [FAIL] fork failed\n");
        return 1;
    }

    if (pid == 0) {
        /* === CHILD PROCESS === */
        /* Step 5: Child attaches the same segment */
        volatile int *child_shared = (volatile int *)shmat(shmid, (void *)0, 0);
        if ((long)child_shared == -1) {
            print("  [FAIL] child shmat failed\n");
            oxide_exit(1);
        }
        print("  [PASS] child shmat OK\n");

        /* Step 6: Verify parent's writes are visible */
        if (child_shared[0] == (int)0xDEADBEEF && child_shared[1] == (int)0xCAFEBABE) {
            print("  [PASS] child sees parent's data!\n");
        } else {
            print("  [FAIL] child doesn't see parent's data\n");
            oxide_exit(1);
        }

        /* Step 7: Child writes its own value */
        child_shared[2] = 0x12345678;
        print("  [INFO] child wrote 0x12345678\n");

        shmdt((void *)child_shared);
        oxide_exit(0);
    }

    /* === PARENT PROCESS === */
    /* Wait for child */
    int status = 0;
    oxide_waitpid(pid, &status, 0);

    /* Step 8: Verify child's write is visible to parent */
    if (shared[2] == 0x12345678) {
        print("  [PASS] parent sees child's data!\n");
    } else {
        print("  [FAIL] parent doesn't see child's data\n");
        shmdt((void *)shared);
        shmctl(shmid, IPC_RMID, (void *)0);
        return 1;
    }

    /* Cleanup */
    shmdt((void *)shared);
    shmctl(shmid, IPC_RMID, (void *)0);
    print("  [PASS] cleanup OK\n");

    print("[SHM-FORK] ALL PASSED\n");
    print("SHM_FORK_PASS\n");
    return 0;
}
