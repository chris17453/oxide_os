/* search.h — POSIX search functions for OXIDE OS
 * — GraveShift: "tsearch, tfind, hsearch... the graveyard of data structures nobody remembers until ncurses asks."
 */

#ifndef _SEARCH_H
#define _SEARCH_H

#include <stddef.h>

/* Hash table (hsearch family) */
typedef enum { FIND, ENTER } ACTION;

typedef struct entry {
    char *key;
    void *data;
} ENTRY;

extern ENTRY *hsearch(ENTRY __item, ACTION __action);
extern int hcreate(size_t __nel);
extern void hdestroy(void);

/* Tree search (tsearch family) */
typedef enum { preorder, postorder, endorder, leaf } VISIT;

typedef int (*__compar_fn_t)(const void *, const void *);

extern void *tsearch(const void *__key, void **__rootp, __compar_fn_t __compar);
extern void *tfind(const void *__key, void *const *__rootp, __compar_fn_t __compar);
extern void *tdelete(const void *__key, void **__rootp, __compar_fn_t __compar);
extern void twalk(const void *__root, void (*__action)(const void *, VISIT, int));

/* Linear search */
extern void *lfind(const void *__key, const void *__base,
                   size_t *__nmemb, size_t __size, __compar_fn_t __compar);
extern void *lsearch(const void *__key, void *__base,
                     size_t *__nmemb, size_t __size, __compar_fn_t __compar);

/* Linked list (insque/remque) */
extern void insque(void *__elem, void *__prev);
extern void remque(void *__elem);

#endif /* _SEARCH_H */
