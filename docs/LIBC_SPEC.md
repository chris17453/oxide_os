# EFFLUX libc Specification

**Version:** 1.0  
**Status:** Draft  
**License:** MIT  

---

## 0) Overview

EFFLUX libc is a custom C library providing Linux source compatibility. Apps recompile against EFFLUX libc to run on EFFLUX OS.

**Goals:**
- POSIX.1-2017 core compliance
- Linux-compatible extensions where useful
- Clean, modern implementation in Rust with C ABI
- No legacy baggage

**Non-goals:**
- Binary compatibility with glibc/musl
- Every obscure POSIX function
- Legacy BSD functions (unless widely used)

---

## 1) Architecture

```
┌─────────────────────────────────────────────┐
│  Application (C/C++/Rust FFI)               │
├─────────────────────────────────────────────┤
│  EFFLUX libc (Rust with C ABI)              │
│  ├── stdio                                  │
│  ├── stdlib                                 │
│  ├── string                                 │
│  ├── unistd                                 │
│  ├── pthread                                │
│  ├── socket                                 │
│  └── ...                                    │
├─────────────────────────────────────────────┤
│  Syscall layer (inline asm)                 │
├─────────────────────────────────────────────┤
│  EFFLUX Kernel                              │
└─────────────────────────────────────────────┘
```

---

## 2) Headers and Functions

### 2.1 stdio.h — Standard I/O

```c
// File operations
FILE *fopen(const char *path, const char *mode);
FILE *fdopen(int fd, const char *mode);
FILE *freopen(const char *path, const char *mode, FILE *stream);
int fclose(FILE *stream);
int fflush(FILE *stream);

// Reading
size_t fread(void *ptr, size_t size, size_t nmemb, FILE *stream);
int fgetc(FILE *stream);
int getc(FILE *stream);
int getchar(void);
char *fgets(char *s, int size, FILE *stream);
int ungetc(int c, FILE *stream);

// Writing
size_t fwrite(const void *ptr, size_t size, size_t nmemb, FILE *stream);
int fputc(int c, FILE *stream);
int putc(int c, FILE *stream);
int putchar(int c);
int fputs(const char *s, FILE *stream);
int puts(const char *s);

// Formatted I/O
int printf(const char *format, ...);
int fprintf(FILE *stream, const char *format, ...);
int sprintf(char *str, const char *format, ...);
int snprintf(char *str, size_t size, const char *format, ...);
int vprintf(const char *format, va_list ap);
int vfprintf(FILE *stream, const char *format, va_list ap);
int vsprintf(char *str, const char *format, va_list ap);
int vsnprintf(char *str, size_t size, const char *format, va_list ap);

int scanf(const char *format, ...);
int fscanf(FILE *stream, const char *format, ...);
int sscanf(const char *str, const char *format, ...);

// Positioning
int fseek(FILE *stream, long offset, int whence);
long ftell(FILE *stream);
void rewind(FILE *stream);
int fgetpos(FILE *stream, fpos_t *pos);
int fsetpos(FILE *stream, const fpos_t *pos);

// Error handling
int feof(FILE *stream);
int ferror(FILE *stream);
void clearerr(FILE *stream);
void perror(const char *s);

// Buffering
int setvbuf(FILE *stream, char *buf, int mode, size_t size);
void setbuf(FILE *stream, char *buf);

// File descriptor access
int fileno(FILE *stream);

// Temporary files
FILE *tmpfile(void);
char *tmpnam(char *s);

// Standard streams
extern FILE *stdin;
extern FILE *stdout;
extern FILE *stderr;
```

### 2.2 stdlib.h — General Utilities

