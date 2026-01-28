/* OXIDE OS dlfcn.h stub - no dynamic loading support */

#ifndef _DLFCN_H
#define _DLFCN_H

#ifdef __cplusplus
extern "C" {
#endif

/* RTLD constants - stubs for CPython */
#define RTLD_LAZY       1
#define RTLD_NOW        2
#define RTLD_GLOBAL     0x100
#define RTLD_LOCAL      0
#define RTLD_NODELETE   0x1000
#define RTLD_NOLOAD     0x2000
#define RTLD_DEEPBIND   0x8

/* Functions (stubs, will fail if called) */
void *dlopen(const char *filename, int flags);
char *dlerror(void);
void *dlsym(void *handle, const char *symbol);
int dlclose(void *handle);

#ifdef __cplusplus
}
#endif

#endif /* _DLFCN_H */
