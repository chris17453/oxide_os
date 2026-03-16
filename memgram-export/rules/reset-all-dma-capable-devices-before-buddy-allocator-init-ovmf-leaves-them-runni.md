# Reset ALL DMA-capable devices BEFORE buddy allocator init — OVMF leaves them running

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `ba34e344c48f` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-14T00:36:48.782070+00:00 |
| Condition | When changing boot initialization order or adding new DMA-capable device drivers |
| Keywords | dma, virtio, buddy, ovmf, bootservices, initorder |
| Files | `kernel/src/init.rs` |

## Details

UEFI firmware (OVMF) leaves VirtIO devices with active DMA virtqueues pointing into BootServices memory. After ExitBootServices, the kernel reclaims those pages via the buddy allocator. Without resetting the devices first, they continue DMA writes that corrupt FreeBlock headers in the buddy free lists. This causes alloc_contiguous to fail silently, breaking GPU framebuffer allocation and potentially any large contiguous allocation. The early_reset_virtio_devices() function in init.rs must run BEFORE buddy allocator initialization.
