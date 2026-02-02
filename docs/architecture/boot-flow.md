# OXIDE OS Boot & Filesystem Mounting

How OXIDE boots and mounts filesystems, compared to Linux.

---

## Boot Flow Comparison

### Linux (typical UEFI + GRUB)

```
 UEFI Firmware
      |
      v
 GRUB (from ESP: /EFI/fedora/grubx64.efi)
      |  reads grub.cfg, loads vmlinuz + initramfs
      v
 Linux Kernel (early init)
      |  unpacks initramfs into rootfs (tmpfs)
      v
 initramfs /init (systemd-based or shell script)
      |  loads block drivers, finds root device
      |  mounts real root filesystem (ext4/btrfs/xfs)
      |  calls switch_root / pivot_root
      v
 Real Root Filesystem (ext4 on /dev/sda2)
      |  old initramfs is freed
      v
 /sbin/init (systemd PID 1)
      |  mounts /etc/fstab entries (home, boot, swap, etc.)
      |  starts services, getty, login
      v
 getty -> login -> shell
```

### OXIDE (current)

```
 UEFI Firmware
      |
      v
 OXIDE Bootloader (from ESP: /EFI/OXIDE/kernel.elf)
      |  loads kernel ELF + initramfs.cpio from ESP
      |  passes BootInfo struct to kernel
      v
 OXIDE Kernel (kernel_main)
      |  mounts tmpfs as /
      |  mounts devfs, procfs, devpts, sysfs
      |  detects VirtIO block devices, parses GPT, finds ext4
      |  mounts initramfs (CPIO) as / (replaces tmpfs)
      |  mounts tmpfs overlays on /run, /tmp, /var/*
      |  mounts ext4 at /mnt/root (NOT as real root)
      |  loads /sbin/init ELF from initramfs
      v
 /sbin/init (PID 1, from initramfs)         <--- RUNS FROM INITRAMFS
      |  mounts /etc/fstab entries:
      |    sysfs on /sys
      |    LABEL=BOOT (vfat) on /boot
      |    LABEL=HOME (ext4) on /home
      |  starts servicemgr (daemon mode)
      |  loads firewall rules
      v
 getty -> login -> shell
```

---

## Key Difference: No Root Transition

Linux has a two-phase root filesystem:

1. **Phase 1 (initramfs)**: Temporary root, just enough to find and mount the real root
2. **Phase 2 (real root)**: `switch_root` or `pivot_root` replaces `/` with the real filesystem, initramfs is freed

OXIDE skips phase 2 entirely. The initramfs **is** the root filesystem for the lifetime of the system. The ext4 partition is mounted as a secondary filesystem at `/mnt/root`, not as `/`.

This is controlled by a hardcoded guard in `kernel/src/init.rs:963`:
```rust
if false && ext4_root_partition.is_some() {
    // ... ext4-as-root code (disabled)
}
```

When this is `false`, the kernel falls through to the initramfs path and mounts ext4 at `/mnt/root` instead.

---

## Detailed Boot Stages

### Stage 1: UEFI Bootloader

**File**: `bootloader/boot-uefi/src/main.rs`

The UEFI application loads two files from the ESP (EFI System Partition):

| File | ESP Path | Purpose |
|------|----------|---------|
| Kernel | `\EFI\OXIDE\kernel.elf` | Kernel binary (ELF64) |
| Initramfs | `\EFI\OXIDE\initramfs.cpio` | Root filesystem (CPIO archive) |

It then:
1. Parses kernel ELF, allocates physical memory, loads segments
2. Allocates pages for initramfs, reads it into physical memory
3. Sets up page tables (identity map + higher-half kernel mapping)
4. Builds `BootInfo` struct with physical addresses, memory map, framebuffer info
5. Exits UEFI boot services
6. Jumps to kernel entry point

`BootInfo` passes initramfs location to kernel:
```
boot_info.initramfs_phys = <physical address>
boot_info.initramfs_size = <byte count>
```

### Stage 2: Kernel VFS Initialization

**File**: `kernel/src/init.rs:560-682`

Before loading the initramfs, the kernel sets up the VFS tree on a tmpfs root:

