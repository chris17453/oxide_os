/* ar.h — archive file format for OXIDE OS
 * — Hexline: the ancient one. This header hasn't changed since 1979.
 * Every Unix since V7 uses the same format. If it ain't broke... */

#ifndef _AR_H
#define _AR_H

#define ARMAG   "!<arch>\n"     /* Magic string */
#define SARMAG  8               /* Length of magic string */
#define ARFMAG  "`\n"           /* Header trailer string */

struct ar_hdr {
    char ar_name[16];   /* Member file name, terminated with '/' */
    char ar_date[12];   /* File date, decimal seconds since epoch */
    char ar_uid[6];     /* User ID, decimal */
    char ar_gid[6];     /* Group ID, decimal */
    char ar_mode[8];    /* File mode, octal */
    char ar_size[10];   /* File size, decimal */
    char ar_fmag[2];    /* Trailer: ARFMAG */
};

#endif /* _AR_H */
