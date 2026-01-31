#ifndef _SYS_MEMFD_H
#define _SYS_MEMFD_H

#include <linux/memfd.h>

int memfd_create(const char *name, unsigned int flags);

#endif /* _SYS_MEMFD_H */
