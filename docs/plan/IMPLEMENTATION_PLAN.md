# EFFLUX Implementation Plan

**Status:** Active
**Target:** All 8 architectures (x86_64, i686, aarch64, arm, mips64, mips32, riscv64, riscv32)

---

## Overview

This plan organizes EFFLUX development into 5 stages with 25 phases. Each phase builds on previous phases. All phases must work on all supported architectures.

---

## Stage 1: Foundation

**Goal:** Bootable kernel with preemptive multitasking.

| Phase | Name | Dependencies | Deliverables |
|-------|------|--------------|--------------|
| 0 | Boot + Serial | None | Boot to Rust, serial output, panic handler |
| 1 | Memory | Phase 0 | Frame allocator, page tables, kernel heap |
| 2 | Interrupts + Timer + Scheduler | Phase 1 | IDT/GIC, timer tick, kernel threads, preemption |
| 3 | User Mode + Syscalls | Phase 2 | Ring 3, ELF loader, sys_exit/write/read |

### Phase 0: Boot + Serial
- Custom bootloader (UEFI, BIOS, ARCS, OpenSBI, U-Boot)
- Kernel entry in Rust
- Serial output on all architectures
- Panic handler with halt

**Exit:** "Hello from EFFLUX" on serial for all 8 architectures

### Phase 1: Memory
- Boot-time bump allocator
- Buddy allocator
- Kernel page tables (arch-specific)
- Slab allocator for kernel heap
- Direct map of physical memory

**Exit:** `Box::new()` works in kernel

### Phase 2: Interrupts + Timer + Scheduler
- Interrupt controller setup (APIC/GIC/PLIC/CP0)
- Timer interrupt at 100Hz+
- Kernel thread creation
- Context switch
- Preemptive round-robin scheduler
- Per-CPU run queues (prep for SMP)

**Exit:** Multiple kernel threads run concurrently, preemption works

### Phase 3: User Mode + Syscalls
- User address space creation
- Static ELF loader
- Ring 0 → Ring 3 transition
- Syscall entry (syscall/svc/ecall)
- Basic syscalls: exit, write, read

**Exit:** User process prints to serial and exits

---

## Stage 2: Core OS

**Goal:** Full process model with filesystem and terminal.

| Phase | Name | Dependencies | Deliverables |
|-------|------|--------------|--------------|
| 4 | Process Model | Phase 3 | fork, exec, wait, COW, process groups |
| 5 | VFS + Filesystems | Phase 4 | VFS layer, devfs, tmpfs, initramfs |
| 6 | TTY + PTY | Phase 5 | Line discipline, PTY pairs, job control |
| 7 | Signals | Phase 6 | Signal delivery, handlers, masks |
| 8 | Libc + Userland | Phase 7 | libc, init, login, shell, coreutils |

### Phase 4: Process Model
- Process structure (PID, PPID, credentials)
- fork() with Copy-on-Write
- exec() replaces process image
- wait()/waitpid() reaps children
- Process groups and sessions

**Exit:** fork-exec-wait cycle works

### Phase 5: VFS + Filesystems
- VFS with vnode abstraction
- devfs (/dev/null, /dev/zero, /dev/console)
- tmpfs (RAM filesystem)
- initramfs loaded at boot (cpio)
- procfs basics (/proc/self)

**Exit:** open/read/write/close on tmpfs work

### Phase 6: TTY + PTY
- TTY line discipline
- Canonical mode (line editing)
- PTY master/slave pairs
- Foreground process group
- Window size (TIOCGWINSZ)

**Exit:** Line editing works, Ctrl+C interrupts

### Phase 7: Signals
- Signal generation and delivery
- Signal handlers (sigaction)
- Signal masks (sigprocmask)
- Core dump signals
- SIGCHLD on child exit

**Exit:** Custom signal handler catches and returns

### Phase 8: Libc + Userland
- Custom libc (musl-inspired API)
- init (PID 1) - mounts, spawns getty, reaps zombies
- login - authentication
- shell - command execution
- coreutils: ls, cat, echo, mkdir, rm, cp, mv, pwd

