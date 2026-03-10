# OXIDE OS

**A Unix-like operating system with a Linux-inspired architecture, written entirely in Rust.**

OXIDE is a from-scratch operating system targeting x86_64. It features a modular
kernel (103 crates, 188 syscalls), a UEFI bootloader, a custom cross-compiler
toolchain, a full-featured libc, 90+ coreutils, an interactive shell, a TCP/IP
networking stack, a complete driver ecosystem, containers, a hypervisor, and
an AI inference subsystem — all written in Rust.

---

## Quick Start

```bash
# Prerequisites: qemu-system-x86_64, edk2-ovmf, Rust nightly, ld.lld
# See CONTRIBUTING.md for full setup instructions.

make build-full    # Build kernel, bootloader, userspace, and rootfs
make run           # Boot in QEMU (auto-detects host)
```

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                                 USERSPACE                                     │
│  init · getty · login · shell(esh) · coreutils(90+) · apps · services        │
│  ssh · rdp · networkd · resolvd · sshd · rdpd · journald · soundd            │
│  devtools: as · ld · ar · make · modutils · search                           │
│  apps: gwbasic · doom · htop · mp3player · curses-demo                       │
│                          ┌──────────────────────┐                            │
│                          │  oxide_libc / oxide-std │                          │
│                          │  ncurses · vte · termcap │                         │
│                          └──────────────────────┘                            │
├──────────────────────────────────────────────────────────────────────────────┤
│                            SYSCALL BOUNDARY  (188 syscalls)                  │
├──────────────────────────────────────────────────────────────────────────────┤
│                                  KERNEL  (103 crates)                         │
│                                                                               │
│  ┌──────────┐ ┌──────┐ ┌──────┐ ┌───────────────────────────────────────┐   │
│  │  sched   │ │  mm  │ │ vfs  │ │              NETWORKING               │   │
│  │ CFS·SMP  │ │ CoW  │ │ ext4 │ │  tcp/ip · udp · dhcp · dns · ssh      │   │
│  │work-steal│ │ VMA  │ │ fat32│ │  smb(stub) · nfs(stub) · rdp(7 crates)│   │
│  │ signals  │ │ slab │ │tmpfs │ └───────────────────────────────────────┘   │
│  └──────────┘ │ heap │ │procfs│                                              │
│               │buddy │ │devfs │ ┌───────────────────────────────────────┐   │
│               └──────┘ │sysfs │ │              SECURITY                 │   │
│                        │oxidefs│ │  crypto · AES · SHA · RSA · ChaCha20  │   │
│                        └──────┘ │  X.509 · TLS · TPM · HMAC · Argon2    │   │
│                                  │  namespaces · cgroups · seccomp       │   │
│                                  └───────────────────────────────────────┘   │
│  ┌──────────────────────────────────────────────────────────────────────┐    │
│  │                           DRIVERS  (20+)                              │    │
│  │  virtio-blk · virtio-net · virtio-gpu · virtio-snd · virtio-input    │    │
│  │  nvme · ahci · xhci · usb-msc · usb-hid · ps2 · uart-8250           │    │
│  │  intel-hda · bochs-display · pci(MSI/MSIX) · acpi                   │    │
│  └──────────────────────────────────────────────────────────────────────┘    │
│                                                                               │
│  ┌──────────┐ ┌────────────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐    │
│  │  tty/pty │ │   CONTAINERS   │ │HYPERVISOR│ │    AI    │ │  ASYNC   │    │
│  │ terminal │ │ ns·cgroups·    │ │ vmx·vmm  │ │ hnsw·    │ │  epoll   │    │
│  │compositor│ │    seccomp     │ │virtio-emu│ │embed·    │ │ io_uring │    │
│  │  vkbd    │ └────────────────┘ └──────────┘ │ indexd   │ └──────────┘    │
│  └──────────┘                                  └──────────┘                  │
│                                                                               │
│  ┌────────────────┐ ┌──────────────┐ ┌──────────────────────────────────┐   │
│  │      arch      │ │   libc-support│ │           modules               │   │
│  │x86_64·aarch64  │ │pthread·mmap·dl│ │  loadable kernel modules (LKM)  │   │
│  │    mips64      │ └──────────────┘ └──────────────────────────────────┘   │
│  └────────────────┘                                                           │
├──────────────────────────────────────────────────────────────────────────────┤
│                           UEFI BOOTLOADER                                     │
│                  (loads kernel ELF + initramfs from ESP)                     │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## Repository Layout

| Directory | Contents |
|-----------|----------|
| `kernel/` | Kernel entry point + 103 subsystem crates (mm, vfs, drivers, net, sched, security, tty, audio, usb, hypervisor, containers, AI, async, ...) |
| `bootloader/` | UEFI bootloader — loads kernel ELF and initramfs from the ESP |
| `userspace/` | All user-land programs — init, shell, 90+ coreutils, services, apps, libs |
| `external/` | Third-party source drops for cross-compilation (vim, cpython, musl, zlib) |
| `toolchain/` | Custom cross-compiler toolchain (`oxide-cc`, `oxide-ld`, linker scripts) |
| `pkgmgr/` | **oxdnf** — DNF-like package manager for Fedora SRPM cross-compilation |
| `docs/` | Subsystem guides, architecture docs, 50+ agent implementation rules |
| `scripts/` | Build and test automation scripts |
| `targets/` | Rust custom target specifications |
| `tools/` | Host-side development tools |

---

## Key Features

