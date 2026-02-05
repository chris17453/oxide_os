//! Filesystem mounting operations for the kernel
//!
//! Provides kernel-side implementation of mount/umount syscalls.

use alloc::sync::Arc;
use block::{BlockDevice, BlockDeviceInfo, BlockError, BlockResult};
use devfs::DevFs;
use fat32::Fat32;
use procfs::ProcFs;
use tmpfs::TmpDir;
use vfs::{MountFlags, VfsError, mount::GLOBAL_VFS};

/// Wrapper for static block device references to implement BlockDevice
///
/// This allows using static device references from the block registry
/// with ext4::mount which expects Arc<dyn BlockDevice>.
struct StaticDeviceRef(&'static dyn BlockDevice);

impl BlockDevice for StaticDeviceRef {
    fn read(&self, start_block: u64, buf: &mut [u8]) -> BlockResult<usize> {
        self.0.read(start_block, buf)
    }

    fn write(&self, start_block: u64, buf: &[u8]) -> BlockResult<usize> {
        self.0.write(start_block, buf)
    }

    fn flush(&self) -> BlockResult<()> {
        self.0.flush()
    }

    fn block_size(&self) -> u32 {
        self.0.block_size()
    }

    fn block_count(&self) -> u64 {
        self.0.block_count()
    }

    fn info(&self) -> BlockDeviceInfo {
        self.0.info()
    }

    fn is_read_only(&self) -> bool {
        self.0.is_read_only()
    }
}

/// Mount syscall error codes
pub mod errno {
    pub const EPERM: i64 = -1;
    pub const ENOENT: i64 = -2;
    pub const EIO: i64 = -5;
    pub const ENOTBLK: i64 = -15;
    pub const EBUSY: i64 = -16;
    pub const ENODEV: i64 = -19;
    pub const EINVAL: i64 = -22;
    pub const ENOSYS: i64 = -38;
}

/// Kernel mount callback
///
/// Called by the syscall handler to mount a filesystem.
///
/// # Arguments
/// * `source` - Device path (e.g., "/dev/virtio0p1") or empty for pseudo-fs
/// * `target` - Mount point (e.g., "/mnt/disk")
/// * `fstype` - Filesystem type (e.g., "ext4", "tmpfs", "devfs", "procfs")
/// * `flags` - Mount flags (MS_RDONLY, MS_REMOUNT, etc.)
///
/// # Returns
/// 0 on success, negative errno on error
pub fn kernel_mount(source: &str, target: &str, fstype: &str, flags: u32) -> i64 {
    use alloc::string::ToString;

    let mount_flags = MountFlags::from_bits_truncate(flags);
    let read_only = mount_flags.contains(MountFlags::MS_RDONLY);
    let is_remount = mount_flags.contains(MountFlags::MS_REMOUNT);
    let is_move = mount_flags.contains(MountFlags::MS_MOVE);

    // Handle remount
    if is_remount {
        return handle_remount(target, mount_flags);
    }

    // Handle move mount
    if is_move {
        return kernel_move_mount(source, target);
    }

    // Check if target is already mounted
    let mounts = GLOBAL_VFS.mounts();
    if mounts.contains(&target.to_string()) {
        return errno::EBUSY;
    }

    // Handle different filesystem types
    match fstype {
        "ext4" => mount_ext4(source, target, mount_flags, read_only),
        "vfat" | "fat32" => mount_vfat(source, target, mount_flags, read_only),
        "tmpfs" => mount_tmpfs(target, mount_flags),
        "devfs" => mount_devfs(target, mount_flags),
        "procfs" | "proc" => mount_procfs(target, mount_flags),
        "sysfs" | "sys" => mount_sysfs(target, mount_flags),
        "devpts" => mount_devpts(target, mount_flags),
        _ => errno::ENOSYS,
    }
}

/// Handle remount operation
fn handle_remount(target: &str, flags: MountFlags) -> i64 {
    // WireSaint: Update mount flags dynamically (e.g., ro↔rw transitions)
    match GLOBAL_VFS.remount(target, flags) {
        Ok(()) => 0,
        Err(VfsError::NotFound) => errno::ENOENT,
        Err(VfsError::InvalidArgument) => errno::EINVAL,
        Err(VfsError::Busy) => errno::EBUSY,
        Err(_) => errno::EIO,
    }
}

/// Mount an ext4 filesystem
fn mount_ext4(source: &str, target: &str, flags: MountFlags, read_only: bool) -> i64 {
    // Source must be a block device path
    if !source.starts_with("/dev/") {
        return errno::ENOENT;
    }

    let dev_name = &source[5..]; // Strip "/dev/" prefix

    // Look up the block device
    let device = match block::get_device(dev_name) {
        Some(dev) => dev,
        None => return errno::ENODEV,
    };

    // Create Arc wrapper using StaticDeviceRef
    let device_arc: Arc<dyn BlockDevice> = Arc::new(StaticDeviceRef(device));

    // Check if it's ext4
    if !ext4::is_ext4(&*device_arc) {
        return errno::EINVAL;
    }

    // Mount the ext4 filesystem
    match ext4::mount(device_arc, read_only) {
        Ok(root_vnode) => match GLOBAL_VFS.mount(root_vnode, target, flags, "ext4") {
            Ok(()) => 0,
            Err(_) => errno::EBUSY,
        },
        Err(_) => errno::EIO,
    }
}

/// Mount a FAT32/VFAT filesystem
fn mount_vfat(source: &str, target: &str, flags: MountFlags, read_only: bool) -> i64 {
    // Source must be a block device path
    if !source.starts_with("/dev/") {
        return errno::ENOENT;
    }

    let dev_name = &source[5..]; // Strip "/dev/" prefix

    // Look up the block device
    let device = match block::get_device(dev_name) {
        Some(dev) => dev,
        None => return errno::ENODEV,
    };

    // Wrap device for Fat32 driver
    let device_arc: Arc<dyn BlockDevice> = Arc::new(StaticDeviceRef(device));

    // Mount FAT32
    match Fat32::mount(device_arc).map(|fs| fs.root()) {
        Ok(root_vnode) => match GLOBAL_VFS.mount(root_vnode, target, flags, "vfat") {
            Ok(()) => 0,
            Err(_) => errno::EBUSY,
        },
        Err(_) => errno::EIO,
    }
}

/// Mount a tmpfs filesystem
fn mount_tmpfs(target: &str, flags: MountFlags) -> i64 {
    let tmpfs = TmpDir::new_root();

    match GLOBAL_VFS.mount(tmpfs, target, flags, "tmpfs") {
        Ok(()) => 0,
        Err(_) => errno::EBUSY,
    }
}

/// Mount devfs
fn mount_devfs(target: &str, flags: MountFlags) -> i64 {
    let devfs = DevFs::new();

    match GLOBAL_VFS.mount(devfs, target, flags, "devfs") {
        Ok(()) => 0,
        Err(_) => errno::EBUSY,
    }
}

/// Mount procfs
fn mount_procfs(target: &str, flags: MountFlags) -> i64 {
    let procfs = ProcFs::new();

    match GLOBAL_VFS.mount(procfs, target, flags, "procfs") {
        Ok(()) => 0,
        Err(_) => errno::EBUSY,
    }
}

/// Mount sysfs (currently uses tmpfs as placeholder)
fn mount_sysfs(target: &str, flags: MountFlags) -> i64 {
    // TODO: Implement real sysfs
    let sysfs = TmpDir::new_root();

    match GLOBAL_VFS.mount(sysfs, target, flags, "sysfs") {
        Ok(()) => 0,
        Err(_) => errno::EBUSY,
    }
}

/// Mount devpts (PTY filesystem)
///
/// Note: devpts is mounted automatically during boot and requires
/// access to the global PtyManager. Re-mounting via syscall is not
/// currently supported.
fn mount_devpts(_target: &str, _flags: MountFlags) -> i64 {
    // devpts requires access to PtyManager which is set up during boot
    // For now, return error - user should use the existing /dev/pts
    errno::ENOSYS
}

/// Kernel move_mount callback
///
/// Moves a mount from one path to another (MS_MOVE semantics).
///
/// # Arguments
/// * `source` - Current mount point path
/// * `target` - New mount point path
///
/// # Returns
/// 0 on success, negative errno on error
pub fn kernel_move_mount(source: &str, target: &str) -> i64 {
    match GLOBAL_VFS.move_mount(source, target) {
        Ok(()) => 0,
        Err(vfs::VfsError::NotFound) => errno::EINVAL,
        Err(_) => errno::EINVAL,
    }
}

/// Kernel pivot_root callback
///
/// Changes the root filesystem. The filesystem at `new_root` becomes
/// the new `/`, and the old root is moved to `put_old`.
///
/// # Arguments
/// * `new_root` - Path to the new root filesystem (must be a mount point)
/// * `put_old` - Path where old root will be placed (must be under new_root)
///
/// # Returns
/// 0 on success, negative errno on error
pub fn kernel_pivot_root(new_root: &str, put_old: &str) -> i64 {
    // Validate new_root is a mount point
    let mounts = GLOBAL_VFS.mounts();
    if !mounts.contains(&alloc::string::String::from(new_root)) {
        return errno::EINVAL;
    }

    // Validate put_old is under new_root
    if !put_old.starts_with(new_root) {
        return errno::EINVAL;
    }

    // Perform the pivot
    match GLOBAL_VFS.pivot_root(new_root, put_old) {
        Ok(()) => 0,
        Err(vfs::VfsError::NotFound) => errno::EINVAL,
        Err(vfs::VfsError::InvalidArgument) => errno::EINVAL,
        Err(_) => errno::EINVAL,
    }
}

/// Kernel umount callback
///
/// Called by the syscall handler to unmount a filesystem.
///
/// # Arguments
/// * `target` - Mount point path
/// * `flags` - Unmount flags (MNT_FORCE, MNT_DETACH)
///
/// # Returns
/// 0 on success, negative errno on error
pub fn kernel_umount(target: &str, _flags: u32) -> i64 {
    // Don't allow unmounting root
    if target == "/" {
        return errno::EBUSY;
    }

    // TODO: Check if mount point has open files

    // Unmount the filesystem
    match GLOBAL_VFS.unmount(target) {
        Ok(()) => 0,
        Err(vfs::VfsError::NotFound) => errno::EINVAL,
        Err(vfs::VfsError::Busy) => errno::EBUSY,
        Err(_) => errno::EINVAL,
    }
}

/// Umount flags
pub mod umount_flags {
    /// Force unmount even if busy
    pub const MNT_FORCE: u32 = 1;
    /// Detach from filesystem tree (lazy unmount)
    pub const MNT_DETACH: u32 = 2;
    /// Mark for expiry
    pub const MNT_EXPIRE: u32 = 4;
    /// Don't follow symlinks
    pub const UMOUNT_NOFOLLOW: u32 = 8;
}
