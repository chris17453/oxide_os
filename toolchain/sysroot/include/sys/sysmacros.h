#ifndef _SYS_SYSMACROS_H
#define _SYS_SYSMACROS_H

#define major(dev) ((unsigned int)(((dev) >> 8) & 0xff))
#define minor(dev) ((unsigned int)((dev) & 0xff))
#define makedev(maj, min) ((unsigned int)(((maj) << 8) | (min)))

unsigned int gnu_dev_major(unsigned long long dev);
unsigned int gnu_dev_minor(unsigned long long dev);
unsigned long long gnu_dev_makedev(unsigned int maj, unsigned int min);

#endif /* _SYS_SYSMACROS_H */
