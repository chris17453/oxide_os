/* OXIDE OS sys/select.h stub - minimal select support */

#ifndef _SYS_SELECT_H
#define _SYS_SELECT_H

#ifdef __cplusplus
extern "C" {
#endif

#include <sys/time.h>

/* Maximum number of file descriptors in fd_set */
#define FD_SETSIZE 1024

/* fd_set type */
typedef struct {
    unsigned long fds_bits[FD_SETSIZE / (8 * sizeof(unsigned long))];
} fd_set;

/* FD_SET macros */
#define __NFDBITS (8 * sizeof(unsigned long))
#define __FD_ELT(d) ((d) / __NFDBITS)
#define __FD_MASK(d) (1UL << ((d) % __NFDBITS))

#define FD_ZERO(set) \
    do { \
        unsigned int __i; \
        fd_set *__arr = (set); \
        for (__i = 0; __i < sizeof(fd_set) / sizeof(unsigned long); __i++) \
            __arr->fds_bits[__i] = 0; \
    } while (0)

#define FD_SET(d, set) \
    ((set)->fds_bits[__FD_ELT(d)] |= __FD_MASK(d))

#define FD_CLR(d, set) \
    ((set)->fds_bits[__FD_ELT(d)] &= ~__FD_MASK(d))

#define FD_ISSET(d, set) \
    (((set)->fds_bits[__FD_ELT(d)] & __FD_MASK(d)) != 0)

/* select() - stub, not implemented */
int select(int nfds, fd_set *readfds, fd_set *writefds,
           fd_set *exceptfds, struct timeval *timeout);

#ifdef __cplusplus
}
#endif

#endif /* _SYS_SELECT_H */
