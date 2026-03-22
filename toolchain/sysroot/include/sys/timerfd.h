/* OXIDE OS sys/timerfd.h — timer file descriptors */

#ifndef _SYS_TIMERFD_H
#define _SYS_TIMERFD_H

#include <time.h>

#ifdef __cplusplus
extern "C" {
#endif

#define TFD_CLOEXEC     02000000
#define TFD_NONBLOCK    00004000
#define TFD_TIMER_ABSTIME   (1 << 0)
#define TFD_TIMER_CANCEL_ON_SET (1 << 1)

int timerfd_create(int clockid, int flags);
int timerfd_settime(int fd, int flags, const struct itimerspec *new_value, struct itimerspec *old_value);
int timerfd_gettime(int fd, struct itimerspec *curr_value);

#ifdef __cplusplus
}
#endif

#endif /* _SYS_TIMERFD_H */
