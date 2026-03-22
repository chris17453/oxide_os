/* OXIDE OS sys/mount.h — filesystem mount interface
 * — GraveShift: "mount/umount. The bedrock ops nobody thinks about until they're gone."
 */

#ifndef _SYS_MOUNT_H
#define _SYS_MOUNT_H

/* Mount flags */
#define MS_RDONLY       1
#define MS_NOSUID       2
#define MS_NODEV        4
#define MS_NOEXEC       8
#define MS_SYNCHRONOUS  16
#define MS_REMOUNT      32
#define MS_MANDLOCK     64
#define MS_DIRSYNC      128
#define MS_NOATIME      1024
#define MS_NODIRATIME   2048
#define MS_BIND         4096
#define MS_MOVE         8192
#define MS_REC          16384
#define MS_SILENT       32768
#define MS_POSIXACL     (1 << 16)
#define MS_UNBINDABLE   (1 << 17)
#define MS_PRIVATE      (1 << 18)
#define MS_SLAVE        (1 << 19)
#define MS_SHARED       (1 << 20)
#define MS_RELATIME     (1 << 21)
#define MS_STRICTATIME  (1 << 24)
#define MS_LAZYTIME     (1 << 25)
#define MS_NOSYMFOLLOW  (1 << 8)

/* umount2 flags */
#define MNT_FORCE       1
#define MNT_DETACH      2
#define MNT_EXPIRE      4
#define UMOUNT_NOFOLLOW 8

int mount(const char *source, const char *target,
          const char *filesystemtype, unsigned long mountflags,
          const void *data);
int umount(const char *target);
int umount2(const char *target, int flags);

#endif /* _SYS_MOUNT_H */
