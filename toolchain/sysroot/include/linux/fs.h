#ifndef _LINUX_FS_H
#define _LINUX_FS_H

#include <sys/types.h>

#define BLKROSET   _IO(0x12, 93)
#define BLKROGET   _IO(0x12, 94)
#define BLKGETSIZE _IO(0x12, 96)
#define BLKFLSBUF  _IO(0x12, 97)
#define BLKSSZGET  _IO(0x12, 104)

#define RENAME_NOREPLACE (1 << 0)
#define RENAME_EXCHANGE  (1 << 1)
#define RENAME_WHITEOUT  (1 << 2)

#define _IO(type, nr)   ((type) << 8 | (nr))
#define _IOW(type, nr, size) _IO(type, nr)
#define _IOR(type, nr, size) _IO(type, nr)

#endif /* _LINUX_FS_H */
