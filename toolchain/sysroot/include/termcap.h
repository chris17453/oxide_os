#ifndef _TERMCAP_H
#define _TERMCAP_H

#ifdef __cplusplus
extern "C" {
#endif

int tgetent(char *bp, const char *name);
int tgetnum(const char *id);
int tgetflag(const char *id);
char *tgetstr(const char *id, char **area);
char *tgoto(const char *cap, int col, int row);
int tputs(const char *str, int affcnt, int (*putc)(int));

#ifdef __cplusplus
}
#endif

#endif /* _TERMCAP_H */
