/* OXIDE OS File Control */

#ifndef _FCNTL_H
#define _FCNTL_H

#include <sys/types.h>

/* Open flags */
#define O_RDONLY    0x0000
#define O_WRONLY    0x0001
#define O_RDWR      0x0002
#define O_ACCMODE   0x0003

#define O_CREAT     0x0040
#define O_EXCL      0x0080
#define O_NOCTTY    0x0100
#define O_TRUNC     0x0200
#define O_APPEND    0x0400
#define O_NONBLOCK  0x0800
#define O_DSYNC     0x1000
#define O_SYNC      0x101000
#define O_RSYNC     O_SYNC
#define O_DIRECTORY 0x10000
#define O_NOFOLLOW  0x20000
#define O_CLOEXEC   0x80000
#define O_NOATIME   0x40000
#define O_PATH      0x200000
#define O_TMPFILE   0x410000
#define O_LARGEFILE 0

/* fcntl commands */
#define F_DUPFD         0
#define F_GETFD         1
#define F_SETFD         2
#define F_GETFL         3
#define F_SETFL         4
#define F_GETLK         5
#define F_SETLK         6
#define F_SETLKW        7
#define F_SETOWN        8
#define F_GETOWN        9
#define F_DUPFD_CLOEXEC 1030

/* fd flags */
#define FD_CLOEXEC  1

/* File lock types */
#define F_RDLCK     0
#define F_WRLCK     1
#define F_UNLCK     2

/* flock structure */
struct flock {
    short l_type;
    short l_whence;
    off_t l_start;
    off_t l_len;
    pid_t l_pid;
};

/* Advisory locking */
#define LOCK_SH     1
#define LOCK_EX     2
#define LOCK_NB     4
#define LOCK_UN     8

/* File creation mode */
#define S_IRWXU     0700
#define S_IRUSR     0400
#define S_IWUSR     0200
#define S_IXUSR     0100
#define S_IRWXG     0070
#define S_IRGRP     0040
#define S_IWGRP     0020
#define S_IXGRP     0010
#define S_IRWXO     0007
#define S_IROTH     0004
#define S_IWOTH     0002
#define S_IXOTH     0001
#define S_ISUID     04000
#define S_ISGID     02000
#define S_ISVTX     01000

/* at flags */
#define AT_FDCWD            (-100)
#define AT_SYMLINK_NOFOLLOW 0x100
#define AT_REMOVEDIR        0x200
#define AT_SYMLINK_FOLLOW   0x400
#define AT_EACCESS          0x200
#define AT_EMPTY_PATH       0x1000

/* Splice flags */
#define SPLICE_F_MOVE       1
#define SPLICE_F_NONBLOCK   2
#define SPLICE_F_MORE       4
#define SPLICE_F_GIFT       8

/* posix_fadvise advice values */
#define POSIX_FADV_NORMAL     0
#define POSIX_FADV_RANDOM     1
#define POSIX_FADV_SEQUENTIAL 2
#define POSIX_FADV_WILLNEED   3
#define POSIX_FADV_DONTNEED   4
#define POSIX_FADV_NOREUSE    5

/* Functions */
int open(const char *pathname, int flags, ...);
int openat(int dirfd, const char *pathname, int flags, ...);
int creat(const char *pathname, mode_t mode);
int fcntl(int fd, int cmd, ...);
int flock(int fd, int operation);
ssize_t splice(int fd_in, off_t *off_in, int fd_out, off_t *off_out,
               size_t len, unsigned int flags);
int posix_fallocate(int fd, off_t offset, off_t len);
int posix_fadvise(int fd, off_t offset, off_t len, int advice);

#endif /* _FCNTL_H */
