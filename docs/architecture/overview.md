# OXIDE OS Architecture Overview

OXIDE is a monolithic kernel OS with a Linux-inspired architecture, written
entirely in Rust. The kernel runs in ring 0 on x86_64, with userspace programs
linked against a custom libc.

## Kernel Subsystems

| Subsystem | Crates | Purpose |
|-----------|--------|---------|
| `arch/` | arch-traits, arch-x86_64, arch-aarch64, arch-mips64 | CPU, interrupts, paging, syscall entry |
| `mm/` | mm-traits, mm-core, mm-manager, mm-slab, mm-paging, mm-heap, mm-cow | Physical/virtual memory, allocators, CoW |
| `sched/` | sched-traits, sched | Process scheduler (round-robin) |
| `proc/` | proc-traits, proc | Process/thread management, fork, exec |
| `vfs/` | vfs, devfs, tmpfs, initramfs, procfs | Virtual filesystem layer |
| `fs/` | oxidefs, fat32, ext4 | Filesystem implementations |
| `block/` | block, gpt | Block device layer, partition tables |
| `net/` | net, tcpip, dhcp, dns, smb, nfs, rdp | Network stack |
| `drivers/` | 20+ driver crates | Device drivers (VirtIO, NVMe, AHCI, PS/2, USB) |
| `tty/` | tty, pty, vt, terminal | Terminal subsystem |
| `security/` | crypto, trust, quarantine, x509 | Cryptography, TPM, certificates |
| `container/` | namespace, cgroup, seccomp | Container isolation primitives |
| `hypervisor/` | vmm, vmx, virtio-emu | Intel VMX virtualization |
| `ai/` | hnsw, embed, indexd | Kernel-level vector search and inference |

## Boot Sequence

1. UEFI firmware loads `bootloader/boot-uefi`
2. Bootloader loads kernel ELF + initramfs from ESP
3. Kernel initializes: GDT → IDT → paging → heap → interrupts → PCI
4. Init process (PID 1) starts from initramfs
5. Root filesystem mounted, getty spawns on TTYs

## Userspace Model

Programs are statically linked against `userspace/libc/` using a custom linker
script (`userspace/userspace.ld`). Syscalls use the x86_64 `syscall` instruction
with Linux-compatible numbers.

See [Boot Flow](boot-flow.md) and [Userspace Architecture](userspace.md) for details.
