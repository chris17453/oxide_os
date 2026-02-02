# Building OXIDE OS

See also: [CONTRIBUTING.md](../../CONTRIBUTING.md) for prerequisites.

## Build Targets

| Command | What It Builds |
|---------|---------------|
| `make build` | Kernel + bootloader (debug) |
| `make build-full` | Everything: kernel, bootloader, userspace, initramfs, rootfs image |
| `make kernel` | Kernel only |
| `make bootloader` | Bootloader only |
| `make userspace` | All userspace packages (debug) |
| `make userspace-release` | All userspace packages (release, for rootfs) |
| `make userspace-pkg PKG=name` | Single userspace package |
| `make initramfs` | Full initramfs CPIO archive |
| `make initramfs-minimal` | Minimal initramfs (init + shell only) |
| `make create-rootfs` | 512MB disk image with ESP + root + home partitions |

## Running

| Command | Description |
|---------|-------------|
| `make run` | Auto-detect QEMU binary and run |
| `make run-fedora` | Run with `qemu-system-x86_64` |
| `make run-rhel` | Run with `qemu-kvm` |

## Debug Builds

Enable debug output channels with feature flags:

```bash
make run RUN_KERNEL_FEATURES="debug-syscall debug-fork debug-sched"
make run RUN_KERNEL_FEATURES="debug-all"  # Everything
```

## Disk Image Layout

`make create-rootfs` produces a 3-partition disk:

| Partition | Type | Size | Mount | Contents |
|-----------|------|------|-------|----------|
| 1 (ESP) | FAT32 | 64 MB | `/boot` | Bootloader, kernel, initramfs |
| 2 (root) | ext4 | 384 MB | `/` | OS binaries, config |
| 3 (home) | ext4 | 64 MB | `/home` | User data |

Note: `create-rootfs` requires `sudo` for `losetup` and `mount`.
