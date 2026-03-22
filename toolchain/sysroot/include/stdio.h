/* OXIDE OS Standard I/O */

#ifndef _STDIO_H
#define _STDIO_H

#include <stddef.h>
#include <stdarg.h>
#include <sys/types.h>

#ifdef __cplusplus
extern "C" {
#endif

/* File operations (declarations match libc implementation) */
#ifndef __FILE_defined
#define __FILE_defined
typedef struct _FILE FILE;
#endif

/* Standard streams */
extern FILE *stdin;
extern FILE *stdout;
extern FILE *stderr;

/* Standard file descriptors */
#define STDIN_FILENO  0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2

/* File operations */
int printf(const char *format, ...);
int fprintf(FILE *stream, const char *format, ...);
int sprintf(char *str, const char *format, ...);
int snprintf(char *str, size_t size, const char *format, ...);

int vprintf(const char *format, va_list ap);
int vfprintf(FILE *stream, const char *format, va_list ap);
int vsprintf(char *str, const char *format, va_list ap);
int vsnprintf(char *str, size_t size, const char *format, va_list ap);

int scanf(const char *format, ...);
int fscanf(FILE *stream, const char *format, ...);
int sscanf(const char *str, const char *format, ...);
int vscanf(const char *format, va_list ap);
int vfscanf(FILE *stream, const char *format, va_list ap);
int vsscanf(const char *str, const char *format, va_list ap);

int putchar(int c);
int puts(const char *s);
int getchar(void);
char *gets(char *s);

/* File I/O */
FILE *fopen(const char *pathname, const char *mode);
FILE *freopen(const char *pathname, const char *mode, FILE *stream);
FILE *fdopen(int fd, const char *mode);
int fclose(FILE *stream);
int fflush(FILE *stream);

size_t fread(void *ptr, size_t size, size_t nmemb, FILE *stream);
size_t fwrite(const void *ptr, size_t size, size_t nmemb, FILE *stream);

int fgetc(FILE *stream);
int getc(FILE *stream);
int ungetc(int c, FILE *stream);
char *fgets(char *s, int size, FILE *stream);
int fputc(int c, FILE *stream);
int fputs(const char *s, FILE *stream);

int fseek(FILE *stream, long offset, int whence);
long ftell(FILE *stream);
void rewind(FILE *stream);

int feof(FILE *stream);
int ferror(FILE *stream);
void clearerr(FILE *stream);
int fileno(FILE *stream);

/* Constants */
#define EOF (-1)

#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

#define BUFSIZ 8192

/* Buffering modes */
#define _IOFBF 0
#define _IOLBF 1
#define _IONBF 2

int setvbuf(FILE *stream, char *buf, int mode, size_t size);
void setbuf(FILE *stream, char *buf);

/* Error reporting */
void perror(const char *s);

/* File removal/rename */
int remove(const char *pathname);
int rename(const char *oldpath, const char *newpath);

/* Temporary files */
FILE *tmpfile(void);
char *tmpnam(char *s);

/* popen/pclose */
FILE *popen(const char *command, const char *type);
int pclose(FILE *stream);

/* getline */
long getline(char **lineptr, size_t *n, FILE *stream);
long getdelim(char **lineptr, size_t *n, int delim, FILE *stream);

/* Filename max */
#define FILENAME_MAX 4096
#define L_tmpnam     20
#define TMP_MAX      238328
#define P_tmpdir     "/tmp"

/* FOPEN_MAX */
#define FOPEN_MAX    16

/* putc */
int putc(int c, FILE *stream);

/* fseeko/ftello (64-bit offset versions) */
int fseeko(FILE *stream, long offset, int whence);
long ftello(FILE *stream);
int fseeko64(FILE *stream, long long offset, int whence);
long long ftello64(FILE *stream);
FILE *fopen64(const char *pathname, const char *mode);
FILE *freopen64(const char *pathname, const char *mode, FILE *stream);

/* fgetpos/fsetpos */
typedef struct { long long __pos; } fpos_t;
int fgetpos(FILE *stream, fpos_t *pos);
int fsetpos(FILE *stream, const fpos_t *pos);

/* dprintf/vdprintf */
int dprintf(int fd, const char *format, ...);
int vdprintf(int fd, const char *format, va_list ap);

/* fread_unlocked/fgets_unlocked */
size_t fread_unlocked(void *ptr, size_t size, size_t nmemb, FILE *stream);

/* Unlocked I/O — no-lock versions for single-threaded fast paths */
int getc_unlocked(FILE *stream);
int putc_unlocked(int c, FILE *stream);
int getchar_unlocked(void);
int putchar_unlocked(int c);

/* Memory streams */
FILE *open_memstream(char **ptr, size_t *sizeloc);
FILE *fmemopen(void *buf, size_t size, const char *mode);

/* flockfile/funlockfile */
void flockfile(FILE *stream);
void funlockfile(FILE *stream);
int ftrylockfile(FILE *stream);

#ifdef __cplusplus
}
#endif

#endif /* _STDIO_H */
