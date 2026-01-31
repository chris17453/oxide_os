/* OXIDE OS POSIX Threads (pthread) API */

#ifndef _PTHREAD_H
#define _PTHREAD_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>
#include <time.h>

/* ===== Types ===== */

/* Thread identifier (u64 in implementation) */
typedef unsigned long pthread_t;

/* Thread attributes */
typedef struct {
    int detachstate;
    size_t stacksize;
    void *stackaddr;
    size_t guardsize;
    int schedpolicy;
    int schedpriority;
} pthread_attr_t;

/* Mutex */
typedef struct {
    unsigned int state;
    unsigned int kind;
    unsigned long owner;
    unsigned int count;
} pthread_mutex_t;

/* Mutex attributes */
typedef struct {
    int kind;
    int pshared;
} pthread_mutexattr_t;

/* Condition variable */
typedef struct {
    unsigned int seq;
    unsigned int waiters;
} pthread_cond_t;

/* Condition variable attributes */
typedef struct {
    int pshared;
    int clock_id;
} pthread_condattr_t;

/* Read-write lock */
typedef struct {
    unsigned int state;
    unsigned long writer;
    unsigned int readers;
} pthread_rwlock_t;

/* Read-write lock attributes */
typedef struct {
    int pshared;
} pthread_rwlockattr_t;

/* Barrier */
typedef struct {
    unsigned int count;
    unsigned int max;
    unsigned int seq;
} pthread_barrier_t;

/* Barrier attributes */
typedef struct {
    int pshared;
} pthread_barrierattr_t;

/* Thread-local storage key */
typedef unsigned int pthread_key_t;

/* One-time initialization */
typedef struct {
    unsigned int done;
} pthread_once_t;

/* ===== Constants ===== */

/* Detach state */
#define PTHREAD_CREATE_JOINABLE 0
#define PTHREAD_CREATE_DETACHED 1

/* Scheduling scope */
#define PTHREAD_SCOPE_SYSTEM  1
#define PTHREAD_SCOPE_PROCESS 2

/* Mutex types */
#define PTHREAD_MUTEX_NORMAL     0
#define PTHREAD_MUTEX_RECURSIVE  1
#define PTHREAD_MUTEX_ERRORCHECK 2
#define PTHREAD_MUTEX_DEFAULT    PTHREAD_MUTEX_NORMAL

/* Process sharing */
#define PTHREAD_PROCESS_PRIVATE 0
#define PTHREAD_PROCESS_SHARED  1

/* Thread cancellation */
#define PTHREAD_CANCEL_ENABLE      0
#define PTHREAD_CANCEL_DISABLE     1
#define PTHREAD_CANCEL_DEFERRED    0
#define PTHREAD_CANCEL_ASYNCHRONOUS 1
#define PTHREAD_CANCELED ((void *)-1)

/* Barrier return value */
#define PTHREAD_BARRIER_SERIAL_THREAD 1

/* Static initializers */
#define PTHREAD_MUTEX_INITIALIZER { 0, 0, 0, 0 }
#define PTHREAD_COND_INITIALIZER { 0, 0 }
#define PTHREAD_RWLOCK_INITIALIZER { 0, 0, 0 }
#define PTHREAD_ONCE_INIT { 0 }

/* ===== Thread Management ===== */

int pthread_create(pthread_t *thread, const pthread_attr_t *attr,
                   void *(*start_routine)(void*), void *arg);
int pthread_join(pthread_t thread, void **retval);
int pthread_detach(pthread_t thread);
void pthread_exit(void *retval) __attribute__((noreturn));
pthread_t pthread_self(void);
int pthread_equal(pthread_t t1, pthread_t t2);

/* ===== Thread Attributes ===== */

