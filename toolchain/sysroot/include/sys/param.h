/* OXIDE OS sys/param.h - System parameters and limits */

#ifndef _SYS_PARAM_H
#define _SYS_PARAM_H

#ifdef __cplusplus
extern "C" {
#endif

#include <limits.h>
#include <sys/types.h>

/* Path length limits */
#ifndef MAXPATHLEN
#define MAXPATHLEN  4096
#endif

#ifndef MAXSYMLINKS
#define MAXSYMLINKS 20
#endif

/* Hostname length */
#ifndef MAXHOSTNAMELEN
#define MAXHOSTNAMELEN 256
#endif

/* Number of file descriptors */
#ifndef NOFILE
#define NOFILE 256
#endif

/* Bits per byte */
#ifndef NBBY
#define NBBY 8
#endif

/* Page size macros */
#ifndef PAGE_SIZE
#define PAGE_SIZE 4096
#endif

#ifndef PAGE_SHIFT
#define PAGE_SHIFT 12
#endif

/* MIN/MAX macros */
#ifndef MIN
#define MIN(a,b) (((a)<(b))?(a):(b))
#endif

#ifndef MAX
#define MAX(a,b) (((a)>(b))?(a):(b))
#endif

/* Byte order */
#define LITTLE_ENDIAN 1234
#define BIG_ENDIAN    4321
#define BYTE_ORDER    LITTLE_ENDIAN

/* Rounding macros */
#define roundup(x, y)   ((((x)+((y)-1))/(y))*(y))
#define rounddown(x, y) (((x)/(y))*(y))

#ifdef __cplusplus
}
#endif

#endif /* _SYS_PARAM_H */