### Kernel
- **103 crates** organized into subsystems with clean trait-based abstractions
- **188 Linux-compatible syscalls** — process, file, network, IPC, time, security, and more
- **CFS scheduler** with SMP work-stealing, per-CPU run queues, and preemption model
- **Memory management** — buddy + slab allocators, VMA subsystem, Copy-on-Write fork, OOM killer
- **Loadable kernel modules** — runtime driver registration via linker sections

### Filesystems
- **VFS abstraction** — inode/dentry/superblock traits unifying all filesystems
- **ext4** (root), **FAT32** (ESP/removable), **OxideFS** (native), **tmpfs**, **procfs**, **devfs**, **sysfs**, **initramfs**
- **Block device layer** with buffered I/O, GPT partition discovery, NVMe, AHCI, VirtIO-blk

### Networking
- **Full TCP/IP stack** — TCP, UDP, IP, ICMP, ARP with RFC compliance
- **DHCP** client, **DNS** resolver with caching, **raw sockets** (ping)
- **SSH** client + server with key exchange and authentication
- **RDP** server (7-crate implementation) for remote desktop access
- **Socket API** — AF_INET, AF_INET6, AF_UNIX; epoll, io_uring, select/poll
- SMB and NFS protocol stubs

### Drivers (20+)
- **Storage**: VirtIO-blk, NVMe, AHCI
- **Network**: VirtIO-net
- **Input**: PS/2 keyboard/mouse, VirtIO-input — unified through shared `input::kbd` module
- **GPU/Display**: VirtIO-GPU (QEMU), Bochs display
- **Audio**: VirtIO-sound, Intel HDA
- **USB**: XHCI host controller, USB mass-storage (MSC), USB HID
- **Serial**: 8250/16550 UART
- **PCI**: Full PCIe enumeration, BAR mapping, MSI/MSIX interrupts

### Security & Isolation
- **Cryptography**: AES, SHA, RSA, ChaCha20-Poly1305, Argon2, HMAC
- **TLS / X.509** certificate parsing and validation
- **TPM** measured boot and trust chain
- **Linux namespaces** — PID, mount, network, user
- **cgroups** — CPU, memory, I/O resource limits
- **seccomp** — BPF-based syscall filtering
- **SMAP / SMEP** enforcement; kernel stack canaries; page-level memory protection

### Terminal & Graphics
- **Full VT100/xterm terminal emulator** with per-VT compositor
- **PTY layer** for pseudo-terminals
- **On-screen virtual keyboard** (Alt+K toggle)
- **Framebuffer rendering** via UEFI GOP; row-batched MMIO writes for smooth output
- **ncurses-compatible** oxide-ncurses library; VTE parser

### Advanced Subsystems
- **Hypervisor** — Intel VMX (hardware virtualization), VirtIO device emulation
- **Containers** — namespaces + cgroups + seccomp, pivot_root, overlay-style isolation
- **AI** — HNSW vector search, embedding inference, index daemon
- **Async I/O** — epoll and io_uring kernel implementations
- **Dynamic linking** — `dl_open`/`dl_sym` runtime loader
- **POSIX threads** — `pthread` create/join/mutex/condvar

### Userspace
- **90+ coreutils** — cat, cp, mv, ls, grep, sed, awk, find, tar, gzip, vim, less, diff, and more
- **esh shell** — interactive shell with job control, pipes, redirections, signal handling
- **System daemons** — init, getty, login, passwd, servicemgr, journald
- **Network daemons** — networkd, resolvd, sshd, rdpd, soundd
- **Applications** — GW-BASIC interpreter, Doom, htop (ncurses), mp3player
- **Dev tools** — assembler (`as`), linker (`ld`), archiver (`ar`), `make`, `modprobe`/`lsmod`

### Build & Toolchain
- **Custom cross-compiler** (`oxide-cc`, `oxide-ld`) targeting `x86_64-oxide-oxide`
- **oxide-std** — Rust std abstraction (fs, io, net, process, thread, collections)
- **Multi-architecture** stubs — aarch64 and MIPS64 ready for implementation
- **30+ gated debug channels** via Cargo feature flags (`debug-sched`, `debug-mm`, `debug-net`, ...)

---

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

---

## Package Manager

OXIDE includes **oxdnf**, a DNF-like package manager that downloads Fedora source RPMs,
cross-compiles them for OXIDE, and manages a local package repository.

```bash
# Sync repository metadata from Fedora
make pkgmgr-sync

# Search for packages
make pkgmgr-search PKG=bash

# Build a package from source RPM
make pkgmgr-build PKG=bash

# Install a built package
make pkgmgr-install PKG=bash

# List available and installed packages
python3 pkgmgr/bin/oxdnf list available
python3 pkgmgr/bin/oxdnf list installed
```

See [pkgmgr/README.md](pkgmgr/README.md) and [pkgmgr/QUICKSTART.md](pkgmgr/QUICKSTART.md) for complete documentation.

---

## Documentation

| Path | Contents |
|------|----------|
| `docs/subsystems/` | Kernel subsystem deep-dives (drivers, filesystem, networking, memory, scheduler, security, terminal, containers) |
| `docs/architecture/` | High-level system overview and kernel layering |
| `docs/development/` | Building, cross-toolchain, debugging guides |
| `docs/agents/` | 50+ implementation rules (ISR safety, scheduler invariants, memory allocation, ABI constraints, ...) |
| `docs/plan/` | Roadmaps, performance audit results, remediation plans |
| `CONTRIBUTING.md` | Build prerequisites and development workflow |
| `story_so_far.md` | Project history and phase-by-phase narrative |

---

## License

MIT — see [LICENSE](LICENSE).
