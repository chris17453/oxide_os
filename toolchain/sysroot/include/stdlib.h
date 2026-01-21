/* OXIDE OS Standard Library */

#ifndef _STDLIB_H
#define _STDLIB_H

#include <stddef.h>

/* Process control */
void exit(int status) __attribute__((noreturn));
void _exit(int status) __attribute__((noreturn));
void abort(void) __attribute__((noreturn));

/* Memory allocation */
void *malloc(size_t size);
void *calloc(size_t nmemb, size_t size);
void *realloc(void *ptr, size_t size);
void free(void *ptr);

/* String conversion */
int atoi(const char *nptr);
long atol(const char *nptr);
long long atoll(const char *nptr);
double atof(const char *nptr);

long strtol(const char *nptr, char **endptr, int base);
unsigned long strtoul(const char *nptr, char **endptr, int base);
long long strtoll(const char *nptr, char **endptr, int base);
unsigned long long strtoull(const char *nptr, char **endptr, int base);
double strtod(const char *nptr, char **endptr);

/* Environment */
char *getenv(const char *name);
int putenv(char *string);
int setenv(const char *name, const char *value, int overwrite);
int unsetenv(const char *name);

/* Pseudo-random numbers */
int rand(void);
void srand(unsigned int seed);

/* Absolute value */
int abs(int j);
long labs(long j);
long long llabs(long long j);

/* Division */
typedef struct { int quot, rem; } div_t;
typedef struct { long quot, rem; } ldiv_t;
typedef struct { long long quot, rem; } lldiv_t;

div_t div(int numer, int denom);
ldiv_t ldiv(long numer, long denom);
lldiv_t lldiv(long long numer, long long denom);

/* Exit codes */
#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1

/* Constants */
#define RAND_MAX 0x7FFFFFFF

#endif /* _STDLIB_H */