```c
// Memory allocation
void *malloc(size_t size);
void *calloc(size_t nmemb, size_t size);
void *realloc(void *ptr, size_t size);
void free(void *ptr);
void *aligned_alloc(size_t alignment, size_t size);

// Process control
void exit(int status);
void _Exit(int status);
void abort(void);
int atexit(void (*func)(void));
int at_quick_exit(void (*func)(void));
void quick_exit(int status);

// Environment
char *getenv(const char *name);
int setenv(const char *name, const char *value, int overwrite);
int unsetenv(const char *name);
int putenv(char *string);

// String conversion
int atoi(const char *nptr);
long atol(const char *nptr);
long long atoll(const char *nptr);
double atof(const char *nptr);
long strtol(const char *nptr, char **endptr, int base);
long long strtoll(const char *nptr, char **endptr, int base);
unsigned long strtoul(const char *nptr, char **endptr, int base);
unsigned long long strtoull(const char *nptr, char **endptr, int base);
double strtod(const char *nptr, char **endptr);
float strtof(const char *nptr, char **endptr);

// Random numbers
int rand(void);
void srand(unsigned int seed);
int rand_r(unsigned int *seedp);

// Sorting and searching
void qsort(void *base, size_t nmemb, size_t size,
           int (*compar)(const void *, const void *));
void *bsearch(const void *key, const void *base, size_t nmemb,
              size_t size, int (*compar)(const void *, const void *));

// Integer arithmetic
int abs(int j);
long labs(long j);
long long llabs(long long j);
div_t div(int numer, int denom);
ldiv_t ldiv(long numer, long denom);
lldiv_t lldiv(long long numer, long long denom);

// Multibyte/wide character conversion
int mblen(const char *s, size_t n);
int mbtowc(wchar_t *pwc, const char *s, size_t n);
int wctomb(char *s, wchar_t wc);
size_t mbstowcs(wchar_t *dest, const char *src, size_t n);
size_t wcstombs(char *dest, const wchar_t *src, size_t n);

// Program execution
int system(const char *command);
```

### 2.3 string.h — String Operations

```c
// Copying
void *memcpy(void *dest, const void *src, size_t n);
void *memmove(void *dest, const void *src, size_t n);
char *strcpy(char *dest, const char *src);
char *strncpy(char *dest, const char *src, size_t n);
char *strdup(const char *s);
char *strndup(const char *s, size_t n);

// Concatenation
char *strcat(char *dest, const char *src);
char *strncat(char *dest, const char *src, size_t n);

// Comparison
int memcmp(const void *s1, const void *s2, size_t n);
int strcmp(const char *s1, const char *s2);
int strncmp(const char *s1, const char *s2, size_t n);
int strcasecmp(const char *s1, const char *s2);
int strncasecmp(const char *s1, const char *s2, size_t n);
int strcoll(const char *s1, const char *s2);

// Searching
void *memchr(const void *s, int c, size_t n);
char *strchr(const char *s, int c);
char *strrchr(const char *s, int c);
char *strstr(const char *haystack, const char *needle);
char *strtok(char *str, const char *delim);
char *strtok_r(char *str, const char *delim, char **saveptr);
size_t strspn(const char *s, const char *accept);
size_t strcspn(const char *s, const char *reject);
char *strpbrk(const char *s, const char *accept);

// Other
void *memset(void *s, int c, size_t n);
size_t strlen(const char *s);
size_t strnlen(const char *s, size_t maxlen);
char *strerror(int errnum);
int strerror_r(int errnum, char *buf, size_t buflen);
size_t strxfrm(char *dest, const char *src, size_t n);
```

### 2.4 unistd.h — POSIX API

