/* OXIDE OS Standard Definitions */

#ifndef _STDDEF_H
#define _STDDEF_H

/* NULL pointer */
#ifndef NULL
#ifdef __cplusplus
#define NULL 0
#else
#define NULL ((void *)0)
#endif
#endif

/* size_t - unsigned integer type for sizeof */
#ifdef __x86_64__
typedef unsigned long size_t;
#else
typedef unsigned int size_t;
#endif

/* ptrdiff_t - signed integer type for pointer arithmetic */
#ifdef __x86_64__
typedef long ptrdiff_t;
#else
typedef int ptrdiff_t;
#endif

/* wchar_t - wide character type */
#ifndef __cplusplus
typedef unsigned int wchar_t;
#endif

/* offsetof - offset of member in structure */
#define offsetof(type, member) __builtin_offsetof(type, member)

#endif /* _STDDEF_H */
