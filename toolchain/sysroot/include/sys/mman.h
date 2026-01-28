/* OXIDE OS Memory Mapping */

#ifndef _SYS_MMAN_H
#define _SYS_MMAN_H

#include <stddef.h>
#include <sys/types.h>

/* Protection flags */
#define PROT_NONE   0x0
#define PROT_READ   0x1
#define PROT_WRITE  0x2
#define PROT_EXEC   0x4

/* Map flags */
#define MAP_SHARED      0x01
#define MAP_PRIVATE     0x02
#define MAP_FIXED       0x10
#define MAP_ANONYMOUS   0x20
#define MAP_ANON        MAP_ANONYMOUS
#define MAP_NORESERVE   0x4000
#define MAP_POPULATE    0x8000
#define MAP_HUGETLB     0x40000

/* MAP_FAILED */
#define MAP_FAILED ((void *)-1)

/* msync flags */
#define MS_ASYNC    1
#define MS_SYNC     4
#define MS_INVALIDATE 2

/* madvise advice */
#define MADV_NORMAL     0
#define MADV_RANDOM     1
#define MADV_SEQUENTIAL 2
#define MADV_WILLNEED   3
#define MADV_DONTNEED   4
#define MADV_FREE       8

/* mremap flags */
#define MREMAP_MAYMOVE  1
#define MREMAP_FIXED    2

/* mlock flags */
#define MCL_CURRENT     1
#define MCL_FUTURE      2

void *mmap(void *addr, size_t length, int prot, int flags, int fd, off_t offset);
int munmap(void *addr, size_t length);
int mprotect(void *addr, size_t len, int prot);
int msync(void *addr, size_t length, int flags);
int madvise(void *addr, size_t length, int advice);
int mlock(const void *addr, size_t len);
int munlock(const void *addr, size_t len);
int mlockall(int flags);
int munlockall(void);
void *mremap(void *old_address, size_t old_size, size_t new_size, int flags, ...);
int mincore(void *addr, size_t length, unsigned char *vec);
int shm_open(const char *name, int oflag, mode_t mode);
int shm_unlink(const char *name);
int posix_madvise(void *addr, size_t len, int advice);

#endif /* _SYS_MMAN_H */
