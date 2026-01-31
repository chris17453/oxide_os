/* OXIDE OS Time Functions */

#ifndef _TIME_H
#define _TIME_H

#include <stddef.h>
#include <sys/types.h>

/* Clock IDs */
#define CLOCK_REALTIME              0
#define CLOCK_MONOTONIC             1
#define CLOCK_PROCESS_CPUTIME_ID    2
#define CLOCK_THREAD_CPUTIME_ID     3
#define CLOCK_MONOTONIC_RAW         4
#define CLOCK_REALTIME_COARSE       5
#define CLOCK_MONOTONIC_COARSE      6
#define CLOCK_BOOTTIME              7

/* Timer flags */
#define TIMER_ABSTIME   1

/* Clocks per second */
#define CLOCKS_PER_SEC  1000000L

typedef long clock_t;

struct timespec {
    time_t tv_sec;
    long tv_nsec;
};

struct tm {
    int tm_sec;
    int tm_min;
    int tm_hour;
    int tm_mday;
    int tm_mon;
    int tm_year;
    int tm_wday;
    int tm_yday;
    int tm_isdst;
    long tm_gmtoff;
    const char *tm_zone;
};

/* Time functions */
time_t time(time_t *tloc);
clock_t clock(void);
double difftime(time_t time1, time_t time0);
time_t mktime(struct tm *tm);

/* Conversion functions */
struct tm *gmtime(const time_t *timep);
struct tm *gmtime_r(const time_t *timep, struct tm *result);
struct tm *localtime(const time_t *timep);
struct tm *localtime_r(const time_t *timep, struct tm *result);
char *asctime(const struct tm *tm);
char *asctime_r(const struct tm *tm, char *buf);
char *ctime(const time_t *timep);
char *ctime_r(const time_t *timep, char *buf);
size_t strftime(char *s, size_t max, const char *format, const struct tm *tm);
char *strptime(const char *s, const char *format, struct tm *tm);

/* Clock functions */
int clock_gettime(clockid_t clk_id, struct timespec *tp);
int clock_getres(clockid_t clk_id, struct timespec *res);
int clock_settime(clockid_t clk_id, const struct timespec *tp);

/* Sleep */
int nanosleep(const struct timespec *req, struct timespec *rem);

/* Timezone */
extern long timezone;
extern long altzone;
extern int daylight;
extern char *tzname[2];
void tzset(void);

/* timegm - inverse of gmtime */
time_t timegm(struct tm *tm);

#endif /* _TIME_H */
