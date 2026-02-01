/* OXIDE OS Implementation Limits */

#ifndef _LIMITS_H
#define _LIMITS_H

/* Number of bits in a char */
#define CHAR_BIT    8

/* Minimum and maximum values for a signed char */
#define SCHAR_MIN   (-128)
#define SCHAR_MAX   127

/* Maximum value for an unsigned char */
#define UCHAR_MAX   255

/* Minimum and maximum values for a char */
#define CHAR_MIN    SCHAR_MIN
#define CHAR_MAX    SCHAR_MAX

/* Minimum and maximum values for a signed short */
#define SHRT_MIN    (-32768)
#define SHRT_MAX    32767

/* Maximum value for an unsigned short */
#define USHRT_MAX   65535

/* Minimum and maximum values for a signed int */
#define INT_MIN     (-2147483647 - 1)
#define INT_MAX     2147483647

/* Maximum value for an unsigned int */
#define UINT_MAX    4294967295U

/* Minimum and maximum values for a signed long */
#define LONG_MIN    (-9223372036854775807L - 1)
#define LONG_MAX    9223372036854775807L

/* Maximum value for an unsigned long */
#define ULONG_MAX   18446744073709551615UL

/* Minimum and maximum values for a signed long long */
#define LLONG_MIN   (-9223372036854775807LL - 1)
#define LLONG_MAX   9223372036854775807LL

/* Maximum value for an unsigned long long */
#define ULLONG_MAX  18446744073709551615ULL

/* POSIX limits */
#define PATH_MAX        4096
#define NAME_MAX        255
#define PIPE_BUF        4096
#define LINE_MAX        2048
#define ARG_MAX         131072
#define OPEN_MAX        256
#define HOST_NAME_MAX   64
#define LOGIN_NAME_MAX  256
#define TTY_NAME_MAX    32
#define NGROUPS_MAX     65536
#define SSIZE_MAX       LONG_MAX
#define _POSIX_PATH_MAX 256
#define _POSIX_NAME_MAX 14
#define _POSIX_PIPE_BUF 512
#define _POSIX_ARG_MAX  4096

/* MB_LEN_MAX */
#define MB_LEN_MAX      4

/* POSIX2 limits */
#define _POSIX2_RE_DUP_MAX 255
#define RE_DUP_MAX         _POSIX2_RE_DUP_MAX
#define _POSIX2_CHARCLASS_NAME_MAX 14
#define CHARCLASS_NAME_MAX         _POSIX2_CHARCLASS_NAME_MAX

/* IOV max */
#define IOV_MAX         1024

/* PTHREAD limits */
#define PTHREAD_KEYS_MAX        128
#define PTHREAD_STACK_MIN       16384
#define PTHREAD_DESTRUCTOR_ITERATIONS 4

#endif /* _LIMITS_H */
