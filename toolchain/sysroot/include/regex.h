#ifndef _REGEX_H
#define _REGEX_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>

/* regoff_t is used for match offsets */
typedef long regoff_t;

/* Opaque regex_t structure - matches musl's layout */
typedef struct re_pattern_buffer {
	size_t re_nsub;
	void *__opaque, *__padding[4];
	size_t __nsub2;
	char __padding2;
} regex_t;

/* Match structure for regexec */
typedef struct {
	regoff_t rm_so;  /* Start offset */
	regoff_t rm_eo;  /* End offset */
} regmatch_t;

/* Flags for regcomp */
#define REG_EXTENDED    1
#define REG_ICASE       2
#define REG_NEWLINE     4
#define REG_NOSUB       8

/* Flags for regexec */
#define REG_NOTBOL      1
#define REG_NOTEOL      2

/* Error codes */
#define REG_OK          0
#define REG_NOMATCH     1
#define REG_BADPAT      2
#define REG_ECOLLATE    3
#define REG_ECTYPE      4
#define REG_EESCAPE     5
#define REG_ESUBREG     6
#define REG_EBRACK      7
#define REG_EPAREN      8
#define REG_EBRACE      9
#define REG_BADBR       10
#define REG_ERANGE      11
#define REG_ESPACE      12
#define REG_BADRPT      13
#define REG_ENOSYS      -1

/* POSIX regex functions */
int regcomp(regex_t *__restrict, const char *__restrict, int);
int regexec(const regex_t *__restrict, const char *__restrict, size_t, regmatch_t *__restrict, int);
void regfree(regex_t *);
size_t regerror(int, const regex_t *__restrict, char *__restrict, size_t);

#ifdef __cplusplus
}
#endif

#endif
