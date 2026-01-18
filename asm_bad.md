# Assembly Refactor Audit

- `bootloader/efflux-boot-uefi/src/main.rs`: Inline `asm!` for CR3 switch and kernel jump sits inside `main`; move into an arch/boot shim module or dedicated boot asm helper crate to isolate unsafe register choreography.
- `crates/arch/efflux-arch-x86_64/src/syscall.rs`: Naked syscall entry routine hand-codes stack/GS handling; extract into a small arch syscall-entry submodule or `.S` file with a safe Rust façade to reduce duplication and make calling convention changes auditable.
- `crates/arch/efflux-arch-x86_64/src/usermode.rs`: Naked iretq transitions live inline; wrap in an arch context-switch module or external asm with Rust wrappers so usermode entry/return paths are centralized and testable.
- `crates/arch/efflux-arch-x86_64/src/exceptions.rs`: Macros emit naked interrupt stubs directly; consider moving stub prologues/epilogues into a shared arch exception shim (global asm or dedicated module) and expose typed Rust handlers to constrain unsafe surface.
- `crates/arch/efflux-arch-x86_64/src/lib.rs`: Port I/O and interrupt flag helpers directly embed `asm!`; prefer routing through a small arch I/O primitives module (or reusing one if present) to keep inline asm localized.
- `crates/mm/efflux-mm-paging/src/mapper.rs`: Uses `asm!` for `invlpg`/`cr3` access inside an arch-neutral MM crate; refactor these TLB and CR3 operations behind an arch trait or x86_64-specific helper to decouple paging logic from ISA-specific asm.
- `crates/drivers/serial/efflux-driver-uart-8250/src/lib.rs`: Performs raw port I/O with inline `asm!`; should call arch-provided port read/write helpers so the driver stays portable and the asm is confined to the arch layer.
- `userspace/init/init.S`: Standalone userspace assembly program; if the goal is to keep assembly inside crates, mirror this in a userspace crate with a minimal Rust entry that links to an asm module instead of top-level `.S`.
