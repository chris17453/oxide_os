# Error: Bootloader fails to load kernel with 'Kernel file not found' — 33MB debug kernel

| Field | Value |
|-------|-------|
| ID | `b8d8bcf60c82` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T13:30:31.397112+00:00 |
| Keywords | bootloader, scratch-arena, kernel-loading, allocate-pages, file-size |

## Error

Bootloader fails to load kernel with 'Kernel file not found' — 33MB debug kernel exceeds 2MB scratch arena

## Cause

load_file_from_esp() allocates file buffer from the 2MB scratch arena (ScratchArena). The kernel ELF in debug mode is 33MB. scratch::alloc_slice(33MB) returns None because 33MB > 2MB. The old uefi-rs code used Vec<u8> which had no size limit.

## Fix

Created load_large_file_from_esp() that uses efi::allocate_pages() directly instead of the scratch arena. Kernel loading now allocates LOADER_DATA pages proportional to file size. The scratch arena remains for small files (config, initramfs headers, directory listings).