```c
// File operations
int close(int fd);
ssize_t read(int fd, void *buf, size_t count);
ssize_t write(int fd, const void *buf, size_t count);
ssize_t pread(int fd, void *buf, size_t count, off_t offset);
ssize_t pwrite(int fd, const void *buf, size_t count, off_t offset);
off_t lseek(int fd, off_t offset, int whence);
int fsync(int fd);
int fdatasync(int fd);
int ftruncate(int fd, off_t length);
int truncate(const char *path, off_t length);

// File descriptor manipulation
int dup(int oldfd);
int dup2(int oldfd, int newfd);
int dup3(int oldfd, int newfd, int flags);
int pipe(int pipefd[2]);
int pipe2(int pipefd[2], int flags);

// Process control
pid_t fork(void);
pid_t vfork(void);
int execve(const char *pathname, char *const argv[], char *const envp[]);
int execv(const char *pathname, char *const argv[]);
int execvp(const char *file, char *const argv[]);
int execl(const char *pathname, const char *arg, ...);
int execlp(const char *file, const char *arg, ...);
int execle(const char *pathname, const char *arg, ...);
void _exit(int status);

// Process info
pid_t getpid(void);
pid_t getppid(void);
pid_t getpgrp(void);
pid_t getpgid(pid_t pid);
int setpgid(pid_t pid, pid_t pgid);
pid_t setsid(void);
pid_t getsid(pid_t pid);

// User/group
uid_t getuid(void);
uid_t geteuid(void);
gid_t getgid(void);
gid_t getegid(void);
int setuid(uid_t uid);
int seteuid(uid_t euid);
int setgid(gid_t gid);
int setegid(gid_t egid);
int getgroups(int size, gid_t list[]);
int setgroups(size_t size, const gid_t *list);

// Directory
int chdir(const char *path);
int fchdir(int fd);
char *getcwd(char *buf, size_t size);
int chroot(const char *path);

// File system
int link(const char *oldpath, const char *newpath);
int linkat(int olddirfd, const char *oldpath, int newdirfd, const char *newpath, int flags);
int unlink(const char *pathname);
int unlinkat(int dirfd, const char *pathname, int flags);
int symlink(const char *target, const char *linkpath);
int symlinkat(const char *target, int newdirfd, const char *linkpath);
ssize_t readlink(const char *pathname, char *buf, size_t bufsiz);
ssize_t readlinkat(int dirfd, const char *pathname, char *buf, size_t bufsiz);
int rmdir(const char *pathname);
int access(const char *pathname, int mode);
int faccessat(int dirfd, const char *pathname, int mode, int flags);

// Sleeping
unsigned int sleep(unsigned int seconds);
int usleep(useconds_t usec);
int pause(void);

// Terminal
int isatty(int fd);
char *ttyname(int fd);
int ttyname_r(int fd, char *buf, size_t buflen);
pid_t tcgetpgrp(int fd);
int tcsetpgrp(int fd, pid_t pgrp);

// Host/domain
int gethostname(char *name, size_t len);
int sethostname(const char *name, size_t len);

// Misc
long sysconf(int name);
long pathconf(const char *path, int name);
long fpathconf(int fd, int name);
int getopt(int argc, char * const argv[], const char *optstring);
extern char *optarg;
extern int optind, opterr, optopt;
```

### 2.5 fcntl.h — File Control

```c
int open(const char *pathname, int flags, ...);
int openat(int dirfd, const char *pathname, int flags, ...);
int creat(const char *pathname, mode_t mode);
int fcntl(int fd, int cmd, ...);

// Flags
#define O_RDONLY    0x0000
#define O_WRONLY    0x0001
#define O_RDWR      0x0002
#define O_CREAT     0x0040
#define O_EXCL      0x0080
#define O_NOCTTY    0x0100
#define O_TRUNC     0x0200
#define O_APPEND    0x0400
#define O_NONBLOCK  0x0800
#define O_SYNC      0x1000
#define O_CLOEXEC   0x80000
#define O_DIRECTORY 0x10000
#define O_NOFOLLOW  0x20000

// fcntl commands
#define F_DUPFD         0
#define F_GETFD         1
#define F_SETFD         2
#define F_GETFL         3
#define F_SETFL         4
#define F_GETLK         5
#define F_SETLK         6
#define F_SETLKW        7
#define F_DUPFD_CLOEXEC 1030
```

### 2.6 sys/stat.h — File Status

```c
int stat(const char *pathname, struct stat *statbuf);
int fstat(int fd, struct stat *statbuf);
int lstat(const char *pathname, struct stat *statbuf);
int fstatat(int dirfd, const char *pathname, struct stat *statbuf, int flags);
int chmod(const char *pathname, mode_t mode);
int fchmod(int fd, mode_t mode);
int fchmodat(int dirfd, const char *pathname, mode_t mode, int flags);
int mkdir(const char *pathname, mode_t mode);
int mkdirat(int dirfd, const char *pathname, mode_t mode);
int mkfifo(const char *pathname, mode_t mode);
int mknod(const char *pathname, mode_t mode, dev_t dev);
mode_t umask(mode_t mask);

struct stat {
    dev_t     st_dev;
    ino_t     st_ino;
    mode_t    st_mode;
    nlink_t   st_nlink;
    uid_t     st_uid;
    gid_t     st_gid;
    dev_t     st_rdev;
    off_t     st_size;
    blksize_t st_blksize;
    blkcnt_t  st_blocks;
    struct timespec st_atim;
    struct timespec st_mtim;
    struct timespec st_ctim;
};
```

