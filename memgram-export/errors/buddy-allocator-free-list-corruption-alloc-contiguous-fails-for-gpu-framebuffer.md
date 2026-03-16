# Error: Buddy allocator free list corruption — alloc_contiguous fails for GPU framebuffe

| Field | Value |
|-------|-------|
| ID | `829efeb56c83` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-14T00:36:38.279426+00:00 |
| Keywords | buddy, dma, virtio, ovmf, bootservices, corruption, alloccontiguous, framebuffer |
| Files | `kernel/src/init.rs`, `kernel/mm/mm-core/src/buddy.rs` |

## Error

Buddy allocator free list corruption — alloc_contiguous fails for GPU framebuffer DMA despite 421MB free RAM

## Cause

OVMF VirtIO drivers leave devices running with DMA virtqueues pointing into BootServices memory after ExitBootServices. Buddy allocator reclaims those pages and writes FreeBlock headers. Still-running devices DMA used ring entries to those pages, overwriting headers. Corrupted free list chains cause alloc_contiguous to fail.

## Fix

Added early_reset_virtio_devices() in kernel/src/init.rs — PCI bus scan before buddy init that writes status=0 to all VirtIO devices' common config to stop DMA. Also added FreeBlock header scrub (zero first 24 bytes) in add_to_zone before inserting reclaimed pages.