int pthread_attr_init(pthread_attr_t *attr);
int pthread_attr_destroy(pthread_attr_t *attr);
int pthread_attr_getdetachstate(const pthread_attr_t *attr, int *detachstate);
int pthread_attr_setdetachstate(pthread_attr_t *attr, int detachstate);
int pthread_attr_getstacksize(const pthread_attr_t *attr, size_t *stacksize);
int pthread_attr_setstacksize(pthread_attr_t *attr, size_t stacksize);
int pthread_attr_getstack(const pthread_attr_t *attr, void **stackaddr, size_t *stacksize);
int pthread_attr_setstack(pthread_attr_t *attr, void *stackaddr, size_t stacksize);
int pthread_attr_getguardsize(const pthread_attr_t *attr, size_t *guardsize);
int pthread_attr_setguardsize(pthread_attr_t *attr, size_t guardsize);
int pthread_attr_getschedpolicy(const pthread_attr_t *attr, int *policy);
int pthread_attr_setschedpolicy(pthread_attr_t *attr, int policy);
int pthread_attr_setscope(pthread_attr_t *attr, int scope);
int pthread_attr_getscope(const pthread_attr_t *attr, int *scope);

/* ===== Mutex Operations ===== */

int pthread_mutex_init(pthread_mutex_t *mutex, const pthread_mutexattr_t *attr);
int pthread_mutex_destroy(pthread_mutex_t *mutex);
int pthread_mutex_lock(pthread_mutex_t *mutex);
int pthread_mutex_trylock(pthread_mutex_t *mutex);
int pthread_mutex_unlock(pthread_mutex_t *mutex);

/* ===== Mutex Attributes ===== */

int pthread_mutexattr_init(pthread_mutexattr_t *attr);
int pthread_mutexattr_destroy(pthread_mutexattr_t *attr);
int pthread_mutexattr_gettype(const pthread_mutexattr_t *attr, int *type);
int pthread_mutexattr_settype(pthread_mutexattr_t *attr, int type);

/* ===== Condition Variables ===== */

int pthread_cond_init(pthread_cond_t *cond, const pthread_condattr_t *attr);
int pthread_cond_destroy(pthread_cond_t *cond);
int pthread_cond_wait(pthread_cond_t *cond, pthread_mutex_t *mutex);
int pthread_cond_timedwait(pthread_cond_t *cond, pthread_mutex_t *mutex,
                           const struct timespec *abstime);
int pthread_cond_signal(pthread_cond_t *cond);
int pthread_cond_broadcast(pthread_cond_t *cond);

/* ===== Condition Variable Attributes ===== */

int pthread_condattr_init(pthread_condattr_t *attr);
int pthread_condattr_destroy(pthread_condattr_t *attr);
int pthread_condattr_getclock(const pthread_condattr_t *attr, int *clock_id);
int pthread_condattr_setclock(pthread_condattr_t *attr, int clock_id);

/* ===== Read-Write Locks ===== */

int pthread_rwlock_init(pthread_rwlock_t *rwlock, const pthread_rwlockattr_t *attr);
int pthread_rwlock_destroy(pthread_rwlock_t *rwlock);
int pthread_rwlock_rdlock(pthread_rwlock_t *rwlock);
int pthread_rwlock_tryrdlock(pthread_rwlock_t *rwlock);
int pthread_rwlock_wrlock(pthread_rwlock_t *rwlock);
int pthread_rwlock_trywrlock(pthread_rwlock_t *rwlock);
int pthread_rwlock_unlock(pthread_rwlock_t *rwlock);

/* ===== Read-Write Lock Attributes ===== */

int pthread_rwlockattr_init(pthread_rwlockattr_t *attr);
int pthread_rwlockattr_destroy(pthread_rwlockattr_t *attr);

/* ===== Barriers ===== */

int pthread_barrier_init(pthread_barrier_t *barrier, const pthread_barrierattr_t *attr,
                         unsigned int count);
int pthread_barrier_destroy(pthread_barrier_t *barrier);
int pthread_barrier_wait(pthread_barrier_t *barrier);

/* ===== Barrier Attributes ===== */

int pthread_barrierattr_init(pthread_barrierattr_t *attr);
int pthread_barrierattr_destroy(pthread_barrierattr_t *attr);

/* ===== Thread-Local Storage ===== */

int pthread_key_create(pthread_key_t *key, void (*destructor)(void*));
int pthread_key_delete(pthread_key_t key);
void *pthread_getspecific(pthread_key_t key);
int pthread_setspecific(pthread_key_t key, const void *value);

/* ===== CPU Clock ===== */

int pthread_getcpuclockid(pthread_t thread, clockid_t *clock_id);

/* ===== One-Time Initialization ===== */

int pthread_once(pthread_once_t *once_control, void (*init_routine)(void));

#ifdef __cplusplus
}
#endif

#endif /* _PTHREAD_H */
