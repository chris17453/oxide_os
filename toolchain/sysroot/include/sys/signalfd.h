/* OXIDE OS sys/signalfd.h — signal file descriptors
 * — ShadePacket: signalfd creates a file descriptor for signal delivery.
 * Wayland uses this for its event loop.
 */

#ifndef _SYS_SIGNALFD_H
#define _SYS_SIGNALFD_H

#include <signal.h>

#ifdef __cplusplus
extern "C" {
#endif

/* signalfd flags */
#define SFD_CLOEXEC     02000000
#define SFD_NONBLOCK    00004000

struct signalfd_siginfo {
    unsigned int ssi_signo;
    int ssi_errno;
    int ssi_code;
    unsigned int ssi_pid;
    unsigned int ssi_uid;
    int ssi_fd;
    unsigned int ssi_tid;
    unsigned int ssi_band;
    unsigned int ssi_overrun;
    unsigned int ssi_trapno;
    int ssi_status;
    int ssi_int;
    unsigned long long ssi_ptr;
    unsigned long long ssi_utime;
    unsigned long long ssi_stime;
    unsigned long long ssi_addr;
    unsigned char __pad[48];
};

int signalfd(int fd, const sigset_t *mask, int flags);

#ifdef __cplusplus
}
#endif

#endif /* _SYS_SIGNALFD_H */
