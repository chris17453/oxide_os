# UEFI GOP vs VirtIO-GPU SET_SCANOUT Conflict

## The Rule
**NEVER send VirtIO-GPU SET_SCANOUT if a UEFI GOP framebuffer already exists.**

## What Happens
1. OVMF (UEFI firmware) may use VirtIO-GPU as the GOP provider, creating **resource A** bound to scanout 0
2. Kernel receives the GOP framebuffer address — this points to resource A's backing memory
3. If our VirtIO-GPU driver then calls `setup_framebuffer()`, it creates **resource B** (blank) and sends SET_SCANOUT
4. SET_SCANOUT replaces resource A with resource B on scanout 0
5. QEMU now displays resource B (empty/black) instead of resource A
6. Kernel keeps writing to resource A's address → content is written but never displayed
7. **Result: black screen, all rendering pipeline works but nothing visible**

## Why 256M vs 512M Matters
OVMF's GOP provider selection depends on available memory and PCI enumeration order:
- At **256M**: OVMF may select VirtIO-GPU for GOP (resource A on VirtIO-GPU scanout)
- At **512M**: OVMF selects VGA std for GOP (resource A on VGA device, VirtIO-GPU unused)

When OVMF uses VGA std, our SET_SCANOUT on VirtIO-GPU doesn't affect the displayed scanout, so the bug is hidden.

## The Fix
In `init_from_pci()`, check `fb::framebuffer().is_some()` before initializing the VirtIO-GPU driver:

```rust
if fb::framebuffer().is_some() {
    // GOP framebuffer already active — skip init to prevent SET_SCANOUT
    // from stealing the display
    return Ok(());
}
```

## Why This Also Kills Keyboard Input
When VirtIO-GPU's SET_SCANOUT corrupts the display, the system is still running — but at 256M RAM, the failed VirtIO-GPU init may also consume DMA memory or disrupt PCI BAR mappings that VirtIO-input shares. The symptom: keyboard dies after ~10 keypresses (VirtIO input queue exhaustion or BAR conflict).

## Diagnostic
- Serial log shows all rendering pipeline working (BLIT events, terminal writes)
- VNC/screendump shows black screen
- Keyboard events stop after ~10 presses
- Removing `-device virtio-gpu-pci` from QEMU flags fixes both display and keyboard

— GlassShift: the GPU stole the display and took the keyboard hostage as collateral
