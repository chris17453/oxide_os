#ifndef _SYS_SYSCALL_H
#define _SYS_SYSCALL_H

/* Minimal syscall numbers for OXIDE OS - x86_64 */
#define SYS_gettid 186
#define SYS_getrandom 318
#define __NR_gettid SYS_gettid
#define __NR_getrandom SYS_getrandom

long syscall(long number, ...);

#endif
