/* OXIDE OS POSIX API */

#ifndef _UNISTD_H
#define _UNISTD_H

/* POSIX threads support */
#define _POSIX_THREADS 200809L

#include <stddef.h>
#include <sys/types.h>
#include <sys/time.h>

/* Standard file descriptors */
#define STDIN_FILENO    0
#define STDOUT_FILENO   1
#define STDERR_FILENO   2

/* access() mode flags */
#define F_OK    0
#define R_OK    4
#define W_OK    2
#define X_OK    1

/* lseek whence */
#define SEEK_SET    0
#define SEEK_CUR    1
#define SEEK_END    2

/* confstr/sysconf/pathconf */
#define _SC_CLK_TCK             2
#define _SC_PAGESIZE            30
#define _SC_PAGE_SIZE           _SC_PAGESIZE
#define _SC_NPROCESSORS_ONLN    84
#define _SC_NPROCESSORS_CONF    83
#define _SC_OPEN_MAX            4
#define _SC_HOST_NAME_MAX       180
#define _SC_GETPW_R_SIZE_MAX    70
#define _SC_GETGR_R_SIZE_MAX    69
#define _SC_TTY_NAME_MAX        71
#define _SC_ARG_MAX             0
#define _SC_CHILD_MAX           1
#define _SC_IOV_MAX             60
#define _SC_SYMLOOP_MAX         173
#define _SC_THREAD_SAFE_FUNCTIONS 68

#define _PC_PATH_MAX    4
#define _PC_NAME_MAX    3
#define _PC_PIPE_BUF    5

/* File I/O */
ssize_t read(int fd, void *buf, size_t count);
ssize_t write(int fd, const void *buf, size_t count);
int close(int fd);
off_t lseek(int fd, off_t offset, int whence);
ssize_t pread(int fd, void *buf, size_t count, off_t offset);
ssize_t pwrite(int fd, const void *buf, size_t count, off_t offset);

/* File operations */
int dup(int oldfd);
int dup2(int oldfd, int newfd);
int pipe(int pipefd[2]);
int pipe2(int pipefd[2], int flags);
int dup3(int oldfd, int newfd, int flags);
int access(const char *path, int mode);
int faccessat(int dirfd, const char *path, int mode, int flags);
int fchmodat(int dirfd, const char *path, mode_t mode, int flags);
int fchownat(int dirfd, const char *path, uid_t owner, gid_t group, int flags);
int linkat(int olddirfd, const char *oldpath, int newdirfd, const char *newpath, int flags);
int symlinkat(const char *target, int newdirfd, const char *linkpath);
ssize_t readlinkat(int dirfd, const char *pathname, char *buf, size_t bufsiz);
int renameat(int olddirfd, const char *oldpath, int newdirfd, const char *newpath);
int unlinkat(int dirfd, const char *pathname, int flags);
int mkdirat(int dirfd, const char *pathname, mode_t mode);
int unlink(const char *pathname);
int rmdir(const char *pathname);
int link(const char *oldpath, const char *newpath);
int symlink(const char *target, const char *linkpath);
ssize_t readlink(const char *pathname, char *buf, size_t bufsiz);
int truncate(const char *path, off_t length);
int ftruncate(int fd, off_t length);
ssize_t copy_file_range(int fd_in, off_t *off_in, int fd_out, off_t *off_out,
                        size_t len, unsigned int flags);
int fsync(int fd);
int fdatasync(int fd);
int lockf(int fd, int cmd, off_t len);
int isatty(int fd);
char *ttyname(int fd);
int ttyname_r(int fd, char *buf, size_t buflen);

/* Process control */
pid_t fork(void);
pid_t vfork(void);
int execve(const char *pathname, char *const argv[], char *const envp[]);
int execv(const char *pathname, char *const argv[]);
int execvp(const char *file, char *const argv[]);
int execvpe(const char *file, char *const argv[], char *const envp[]);
int fexecve(int fd, char *const argv[], char *const envp[]);
int close_range(unsigned int first, unsigned int last, unsigned int flags);
int fdwalk(int (*func)(void *, int), void *arg);
int execl(const char *pathname, const char *arg, ...);
int execlp(const char *file, const char *arg, ...);
void _exit(int status) __attribute__((noreturn));

/* Process info */
pid_t getpid(void);
pid_t getppid(void);
pid_t getpgid(pid_t pid);
pid_t getpgrp(void);
int setpgid(pid_t pid, pid_t pgid);
int setpgrp(void);
pid_t setsid(void);
pid_t getsid(pid_t pid);
pid_t tcgetpgrp(int fd);
int tcsetpgrp(int fd, pid_t pgrp);

/* User/Group */
uid_t getuid(void);
uid_t geteuid(void);
gid_t getgid(void);
gid_t getegid(void);
int setuid(uid_t uid);
int setgid(gid_t gid);
int seteuid(uid_t euid);
int setegid(gid_t egid);
int setreuid(uid_t ruid, uid_t euid);
int setregid(gid_t rgid, gid_t egid);
int setresuid(uid_t ruid, uid_t euid, uid_t suid);
int setresgid(gid_t rgid, gid_t egid, gid_t sgid);
int getresuid(uid_t *ruid, uid_t *euid, uid_t *suid);
int getresgid(gid_t *rgid, gid_t *egid, gid_t *sgid);
int getgroups(int size, gid_t list[]);
int setgroups(size_t size, const gid_t *list);

/* Directory operations */
int chdir(const char *path);
int fchdir(int fd);
char *getcwd(char *buf, size_t size);
int chown(const char *pathname, uid_t owner, gid_t group);
int lchown(const char *pathname, uid_t owner, gid_t group);
int fchown(int fd, uid_t owner, gid_t group);

/* System */
int gethostname(char *name, size_t len);
int sethostname(const char *name, size_t len);
long sysconf(int name);
long pathconf(const char *path, int name);
long fpathconf(int fd, int name);
unsigned int sleep(unsigned int seconds);
int usleep(unsigned int usec);
unsigned int alarm(unsigned int seconds);
int pause(void);
int nice(int inc);
void swab(const void *from, void *to, ssize_t n);
void sync(void);
int futimes(int fd, const struct timeval times[2]);
int lutimes(const char *path, const struct timeval times[2]);

/* Misc */
int brk(void *addr);
void *sbrk(intptr_t increment);
int getpagesize(void);
char *getlogin(void);
int getlogin_r(char *buf, size_t bufsize);
int getopt(int argc, char * const argv[], const char *optstring);
char *ctermid(char *s);
int ctermid_r(char *s);

#define L_ctermid 32

extern char *optarg;
extern int optind, opterr, optopt;

/* Confstr */
size_t confstr(int name, char *buf, size_t len);

/* File operations (additional) */
int rename(const char *oldpath, const char *newpath);
int chroot(const char *path);

#endif /* _UNISTD_H */
