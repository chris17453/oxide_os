/* OXIDE OS Process Wait */

#ifndef _SYS_WAIT_H
#define _SYS_WAIT_H

#include <sys/types.h>
#include <signal.h>

/* idtype_t for waitid */
typedef enum {
    P_ALL = 0,
    P_PID = 1,
    P_PGID = 2,
} idtype_t;

/* Wait options */
#define WNOHANG     1
#define WUNTRACED   2
#define WCONTINUED  8

/* Wait status macros */
#define WEXITSTATUS(s)  (((s) >> 8) & 0xFF)
#define WTERMSIG(s)     ((s) & 0x7F)
#define WSTOPSIG(s)     WEXITSTATUS(s)
#define WIFEXITED(s)    (WTERMSIG(s) == 0)
#define WIFSIGNALED(s)  (((signed char)(((s) & 0x7F) + 1) >> 1) > 0)
#define WIFSTOPPED(s)   (((s) & 0xFF) == 0x7F)
#define WIFCONTINUED(s) ((s) == 0xFFFF)
#define WCOREDUMP(s)    ((s) & 0x80)

pid_t wait(int *wstatus);
pid_t waitpid(pid_t pid, int *wstatus, int options);
int waitid(idtype_t idtype, id_t id, siginfo_t *infop, int options);
pid_t wait3(int *wstatus, int options, void *rusage);
pid_t wait4(pid_t pid, int *wstatus, int options, void *rusage);

#endif /* _SYS_WAIT_H */
