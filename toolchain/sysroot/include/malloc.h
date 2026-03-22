/* malloc.h — compatibility shim for OXIDE OS
 * — PulseForge: Linux puts some extra decls in malloc.h that aren't in stdlib.h.
 * Most code only needs malloc/free/realloc from stdlib.h. This header just
 * includes stdlib.h and adds the few extras that packages check for.
 */
#ifndef _MALLOC_H
#define _MALLOC_H

#include <stdlib.h>

#ifdef __cplusplus
extern "C" {
#endif

/* POSIX aligned allocation */
void *memalign(size_t alignment, size_t size);
void *posix_memalign_wrapper(void **memptr, size_t alignment, size_t size);

#ifdef __cplusplus
}
#endif

#endif /* _MALLOC_H */