### 2.7 sys/wait.h — Process Wait

```c
pid_t wait(int *wstatus);
pid_t waitpid(pid_t pid, int *wstatus, int options);
int waitid(idtype_t idtype, id_t id, siginfo_t *infop, int options);

// Macros
#define WIFEXITED(status)   ...
#define WEXITSTATUS(status) ...
#define WIFSIGNALED(status) ...
#define WTERMSIG(status)    ...
#define WIFSTOPPED(status)  ...
#define WSTOPSIG(status)    ...
#define WIFCONTINUED(status) ...

// Options
#define WNOHANG   1
#define WUNTRACED 2
```

### 2.8 signal.h — Signals

```c
typedef void (*sighandler_t)(int);

sighandler_t signal(int signum, sighandler_t handler);
int sigaction(int signum, const struct sigaction *act, struct sigaction *oldact);
int kill(pid_t pid, int sig);
int raise(int sig);
int sigprocmask(int how, const sigset_t *set, sigset_t *oldset);
int sigsuspend(const sigset_t *mask);
int sigpending(sigset_t *set);
int sigwait(const sigset_t *set, int *sig);
int sigwaitinfo(const sigset_t *set, siginfo_t *info);
int sigtimedwait(const sigset_t *set, siginfo_t *info, const struct timespec *timeout);

// Signal set operations
int sigemptyset(sigset_t *set);
int sigfillset(sigset_t *set);
int sigaddset(sigset_t *set, int signum);
int sigdelset(sigset_t *set, int signum);
int sigismember(const sigset_t *set, int signum);

// Alarm
unsigned int alarm(unsigned int seconds);

struct sigaction {
    union {
        sighandler_t sa_handler;
        void (*sa_sigaction)(int, siginfo_t *, void *);
    };
    sigset_t sa_mask;
    int sa_flags;
};
```

### 2.9 pthread.h — POSIX Threads

```c
// Thread management
int pthread_create(pthread_t *thread, const pthread_attr_t *attr,
                   void *(*start_routine)(void *), void *arg);
void pthread_exit(void *retval);
int pthread_join(pthread_t thread, void **retval);
int pthread_detach(pthread_t thread);
pthread_t pthread_self(void);
int pthread_equal(pthread_t t1, pthread_t t2);
int pthread_cancel(pthread_t thread);
int pthread_setcancelstate(int state, int *oldstate);
int pthread_setcanceltype(int type, int *oldtype);
void pthread_testcancel(void);

// Thread attributes
int pthread_attr_init(pthread_attr_t *attr);
int pthread_attr_destroy(pthread_attr_t *attr);
int pthread_attr_setdetachstate(pthread_attr_t *attr, int detachstate);
int pthread_attr_getdetachstate(const pthread_attr_t *attr, int *detachstate);
int pthread_attr_setstacksize(pthread_attr_t *attr, size_t stacksize);
int pthread_attr_getstacksize(const pthread_attr_t *attr, size_t *stacksize);

// Mutex
int pthread_mutex_init(pthread_mutex_t *mutex, const pthread_mutexattr_t *attr);
int pthread_mutex_destroy(pthread_mutex_t *mutex);
int pthread_mutex_lock(pthread_mutex_t *mutex);
int pthread_mutex_trylock(pthread_mutex_t *mutex);
int pthread_mutex_timedlock(pthread_mutex_t *mutex, const struct timespec *abstime);
int pthread_mutex_unlock(pthread_mutex_t *mutex);

// Condition variables
int pthread_cond_init(pthread_cond_t *cond, const pthread_condattr_t *attr);
int pthread_cond_destroy(pthread_cond_t *cond);
int pthread_cond_wait(pthread_cond_t *cond, pthread_mutex_t *mutex);
int pthread_cond_timedwait(pthread_cond_t *cond, pthread_mutex_t *mutex,
                           const struct timespec *abstime);
int pthread_cond_signal(pthread_cond_t *cond);
int pthread_cond_broadcast(pthread_cond_t *cond);

// Read-write locks
int pthread_rwlock_init(pthread_rwlock_t *rwlock, const pthread_rwlockattr_t *attr);
int pthread_rwlock_destroy(pthread_rwlock_t *rwlock);
int pthread_rwlock_rdlock(pthread_rwlock_t *rwlock);
int pthread_rwlock_wrlock(pthread_rwlock_t *rwlock);
int pthread_rwlock_tryrdlock(pthread_rwlock_t *rwlock);
int pthread_rwlock_trywrlock(pthread_rwlock_t *rwlock);
int pthread_rwlock_unlock(pthread_rwlock_t *rwlock);

// Thread-local storage
int pthread_key_create(pthread_key_t *key, void (*destructor)(void*));
int pthread_key_delete(pthread_key_t key);
void *pthread_getspecific(pthread_key_t key);
int pthread_setspecific(pthread_key_t key, const void *value);

// Once
int pthread_once(pthread_once_t *once_control, void (*init_routine)(void));

// Barriers
int pthread_barrier_init(pthread_barrier_t *barrier,
                         const pthread_barrierattr_t *attr, unsigned count);
int pthread_barrier_destroy(pthread_barrier_t *barrier);
int pthread_barrier_wait(pthread_barrier_t *barrier);

// Spinlocks
int pthread_spin_init(pthread_spinlock_t *lock, int pshared);
int pthread_spin_destroy(pthread_spinlock_t *lock);
int pthread_spin_lock(pthread_spinlock_t *lock);
int pthread_spin_trylock(pthread_spinlock_t *lock);
int pthread_spin_unlock(pthread_spinlock_t *lock);
```

