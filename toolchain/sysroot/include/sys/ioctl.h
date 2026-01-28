/* OXIDE OS I/O Control */

#ifndef _SYS_IOCTL_H
#define _SYS_IOCTL_H

/* Terminal size structure */
struct winsize {
    unsigned short ws_row;
    unsigned short ws_col;
    unsigned short ws_xpixel;
    unsigned short ws_ypixel;
};

/* ioctl requests */
#define TIOCGWINSZ  0x5413
#define TIOCSWINSZ  0x5414
#define TIOCGPGRP   0x540F
#define TIOCSPGRP   0x5410
#define TIOCSCTTY   0x540E
#define TIOCNOTTY   0x5422
#define FIONREAD    0x541B
#define FIONBIO     0x5421
#define FIOCLEX     0x5451
#define FIONCLEX    0x5450

int ioctl(int fd, unsigned long request, ...);

#endif /* _SYS_IOCTL_H */
