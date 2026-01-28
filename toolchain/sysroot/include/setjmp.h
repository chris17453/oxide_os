/* OXIDE OS Non-local Jumps */

#ifndef _SETJMP_H
#define _SETJMP_H

/* jmp_buf: saves rbx, rbp, r12-r15, rsp, rip (8 registers * 8 bytes = 64 bytes)
   Plus signal mask storage */
typedef long jmp_buf[8];
typedef long sigjmp_buf[8 + 1 + 16]; /* registers + flag + sigset */

int setjmp(jmp_buf env);
void longjmp(jmp_buf env, int val) __attribute__((noreturn));

int _setjmp(jmp_buf env);
void _longjmp(jmp_buf env, int val) __attribute__((noreturn));

int sigsetjmp(sigjmp_buf env, int savesigs);
void siglongjmp(sigjmp_buf env, int val) __attribute__((noreturn));

#endif /* _SETJMP_H */
