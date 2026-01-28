/* OXIDE OS Assertions */

#ifndef _ASSERT_H
#define _ASSERT_H

#ifdef NDEBUG
#define assert(expr) ((void)0)
#else

void __assert_fail(const char *expr, const char *file, int line, const char *func)
    __attribute__((noreturn));

#define assert(expr) \
    ((expr) ? (void)0 : __assert_fail(#expr, __FILE__, __LINE__, __func__))

#endif /* NDEBUG */

/* C11 static_assert */
#ifndef __cplusplus
#define static_assert _Static_assert
#endif

#endif /* _ASSERT_H */
