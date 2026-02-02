# Filesystem Subsystem

## Crates

| Crate | Purpose |
|-------|---------|
| `vfs` | Virtual filesystem switch — common inode/dentry/superblock abstractions |
| `devfs` | Device filesystem (`/dev`) |
| `tmpfs` | In-memory temporary filesystem (`/tmp`) |
| `initramfs` | CPIO initramfs unpacking |
| `procfs` | Process information filesystem (`/proc`) |
| `oxidefs` | Native OXIDE filesystem |
| `fat32` | FAT32 filesystem (ESP, removable media) |
| `ext4` | ext4 filesystem (root partition) |
| `block` | Block device abstraction layer |
| `gpt` | GPT partition table parsing |

## Architecture

The VFS layer provides a unified interface for all filesystem operations.
Filesystems register with the VFS and implement the inode/superblock traits.

Block devices go through the `block` crate which provides buffered I/O
and partition discovery via `gpt`. The storage drivers (virtio-blk, nvme,
ahci) implement the block device trait.

The boot flow mounts initramfs first, then discovers and mounts the root
ext4 partition, with devfs/procfs/tmpfs mounted at standard paths.
