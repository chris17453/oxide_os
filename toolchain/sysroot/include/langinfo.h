/* OXIDE OS Language Info Stubs */

#ifndef _LANGINFO_H
#define _LANGINFO_H

#ifdef __cplusplus
extern "C" {
#endif

/* nl_item type */
typedef int nl_item;

/* Locale categories - stub values */
#define CODESET 0

/* Stub function - always return UTF-8 */
static inline char *nl_langinfo(nl_item item) {
    return "UTF-8";
}

#ifdef __cplusplus
}
#endif

#endif /* _LANGINFO_H */
