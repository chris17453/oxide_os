# Graphics Subsystem Migration Analysis

**Document Purpose**: Compare current graphics implementation with proposed VirtIO-GPU architecture and evaluate migration feasibility.

**Date**: 2026-01-22

---

## Executive Summary

The current OXIDE graphics stack is **well-architected** for the proposed migration. The existing `Framebuffer` trait provides a clean abstraction layer that aligns closely with the proposed `Display` trait. Migration is **feasible** and can be accomplished incrementally without breaking existing functionality.

**Key Finding**: ~70% of the infrastructure needed for VirtIO-GPU already exists. The main work involves implementing the VirtIO transport layer and GPU-specific commands.

---

## Current Implementation Analysis

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│ Terminal Layer (crates/terminal)                             │
└────────────────────┬────────────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────────────┐
│ FbConsole (crates/graphics/fb/src/console.rs)               │
│ - 80x25 character grid, ANSI parsing, 8x16 PSF2 glyphs      │
└────────────────────┬────────────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────────────┐
│ Framebuffer trait + LinearFramebuffer                        │
│ (crates/graphics/fb/src/framebuffer.rs)                      │
└────────────────────┬────────────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────────────┐
│ GOP Framebuffer (via UEFI, memory-mapped)                    │
└─────────────────────────────────────────────────────────────┘
```

### Strengths

| Aspect | Status | Notes |
|--------|--------|-------|
| Trait abstraction | ✓ Complete | `Framebuffer` trait allows swappable backends |
| Pixel format handling | ✓ Complete | RGB/BGR, 16/24/32-bit all supported |
| Console layer | ✓ Complete | Decoupled from backend implementation |
| Boot handoff | ✓ Complete | `BootInfo.framebuffer` properly passed |
| Video mode enumeration | ✓ Complete | Up to 64 modes stored in `VideoModeList` |

### Current `Framebuffer` Trait (crates/graphics/fb/src/framebuffer.rs)

```rust
pub trait Framebuffer: Send + Sync {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn format(&self) -> PixelFormat;
    fn stride(&self) -> u32;
    fn buffer(&self) -> *mut u8;
    fn size(&self) -> usize;
    fn set_pixel(&self, x: u32, y: u32, color: Color);
    fn get_pixel(&self, x: u32, y: u32) -> Color;
    fn fill_rect(&self, x: u32, y: u32, w: u32, h: u32, color: Color);
    fn clear(&self, color: Color);
    fn copy_rect(&self, src_x: u32, src_y: u32, dst_x: u32, dst_y: u32, w: u32, h: u32);
    fn hline(&self, x: u32, y: u32, w: u32, color: Color);
    fn vline(&self, x: u32, y: u32, h: u32, color: Color);
    fn draw_rect(&self, x: u32, y: u32, w: u32, h: u32, color: Color);
    fn flush(&self) {}  // No-op for GOP, critical for VirtIO
}
```

---

## Proposed Implementation (from graphics.md)

### Proposed `Display` Trait

```rust
trait Display {
    fn get_info() -> DisplayInfo;
    fn get_modes() -> Vec<Mode>;
    fn set_mode(Mode) -> Result<(), DisplayError>;
    fn framebuffer() -> &mut [u8];
    fn flush(Option<Rect>) -> Result<(), DisplayError>;
}
```

### Key Additions Over Current

| Feature | Current | Proposed |
|---------|---------|----------|
| Mode switching | Not supported | `set_mode()` |
| Partial flush | Full flush only | `flush(Option<Rect>)` |
| Error handling | Implicit | `Result<T, DisplayError>` |
| Mode enumeration | Static slice | Dynamic via `get_modes()` |
| DisplayManager | None | Automatic backend selection |

---

## Gap Analysis

### What Exists (Ready to Use)

| Component | Location | Reusable? |
|-----------|----------|-----------|
| PixelFormat enum | `crates/graphics/fb/src/color.rs` | Yes, directly |
| Color handling | `crates/graphics/fb/src/color.rs` | Yes, directly |
| Console/text layer | `crates/graphics/fb/src/console.rs` | Yes, unchanged |
| Font rendering | `crates/graphics/fb/src/font.rs` | Yes, unchanged |
| Boot handoff structs | `crates/boot/boot-proto/src/lib.rs` | Yes, extend only |
| Video mode storage | `boot-proto::VideoModeList` | Yes, directly |
| GOP backend | `fb/src/framebuffer.rs` | Yes, as fallback |

### What Needs Building

| Component | Effort | Dependencies |
|-----------|--------|--------------|
| **PCI enumeration** | Medium | ECAM base from ACPI |
| **Virtqueue implementation** | High | ✓ DMA allocator (available) |
| **VirtIO device init** | Medium | Virtqueues |
| **VirtIO-GPU commands** | Medium | VirtIO init |
| **Display trait refinement** | Low | None |
| **DisplayManager** | Low | Both backends complete |

### Critical Missing Infrastructure

1. **PCI Bus Scanner**
   - Need to enumerate configuration space
   - Identify VirtIO devices (vendor `0x1AF4`)
   - Extract BAR addresses
   - **Estimated files**: 1-2 new crates

2. **VirtIO Transport Layer**
   - Descriptor table, available ring, used ring
   - Memory allocation for DMA buffers (use existing `alloc_frames()`)
   - Notification mechanism
   - **Estimated files**: 1 crate (~500-800 lines)

3. ~~**DMA-Safe Allocator**~~ ✓ **AVAILABLE**
   - `mm-frame::alloc_frames(count)` provides contiguous physical pages
   - `mm-paging::phys_to_virt()` for kernel access
   - **Status**: Ready to use, no additional work needed

---

## Migration Plan

### Phase 1: Infrastructure (No Breaking Changes)

**Goal**: Build PCI and VirtIO infrastructure without touching existing graphics.

| Task | Description | New Files |
|------|-------------|-----------|
| 1.1 | Create `crates/drivers/pci/` crate | `lib.rs`, `config.rs`, `device.rs` |
| 1.2 | Implement CAM/ECAM enumeration | Within PCI crate |
| 1.3 | Create `crates/drivers/virtio/` core | `lib.rs`, `queue.rs`, `device.rs` |
| 1.4 | Implement virtqueue structures | Within VirtIO crate |
| 1.5 | Test with VirtIO-blk (simpler device) | Optional validation |

### Phase 2: Trait Evolution (Backwards Compatible)

**Goal**: Extend existing traits to support new capabilities.

| Task | Description | Changed Files |
|------|-------------|---------------|
| 2.1 | Add `DisplayInfo` struct | `crates/graphics/fb/src/lib.rs` |
| 2.2 | Add `DisplayError` enum | `crates/graphics/fb/src/lib.rs` |
| 2.3 | Create `Display` trait (extends `Framebuffer`) | `crates/graphics/fb/src/framebuffer.rs` |
| 2.4 | Implement `Display` for `LinearFramebuffer` | Same file |
| 2.5 | Add `set_mode()` stub (returns `Unsupported`) | Same file |

**Backwards Compatibility**: `Display` can extend `Framebuffer`, or `LinearFramebuffer` can impl both.

### Phase 3: VirtIO-GPU Backend

**Goal**: Implement the VirtIO-GPU driver.

| Task | Description | New Files |
|------|-------------|-----------|
| 3.1 | Create `crates/drivers/gpu/virtio-gpu/` | New crate |
| 3.2 | VirtIO-GPU device detection | `device.rs` |
| 3.3 | Resource management (CREATE_2D, ATTACH_BACKING) | `resources.rs` |
| 3.4 | Display commands (SET_SCANOUT, FLUSH) | `display.rs` |
| 3.5 | Implement `Display` trait | `lib.rs` |
| 3.6 | Shadow buffer for `framebuffer()` method | `buffer.rs` |

### Phase 4: DisplayManager Integration

**Goal**: Automatic backend selection with fallback.

```rust
// crates/graphics/fb/src/manager.rs (new file)
pub fn init() -> Box<dyn Display> {
    if let Ok(virtio) = virtio_gpu::probe() {
        Box::new(virtio)
    } else {
        Box::new(GopDisplay::from_bootinfo())
    }
}
```

| Task | Description |
|------|-------------|
| 4.1 | Create `DisplayManager` struct |
| 4.2 | Implement probe logic |
| 4.3 | Update kernel init to use manager |
| 4.4 | Test fallback behavior |

### Phase 5: Validation

| Test Case | QEMU Config | Expected |
|-----------|-------------|----------|
| GOP only | `-vga std` | LinearFramebuffer |
| VirtIO only | `-vga none -device virtio-gpu-pci` | VirtIO-GPU |
| Both available | `-device virtio-gpu-pci` | VirtIO-GPU (preferred) |
| Neither | `-vga none` | Panic with clear message |

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation | Status |
|------|------------|--------|------------|--------|
| VirtIO init complexity | Medium | High | Start with spec-compliant minimal impl | Open |
| DMA allocator missing | ~~Low~~ | ~~High~~ | ~~Verify memory crate capabilities first~~ | ✓ Resolved |
| PCI ECAM address unknown | Medium | Medium | Query ACPI MCFG table | Open |
| Performance regression | Low | Medium | Keep GOP as fallback, benchmark both | Open |
| Console compatibility | Very Low | Low | Console layer is backend-agnostic | Open |

---

## Recommended Approach

### Option A: Incremental (Recommended)

Build infrastructure (Phases 1-2) first, then VirtIO-GPU (Phase 3), then integrate (Phase 4).

**Pros**:
- No disruption to current functionality
- Each phase testable independently
- Clear rollback points
- GOP remains available throughout

**Cons**:
- Longer total time
- Some code may need refactoring later

### Option B: Parallel Development

Build VirtIO-GPU in separate branch, merge when complete.

**Pros**:
- Main branch stays stable
- Can experiment freely

**Cons**:
- Merge conflicts likely
- Duplicate infrastructure work

### Option C: Big Bang Replacement

Replace entire graphics stack at once.

**Pros**:
- Clean design from start

**Cons**:
- High risk of regressions
- No working graphics during development
- Not recommended

---

## DMA Allocator Status (VERIFIED)

**Result**: ✓ **Sufficient for VirtIO-GPU implementation**

### What Exists

| Capability | Status | Location |
|------------|--------|----------|
| Contiguous page allocation | ✓ Available | `crates/mm/mm-frame/src/lib.rs` |
| Physical address retrieval | ✓ Available | `crates/mm/mm-frame/src/bitmap.rs` |
| Phys↔Virt translation | ✓ Available | `crates/mm/mm-paging/src/lib.rs` |
| Scatter-gather structs | ✓ Available | AHCI/NVMe drivers (can adapt) |

### Key APIs for VirtIO

```rust
// Allocate contiguous physical frames (crates/mm/mm-frame)
pub fn alloc_frames(count: usize) -> Option<PhysAddr>;
pub fn free_frames(addr: PhysAddr, count: usize);

