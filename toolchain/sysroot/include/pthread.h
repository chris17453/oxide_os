#ifndef _PTHREAD_H
#define _PTHREAD_H

#ifdef __cplusplus
extern "C" {
#endif

/* Stub pthread types for CPython compilation
 * These are not actually implemented - Python is configured without threading
 * But the headers still reference these types, so we define them as stubs
 */

typedef struct { int __stub; } pthread_t;
typedef struct { int __stub; } pthread_attr_t;
typedef struct { int __stub; } pthread_mutex_t;
typedef struct { int __stub; } pthread_mutexattr_t;
typedef struct { int __stub; } pthread_cond_t;
typedef struct { int __stub; } pthread_condattr_t;
typedef struct { int __stub; } pthread_rwlock_t;
typedef struct { int __stub; } pthread_rwlockattr_t;
typedef struct { int __stub; } pthread_key_t;
typedef struct { int __stub; } pthread_once_t;

/* Stub macros */
#define PTHREAD_MUTEX_INITIALIZER { 0 }
#define PTHREAD_COND_INITIALIZER { 0 }
#define PTHREAD_ONCE_INIT { 0 }

/* Stub functions - these will never be called since threading is disabled */
static inline int pthread_create(pthread_t *thread, const pthread_attr_t *attr,
                                  void *(*start_routine)(void*), void *arg) {
    return -1; /* Not supported */
}

static inline int pthread_join(pthread_t thread, void **retval) {
    return -1;
}

static inline int pthread_mutex_init(pthread_mutex_t *mutex, const pthread_mutexattr_t *attr) {
    return 0; /* Stub */
}

static inline int pthread_mutex_destroy(pthread_mutex_t *mutex) {
    return 0;
}

static inline int pthread_mutex_lock(pthread_mutex_t *mutex) {
    return 0;
}

static inline int pthread_mutex_unlock(pthread_mutex_t *mutex) {
    return 0;
}

static inline int pthread_cond_init(pthread_cond_t *cond, const pthread_condattr_t *attr) {
    return 0;
}

static inline int pthread_cond_destroy(pthread_cond_t *cond) {
    return 0;
}

static inline int pthread_cond_wait(pthread_cond_t *cond, pthread_mutex_t *mutex) {
    return 0;
}

static inline int pthread_cond_signal(pthread_cond_t *cond) {
    return 0;
}

static inline int pthread_cond_broadcast(pthread_cond_t *cond) {
    return 0;
}

#ifdef __cplusplus
}
#endif

#endif /* _PTHREAD_H */
