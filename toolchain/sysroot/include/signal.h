/* OXIDE OS Signal Handling */

#ifndef _SIGNAL_H
#define _SIGNAL_H

#include <sys/types.h>

typedef volatile int sig_atomic_t;

/* Signal numbers */
#define SIGHUP      1
#define SIGINT      2
#define SIGQUIT     3
#define SIGILL      4
#define SIGTRAP     5
#define SIGABRT     6
#define SIGIOT      SIGABRT
#define SIGBUS      7
#define SIGFPE      8
#define SIGKILL     9
#define SIGUSR1     10
#define SIGSEGV     11
#define SIGUSR2     12
#define SIGPIPE     13
#define SIGALRM     14
#define SIGTERM     15
#define SIGSTKFLT   16
#define SIGCHLD     17
#define SIGCONT     18
#define SIGSTOP     19
#define SIGTSTP     20
#define SIGTTIN     21
#define SIGTTOU     22
#define SIGURG      23
#define SIGXCPU     24
#define SIGXFSZ     25
#define SIGVTALRM   26
#define SIGPROF     27
#define SIGWINCH    28
#define SIGIO       29
#define SIGPOLL     SIGIO
#define SIGPWR      30
#define SIGSYS      31
#define _NSIG       65

/* Signal actions */
#define SIG_DFL     ((void (*)(int))0)
#define SIG_IGN     ((void (*)(int))1)
#define SIG_ERR     ((void (*)(int))-1)

/* sigaction flags */
#define SA_NOCLDSTOP    0x00000001
#define SA_NOCLDWAIT    0x00000002
#define SA_SIGINFO      0x00000004
#define SA_ONSTACK      0x08000000
#define SA_RESTART      0x10000000
#define SA_NODEFER      0x40000000
#define SA_RESETHAND    0x80000000

/* sigprocmask how */
#define SIG_BLOCK       0
#define SIG_UNBLOCK     1
#define SIG_SETMASK     2

/* Signal set type */
typedef unsigned long sigset_t;

/* siginfo_t */
typedef struct {
    int si_signo;
    int si_errno;
    int si_code;
    pid_t si_pid;
    uid_t si_uid;
    int si_status;
    void *si_addr;
    long si_band;
    union {
        int _pad[28];
    } _sifields;
} siginfo_t;

/* sigaction structure */
struct sigaction {
    union {
        void (*sa_handler)(int);
        void (*sa_sigaction)(int, siginfo_t *, void *);
    } __sa_handler;
    sigset_t sa_mask;
    int sa_flags;
    void (*sa_restorer)(void);
};

#define sa_handler   __sa_handler.sa_handler
#define sa_sigaction __sa_handler.sa_sigaction

/* Stack */
typedef struct {
    void *ss_sp;
    int ss_flags;
    size_t ss_size;
} stack_t;

/* Functions */
int sigaction(int signum, const struct sigaction *act, struct sigaction *oldact);
int sigprocmask(int how, const sigset_t *set, sigset_t *oldset);
int sigpending(sigset_t *set);
int sigsuspend(const sigset_t *mask);
int kill(pid_t pid, int sig);
int raise(int sig);
void (*signal(int signum, void (*handler)(int)))(int);
int sigaltstack(const stack_t *ss, stack_t *old_ss);
int pthread_sigmask(int how, const sigset_t *set, sigset_t *oldset);
int sigwait(const sigset_t *set, int *sig);

int sigemptyset(sigset_t *set);
int sigfillset(sigset_t *set);
int sigaddset(sigset_t *set, int signum);
int sigdelset(sigset_t *set, int signum);
int sigismember(const sigset_t *set, int signum);

#endif /* _SIGNAL_H */
