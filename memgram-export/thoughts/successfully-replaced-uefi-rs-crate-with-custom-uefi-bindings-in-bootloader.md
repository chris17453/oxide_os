# Successfully replaced uefi-rs crate with custom UEFI bindings in bootloader

| Field | Value |
|-------|-------|
| ID | `dd3fe997363c` |
| Type | decision |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T12:01:54.999829+00:00 |
| Accessed | 0 times |
| Keywords | uefi, bootloader, custom-bindings, zero-alloc, scratch-arena, FmtBuf |
| Files | `bootloader/boot-uefi/src/efi/mod.rs`, `bootloader/boot-uefi/src/efi/scratch.rs`, `bootloader/boot-uefi/src/efi/fmt.rs`, `bootloader/boot-uefi/Cargo.toml`, `mk/kernel.mk` |

## Content

Completed full replacement of the uefi-rs (v0.32) crate with custom #[repr(C)] UEFI 2.10 bindings.

Key changes:
- Created 12 new files in bootloader/boot-uefi/src/efi/ (~1000 lines)
- Rewrote all 9 existing source files to use custom bindings
- Removed `uefi` dependency from Cargo.toml
- Removed `fb` dependency (bootloader didn't use it — was causing duplicate lang item error)
- Changed mk/kernel.mk: -Zbuild-std=core,alloc → -Zbuild-std=core
- Used #[unsafe(no_mangle)] for Rust 2024 edition compatibility
- Added inner unsafe blocks in unsafe fn bodies for Rust 2024 edition

Key architectural decisions:
- ScratchArena: single 2MB bump allocator replaces global_allocator
- FmtBuf<N>: stack-based core::fmt::Write replaces all format!() calls
- Fixed arrays replace all Vec usage: [Segment; 16], [MemoryRegion; MAX], [Entry; 32]
- Raw EFI function pointer calls: ((*protocol).function)(protocol, args...)
- UCS-2 string handling: ascii_to_ucs2() helper + encode_ucs2_const() for literals

Dependency tree: boot-uefi → boot-proto (only workspace crates, zero third-party deps)
Build result: zero errors, zero warnings
