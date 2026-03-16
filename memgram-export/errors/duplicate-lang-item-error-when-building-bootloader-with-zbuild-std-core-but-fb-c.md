# Error: Duplicate lang item error when building bootloader with -Zbuild-std=core but fb 

| Field | Value |
|-------|-------|
| ID | `2ad530fe8271` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T12:02:10.670839+00:00 |
| Keywords | build-std, duplicate-lang-item, alloc, fb-crate, uefi-bootloader |

## Error

Duplicate lang item error when building bootloader with -Zbuild-std=core but fb crate (kernel dependency) needs alloc

## Cause

The fb crate (kernel/graphics/fb) uses extern crate alloc with Vec, Arc, etc. When the bootloader depended on fb via Cargo.toml but never actually imported it, the fb crate still pulled in alloc from the pre-built standard library, creating a conflict with the -Zbuild-std=core (which builds core from source). Two different versions of core (pre-built via alloc's dep vs source-built) = duplicate lang item.

## Fix

Removed fb from bootloader/boot-uefi/Cargo.toml since the bootloader never actually used it. The dependency was a leftover. Now the bootloader only depends on boot-proto (which is no_alloc).
