#ifndef _SCHED_H
#define _SCHED_H

#include <sys/types.h>

#define SCHED_OTHER  0
#define SCHED_FIFO   1
#define SCHED_RR     2
#define SCHED_BATCH  3

struct sched_param {
    int sched_priority;
};

typedef struct {
    unsigned long __bits[1024 / (8 * sizeof(unsigned long))];
} cpu_set_t;

#define CPU_ZERO(set) do { \
    unsigned long *__p = (set)->__bits; \
    for (int __i = 0; __i < (int)(sizeof((set)->__bits)/sizeof(unsigned long)); __i++) \
        __p[__i] = 0; \
} while(0)
#define CPU_SET(cpu, set) ((set)->__bits[(cpu) / (8 * sizeof(unsigned long))] |= (1UL << ((cpu) % (8 * sizeof(unsigned long)))))
#define CPU_CLR(cpu, set) ((set)->__bits[(cpu) / (8 * sizeof(unsigned long))] &= ~(1UL << ((cpu) % (8 * sizeof(unsigned long)))))
#define CPU_ISSET(cpu, set) (((set)->__bits[(cpu) / (8 * sizeof(unsigned long))] >> ((cpu) % (8 * sizeof(unsigned long)))) & 1)

int sched_yield(void);
int sched_get_priority_max(int policy);
int sched_get_priority_min(int policy);
int sched_setscheduler(pid_t pid, int policy, const struct sched_param *param);
int sched_getscheduler(pid_t pid);
int sched_setparam(pid_t pid, const struct sched_param *param);
int sched_getparam(pid_t pid, struct sched_param *param);
int sched_rr_get_interval(pid_t pid, struct timespec *tp);
int sched_setaffinity(pid_t pid, size_t cpusetsize, const cpu_set_t *mask);
int sched_getaffinity(pid_t pid, size_t cpusetsize, cpu_set_t *mask);

#endif /* _SCHED_H */
