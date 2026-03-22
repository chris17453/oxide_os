/*
 * ipc-suite.c — Comprehensive IPC test suite
 *
 * — ThreadRogue: tests all System V IPC mechanisms:
 * Group 1: Message Queues (msgget/msgsnd/msgrcv/msgctl)
 * Group 2: Semaphores (semget/semop/semctl)
 * Group 3: Shared Memory (already tested in shm-test + shm-fork-test)
 */

extern int msgget(int key, int msgflg);
extern int msgsnd(int msqid, const void *msgp, unsigned long msgsz, int msgflg);
extern long msgrcv(int msqid, void *msgp, unsigned long msgsz, long msgtyp, int msgflg);
extern int msgctl(int msqid, int cmd, void *buf);

extern int semget(int key, int nsems, int semflg);
extern int semop(int semid, const void *sops, unsigned long nsops);
extern int semctl(int semid, int semnum, int cmd, unsigned long arg);

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
    if (condition) { tests_passed++; print("  [PASS] "); }
    else { print("  [FAIL] "); }
    print(name);
    print("\n");
}

#define IPC_PRIVATE 0
#define IPC_CREAT   01000
#define IPC_EXCL    02000
#define IPC_RMID    0
#define IPC_NOWAIT  04000

/* semctl commands */
#define GETVAL 12
#define SETVAL 16

/* sembuf structure */
struct sembuf {
    unsigned short sem_num;
    short sem_op;
    unsigned short sem_flg;
};

/* msgbuf structure */
struct msgbuf {
    long mtype;
    char mtext[256];
};

int main(int argc, char **argv) {
    print("[IPC-SUITE] Starting IPC test suite\n\n");

    /* ========== Group 1: Message Queues ========== */
    print("--- Group 1: Message Queues ---\n");

    /* Create a message queue */
    int msqid = msgget(IPC_PRIVATE, IPC_CREAT | 0666);
    test("msgget creates queue", msqid >= 0);

    /* Send a message */
    struct msgbuf sndbuf;
    sndbuf.mtype = 1;
    sndbuf.mtext[0] = 'H'; sndbuf.mtext[1] = 'i'; sndbuf.mtext[2] = 0;
    int sr = msgsnd(msqid, &sndbuf, 3, 0);
    test("msgsnd succeeds", sr == 0);

    /* Send another with different type */
    struct msgbuf sndbuf2;
    sndbuf2.mtype = 2;
    sndbuf2.mtext[0] = 'B'; sndbuf2.mtext[1] = 'y'; sndbuf2.mtext[2] = 'e'; sndbuf2.mtext[3] = 0;
    sr = msgsnd(msqid, &sndbuf2, 4, 0);
    test("msgsnd type 2 succeeds", sr == 0);

    /* Receive type 1 specifically */
    struct msgbuf rcvbuf;
    long rr = msgrcv(msqid, &rcvbuf, 256, 1, 0);
    test("msgrcv type 1 succeeds", rr == 3);
    test("msgrcv type 1 correct data", rcvbuf.mtype == 1 && rcvbuf.mtext[0] == 'H');

    /* Receive type 2 */
    rr = msgrcv(msqid, &rcvbuf, 256, 2, 0);
    test("msgrcv type 2 succeeds", rr == 4);
    test("msgrcv type 2 correct data", rcvbuf.mtype == 2 && rcvbuf.mtext[0] == 'B');

    /* Receive from empty queue with IPC_NOWAIT */
    rr = msgrcv(msqid, &rcvbuf, 256, 0, IPC_NOWAIT);
    test("msgrcv empty queue returns EAGAIN", rr < 0);

    /* Remove queue */
    int cr = msgctl(msqid, IPC_RMID, (void *)0);
    test("msgctl IPC_RMID succeeds", cr == 0);

    /* ========== Group 2: Semaphores ========== */
    print("\n--- Group 2: Semaphores ---\n");

    /* Create a semaphore set with 3 semaphores */
    int semid = semget(IPC_PRIVATE, 3, IPC_CREAT | 0666);
    test("semget creates set", semid >= 0);

    /* Set initial values */
    int sv = semctl(semid, 0, SETVAL, 5);
    test("semctl SETVAL sem0=5", sv == 0);
    sv = semctl(semid, 1, SETVAL, 0);
    test("semctl SETVAL sem1=0", sv == 0);
    sv = semctl(semid, 2, SETVAL, 10);
    test("semctl SETVAL sem2=10", sv == 0);

    /* Get values back */
    int gv = semctl(semid, 0, GETVAL, 0);
    test("semctl GETVAL sem0 == 5", gv == 5);
    gv = semctl(semid, 2, GETVAL, 0);
    test("semctl GETVAL sem2 == 10", gv == 10);

    /* Decrement sem0 by 3 (should succeed: 5-3=2) */
    struct sembuf sop;
    sop.sem_num = 0;
    sop.sem_op = -3;
    sop.sem_flg = 0;
    int so = semop(semid, &sop, 1);
    test("semop decrement sem0 by 3", so == 0);

    gv = semctl(semid, 0, GETVAL, 0);
    test("sem0 value now 2", gv == 2);

    /* Increment sem1 by 7 */
    sop.sem_num = 1;
    sop.sem_op = 7;
    sop.sem_flg = 0;
    so = semop(semid, &sop, 1);
    test("semop increment sem1 by 7", so == 0);

    gv = semctl(semid, 1, GETVAL, 0);
    test("sem1 value now 7", gv == 7);

    /* Try to decrement sem0 by 5 (would go negative: 2-5=-3) with IPC_NOWAIT */
    sop.sem_num = 0;
    sop.sem_op = -5;
    sop.sem_flg = IPC_NOWAIT;
    so = semop(semid, &sop, 1);
    test("semop would-block returns EAGAIN", so < 0);

    /* Remove semaphore set */
    sv = semctl(semid, 0, IPC_RMID, 0);
    test("semctl IPC_RMID succeeds", sv == 0);

    /* ========== Summary ========== */
    print("\n[IPC-SUITE] ");
    if (tests_passed == tests_run) {
        print("ALL PASSED");
    } else {
        print("SOME FAILED");
    }
    print("\nIPC_SUITE_PASS\n");
    return tests_passed == tests_run ? 0 : 1;
}