### 2.10 sys/socket.h — Sockets

```c
int socket(int domain, int type, int protocol);
int bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen);
int listen(int sockfd, int backlog);
int accept(int sockfd, struct sockaddr *addr, socklen_t *addrlen);
int accept4(int sockfd, struct sockaddr *addr, socklen_t *addrlen, int flags);
int connect(int sockfd, const struct sockaddr *addr, socklen_t addrlen);
ssize_t send(int sockfd, const void *buf, size_t len, int flags);
ssize_t sendto(int sockfd, const void *buf, size_t len, int flags,
               const struct sockaddr *dest_addr, socklen_t addrlen);
ssize_t sendmsg(int sockfd, const struct msghdr *msg, int flags);
ssize_t recv(int sockfd, void *buf, size_t len, int flags);
ssize_t recvfrom(int sockfd, void *buf, size_t len, int flags,
                 struct sockaddr *src_addr, socklen_t *addrlen);
ssize_t recvmsg(int sockfd, struct msghdr *msg, int flags);
int shutdown(int sockfd, int how);
int getsockopt(int sockfd, int level, int optname, void *optval, socklen_t *optlen);
int setsockopt(int sockfd, int level, int optname, const void *optval, socklen_t optlen);
int getsockname(int sockfd, struct sockaddr *addr, socklen_t *addrlen);
int getpeername(int sockfd, struct sockaddr *addr, socklen_t *addrlen);
int socketpair(int domain, int type, int protocol, int sv[2]);

// Address families
#define AF_UNIX     1
#define AF_LOCAL    AF_UNIX
#define AF_INET     2
#define AF_INET6    10

// Socket types
#define SOCK_STREAM    1
#define SOCK_DGRAM     2
#define SOCK_RAW       3
#define SOCK_SEQPACKET 5
#define SOCK_NONBLOCK  0x800
#define SOCK_CLOEXEC   0x80000
```

### 2.11 sys/mman.h — Memory Mapping

```c
void *mmap(void *addr, size_t length, int prot, int flags, int fd, off_t offset);
int munmap(void *addr, size_t length);
int mprotect(void *addr, size_t len, int prot);
int msync(void *addr, size_t length, int flags);
int mlock(const void *addr, size_t len);
int munlock(const void *addr, size_t len);
int mlockall(int flags);
int munlockall(void);
int madvise(void *addr, size_t length, int advice);
void *mremap(void *old_address, size_t old_size, size_t new_size, int flags, ...);

// Protection flags
#define PROT_NONE   0x0
#define PROT_READ   0x1
#define PROT_WRITE  0x2
#define PROT_EXEC   0x4

// Map flags
#define MAP_SHARED     0x01
#define MAP_PRIVATE    0x02
#define MAP_FIXED      0x10
#define MAP_ANONYMOUS  0x20
#define MAP_ANON       MAP_ANONYMOUS
```

