/* OXIDE OS mntent.h — Mount table entry access
 * — PulseForge: glib reads /proc/mounts via these functions.
 * Standard Linux interface for iterating mounted filesystems.
 */

#ifndef _MNTENT_H
#define _MNTENT_H

#include <stdio.h>

#ifdef __cplusplus
extern "C" {
#endif

struct mntent {
    char *mnt_fsname;   /* Device or server for filesystem */
    char *mnt_dir;      /* Directory mounted on */
    char *mnt_type;     /* Type of filesystem: ufs, nfs, etc. */
    char *mnt_opts;     /* Comma-separated options for fs */
    int   mnt_freq;     /* Dump frequency (days) */
    int   mnt_passno;   /* Pass number for fsck */
};

FILE *setmntent(const char *filename, const char *type);
struct mntent *getmntent(FILE *stream);
struct mntent *getmntent_r(FILE *stream, struct mntent *result,
                           char *buffer, int bufsize);
int addmntent(FILE *stream, const struct mntent *mnt);
int endmntent(FILE *stream);
char *hasmntopt(const struct mntent *mnt, const char *opt);

/* Standard mount table files */
#define _PATH_MOUNTED   "/proc/mounts"
#define _PATH_MNTTAB    "/etc/fstab"
#define MOUNTED         _PATH_MOUNTED
#define MNTTAB          _PATH_MNTTAB

#ifdef __cplusplus
}
#endif

#endif /* _MNTENT_H */