```
/                   tmpfs (temporary, replaced by initramfs later)
/dev                devfs (null, zero, random, urandom, console, serial, kmsg, fb0)
/dev/tty1-6         VT devices (virtual terminals)
/dev/pts            devpts (PTY devices)
/proc               procfs
```

### Stage 3: Block Device Discovery

**File**: `kernel/src/init.rs` (block device section)

The kernel probes VirtIO PCI devices and:
1. Initializes VirtIO block drivers
2. Reads GPT partition tables
3. Creates device nodes: `/dev/virtio0`, `/dev/virtio0p1`, `/dev/virtio0p2`, `/dev/virtio0p3`
4. Identifies ext4 partitions by checking superblock magic

### Stage 4: Root Filesystem Mount

**File**: `kernel/src/init.rs:1020-1105`

Since ext4-as-root is disabled, the kernel:
1. Reads initramfs from the physical address in BootInfo
2. Parses the CPIO archive into an in-memory filesystem tree
3. Mounts initramfs as `/` (replacing the tmpfs root)
4. Mounts tmpfs overlays for writable directories:
   - `/run`, `/tmp`, `/var/log`, `/var/lib`, `/var/run`
5. Mounts ext4 at `/mnt/root` as a secondary filesystem

The initramfs is **read-only** (it's an unpacked CPIO archive in memory). The tmpfs overlays provide writable space for runtime data.

### Stage 5: Init Process (PID 1)

**File**: `userspace/init/src/main.rs`

The kernel loads `/sbin/init` from the initramfs and executes it as PID 1. Init then:

1. Reads `/etc/fstab` and mounts entries via `mount()` syscall
2. Resolves `LABEL=` entries through hardcoded label map:
   - `LABEL=BOOT` -> `/dev/virtio0p1` (vfat, mounted at `/boot`)
   - `LABEL=ROOT` -> `/dev/virtio0p2` (ext4, available but not used as root)
   - `LABEL=HOME` -> `/dev/virtio0p3` (ext4, mounted at `/home`)
3. Loads firewall rules from `/etc/fw.rules` if present
4. Starts service manager (`/bin/servicemgr`) in daemon mode
5. Forks and execs `/bin/getty`

### Stage 6: Login Flow

```
getty          waits for keypress, prints "OXIDE OS login:", execs login
  -> login    prompts username/password, validates against /etc/passwd, execs shell
    -> esh    interactive shell (OXIDE's shell)
```

When the shell exits, init detects getty's death and respawns it after 2 seconds.

---

## Disk Layout

The QEMU disk image has a GPT partition table:

```
+-------+------------+--------+-----------+------------------+
| Part  | Label      | FS     | Device    | Mount Point      |
+-------+------------+--------+-----------+------------------+
|  1    | BOOT       | vfat   | virtio0p1 | /boot            |
|  2    | ROOT       | ext4   | virtio0p2 | /mnt/root (*)    |
|  3    | HOME       | ext4   | virtio0p3 | /home            |
+-------+------------+--------+-----------+------------------+

(*) ROOT partition is mounted at /mnt/root, NOT at /
    The actual / is the initramfs (in-memory CPIO)
```

The ESP (EFI System Partition) that holds the bootloader, kernel, and initramfs
is separate from these data partitions. UEFI firmware reads the ESP directly.

---

## Filesystem Tree at Runtime

```
/                       initramfs (CPIO, read-only, in-memory)
  /bin/                 userspace binaries (init, esh, getty, login, ls, cat, ...)
  /sbin/                init symlink
  /etc/                 config files (fstab, passwd, group, services.d/)
  /dev/                 devfs (kernel-managed)
    /dev/null
    /dev/zero
    /dev/random
    /dev/urandom
    /dev/console
    /dev/serial
    /dev/kmsg
    /dev/fb0
    /dev/tty1-6         virtual terminals
    /dev/pts/           PTY devices
    /dev/virtio0        whole disk
    /dev/virtio0p1      partition 1 (BOOT)
    /dev/virtio0p2      partition 2 (ROOT)
    /dev/virtio0p3      partition 3 (HOME)
  /proc/                procfs
  /sys/                 sysfs
  /run/                 tmpfs (writable)
  /tmp/                 tmpfs (writable)
  /var/log/             tmpfs (writable)
  /var/lib/             tmpfs (writable)
  /var/run/             tmpfs (writable)
  /boot/                vfat partition (LABEL=BOOT)
  /home/                ext4 partition (LABEL=HOME)
  /mnt/root/            ext4 partition (LABEL=ROOT) -- the "real" root, unused
```

---

## What's Missing vs Linux

### 1. switch_root / pivot_root

**What Linux does**: After the initramfs loads drivers and finds the root device, it calls `switch_root` (or `pivot_root`) to replace `/` with the real filesystem. The initramfs is unmounted and its memory freed. All subsequent file access goes to the on-disk filesystem.

**What OXIDE does**: Nothing. The initramfs stays as `/` forever. The ext4 ROOT partition sits at `/mnt/root` unused.

**Impact**: All binaries and config live in RAM (the unpacked CPIO). Changes to `/etc` or `/bin` are lost on reboot (initramfs is read-only). The ext4 ROOT partition, which has persistent storage, is not used as the root.

**To fix**: Implement `pivot_root` syscall and an initramfs `/init` script (or modify the kernel) that:
1. Mounts ext4 ROOT at a temporary mountpoint
2. Moves existing mounts (devfs, procfs, etc.) into the new root
3. Calls `pivot_root` to swap `/` to the ext4 filesystem
4. Unmounts old initramfs root
5. Execs the real `/sbin/init` from ext4

### 2. Proper Label Resolution

**What Linux does**: `blkid` scans all block devices and reads filesystem superblock UUIDs/labels. `LABEL=` and `UUID=` fstab entries are resolved dynamically at mount time.

**What OXIDE does**: Hardcoded mapping in init's `map_label()`:
```rust
"BOOT" => Some("/dev/virtio0p1"),
"HOME" => Some("/dev/virtio0p3"),
"ROOT" => Some("/dev/virtio0p2"),
```

**Impact**: Only works with the exact disk layout OXIDE expects. A second disk or different partition order would break.

**To fix**: Read filesystem labels from superblocks (ext4 superblock at offset 0x478, vfat at offset 0x47) or implement a `/dev/disk/by-label/` directory populated by the kernel.

### 3. Bootloader (no GRUB equivalent)

**What Linux does**: GRUB reads its config, presents a menu, can chainload, handles multiple kernels, supports boot parameters.

**What OXIDE does**: Direct UEFI application that loads a single kernel and initramfs from hardcoded paths. No boot menu, no kernel selection, no boot parameters.

**Impact**: No multi-boot, no fallback kernel, no boot-time configuration.

### 4. Initramfs Is Not a True Early Userspace

**What Linux does**: The initramfs contains a minimal userspace with `udev`, `modprobe`, `mount`, etc. Its `/init` script discovers hardware, loads modules, assembles RAID/LVM, unlocks LUKS, and mounts the real root.

**What OXIDE does**: The initramfs is the complete userspace (all binaries, all config). The kernel does all hardware discovery and filesystem mounting internally, not via userspace tools.

**Impact**: All driver loading and device discovery must happen in kernel code. No flexibility to add new storage drivers or root discovery logic without kernel changes.

### 5. No Persistent Root Filesystem

**What Linux does**: The root filesystem is on disk. Package installs, config changes, and system updates persist across reboots.

**What OXIDE does**: Root is initramfs (built at compile time). Only `/home` (ext4) and `/boot` (vfat) are persistent. Everything else resets on reboot.

**Impact**: Cannot install packages, update config, or make any system-level changes that survive a reboot.

---

## Path Forward

To transition OXIDE from "runs entirely from initramfs" to "boots from a real root filesystem":

1. **Enable ext4-as-root in kernel** -- remove the `if false &&` guard in `init.rs:963`
2. **Populate the ROOT partition** -- put `/sbin/init`, `/bin/*`, `/etc/*` on ext4
3. **Or implement pivot_root** -- keep the early initramfs for device discovery, then switch to ext4
4. **Implement /dev/disk/by-label/** -- dynamic label resolution instead of hardcoded map
5. **Move device discovery to userspace** -- long-term, let init scripts handle mounting
