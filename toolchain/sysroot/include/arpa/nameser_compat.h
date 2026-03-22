/* OXIDE OS arpa/nameser_compat.h — DNS constants for glib */
#ifndef _ARPA_NAMESER_COMPAT_H
#define _ARPA_NAMESER_COMPAT_H

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

#endif /* _ARPA_NAMESER_COMPAT_H */
