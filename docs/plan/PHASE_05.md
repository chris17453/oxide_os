# Phase 5: VFS + Filesystems

**Stage:** 2 - Core OS
**Status:** Not Started
**Dependencies:** Phase 4 (Process Model)

---

## Goal

Implement Virtual Filesystem layer with initial filesystem implementations.

---

## Deliverables

| Item | Status |
|------|--------|
| VFS layer with vnode abstraction | [ ] |
| File descriptor table per process | [ ] |
| devfs (/dev/null, /dev/zero, /dev/console) | [ ] |
| tmpfs (RAM filesystem) | [ ] |
| initramfs (cpio) loaded at boot | [ ] |
| procfs basics (/proc/self, /proc/[pid]) | [ ] |

---

## Architecture Status

| Arch | VFS | devfs | tmpfs | initramfs | procfs | Done |
|------|-----|-------|-------|-----------|--------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Syscalls to Implement

| Number | Name | Args | Return |
|--------|------|------|--------|
| 20 | sys_open | path, flags, mode | fd or -errno |
| 21 | sys_close | fd | 0 or -errno |
| 22 | sys_read | fd, buf, len | bytes read or -errno |
| 23 | sys_write | fd, buf, len | bytes written or -errno |
| 24 | sys_lseek | fd, offset, whence | new offset or -errno |
| 25 | sys_stat | path, statbuf | 0 or -errno |
| 26 | sys_fstat | fd, statbuf | 0 or -errno |
| 27 | sys_mkdir | path, mode | 0 or -errno |
| 28 | sys_rmdir | path | 0 or -errno |
| 29 | sys_unlink | path | 0 or -errno |
| 30 | sys_rename | oldpath, newpath | 0 or -errno |
| 31 | sys_readdir | fd, dirent, count | entries read or -errno |
| 32 | sys_getcwd | buf, size | buf or NULL |
| 33 | sys_chdir | path | 0 or -errno |
| 34 | sys_dup | oldfd | newfd or -errno |
| 35 | sys_dup2 | oldfd, newfd | newfd or -errno |
| 36 | sys_pipe | pipefd[2] | 0 or -errno |
| 37 | sys_mount | source, target, fstype, flags, data | 0 or -errno |
| 38 | sys_umount | target | 0 or -errno |

---

## VFS Architecture

```
                    ┌─────────────┐
                    │   Process   │
                    │  fd table   │
                    └──────┬──────┘
                           │
                    ┌──────▼──────┐
                    │    VFS      │
                    │  (vnode)    │
                    └──────┬──────┘
                           │
        ┌──────────┬───────┼───────┬──────────┐
        ▼          ▼       ▼       ▼          ▼
    ┌──────┐  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐
    │devfs │  │tmpfs │ │procfs│ │initrd│ │efflux│
    └──────┘  └──────┘ └──────┘ └──────┘ └──────┘
```

---

## Vnode Operations

```rust
pub trait VnodeOps {
    fn lookup(&self, name: &str) -> Result<Arc<Vnode>>;
    fn create(&self, name: &str, mode: Mode) -> Result<Arc<Vnode>>;
    fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize>;
    fn write(&self, offset: u64, buf: &[u8]) -> Result<usize>;
    fn readdir(&self, offset: u64) -> Result<Option<DirEntry>>;
    fn mkdir(&self, name: &str, mode: Mode) -> Result<Arc<Vnode>>;
    fn rmdir(&self, name: &str) -> Result<()>;
    fn unlink(&self, name: &str) -> Result<()>;
    fn rename(&self, old: &str, new_dir: &Vnode, new: &str) -> Result<()>;
    fn stat(&self) -> Result<Stat>;
    fn truncate(&self, size: u64) -> Result<()>;
}
```

---

## Key Files

```
crates/vfs/efflux-vfs/src/
├── lib.rs
├── vnode.rs           # Vnode abstraction
├── file.rs            # File handle
├── mount.rs           # Mount points
└── path.rs            # Path resolution

crates/vfs/efflux-devfs/src/
├── lib.rs
├── null.rs            # /dev/null
├── zero.rs            # /dev/zero
└── console.rs         # /dev/console

crates/vfs/efflux-tmpfs/src/
├── lib.rs
└── inode.rs           # In-memory inodes

crates/vfs/efflux-procfs/src/
├── lib.rs
├── self.rs            # /proc/self
└── pid.rs             # /proc/[pid]

crates/vfs/efflux-initramfs/src/
├── lib.rs
└── cpio.rs            # CPIO parser
```

---

## Exit Criteria

- [ ] open/read/write/close work on files
- [ ] Directory operations (mkdir, rmdir, readdir)
- [ ] /dev/null, /dev/zero, /dev/console work
- [ ] tmpfs supports file creation and I/O
- [ ] initramfs loads and mounts at boot
- [ ] /proc/self/exe resolves correctly
- [ ] Works on all 8 architectures

---

## Test Program

```c
int main() {
    // Create and write to file
    int fd = open("/tmp/test.txt", O_CREAT | O_WRONLY, 0644);
    write(fd, "Hello VFS!\n", 11);
    close(fd);

    // Read it back
    fd = open("/tmp/test.txt", O_RDONLY);
    char buf[64];
    int n = read(fd, buf, sizeof(buf));
    write(1, buf, n);  // stdout
    close(fd);

    // Test /dev/null
    fd = open("/dev/null", O_WRONLY);
    write(fd, "discarded", 9);
    close(fd);

    return 0;
}
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 5 of EFFLUX Implementation*
