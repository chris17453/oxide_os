# OXIDE OS Kernel Architecture Overview

## Boot & Init
- Entry: `kernel/src/main.rs` → `init()` orchestrates early serial, framebuffer bring-up, terminal+VT wiring, memory manager/pagedb registration, heap init, scheduler start, VFS mount, and first userspace launch (`/init`).
- SMP: `smp_init` boots APs, sets up per-CPU data, TLB shootdown hooks, and registers idle tasks per core (up to 256 CPUs).
- Arch: x86_64 implementation provides GDT/IDT setup, APIC timer/interrupts, syscall MSRs, context switch helpers, and HAL traits; other arch dirs stubbed for future ports.

## Architecture Graph (Mermaid)
```mermaid
flowchart TD
  Boot[Bootloader] --> Init[init.rs]
  Init --> Arch[x86_64 HAL]
  Init --> MM[Memory Mgmt\nbuddy/pagedb/heap/paging/COW/VMA]
  Init --> Sched[Scheduler\nRT/CFS/Idle per-CPU]
  Init --> Vfs[VFS core\nmounts/fd/pipes/epoll/io_uring]
  Init --> Fs[Filesystems\nprocfs/devfs/tmpfs/initramfs/sysfs]
  Init --> Drv[Drivers\nvirtio/PCI/AHCI/NVMe/GPU/Net/USB/Audio]
  Init --> Tty[TTY/Terminal\nVT manager + renderer]
  Init --> Net[TCP/IP\nARP/IP/ICMP/TCP/UDP/IPv6/DHCP]
  Init --> Sec[Security\nseccomp/cgroups/ns/caps/fw]
  Init --> User[/init]

  MM --> Sched
  Sched --> Vfs
  Vfs --> Sys[Syscall Layer ~200 ABI]
  Sys --> MM
  Sys --> Sched
  Sys --> Vfs
  Sys --> Net
  Sys --> Tty
  Sys --> Sec
  Drv --> Vfs
  Drv --> Net
  Tty --> Fb[Framebuffer/MMIO]
  Drv --> Fb
```

## Memory Management
- Crates: `mm-core` (buddy, zones DMA/Normal/High, orders 0–10), `mm-manager` (global facade + OOM callback), `mm-heap` (linked-list GlobalAlloc with hardening option), `mm-paging` (4-level tables, map/unmap/translate, PHYS_MAP_BASE direct map), `mm-cow` (refcounts + try_claim_exclusive), `mm-vma` (sorted VMAs with flags/types), `mm-pagedb` (struct-page metadata + event ring), `mm-traits` (FrameAllocator + frame watch).
- OOM: `kernel/src/oom.rs` scores processes by allocated frames, skips PID≤1, SIGKILLs worst, signals allocator to retry.
- Heap: first-fit linked list with coalescing; optional redzones/canaries. Page frame database tracks PF_FREE/ALLOCATED/PAGETABLE/COW/LOCKED with diagnostics ring.
- Paging: helpers to map/unmap/update flags and flush TLB (local/all CPUs). COW path uses pagedb flags + refcounts to avoid TOCTOU.

## Scheduler & Idle
- Per-CPU run queues with three classes: RT (SCHED_FIFO/RR), CFS (vruntime heap, 6ms period, 1ms min granularity), Idle.
- ISR `scheduler_tick` charges vruntime (weight-adjusted), checks preemption, and sets need_resched; double-charge prevention flag on each task.
- Work stealing from idle CPUs, per-CPU `GLOBAL_CLOCK` sync (BSP increments), and O(1) PID→CPU/slot hint tables. Idle tasks HLT-loop when no runnable work.

## Tasks, Syscalls, Signals
- Task/process metadata exposed via scheduler helpers; context switch validates RIP/RSP/CS before iretq. Kernel stack + CR3 per task.
- Syscalls: ~200 dispatched Linux x86_64 ABI numbers (read/write/open/mmap/futex/clone3/epoll/timerfd/io_uring/mount/namespace/net/pty/etc.) with helpers in `kernel/syscall/syscall/src/lib.rs`.
- Signals: POSIX set/mask/actions, pending queues, sigaltstack, rt_sigreturn frames; delivery checks pending signals in blocking paths.

## VFS, Procfs, Devfs, Tmpfs
- VFS: trait-based inodes/vnodes, path lookup with symlink/mount crossing, mount flags (ro/nosuid/nodev/noexec/bind/etc.), fd tables, pipes (64KB ring, blocking with scheduler waiters), epoll, io_uring.
- Procfs: dynamic /proc with pid dirs exposing status/cmdline/exe/cwd/fd/maps/stat/statm plus global meminfo/cpuinfo/uptime/loadavg/mounts/filesystems/devices.
- Devfs: defaults /dev/null, zero, console/tty, fb0, random/urandom, kmsg, dsp/mixer, serial, input/, plus registration API.
- Tmpfs: in-memory, owning uid/gid/timestamps; Initramfs: read-only CPIO; Sysfs: minimal static tree for kernel/class/bus placeholders.

## TTY, Terminal, Graphics, Input
- VT manager (6 VTs) with lock-free input rings; VT switch via Ctrl+Alt+F1–F6 using try_write on ACTIVE_VT for ISR safety.
- Terminal: VT100/ANSI emulator with alternate screen, 256 colors, mouse modes, 10k-line scrollback; renderer uses RAM back buffer + per-row dirty tracking → blits to framebuffer.
- Framebuffer: Linear fb trait with width/height/stride/format and flush callback for GPU; devfs /dev/fb0 exposes screen/fixed info and read/write mmap-style access.
- Input: keyboard handler tracks modifiers, maps Ctrl combos, emits VT switches and ANSI sequences; mouse handled via separate lock. Compositor (optional tiling) supports fullscreen/HSplit/VSplit/Quad and focus cycling.

## Networking
- TCP/IP stack (`kernel/net/tcpip`): ARP, IPv4/IPv6, ICMP/ICMPv6, UDP, TCP, conntrack, filters; dhcp_client for address acquisition. Sockets wired through syscall layer (bind/listen/accept/send/recv, etc.).

## Drivers & I/O
- Driver crates: virtio (core/net/blk/gpu/snd/input), AHCI, NVMe, Bochs display, UART 8250, PCI, XHCI USB, audio (Intel HDA, VirtIO-snd), storage/usb MSC, driver-core/traits for registration.
- Block layer (`block`, `gpt`), audio, media, usb, pci glue; virtio-emu/vmm/vmx for virtualization hooks.

## Security, Namespaces, Containers
- Security modules: crypto/x509/trust/quarantine; seccomp, namespaces (mount/net/uts/pid), cgroups; firewall syscalls FW_*; capabilities (capget/capset).

## Async & Eventing
- Epoll, io_uring, eventfd, timerfd; async crate present; poll/select still supported for compatibility.

## Filesystem Mount Flow
- Early boot mounts initramfs/root, then devfs/procfs/sysfs/tmpfs; mount/unmount exposed via syscalls with flag handling and per-mount refcounts.

## Key Counts & Limits
- Syscalls handled: ~200; CPUs: up to 256; VT count: 6; Pipe buffer: 64KB; Scrollback: 10k lines; Buddy orders: 0–10 (4KB–4MB).