**Exit:** Boot to shell prompt, run commands

---

## Stage 3: Hardware

**Goal:** Multi-core, persistent storage, networking, peripherals.

| Phase | Name | Dependencies | Deliverables |
|-------|------|--------------|--------------|
| 9 | SMP | Phase 8 | AP boot, per-CPU data, TLB shootdown |
| 10 | Modules | Phase 9 | Loadable kernel modules, insmod/rmmod |
| 11 | Storage | Phase 10 | Block devices, GPT, effluxfs, FAT32 |
| 12 | Network | Phase 11 | TCP/IP, virtio-net, sockets |
| 13 | Input | Phase 8 | Keyboard, mouse, input events |
| 14 | Graphics | Phase 8 | Framebuffer, virtio-gpu |
| 15 | Audio | Phase 10 | virtio-snd, mixer |
| 16 | USB | Phase 10 | xHCI, mass storage, HID |

### Phase 9: SMP
- Application Processor boot (SIPI/PSCI/HSM)
- Per-CPU data structures
- Per-CPU run queues
- TLB shootdowns via IPI
- Work stealing scheduler

**Exit:** All cores active, threads run on all cores

### Phase 10: Modules
- Module binary format (relocatable ELF)
- Module loader with symbol resolution
- init/exit hooks
- Dependency ordering
- insmod/rmmod utilities

**Exit:** Driver module loads and unloads at runtime

### Phase 11: Storage
- Block device interface
- GPT partition parsing
- virtio-blk driver
- NVMe driver
- AHCI/SATA driver
- effluxfs driver (native)
- FAT32 driver (EFI/compat)

**Exit:** Mount effluxfs root, read/write files

### Phase 12: Network
- Network device interface
- virtio-net driver
- smoltcp TCP/IP stack
- Socket syscalls
- DHCP client
- DNS resolver

**Exit:** TCP connection to external host

### Phase 13: Input
- PS/2 keyboard/mouse (x86)
- virtio-input
- Input event subsystem
- Key repeat, mouse acceleration

**Exit:** Type in shell, mouse cursor moves

### Phase 14: Graphics
- UEFI GOP framebuffer
- virtio-gpu driver
- Framebuffer console
- Resolution switching

**Exit:** Text console on framebuffer

### Phase 15: Audio
- Audio device interface
- virtio-snd driver
- PCM playback
- Volume mixer

**Exit:** Play audio file

### Phase 16: USB
- xHCI host controller
- USB device enumeration
- Mass storage class
- HID class (keyboard, mouse)

**Exit:** USB keyboard works, USB drive mounts

---

## Stage 4: Advanced

**Goal:** Containers, virtualization, self-hosting, AI features, security.

| Phase | Name | Dependencies | Deliverables |
|-------|------|--------------|--------------|
| 17 | Containers | Phase 9 | Namespaces, cgroups, seccomp |
| 18 | Hypervisor | Phase 9 | VT-x/EL2, nested paging, VM lifecycle |
| 19 | Self-Hosting | Phase 11,12 | LLVM, rustc, cargo on EFFLUX |
| 20 | AI Indexing | Phase 11 | indexd, embeddings, semantic search |
| 21 | Security | Phase 11 | Signing, encryption, trust store, quarantine |

### Phase 17: Containers
- PID namespaces
- Mount namespaces
- Network namespaces
- User namespaces
- Cgroups (CPU, memory)
- Seccomp syscall filtering

**Exit:** Isolated container runs

### Phase 18: Hypervisor
- Hardware virtualization (VT-x, ARM EL2)
- VMCS/VGIC management
- Nested page tables (EPT, Stage 2)
- virtio device emulation
- VM create/run/destroy

**Exit:** Guest OS boots to serial

### Phase 19: Self-Hosting
- Port LLVM
- Port rustc
- Port cargo
- Full pthread support

**Exit:** Kernel compiles on itself

