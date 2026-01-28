/* OXIDE OS Random Number Generation */

#ifndef _SYS_RANDOM_H
#define _SYS_RANDOM_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>

/* Flags for getrandom() */
#define GRND_NONBLOCK 0x0001
#define GRND_RANDOM   0x0002

/* Get random bytes from kernel */
long getrandom(void *buf, size_t buflen, unsigned int flags);

#ifdef __cplusplus
}
#endif

#endif /* _SYS_RANDOM_H */
