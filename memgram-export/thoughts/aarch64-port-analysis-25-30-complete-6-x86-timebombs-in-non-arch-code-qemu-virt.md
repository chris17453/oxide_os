# aarch64 port analysis: ~25-30% complete, 6+ x86 timebombs in non-arch code, QEMU virt is best test path

| Field | Value |
|-------|-------|
| ID | `ae4d0c2b88bc` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-07T12:53:32.751885+00:00 |
| Accessed | 0 times |
| Keywords | aarch64, raspberrypi, arm64, port, qemu, gic, pl011, timebomb, architecture |
| Session | [d9de09e11116](../sessions/analyze-what-it-would-take-to-port-oxide-os-to-raspberry-pi-aarch64.md) |

## Content

Full analysis of what it takes to port OXIDE OS to Raspberry Pi / aarch64. Key findings: (1) arch-traits HAL is well-designed, (2) aarch64 crate has TLB/cache/atomics/context frames done but exception vectors, GIC, syscall handler, PL011 UART, timer driver, PSCI AP boot are all missing, (3) x86 port I/O leaked into terminal/lib.rs, renderer.rs, fb/lib.rs, ps2 driver, virtio-blk debug, vfs/file.rs — all outb(0x3F8) calls, (4) QEMU -M virt with edk2-aarch64 UEFI firmware is the easiest test path (reuses UEFI boot flow + VirtIO drivers), (5) Estimated 26-40 hours for minimal QEMU boot. Plan saved to session plan.md.
