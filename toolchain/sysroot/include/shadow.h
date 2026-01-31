#ifndef _SHADOW_H
#define _SHADOW_H

struct spwd {
    char *sp_namp;      /* Login name */
    char *sp_pwdp;      /* Encrypted password */
    long  sp_lstchg;    /* Date of last change */
    long  sp_min;       /* Min days between changes */
    long  sp_max;       /* Max days between changes */
    long  sp_warn;      /* Days before password expires to warn */
    long  sp_inact;     /* Days after password expires until account disabled */
    long  sp_expire;    /* Date when account expires */
    unsigned long sp_flag; /* Reserved */
};

struct spwd *getspnam(const char *name);
struct spwd *getspent(void);
void setspent(void);
void endspent(void);

#endif /* _SHADOW_H */
