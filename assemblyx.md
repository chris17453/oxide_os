## Architecture-specific assembly and inline asm (needs abstractions)

- `bootloader/boot-uefi/src/main.rs` — inline asm for UEFI exit/stack switch/hlt in bootloader path.
- `kernel/src/process.rs` — inline asm for CR3 switches during fork/exec/clone, TLB shootdowns; needs arch trait.
- `kernel/src/scheduler.rs` — inline asm for CR3 swap on context switch; should call arch paging/context helper.
- `kernel/src/smp_init.rs` — `hlt` loops while waiting for APs; use arch::halt.
- `kernel/src/fault.rs` — CR3 read; replace with arch accessor.
- `kernel/src/console.rs` — `hlt` in panic/idle; use arch::halt.
- `crates/smp/smp/src/tlb.rs` — CR3 read/write for TLB shootdown; should delegate to arch paging.
- `crates/drivers/input/ps2/src/lib.rs` — port I/O in driver; should use arch I/O abstraction.
- `crates/drivers/pci/src/lib.rs` — config-space port I/O; needs arch I/O abstraction.
- `crates/drivers/net/virtio-net/src/lib.rs` — port I/O for legacy virtio; abstract via arch I/O.
- `crates/hypervisor/vmx/src/*` — vmxon/off, vmread/vmwrite, CR0/3/4 saves, sgdt/sidt; should sit behind arch/hypervisor interface.
- `userspace/libc/src/lib.rs` (naked asm entry) and `userspace/libc/src/arch/x86_64/syscall.rs` — syscall trampolines; should be per-arch module.
- `userspace/init/init.S` — test program in assembly; arch-specific sample.
- `apps/gwbasic/**` (watos_main.rs, watos_platform.rs, graphics_backend/watos_vga.rs) — inline port I/O/VGA access; should route through arch/platform abstraction or be gated to x86.
