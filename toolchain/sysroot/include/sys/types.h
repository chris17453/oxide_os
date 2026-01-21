/* OXIDE OS POSIX Types */

#ifndef _SYS_TYPES_H
#define _SYS_TYPES_H

#include <stdint.h>
#include <stddef.h>

/* Process/thread IDs */
typedef int pid_t;
typedef int tid_t;

/* User/group IDs */
typedef unsigned int uid_t;
typedef unsigned int gid_t;

/* File types */
typedef unsigned int mode_t;
typedef unsigned long dev_t;
typedef unsigned long ino_t;
typedef unsigned long nlink_t;
typedef long off_t;
typedef long blksize_t;
typedef long blkcnt_t;

/* Time */
typedef long time_t;
typedef long suseconds_t;
typedef int clockid_t;

/* Size types */
typedef long ssize_t;

/* File descriptor */
typedef int fd_t;

#endif /* _SYS_TYPES_H */
