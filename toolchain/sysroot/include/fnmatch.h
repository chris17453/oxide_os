/* OXIDE OS fnmatch.h — filename pattern matching
 * — PulseForge: POSIX fnmatch(3) for shell-style wildcards.
 * glib's xdgmime uses this for MIME type glob matching.
 */

#ifndef _FNMATCH_H
#define _FNMATCH_H

#ifdef __cplusplus
extern "C" {
#endif

/* Flags */
#define FNM_PATHNAME    (1 << 0)  /* No wildcard can match '/' */
#define FNM_NOESCAPE    (1 << 1)  /* Backslashes don't quote metacharacters */
#define FNM_PERIOD      (1 << 2)  /* Leading '.' is matched only explicitly */
#define FNM_CASEFOLD    (1 << 4)  /* Case-insensitive matching (GNU extension) */

/* Return values */
#define FNM_NOMATCH     1         /* String does not match pattern */

int fnmatch(const char *pattern, const char *string, int flags);

#ifdef __cplusplus
}
#endif

#endif /* _FNMATCH_H */
