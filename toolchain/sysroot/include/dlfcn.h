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
#define RTLD_DEFAULT    ((void *)0)
#define RTLD_NEXT       ((void *)-1)

/* Symbol info (for dladdr) */
typedef struct {
    const char *dli_fname;
    void *dli_fbase;
    const char *dli_sname;
    void *dli_saddr;
} Dl_info;

/* Functions */
void *dlopen(const char *filename, int flags);
char *dlerror(void);
void *dlsym(void *handle, const char *symbol);
int dlclose(void *handle);
int dladdr(const void *addr, Dl_info *info);

#ifdef __cplusplus
}
#endif

#endif /* _DLFCN_H */