// Address translation (crates/mm/mm-paging)
pub const fn phys_to_virt(phys: PhysAddr) -> VirtAddr;
pub const fn virt_to_phys(virt: VirtAddr) -> PhysAddr;
```

### Usage Pattern for VirtIO Queues

```rust
// 1. Allocate contiguous frames for virtqueue
let queue_phys = frame_allocator().alloc_frames(4)?;  // 16KB

// 2. Get kernel-accessible virtual address
let queue_virt = phys_to_virt(queue_phys);
let queue_ptr = queue_virt.as_mut_ptr::<VirtqDesc>();

// 3. Pass physical address to device registers
device.write_queue_addr(queue_phys.as_u64());
```

### What's Missing (Not Needed for VirtIO-GPU)

| Feature | Status | Notes |
|---------|--------|-------|
| Zone-aware allocation (DMA/DMA32) | ✗ Not implemented | VirtIO is 64-bit capable, doesn't need |
| IOMMU integration | ✗ Not implemented | Not required for QEMU virtio |
| Bounce buffers | ✗ Not implemented | Direct access works |
| DMA coherency API | ✗ Not implemented | x86 is cache-coherent |

**Conclusion**: The existing `alloc_frames()` API is sufficient. No additional DMA infrastructure needed for Phase 1.

---

## Open Questions (from graphics.md) - RESOLVED

1. **Pixel format flexibility**
   - **Answer**: Keep current flexibility (already handles multiple formats)
   - VirtIO-GPU will use R8G8B8A8, conversion handled by existing `PixelFormat`

2. **DMA allocator**
   - **Answer**: ✓ Already available via `mm-frame::alloc_frames()`
   - No additional work needed

3. **Virtqueue buffer strategy**
   - **Recommendation**: Static pre-allocated pool (simpler, predictable)
   - Allocate queue buffers at device init, reuse for lifetime

4. **Error handling model**
   - **Recommendation**: Degrade to headless with serial-only output
   - Panic only if serial also unavailable

---

## Effort Estimate

| Phase | Components | Estimated Complexity |
|-------|------------|---------------------|
| Phase 1 | PCI + VirtIO core | High |
| Phase 2 | Trait evolution | Low |
| Phase 3 | VirtIO-GPU driver | High |
| Phase 4 | DisplayManager | Low |
| Phase 5 | Testing | Medium |

**Total New Code**: ~2000-3000 lines across 3-4 new crates

---

## Conclusion

Migration to the VirtIO-GPU architecture is **feasible and well-supported** by the current codebase design. The existing `Framebuffer` trait abstraction was clearly designed with future backends in mind.

**Recommended Next Steps**:
1. Verify DMA allocator capabilities in memory crate
2. Begin PCI enumeration implementation
3. Prototype VirtIO virtqueue with simpler device first (e.g., virtio-rng)
4. Then implement virtio-gpu commands

The GOP fallback ensures the system remains usable throughout development.
