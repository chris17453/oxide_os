/* memory.h — legacy header, includes string.h
 * — Hexline: some ancient programs include <memory.h> for memcpy/memset.
 * POSIX says use <string.h>. We just redirect. */
#ifndef _MEMORY_H
#define _MEMORY_H
#include <string.h>
#endif
