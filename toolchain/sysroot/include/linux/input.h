/* OXIDE OS linux/input.h — Input subsystem definitions */

#ifndef _LINUX_INPUT_H
#define _LINUX_INPUT_H

#include <sys/types.h>
#include <linux/input-event-codes.h>

#ifdef __cplusplus
extern "C" {
#endif

struct input_event {
    struct timeval time;
    unsigned short type;
    unsigned short code;
    int value;
};

struct input_id {
    unsigned short bustype;
    unsigned short vendor;
    unsigned short product;
    unsigned short version;
};

struct input_absinfo {
    int value;
    int minimum;
    int maximum;
    int fuzz;
    int flat;
    int resolution;
};

#define EVIOCGNAME(len)     (0x80000000 | ((len) << 16) | 'E' << 8 | 0x06)
#define EVIOCGID            (0x80000000 | (sizeof(struct input_id) << 16) | 'E' << 8 | 0x02)

#ifdef __cplusplus
}
#endif

#endif /* _LINUX_INPUT_H */
