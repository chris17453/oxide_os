/* OXIDE OS arpa/nameser_compat.h — DNS constants for glib */
#ifndef _ARPA_NAMESER_COMPAT_H
#define _ARPA_NAMESER_COMPAT_H

#ifdef __cplusplus
extern "C" {
#endif

/* DNS query classes */
#define C_IN 1     /* Internet */
#define C_CHAOS 3  /* CHAOS */
#define C_ANY 255  /* Any class */

/* DNS query types */
#define T_A     1   /* IPv4 address */
#define T_NS    2   /* Name server */
#define T_CNAME 5   /* Canonical name */
#define T_SOA   6   /* Start of authority */
#define T_PTR   12  /* Pointer */
#define T_MX    15  /* Mail exchange */
#define T_TXT   16  /* Text */
#define T_AAAA  28  /* IPv6 address */
#define T_SRV   33  /* Service locator */
#define T_ANY   255 /* Any type */

/* Network byte order extraction macros (used by glib gthreadedresolver) */
#define GETSHORT(s, cp) do { \
    const unsigned char *t_cp = (const unsigned char *)(cp); \
    (s) = ((unsigned short)t_cp[0] << 8) | ((unsigned short)t_cp[1]); \
    (cp) += 2; \
} while (0)

#define GETLONG(l, cp) do { \
    const unsigned char *t_cp = (const unsigned char *)(cp); \
    (l) = ((unsigned int)t_cp[0] << 24) | ((unsigned int)t_cp[1] << 16) | \
          ((unsigned int)t_cp[2] << 8) | ((unsigned int)t_cp[3]); \
    (cp) += 4; \
} while (0)

#define PUTSHORT(s, cp) do { \
    unsigned short t_s = (unsigned short)(s); \
    unsigned char *t_cp = (unsigned char *)(cp); \
    t_cp[0] = t_s >> 8; t_cp[1] = t_s; \
    (cp) += 2; \
} while (0)

#define PUTLONG(l, cp) do { \
    unsigned int t_l = (unsigned int)(l); \
    unsigned char *t_cp = (unsigned char *)(cp); \
    t_cp[0] = t_l >> 24; t_cp[1] = t_l >> 16; t_cp[2] = t_l >> 8; t_cp[3] = t_l; \
    (cp) += 4; \
} while (0)

#ifdef __cplusplus
}
#endif

#endif /* _ARPA_NAMESER_COMPAT_H */
