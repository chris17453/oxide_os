/* OXIDE OS POSIX Threads (stub) */

#ifndef _PTHREAD_H
#define _PTHREAD_H

#include <stddef.h>
#include <sys/types.h>
#include <time.h>

typedef unsigned long pthread_t;
typedef int pthread_key_t;
typedef int pthread_once_t;

#define PTHREAD_ONCE_INIT 0

/* Mutex types */
#define PTHREAD_MUTEX_NORMAL     0
#define PTHREAD_MUTEX_RECURSIVE  1
#define PTHREAD_MUTEX_ERRORCHECK 2
#define PTHREAD_MUTEX_DEFAULT    PTHREAD_MUTEX_NORMAL

/* Mutex initializers */
#define PTHREAD_MUTEX_INITIALIZER { 0 }
#define PTHREAD_RECURSIVE_MUTEX_INITIALIZER_NP { { 0, 0, 0, PTHREAD_MUTEX_RECURSIVE, 0, { 0 } } }
#define PTHREAD_COND_INITIALIZER { 0 }
#define PTHREAD_RWLOCK_INITIALIZER { 0 }

/* Process shared */
#define PTHREAD_PROCESS_PRIVATE 0
#define PTHREAD_PROCESS_SHARED  1

/* Detach state */
#define PTHREAD_CREATE_JOINABLE 0
#define PTHREAD_CREATE_DETACHED 1

/* Scope */
#define PTHREAD_SCOPE_SYSTEM    0
#define PTHREAD_SCOPE_PROCESS   1

/* Cancel */
#define PTHREAD_CANCEL_ENABLE   0
#define PTHREAD_CANCEL_DISABLE  1
#define PTHREAD_CANCEL_DEFERRED     0
#define PTHREAD_CANCEL_ASYNCHRONOUS 1
#define PTHREAD_CANCELED ((void *)-1)

/* Attribute types */
typedef struct { int __dummy; } pthread_mutexattr_t;
typedef struct { int __dummy; } pthread_condattr_t;
typedef struct { int __dummy; } pthread_rwlockattr_t;
typedef struct { int __dummy; } pthread_attr_t;

/* Mutex */
typedef struct {
    int __lock;
    unsigned int __count;
    int __owner;
    int __kind;
    unsigned int __nusers;
    union { int __dummy; } __data;
} pthread_mutex_t;

/* Condition variable */
typedef struct {
    int __dummy;
} pthread_cond_t;

/* Read-write lock */
typedef struct {
    int __dummy;
} pthread_rwlock_t;

/* Thread functions */
int pthread_create(pthread_t *thread, const pthread_attr_t *attr,
                   void *(*start_routine)(void *), void *arg);
int pthread_join(pthread_t thread, void **retval);
int pthread_detach(pthread_t thread);
pthread_t pthread_self(void);
int pthread_equal(pthread_t t1, pthread_t t2);
void pthread_exit(void *retval) __attribute__((noreturn));
int pthread_cancel(pthread_t thread);
int pthread_setcancelstate(int state, int *oldstate);
int pthread_setcanceltype(int type, int *oldtype);
void pthread_testcancel(void);
int pthread_sigmask(int how, const void *set, void *oldset);

/* Thread attributes */
int pthread_attr_init(pthread_attr_t *attr);
int pthread_attr_destroy(pthread_attr_t *attr);
int pthread_attr_setdetachstate(pthread_attr_t *attr, int detachstate);
int pthread_attr_getdetachstate(const pthread_attr_t *attr, int *detachstate);
int pthread_attr_setstacksize(pthread_attr_t *attr, size_t stacksize);
int pthread_attr_getstacksize(const pthread_attr_t *attr, size_t *stacksize);
int pthread_attr_setscope(pthread_attr_t *attr, int scope);
int pthread_attr_getscope(const pthread_attr_t *attr, int *scope);

/* Mutex functions */
int pthread_mutex_init(pthread_mutex_t *mutex, const pthread_mutexattr_t *attr);
int pthread_mutex_destroy(pthread_mutex_t *mutex);
int pthread_mutex_lock(pthread_mutex_t *mutex);
int pthread_mutex_trylock(pthread_mutex_t *mutex);
int pthread_mutex_unlock(pthread_mutex_t *mutex);
int pthread_mutex_timedlock(pthread_mutex_t *mutex, const struct timespec *abstime);

/* Mutex attributes */
int pthread_mutexattr_init(pthread_mutexattr_t *attr);
int pthread_mutexattr_destroy(pthread_mutexattr_t *attr);
int pthread_mutexattr_settype(pthread_mutexattr_t *attr, int type);
int pthread_mutexattr_gettype(const pthread_mutexattr_t *attr, int *type);
int pthread_mutexattr_setpshared(pthread_mutexattr_t *attr, int pshared);
int pthread_mutexattr_getpshared(const pthread_mutexattr_t *attr, int *pshared);

/* Condition variable functions */
int pthread_cond_init(pthread_cond_t *cond, const pthread_condattr_t *attr);
int pthread_cond_destroy(pthread_cond_t *cond);
int pthread_cond_signal(pthread_cond_t *cond);
int pthread_cond_broadcast(pthread_cond_t *cond);
int pthread_cond_wait(pthread_cond_t *cond, pthread_mutex_t *mutex);
int pthread_cond_timedwait(pthread_cond_t *cond, pthread_mutex_t *mutex,
                           const struct timespec *abstime);

/* Condition variable attributes */
int pthread_condattr_init(pthread_condattr_t *attr);
int pthread_condattr_destroy(pthread_condattr_t *attr);
int pthread_condattr_setpshared(pthread_condattr_t *attr, int pshared);
int pthread_condattr_getpshared(const pthread_condattr_t *attr, int *pshared);
int pthread_condattr_setclock(pthread_condattr_t *attr, clockid_t clock_id);
int pthread_condattr_getclock(const pthread_condattr_t *attr, clockid_t *clock_id);

/* Read-write lock functions */
int pthread_rwlock_init(pthread_rwlock_t *rwlock, const pthread_rwlockattr_t *attr);
int pthread_rwlock_destroy(pthread_rwlock_t *rwlock);
int pthread_rwlock_rdlock(pthread_rwlock_t *rwlock);
int pthread_rwlock_wrlock(pthread_rwlock_t *rwlock);
int pthread_rwlock_tryrdlock(pthread_rwlock_t *rwlock);
int pthread_rwlock_trywrlock(pthread_rwlock_t *rwlock);
int pthread_rwlock_unlock(pthread_rwlock_t *rwlock);

/* Thread-specific data */
int pthread_key_create(pthread_key_t *key, void (*destructor)(void *));
int pthread_key_delete(pthread_key_t key);
void *pthread_getspecific(pthread_key_t key);
int pthread_setspecific(pthread_key_t key, const void *value);

/* Once */
int pthread_once(pthread_once_t *once_control, void (*init_routine)(void));

/* Cleanup */
void pthread_cleanup_push(void (*routine)(void *), void *arg);
void pthread_cleanup_pop(int execute);

/* Scheduling */
int pthread_setschedparam(pthread_t thread, int policy, const void *param);
int pthread_getschedparam(pthread_t thread, int *policy, void *param);

/* Misc */
int pthread_atfork(void (*prepare)(void), void (*parent)(void), void (*child)(void));

#endif /* _PTHREAD_H */