### 2.12 poll.h / sys/epoll.h — I/O Multiplexing

```c
// poll
int poll(struct pollfd *fds, nfds_t nfds, int timeout);
int ppoll(struct pollfd *fds, nfds_t nfds, const struct timespec *tmo_p,
          const sigset_t *sigmask);

struct pollfd {
    int   fd;
    short events;
    short revents;
};

#define POLLIN     0x001
#define POLLOUT    0x004
#define POLLERR    0x008
#define POLLHUP    0x010
#define POLLNVAL   0x020

// select
int select(int nfds, fd_set *readfds, fd_set *writefds, fd_set *exceptfds,
           struct timeval *timeout);
int pselect(int nfds, fd_set *readfds, fd_set *writefds, fd_set *exceptfds,
            const struct timespec *timeout, const sigset_t *sigmask);

// epoll (EFFLUX equivalent)
int epoll_create(int size);
int epoll_create1(int flags);
int epoll_ctl(int epfd, int op, int fd, struct epoll_event *event);
int epoll_wait(int epfd, struct epoll_event *events, int maxevents, int timeout);
int epoll_pwait(int epfd, struct epoll_event *events, int maxevents, int timeout,
                const sigset_t *sigmask);

struct epoll_event {
    uint32_t events;
    epoll_data_t data;
};

#define EPOLLIN     0x001
#define EPOLLOUT    0x004
#define EPOLLERR    0x008
#define EPOLLHUP    0x010
#define EPOLLET     (1u << 31)
#define EPOLLONESHOT (1u << 30)

#define EPOLL_CTL_ADD 1
#define EPOLL_CTL_DEL 2
#define EPOLL_CTL_MOD 3
```

### 2.13 time.h — Time Functions

```c
time_t time(time_t *tloc);
int clock_gettime(clockid_t clk_id, struct timespec *tp);
int clock_settime(clockid_t clk_id, const struct timespec *tp);
int clock_getres(clockid_t clk_id, struct timespec *res);
int gettimeofday(struct timeval *tv, struct timezone *tz);
int settimeofday(const struct timeval *tv, const struct timezone *tz);
int nanosleep(const struct timespec *req, struct timespec *rem);
int clock_nanosleep(clockid_t clock_id, int flags, const struct timespec *request,
                    struct timespec *remain);

struct tm *localtime(const time_t *timep);
struct tm *localtime_r(const time_t *timep, struct tm *result);
struct tm *gmtime(const time_t *timep);
struct tm *gmtime_r(const time_t *timep, struct tm *result);
time_t mktime(struct tm *tm);
time_t timegm(struct tm *tm);
char *asctime(const struct tm *tm);
char *asctime_r(const struct tm *tm, char *buf);
char *ctime(const time_t *timep);
char *ctime_r(const time_t *timep, char *buf);
size_t strftime(char *s, size_t max, const char *format, const struct tm *tm);
char *strptime(const char *s, const char *format, struct tm *tm);
double difftime(time_t time1, time_t time0);

#define CLOCK_REALTIME           0
#define CLOCK_MONOTONIC          1
#define CLOCK_PROCESS_CPUTIME_ID 2
#define CLOCK_THREAD_CPUTIME_ID  3
#define CLOCK_BOOTTIME           7
```

### 2.14 dirent.h — Directory Operations