### Phase 20: AI Indexing
- indexd daemon
- Candle embedding runtime
- HNSW vector index
- Extended metadata on effluxfs
- Overlay metadata for other FS
- Semantic search API

**Exit:** Semantic file search returns relevant results

### Phase 21: Security
- X.509 certificate management
- Ed25519 file signing
- AES-256-GCM / ChaCha20-Poly1305 encryption
- Trust store with revocation
- Quarantine system
- Trust sharing (QR, NFC, file)

**Exit:** Sign file, verify on another machine

---

## Stage 5: Polish

**Goal:** Full compatibility, async I/O, external media handling.

| Phase | Name | Dependencies | Deliverables |
|-------|------|--------------|--------------|
| 22 | Async I/O | Phase 12 | epoll/kqueue equivalent |
| 23 | External Media | Phase 11,21 | USB/network share policies |
| 24 | Compat Runtimes | Phase 8 | DOS V86, Python sandbox |
| 25 | Full Libc | Phase 19 | Source compat with Linux apps |

### Phase 22: Async I/O
- epoll-like interface
- Async file I/O
- Async network I/O
- Event loop support

**Exit:** Async TCP server handles multiple connections

### Phase 23: External Media
- USB media detection
- Network share mounting
- Read-only by default policy
- User promotion workflow
- Trust verification

**Exit:** USB drive contents verified before execute

### Phase 24: Compat Runtimes
- DOS emulation (V86 on x86)
- Python interpreter (sandboxed)
- Syscall translation layers

**Exit:** Run DOS game, run Python script

### Phase 25: Full Libc
- Complete POSIX coverage
- glibc compatibility shims
- Dynamic linking
- dlopen/dlsym

**Exit:** Complex Linux apps recompile and run

---

## Architecture Checklist

Each phase must pass on all architectures:

| Arch | QEMU Target | Boot Method |
|------|-------------|-------------|
| x86_64 | q35 | UEFI |
| i686 | pc | BIOS/UEFI |
| aarch64 | virt | UEFI |
| arm | virt | U-Boot |
| mips64 | malta | YAMON |
| mips32 | malta | YAMON |
| riscv64 | virt | OpenSBI |
| riscv32 | virt | OpenSBI |

---

## Tracking

Progress is tracked in individual phase files:
- [PHASE_00.md](PHASE_00.md) - Boot + Serial
- [PHASE_01.md](PHASE_01.md) - Memory
- ... (created as phases begin)

---

## Dependencies Graph

```
Phase 0 (Boot)
    │
    v
Phase 1 (Memory)
    │
    v
Phase 2 (Scheduler)
    │
    v
Phase 3 (User Mode)
    │
    ├──────────────────────────────────────┐
    v                                      v
Phase 4 (Process)                    Phase 13 (Input)
    │                                Phase 14 (Graphics)
    v
Phase 5 (VFS)
    │
    v
Phase 6 (TTY)
    │
    v
Phase 7 (Signals)
    │
    v
Phase 8 (Userland) ────────────────────────┐
    │                                      │
    v                                      v
Phase 9 (SMP)                        Phase 17 (Containers)
    │                                Phase 18 (Hypervisor)
    v
Phase 10 (Modules)
    │
    ├───────────────────┬──────────────────┐
    v                   v                  v
Phase 11 (Storage)  Phase 15 (Audio)   Phase 16 (USB)
    │
    ├───────────────────┬──────────────────┐
    v                   v                  v
Phase 12 (Network)  Phase 20 (AI)     Phase 21 (Security)
    │                                      │
    v                                      v
Phase 22 (Async)                     Phase 23 (External)
    │
    v
Phase 19 (Self-Host)
    │
    v
Phase 25 (Full Libc)
    │
    v
Phase 24 (Compat)
```

---

## Start Here

1. Read EFFLUX_MASTER_SPEC.md for full context
2. Review arch-specific docs in docs/arch/*/
3. Begin Phase 0: Boot + Serial
4. Update PHASE_00.md as you progress

---

*End of EFFLUX Implementation Plan*
