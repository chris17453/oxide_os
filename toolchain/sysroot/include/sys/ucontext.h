#ifndef _SYS_UCONTEXT_H
#define _SYS_UCONTEXT_H
typedef struct { long gregs[23]; } mcontext_t;
typedef struct ucontext_t { mcontext_t uc_mcontext; struct ucontext_t *uc_link; } ucontext_t;
#endif