```c
DIR *opendir(const char *name);
DIR *fdopendir(int fd);
int closedir(DIR *dirp);
struct dirent *readdir(DIR *dirp);
int readdir_r(DIR *dirp, struct dirent *entry, struct dirent **result);
void rewinddir(DIR *dirp);
void seekdir(DIR *dirp, long loc);
long telldir(DIR *dirp);
int dirfd(DIR *dirp);
int scandir(const char *dirp, struct dirent ***namelist,
            int (*filter)(const struct dirent *),
            int (*compar)(const struct dirent **, const struct dirent **));

struct dirent {
    ino_t          d_ino;
    off_t          d_off;
    unsigned short d_reclen;
    unsigned char  d_type;
    char           d_name[256];
};

#define DT_UNKNOWN 0
#define DT_FIFO    1
#define DT_CHR     2
#define DT_DIR     4
#define DT_BLK     6
#define DT_REG     8
#define DT_LNK     10
#define DT_SOCK    12
```

### 2.15 termios.h — Terminal I/O

```c
int tcgetattr(int fd, struct termios *termios_p);
int tcsetattr(int fd, int optional_actions, const struct termios *termios_p);
int tcsendbreak(int fd, int duration);
int tcdrain(int fd);
int tcflush(int fd, int queue_selector);
int tcflow(int fd, int action);
void cfmakeraw(struct termios *termios_p);
speed_t cfgetispeed(const struct termios *termios_p);
speed_t cfgetospeed(const struct termios *termios_p);
int cfsetispeed(struct termios *termios_p, speed_t speed);
int cfsetospeed(struct termios *termios_p, speed_t speed);
int cfsetspeed(struct termios *termios_p, speed_t speed);

struct termios {
    tcflag_t c_iflag;
    tcflag_t c_oflag;
    tcflag_t c_cflag;
    tcflag_t c_lflag;
    cc_t     c_cc[NCCS];
    speed_t  c_ispeed;
    speed_t  c_ospeed;
};
```

### 2.16 errno.h — Error Numbers

```c
extern int errno;

#define EPERM           1
#define ENOENT          2
#define ESRCH           3
#define EINTR           4
#define EIO             5
#define ENXIO           6
#define E2BIG           7
#define ENOEXEC         8
#define EBADF           9
#define ECHILD         10
#define EAGAIN         11
#define ENOMEM         12
#define EACCES         13
#define EFAULT         14
#define EBUSY          16
#define EEXIST         17
#define EXDEV          18
#define ENODEV         19
#define ENOTDIR        20
#define EISDIR         21
#define EINVAL         22
#define ENFILE         23
#define EMFILE         24
#define ENOTTY         25
#define EFBIG          27
#define ENOSPC         28
#define ESPIPE         29
#define EROFS          30
#define EMLINK         31
#define EPIPE          32
#define EDOM           33
#define ERANGE         34
#define EDEADLK        35
#define ENAMETOOLONG   36
#define ENOLCK         37
#define ENOSYS         38
#define ENOTEMPTY      39
#define ELOOP          40
#define EWOULDBLOCK    EAGAIN
#define ENOMSG         42
#define ENOTSOCK       88
#define ECONNREFUSED  111
#define ETIMEDOUT     110
// ... (full list in implementation)
```

---

## 3) Linux Extensions Supported

| Extension | Header | Notes |
|-----------|--------|-------|
| epoll | sys/epoll.h | Async I/O multiplexing |
| eventfd | sys/eventfd.h | Event notification |
| signalfd | sys/signalfd.h | Signal via fd |
| timerfd | sys/timerfd.h | Timer via fd |
| inotify | sys/inotify.h | File watching |
| sendfile | sys/sendfile.h | Zero-copy transfer |
| splice | fcntl.h | Pipe zero-copy |
| getrandom | sys/random.h | Random bytes |
| memfd_create | sys/mman.h | Anonymous file |
| copy_file_range | unistd.h | File copy |

---

## 4) Implementation Notes

### 4.1 Thread-Safety

All functions are thread-safe unless noted. errno is thread-local.

### 4.2 Signal-Safety

Async-signal-safe functions marked per POSIX. Minimal set for signal handlers.

### 4.3 Locale

Basic C locale by default. UTF-8 assumed for multibyte.

### 4.4 Large File Support

64-bit off_t by default. No _FILE_OFFSET_BITS needed.

---

## 5) Exit Criteria

- [ ] All listed functions implemented
- [ ] Header compatibility with common programs
- [ ] Compiles busybox, coreutils
- [ ] Compiles Python 3.x
- [ ] Passes basic POSIX test suite

---

*End of EFFLUX libc Specification*
