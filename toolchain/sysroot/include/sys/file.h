/* OXIDE OS File Locking */

#ifndef _SYS_FILE_H
#define _SYS_FILE_H

#include <fcntl.h>

#ifndef LOCK_SH
#define LOCK_SH 1
#define LOCK_EX 2
#define LOCK_NB 4
#define LOCK_UN 8
#endif

int flock(int fd, int operation);

#endif /* _SYS_FILE_H */
