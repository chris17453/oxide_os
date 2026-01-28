/* OXIDE OS Wide Character Support */

#ifndef _WCHAR_H
#define _WCHAR_H

#include <stddef.h>
#include <stdarg.h>
#include <stdio.h>

typedef int wint_t;
typedef struct { int __count; unsigned int __value; } mbstate_t;

#define WEOF ((wint_t)-1)
#define WCHAR_MIN 0
#define WCHAR_MAX 0x7FFFFFFF

/* Wide string functions */
size_t wcslen(const wchar_t *s);
wchar_t *wcscpy(wchar_t *dest, const wchar_t *src);
wchar_t *wcsncpy(wchar_t *dest, const wchar_t *src, size_t n);
wchar_t *wcscat(wchar_t *dest, const wchar_t *src);
wchar_t *wcsncat(wchar_t *dest, const wchar_t *src, size_t n);
int wcscmp(const wchar_t *s1, const wchar_t *s2);
int wcsncmp(const wchar_t *s1, const wchar_t *s2, size_t n);
wchar_t *wcschr(const wchar_t *s, wchar_t c);
wchar_t *wcsrchr(const wchar_t *s, wchar_t c);
wchar_t *wcsstr(const wchar_t *haystack, const wchar_t *needle);
size_t wcsspn(const wchar_t *s, const wchar_t *accept);
size_t wcscspn(const wchar_t *s, const wchar_t *reject);
wchar_t *wcspbrk(const wchar_t *s, const wchar_t *accept);
wchar_t *wcstok(wchar_t *s, const wchar_t *delim, wchar_t **saveptr);
int wcscoll(const wchar_t *s1, const wchar_t *s2);
size_t wcsxfrm(wchar_t *dest, const wchar_t *src, size_t n);
wchar_t *wcsdup(const wchar_t *s);

/* Wide memory functions */
wchar_t *wmemcpy(wchar_t *dest, const wchar_t *src, size_t n);
wchar_t *wmemmove(wchar_t *dest, const wchar_t *src, size_t n);
wchar_t *wmemset(wchar_t *s, wchar_t c, size_t n);
int wmemcmp(const wchar_t *s1, const wchar_t *s2, size_t n);
wchar_t *wmemchr(const wchar_t *s, wchar_t c, size_t n);

/* Multibyte/wide conversion */
int mbsinit(const mbstate_t *ps);
size_t mbrtowc(wchar_t *pwc, const char *s, size_t n, mbstate_t *ps);
size_t wcrtomb(char *s, wchar_t wc, mbstate_t *ps);
size_t mbrlen(const char *s, size_t n, mbstate_t *ps);
size_t mbsrtowcs(wchar_t *dest, const char **src, size_t len, mbstate_t *ps);
size_t wcsrtombs(char *dest, const wchar_t **src, size_t len, mbstate_t *ps);
size_t mbsnrtowcs(wchar_t *dest, const char **src, size_t nms, size_t len, mbstate_t *ps);
size_t wcsnrtombs(char *dest, const wchar_t **src, size_t nwc, size_t len, mbstate_t *ps);
int mbtowc(wchar_t *pwc, const char *s, size_t n);
int wctomb(char *s, wchar_t wc);
size_t mbstowcs(wchar_t *dest, const char *src, size_t n);
size_t wcstombs(char *dest, const wchar_t *src, size_t n);
int wctob(wint_t c);
wint_t btowc(int c);

/* Wide character I/O */
wint_t fgetwc(FILE *stream);
wchar_t *fgetws(wchar_t *s, int n, FILE *stream);
wint_t fputwc(wchar_t wc, FILE *stream);
int fputws(const wchar_t *s, FILE *stream);
wint_t getwc(FILE *stream);
wint_t getwchar(void);
wint_t putwc(wchar_t wc, FILE *stream);
wint_t putwchar(wchar_t wc);
wint_t ungetwc(wint_t wc, FILE *stream);

/* Wide string conversion */
long wcstol(const wchar_t *nptr, wchar_t **endptr, int base);
unsigned long wcstoul(const wchar_t *nptr, wchar_t **endptr, int base);
long long wcstoll(const wchar_t *nptr, wchar_t **endptr, int base);
unsigned long long wcstoull(const wchar_t *nptr, wchar_t **endptr, int base);
double wcstod(const wchar_t *nptr, wchar_t **endptr);
float wcstof(const wchar_t *nptr, wchar_t **endptr);
long double wcstold(const wchar_t *nptr, wchar_t **endptr);

/* Wide printf/scanf */
int swprintf(wchar_t *s, size_t n, const wchar_t *format, ...);
int vswprintf(wchar_t *s, size_t n, const wchar_t *format, va_list ap);
int fwprintf(FILE *stream, const wchar_t *format, ...);
int wprintf(const wchar_t *format, ...);
int vfwprintf(FILE *stream, const wchar_t *format, va_list ap);
int vwprintf(const wchar_t *format, va_list ap);

/* Time */
size_t wcsftime(wchar_t *s, size_t maxsize, const wchar_t *format,
                const struct tm *timeptr);

#endif /* _WCHAR_H */
