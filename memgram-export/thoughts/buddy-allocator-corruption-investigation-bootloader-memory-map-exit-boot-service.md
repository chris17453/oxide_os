# Buddy allocator corruption investigation - bootloader memory map + exit_boot_services flow

| Field | Value |
|-------|-------|
| ID | `d5a14cc3d271` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T15:30:24.373389+00:00 |
| Accessed | 0 times |
| Keywords | bootloader, memory-map, exit-boot-services, corruption, buddy, canary |
| Files | `bootloader/boot-uefi/src/main.rs`, `bootloader/boot-uefi/src/efi/mod.rs` |

## Content

## Bootloader Memory Map Handling (main.rs:186-239)

The bootloader:
1. Captures memory map BEFORE exit_boot_services (lines 192-239)
2. Reads EFI_CONVENTIONAL_MEMORY → BootMemoryType::Usable (line 221)
3. Reads EFI_BOOT_SERVICES_CODE/DATA → BootMemoryType::BootServices (lines 222-223)
4. Reads EFI_LOADER_CODE/DATA → BootMemoryType::Bootloader (lines 226-227)
5. Everything else → BootMemoryType::Reserved (line 228)

CRITICAL: Memory map is parsed into stack-local array before exit_boot_services.
After exit_boot_services (lines 303-310), the firmware is dead but memory map is already captured.

## exit_boot_services Flow (efi/mod.rs:388-440)
- Retries up to 3 times if map_key goes stale
- On success (line 430): Sets SYSTEM_TABLE = null to prevent further EFI calls
- Returns to bootloader main.rs which then jumps to kernel

Between memory map capture and exit_boot_services:
- Framebuffer info queries (lines 163, 536-562) - read-only
- Video modes enumeration (lines 166, 585-626) - read-only
- Page tables setup (line 169, paging::setup_page_tables)
- Boot info allocation (line 172)
- Stack allocation (line 176)
- Kernel/initramfs allocations (lines 151, 160) - THESE HAPPEN BEFORE MEMORY MAP!

The kernel and initramfs are loaded/allocated BEFORE the memory map is captured. This is safe.
All allocations are done via efi::allocate_pages which returns valid EFI memory handles.
