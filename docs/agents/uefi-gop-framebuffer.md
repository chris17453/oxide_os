# UEFI GOP Framebuffer Rule

## Rule
When running under QEMU with OVMF, the display shows the **UEFI GOP memory-mapped framebuffer**, NOT a VirtIO-GPU scanout resource. Do NOT replace the UEFI GOP framebuffer with a VirtIO-GPU DMA buffer — the display will freeze.

## Details

- UEFI GOP framebuffer is at physical ~0x80000000 (memory-mapped by OVMF firmware)
- Writes to this buffer appear on screen **immediately** — no explicit GPU flush commands needed
- VirtIO-GPU's `SET_SCANOUT` to a new resource does NOT take effect — QEMU ignores it and continues displaying from UEFI's GOP memory
- VirtIO-GPU commands (RESOURCE_CREATE_2D, TRANSFER_TO_HOST_2D, RESOURCE_FLUSH) succeed (return OK_NODATA) but don't change what's displayed
- Linux works around this differently (it replaces the scanout during full modesetting), but for our simple framebuffer use case, keeping the UEFI GOP buffer is correct

## What This Means for Code

1. `virtio_gpu::init_from_pci()` must NOT call `fb::init()` to replace the global framebuffer
2. `terminal::update_framebuffer()` should NOT be called during boot — the UEFI GOP fb is already the right one
3. The `fb::set_flush_callback()` GPU callback is unnecessary for the UEFI GOP buffer since writes are immediately visible
4. VirtIO-GPU driver should still probe and init (for future mode switching), but must not replace the active framebuffer

## Verification

Acid test: Write a colored bar to the VirtIO-GPU DMA buffer AND a different colored bar to the UEFI GOP buffer. Only the UEFI GOP bar will appear on the display.

— GlassSignal: The UEFI GOP buffer was always the one true framebuffer. VirtIO-GPU was a lie the hardware told us.
