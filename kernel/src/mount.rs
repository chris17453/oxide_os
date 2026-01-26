//! Filesystem mounting operations for the kernel
//!
//! Provides kernel-side implementation of mount/umount syscalls.

use alloc::sync::Arc;
use block::{BlockDevice, BlockDeviceInfo, BlockError, BlockResult};
use devfs::DevFs;
use procfs::ProcFs;
use tmpfs::TmpDir;
use vfs::{MountFlags, mount::GLOBAL_VFS};

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

    // Handle remount
    if is_remount {
        return handle_remount(target, mount_flags);
    }

    // Check if target is already mounted
    let mounts = GLOBAL_VFS.mounts();
    if mounts.contains(&target.to_string()) {
        return errno::EBUSY;
    }

    // Handle different filesystem types
    match fstype {
        "ext4" => mount_ext4(source, target, mount_flags, read_only),
        "tmpfs" => mount_tmpfs(target, mount_flags),
        "devfs" => mount_devfs(target, mount_flags),
        "procfs" | "proc" => mount_procfs(target, mount_flags),
        "sysfs" | "sys" => mount_sysfs(target, mount_flags),
        "devpts" => mount_devpts(target, mount_flags),
        _ => errno::ENOSYS,
    }
}

/// Handle remount operation
fn handle_remount(target: &str, _flags: MountFlags) -> i64 {
    use alloc::string::ToString;

    // Check if mount exists
    let mounts = GLOBAL_VFS.mounts();
    if !mounts.contains(&target.to_string()) {
        return errno::EINVAL;
    }

    // TODO: Implement full remount support
    // For now, just return success - the mount exists
    // A full implementation would update the mount flags
    0
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
