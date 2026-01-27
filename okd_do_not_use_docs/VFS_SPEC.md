# OXIDE Virtual Filesystem (VFS) Specification

**Version:** 1.0  
**Status:** Draft  
**License:** MIT  

---

## 0) Overview

The VFS provides a unified interface for all filesystem operations.

---

## 1) Core Traits

```rust
pub trait FilesystemType: Send + Sync {
    fn name(&self) -> &str;
    fn mount(&self, device: Option<Arc<dyn BlockDevice>>, flags: MountFlags,
             options: &str) -> Result<Superblock>;
}

pub trait InodeOps: Send + Sync {
    fn lookup(&self, dir: &Inode, name: &str) -> Result<Arc<Inode>>;
    fn create(&self, dir: &Inode, name: &str, mode: FileMode) -> Result<Arc<Inode>>;
    fn mkdir(&self, dir: &Inode, name: &str, mode: FileMode) -> Result<Arc<Inode>>;
    fn unlink(&self, dir: &Inode, name: &str) -> Result<()>;
    fn rmdir(&self, dir: &Inode, name: &str) -> Result<()>;
    fn rename(&self, old_dir: &Inode, old_name: &str, new_dir: &Inode, new_name: &str) -> Result<()>;
    fn symlink(&self, dir: &Inode, name: &str, target: &str) -> Result<Arc<Inode>>;
    fn readlink(&self, inode: &Inode) -> Result<String>;
    fn truncate(&self, inode: &Inode, size: u64) -> Result<()>;
}

pub trait FileOps: Send + Sync {
    fn read(&self, file: &File, buf: &mut [u8], offset: u64) -> Result<usize>;
    fn write(&self, file: &File, buf: &[u8], offset: u64) -> Result<usize>;
    fn seek(&self, file: &File, offset: i64, whence: SeekWhence) -> Result<u64>;
    fn readdir(&self, file: &File, callback: &mut dyn FnMut(DirEntry) -> bool) -> Result<()>;
    fn ioctl(&self, file: &File, cmd: u32, arg: usize) -> Result<usize>;
    fn poll(&self, file: &File, events: PollEvents) -> PollEvents;
    fn fsync(&self, file: &File, datasync: bool) -> Result<()>;
    fn mmap(&self, file: &File, offset: u64, len: usize, prot: MemProt) -> Result<*mut u8>;
}
```

---

## 2) Mount Table with Aliases

```rust
pub struct Mount {
    pub sb: Arc<Superblock>,
    pub mountpoint: PathBuf,
    pub source: String,
    pub flags: MountFlags,
    pub drive_letter: Option<char>,  // C:, D:, etc.
    pub alias: Option<String>,       // BACKUP:, etc.
    pub access_mode: AccessMode,
}

pub struct MountTable {
    mounts: RwLock<Vec<Arc<Mount>>>,
    by_letter: RwLock<HashMap<char, Arc<Mount>>>,
    by_alias: RwLock<HashMap<String, Arc<Mount>>>,
    next_letter: AtomicU8,  // Starts at 'C'
}
```

Drive letters A: and B: are reserved. Auto-assignment starts at C:.

---

## 3) Path Resolution

Supports:
- Unix paths: `/home/user/file.txt`
- Drive letters: `C:/Users/file.txt` or `C:\Users\file.txt`
- Named aliases: `BACKUP:/data/file.txt`

```rust
pub fn resolve_path(path: &str) -> Result<PathBuf> {
    if let Some(letter) = extract_drive_letter(path) {
        let mount = mount_table.find_by_letter(letter)?;
        return Ok(mount.mountpoint.join(&path[2..]));
    }
    if let Some((alias, rest)) = extract_alias(path) {
        let mount = mount_table.find_by_alias(&alias)?;
        return Ok(mount.mountpoint.join(rest));
    }
    Ok(PathBuf::from(path))
}
```

---

## 4) Filesystems Required

| Name | Type | Purpose |
|------|------|---------|
| oxide.fs | Disk | Native filesystem |
| FAT32 | Disk | Boot, USB compatibility |
| tmpfs | RAM | Temporary files |
| devfs | Virtual | Device nodes |
| procfs | Virtual | Process information |
| sysfs | Virtual | System/device info |
| overlayfs | Union | COW for sandboxes |

---

## 5) devfs Layout

```
/dev/
├── null
├── zero
├── random
├── urandom
├── console
├── tty
├── tty[0-N]
├── pts/
│   └── [0-N]
├── ptmx
├── fd/
│   └── [0-N] -> /proc/self/fd/[0-N]
├── stdin -> fd/0
├── stdout -> fd/1
├── stderr -> fd/2
├── nvme[0-N]
│   └── nvme[0-N]p[1-N]
├── sd[a-z]
│   └── sd[a-z][1-N]
├── input/
│   ├── event[0-N]
│   └── mice
├── fb[0-N]
├── dri/
│   └── card[0-N]
└── snd/
    └── ...
```

---

## 6) procfs Layout

```
/proc/
├── [pid]/
│   ├── cmdline
│   ├── cwd -> ...
│   ├── exe -> ...
│   ├── environ
│   ├── fd/
│   │   └── [0-N] -> ...
│   ├── maps
│   ├── mem
│   ├── root -> ...
│   ├── stat
│   ├── status
│   └── task/
│       └── [tid]/
├── self -> [current pid]
├── cpuinfo
├── meminfo
├── mounts
├── filesystems
├── uptime
├── loadavg
├── version
└── sys/
    ├── kernel/
    └── vm/
```

---

## 7) Exit Criteria

- [ ] All filesystems mount correctly
- [ ] Path resolution handles Unix, drive letters, aliases
- [ ] procfs shows accurate process info
- [ ] devfs creates correct device nodes
- [ ] Overlay filesystem works for sandboxing

---

*End of OXIDE VFS Specification*
