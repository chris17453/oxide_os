/* OXIDE OS sys/times.h - process times */

#ifndef _SYS_TIMES_H
#define _SYS_TIMES_H

#ifdef __cplusplus
extern "C" {
#endif

#include <sys/types.h>

/* Clock ticks type */
typedef long clock_t;

/* Structure describing CPU time used by a process and its children */
struct tms {
    clock_t tms_utime;  /* User CPU time */
    clock_t tms_stime;  /* System CPU time */
    clock_t tms_cutime; /* User CPU time of dead children */
    clock_t tms_cstime; /* System CPU time of dead children */
};

/* Get process times - stub, not implemented */
clock_t times(struct tms *buf);

#ifdef __cplusplus
}
#endif

#endif /* _SYS_TIMES_H */
