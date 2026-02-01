/* OXIDE OS Wide Character Classification */

#ifndef _WCTYPE_H
#define _WCTYPE_H

#include <wchar.h>
#include <locale.h>  /* For locale_t */

typedef unsigned long wctype_t;
typedef const int *wctrans_t;

int iswalpha(wint_t wc);
int iswdigit(wint_t wc);
int iswalnum(wint_t wc);
int iswspace(wint_t wc);
int iswupper(wint_t wc);
int iswlower(wint_t wc);
int iswprint(wint_t wc);
int iswpunct(wint_t wc);
int iswcntrl(wint_t wc);
int iswxdigit(wint_t wc);
int iswgraph(wint_t wc);
int iswblank(wint_t wc);

wint_t towupper(wint_t wc);
wint_t towlower(wint_t wc);

wctype_t wctype(const char *name);
int iswctype(wint_t wc, wctype_t desc);
wctrans_t wctrans(const char *name);
wint_t towctrans(wint_t wc, wctrans_t desc);

/* _l locale variants */
int iswalpha_l(wint_t wc, locale_t locale);
int iswdigit_l(wint_t wc, locale_t locale);
int iswalnum_l(wint_t wc, locale_t locale);
int iswspace_l(wint_t wc, locale_t locale);
int iswupper_l(wint_t wc, locale_t locale);
int iswlower_l(wint_t wc, locale_t locale);
int iswprint_l(wint_t wc, locale_t locale);
int iswpunct_l(wint_t wc, locale_t locale);
int iswcntrl_l(wint_t wc, locale_t locale);
int iswxdigit_l(wint_t wc, locale_t locale);
int iswgraph_l(wint_t wc, locale_t locale);
int iswblank_l(wint_t wc, locale_t locale);
wint_t towupper_l(wint_t wc, locale_t locale);
wint_t towlower_l(wint_t wc, locale_t locale);
wctype_t wctype_l(const char *name, locale_t locale);
int iswctype_l(wint_t wc, wctype_t desc, locale_t locale);
wctrans_t wctrans_l(const char *name, locale_t locale);
wint_t towctrans_l(wint_t wc, wctrans_t desc, locale_t locale);

#endif /* _WCTYPE_H */
