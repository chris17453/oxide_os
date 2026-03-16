# OXIDE Boot Manager implementation completed — 8 new modules in bootloader, boot protocol extension, kernel cmdline parser

| Field | Value |
|-------|-------|
| ID | `5dca7852a43b` |
| Type | decision |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T11:04:58.226449+00:00 |
| Accessed | 0 times |
| Keywords | boot-manager, bootloader, uefi, boot-menu, config-parser, cmdline, kernel-discovery, font-renderer |
| Files | `bootloader/boot-uefi/src/font.rs`, `bootloader/boot-uefi/src/config.rs`, `bootloader/boot-uefi/src/discovery.rs`, `bootloader/boot-uefi/src/menu.rs`, `bootloader/boot-uefi/src/input.rs`, `bootloader/boot-uefi/src/editor.rs`, `bootloader/boot-uefi/src/console.rs`, `bootloader/boot-uefi/src/main.rs`, `kernel/boot/boot-proto/src/lib.rs`, `kernel/src/cmdline.rs` |
| Session | [505ce53acb4e](../sessions/implement-oxide-boot-manager-custom-pre-boot-environment-with-graphical-boot-men.md) |

## Content

Implemented a complete GRUB-like pre-boot environment for OXIDE:

## New Bootloader Modules (bootloader/boot-uefi/src/)
1. **font.rs** — 8x16 VGA bitmap font renderer (2KB CP437 font, draw_char/draw_string/fill_rect)
2. **config.rs** — INI-style boot.cfg parser (fixed-size buffers, no heap strings, max 8 entries)
3. **discovery.rs** — Kernel scanning: config-driven + auto-scan fallback via UEFI SimpleFileSystem
4. **menu.rs** — Graphical boot menu with cyberpunk theme (orange/cyan/dark), entry highlighting, options display
5. **input.rs** — Event loop with stall-based timing (100ms poll, 1s countdown ticks)
6. **editor.rs** — Single-line text editor for boot options (256-char buffer, cursor, insert/delete)
7. **console.rs** — UEFI diagnostic console (ls, info, boot, set, reboot, exit commands)

## Boot Protocol Extension (boot-proto/src/lib.rs)
- Added `cmdline: [u8; 256]` and `cmdline_len: u32` to BootInfo
- Added `cmdline()` accessor method returning Option<&str>

## Kernel Side (kernel/src/cmdline.rs)
- Command line parser: debug-*, quiet, nosmp, console=serial, root= options
- Static KernelOptions struct, parsed during early init before SMP

## Build Integration (mk/rootfs.mk, mk/config.mk)
- OXIDE_VERSION extracted from Cargo.toml
- Versioned kernel installed as kernel-{VERSION}.elf
- boot.cfg auto-generated during rootfs creation

## Key Design Decisions
- All boot manager state is stack-allocated (no heap strings in config/menu)
- Polling-based input (UEFI timer events are unreliable across firmware)
- Boot options passed through BootInfo.cmdline (not through memory-mapped region)
- Logo drawing made pub(crate) and shared between menu.rs and main.rs
- Char16 conversion uses `(*ch).into()` not `.to_u16()` (uefi-rs 0.32 API)
