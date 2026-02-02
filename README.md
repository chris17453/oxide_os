# OXIDE OS

**A Unix-like operating system with a Linux-inspired architecture, written in Rust.**

OXIDE is a from-scratch operating system targeting x86_64, featuring a modular
kernel, UEFI bootloader, custom libc, 90+ coreutils, a shell, networking stack,
and driver ecosystem — all written in Rust.

---

## Quick Start

```bash
# Prerequisites: qemu-system-x86_64, edk2-ovmf, Rust nightly, ld.lld
# See CONTRIBUTING.md for full setup instructions.

make build-full    # Build kernel, bootloader, userspace, and rootfs
make run           # Boot in QEMU (auto-detects host)
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    USERSPACE                         │
│  init · shell · coreutils · ssh · services · apps   │
│                   ┌───────┐                         │
│                   │ libc  │                         │
├───────────────────┴───────┴─────────────────────────┤
│                   SYSCALL BOUNDARY                   │
├─────────────────────────────────────────────────────┤
│                     KERNEL                           │
│  ┌────────┐ ┌─────┐ ┌─────┐ ┌──────┐ ┌──────────┐ │
│  │ sched  │ │ mm  │ │ vfs │ │ net  │ │ security │ │
│  └────────┘ └─────┘ └─────┘ └──────┘ └──────────┘ │
│  ┌─────────────────────────────────────────────────┐│
│  │               DRIVERS                           ││
│  │ virtio-blk · nvme · ahci · ps2 · virtio-net    ││
│  │ virtio-gpu · xhci · usb-hid · uart-8250        ││
│  └─────────────────────────────────────────────────┘│
│  ┌────────┐ ┌──────────┐ ┌────────────┐            │
│  │  arch  │ │   boot   │ │  platform  │            │
│  │ x86_64 │ │  proto   │ │  pci/usb   │            │
│  └────────┘ └──────────┘ └────────────┘            │
├─────────────────────────────────────────────────────┤
│                  UEFI BOOTLOADER                     │
│              (loads kernel + initramfs)              │
└─────────────────────────────────────────────────────┘
```

## Repository Layout

| Directory | Contents |
|-----------|----------|
| `kernel/` | Kernel entry point and all subsystems (mm, vfs, drivers, net, sched, security, ...) |
| `bootloader/` | UEFI bootloader |
| `userspace/` | All userspace programs — system, shell, coreutils, services, apps, libs |
| `external/` | Third-party source drops for cross-compilation (vim, cpython, musl, zlib) |
| `toolchain/` | Custom cross-compiler toolchain (`oxide-cc`, `oxide-ld`, etc.) |
| `docs/` | Architecture documentation, subsystem guides, porting notes |
| `scripts/` | Build and test automation scripts |
| `targets/` | Rust custom target specifications |
| `tools/` | Host-side development tools |

## Key Features

- **Modular kernel** — 30+ subsystem crates with trait-based abstractions
- **Full userspace** — custom libc, 90 coreutils, shell with job control, SSH client/server
- **Filesystem support** — VFS with ext4, FAT32, OxideFS, tmpfs, procfs, devfs
- **Networking** — TCP/IP stack, DHCP, DNS, SSH
- **Driver ecosystem** — VirtIO (block, net, GPU, input, sound), NVMe, AHCI, PS/2, USB/XHCI
- **Security** — crypto, X.509, TPM trust, seccomp, namespaces, cgroups
- **Containerization** — namespaces, cgroups, seccomp filtering
- **Hypervisor** — Intel VMX support, VirtIO device emulation
- **Multi-architecture** — x86_64 primary, aarch64 and MIPS64 stubs
- **Debug infrastructure** — 30+ gated debug channels via Cargo feature flags

## Build Commands

| Command | Description |
|---------|-------------|
| `make build` | Compile kernel + bootloader |
| `make build-full` | Full system: kernel, bootloader, userspace, rootfs |
| `make run` | Boot in QEMU (auto-detects Fedora/RHEL) |
| `make test` | Automated boot test via QEMU + serial log |
| `make userspace` | Rebuild all userspace packages |
| `make toolchain` | Build the cross-compiler toolchain |
| `make clean` | Remove build artifacts |

## License

MIT — see [LICENSE](LICENSE).
