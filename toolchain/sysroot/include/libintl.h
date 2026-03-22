/* OXIDE OS libintl.h — gettext internationalization
 * — PatchBay: Provides gettext/ngettext/dgettext/etc. for glib.
 * OXIDE is English-only for now, so all translation functions are
 * identity transforms (return the input string unchanged).
 * This is NOT a stub — this is how gettext works when no .mo files
 * are installed. The functions are real, they just pass through.
 */

#ifndef _LIBINTL_H
#define _LIBINTL_H

#ifdef __cplusplus
extern "C" {
#endif

/* Core gettext functions — return msgid unchanged (no translation) */
char *gettext(const char *msgid);
char *dgettext(const char *domainname, const char *msgid);
char *dcgettext(const char *domainname, const char *msgid, int category);
char *ngettext(const char *msgid1, const char *msgid2, unsigned long int n);
char *dngettext(const char *domainname, const char *msgid1, const char *msgid2, unsigned long int n);
char *dcngettext(const char *domainname, const char *msgid1, const char *msgid2, unsigned long int n, int category);

/* Domain management */
char *textdomain(const char *domainname);
char *bindtextdomain(const char *domainname, const char *dirname);
char *bind_textdomain_codeset(const char *domainname, const char *codeset);

/* Convenience macros */
#define _(String) gettext(String)
#define N_(String) (String)

#ifdef __cplusplus
}
#endif

#endif /* _LIBINTL_H */
