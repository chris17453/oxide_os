/* OXIDE OS String Operations */

#ifndef _STRING_H
#define _STRING_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Memory functions */
void *memcpy(void *dest, const void *src, size_t n);
void *memmove(void *dest, const void *src, size_t n);
void *memset(void *s, int c, size_t n);
int memcmp(const void *s1, const void *s2, size_t n);
void *memchr(const void *s, int c, size_t n);
void *memrchr(const void *s, int c, size_t n);
void explicit_bzero(void *s, size_t n);

/* String functions */
size_t strlen(const char *s);
char *strcpy(char *dest, const char *src);
char *strncpy(char *dest, const char *src, size_t n);
char *strcat(char *dest, const char *src);
char *strncat(char *dest, const char *src, size_t n);
int strcmp(const char *s1, const char *s2);
int strncmp(const char *s1, const char *s2, size_t n);
char *strchr(const char *s, int c);
char *strrchr(const char *s, int c);
char *strstr(const char *haystack, const char *needle);
size_t strspn(const char *s, const char *accept);
size_t strcspn(const char *s, const char *reject);
char *strpbrk(const char *s, const char *accept);
char *strtok(char *str, const char *delim);
char *strtok_r(char *str, const char *delim, char **saveptr);
char *strdup(const char *s);

/* BSD/GNU extensions */
size_t strlcpy(char *dst, const char *src, size_t size);
size_t strlcat(char *dst, const char *src, size_t size);
char *strsep(char **stringp, const char *delim);

/* Error strings */
char *strerror(int errnum);
char *strsignal(int sig);
int strerror_r(int errnum, char *buf, size_t buflen);

/* String duplication */
char *strndup(const char *s, size_t n);

/* Locale-aware comparison */
int strcoll(const char *s1, const char *s2);
size_t strxfrm(char *dest, const char *src, size_t n);

/* Case-insensitive comparison (POSIX, also in strings.h) */
int strcasecmp(const char *s1, const char *s2);
int strncasecmp(const char *s1, const char *s2, size_t n);

/* Bit operations (POSIX, also in strings.h) */
int ffs(int i);
int ffsl(long i);
int ffsll(long long i);

/* GNU extension */
void *memmem(const void *haystack, size_t haystacklen, const void *needle, size_t needlelen);

#ifdef __cplusplus
}
#endif

#endif /* _STRING_H */
