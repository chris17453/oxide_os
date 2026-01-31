/* OXIDE OS Process Spawn */

#ifndef _SPAWN_H
#define _SPAWN_H

#include <sys/types.h>
#include <signal.h>

typedef struct {
    short flags;
    pid_t pgroup;
    sigset_t sigdefault;
    sigset_t sigmask;
} posix_spawnattr_t;

typedef struct {
    int action_count;
} posix_spawn_file_actions_t;

#define POSIX_SPAWN_RESETIDS    0x0001
#define POSIX_SPAWN_SETPGROUP   0x0002
#define POSIX_SPAWN_SETSIGDEF   0x0004
#define POSIX_SPAWN_SETSIGMASK  0x0008

int posix_spawn(pid_t *pid, const char *path,
                const posix_spawn_file_actions_t *file_actions,
                const posix_spawnattr_t *attrp,
                char *const argv[], char *const envp[]);
int posix_spawnp(pid_t *pid, const char *file,
                 const posix_spawn_file_actions_t *file_actions,
                 const posix_spawnattr_t *attrp,
                 char *const argv[], char *const envp[]);

int posix_spawnattr_init(posix_spawnattr_t *attr);
int posix_spawnattr_destroy(posix_spawnattr_t *attr);
int posix_spawnattr_setflags(posix_spawnattr_t *attr, short flags);
int posix_spawnattr_getflags(const posix_spawnattr_t *attr, short *flags);
int posix_spawnattr_setsigdefault(posix_spawnattr_t *attr, const sigset_t *sigdefault);
int posix_spawnattr_getsigdefault(const posix_spawnattr_t *attr, sigset_t *sigdefault);
int posix_spawnattr_setsigmask(posix_spawnattr_t *attr, const sigset_t *sigmask);
int posix_spawnattr_getsigmask(const posix_spawnattr_t *attr, sigset_t *sigmask);
int posix_spawnattr_setpgroup(posix_spawnattr_t *attr, pid_t pgroup);
int posix_spawnattr_getpgroup(const posix_spawnattr_t *attr, pid_t *pgroup);

int posix_spawn_file_actions_init(posix_spawn_file_actions_t *file_actions);
int posix_spawn_file_actions_destroy(posix_spawn_file_actions_t *file_actions);
int posix_spawn_file_actions_addopen(posix_spawn_file_actions_t *file_actions,
                                     int fildes, const char *path,
                                     int oflag, mode_t mode);
int posix_spawn_file_actions_addclose(posix_spawn_file_actions_t *file_actions,
                                      int fildes);
int posix_spawn_file_actions_adddup2(posix_spawn_file_actions_t *file_actions,
                                     int fildes, int newfildes);

#endif /* _SPAWN_H */
