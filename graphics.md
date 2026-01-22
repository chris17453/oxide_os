## Oxid Graphics Subsystem — Implementation Plan

### Phase 1: Foundation

**1.1 GOP Framebuffer Preservation (Bootloader)**
- Modify bootloader to query GOP before `ExitBootServices()`
- Stash: base address, stride, width, height, pixel format
- Mark framebuffer region as reserved in memory map passed to kernel
- Define shared struct for bootloader→kernel handoff

**1.2 Kernel Memory Integration**
- Ensure physical memory allocator respects reserved regions
- Map GOP framebuffer into kernel virtual address space (write-combining if possible)
- Expose static accessor for early boot console use

---

### Phase 2: Trait Definitions

**2.1 Core Display Trait**
```
Display
  get_info() -> DisplayInfo
  get_modes() -> Vec<Mode>  // or static slice
  set_mode(Mode) -> Result<(), DisplayError>
  framebuffer() -> &mut [u8]
  flush(Option<Rect>) -> Result<(), DisplayError>
```

**2.2 Supporting Types**
- `DisplayInfo`: width, height, stride, pixel_format,

- `Mode`: width, height, refresh (optional)
- `PixelFormat`: enum (Bgr8, Rgb8, Bgrx8, etc.)
- `DisplayError`: enum (Unsupported, DeviceLost, InvalidParameter)

**2.3 Optional 3D Trait (stub for now)**
```
GpuContext
  create_context() -> Result<ContextHandle>
  submit_command_buffer(&[u8]) -> Result<FenceId>
  wait_fence(FenceId) -> Result<()>
```

---

### Phase 3: GopDisplay Backend

**3.1 Implementation**
- Wrap bootloader-provided framebuffer info
- `get_info()`: return static config
- `get_modes()`: return single-element slice (current mode only)
- `set_mode()`: return `Err(Unsupported)`
- `framebuffer()`: return slice over mapped region
- `flush()`: no-op, return `Ok(())`

**3.2 Validation**
- Boot kernel, write test pattern to framebuffer
- Confirm pixels appear on screen (QEMU or real hardware)

---

### Phase 4: PCI Enumeration

**4.1 PCI Bus Scanner**
- Enumerate PCI configuration space (CAM or ECAM depending on platform)
- Build device list: vendor, device, class, BARs
- Identify VirtIO devices (vendor `0x1AF4`)

**4.2 VirtIO-GPU Detection**
- Device ID `0x1050` (transitional) or `0x1040` + subsystem (modern)
- Extract BAR addresses for virtqueue and control registers

---

### Phase 5: VirtIO Core Infrastructure

**5.1 Virtqueue Implementation**
- Descriptor table, available ring, used ring
- Memory allocation for queue buffers (DMA-safe, physically contiguous or scatter-gather)
- Notification mechanism (write to queue notify register)

**5.2 VirtIO Device Initialization Sequence**
- Reset device
- Set `ACKNOWLEDGE` + `DRIVER` status bits
- Negotiate feature bits
- Configure virtqueues (controlq, cursorq)
- Set `DRIVER_OK`

---

### Phase 6: VirtIO-GPU 2D Backend

**6.1 Resource Management**
- `RESOURCE_CREATE_2D`: allocate host-side resource (R8G8B8A8 or similar)
- `RESOURCE_ATTACH_BACKING`: attach guest physical pages
- `SET_SCANOUT`: bind resource to display output

**6.2 Display Trait Implementation**
- `get_info()`: query from device or return configured mode
- `get_modes()`: enumerate via `GET_DISPLAY_INFO` command
- `set_mode()`: `SET_SCANOUT` with new dimensions (if supported)
- `framebuffer()`: return guest-side shadow buffer
- `flush()`: `TRANSFER_TO_HOST_2D` + `RESOURCE_FLUSH`

**6.3 Command Submission**
- Build command structs in controlq buffer
- Ring doorbell
- Poll or wait for completion in used ring

---

### Phase 7: DisplayManager

**7.1 Probe Logic**
```
DisplayManager::init()
  if virtio_gpu_present():
    return VirtioGpuDisplay::init()?
  else:
    return GopDisplay::new(bootloader_gop_info)
```

**7.2 Runtime Interface**
- Hold `Box<dyn Display>` or enum dispatch (avoid vtable if perf-sensitive)
- Forward all trait calls to active backend
- Expose to rest of kernel via static singleton or explicit handle

---

### Phase 8: Integration & Testing

**8.1 QEMU Test Matrix**
| Config | Expected Backend |
|--------|------------------|
| `-vga std` | GOP |
| `-device virtio-gpu-pci` | VirtIO-GPU |
| `-vga none -device virtio-gpu-pci` | VirtIO-GPU |

**8.2 Test Cases**
- Solid color fill
- Gradient pattern
- Text rendering (if you have font rasterizer)
- Mode enumeration printout
- Flush
  
  
  
  
  latency measurement (
  
  
  
  optional)

**8.3 Failure Modes**
- VirtIO init fails → fallback to GOP
- GOP info missing → panic with clear message
- Invalid pixel format → convert or reject

---

### Phase 9: Future Work (Out of Scope for Initial Bring-up)

- VirtIO-GPU 3D (virgl contexts)
- Cursor plane support
- Multi-head / multi-monitor
- Userspace framebuffer access (syscall interface)
- DRM-like abstraction for eventual userspace compositor

---

### Dependencies / Prerequisites

| Phase | Requires |
|-------|----------|
| 1 | UEFI bootloader with GOP access |
| 2 | None (pure Rust types) |
| 3 | Phase 1, 2 |
| 4 | PCI ECAM base address from ACPI/DT or hardcoded |
| 5 | Phase 4, DMA-capable allocator |
| 6 | Phase 2, 5 |
| 7 | Phase 3, 6 |
| 8 | All above |

---

### Open Questions for You

1. **Pixel format flexibility**: Support multiple formats with conversion, or mandate one (e.g., BGRX8888) and require bootloader to set it?

2. **DMA allocator**: Do you have one, or does Phase 5 need to include building it?

3. **Virtqueue buffer strategy**: Static pre-allocated pool, or dynamic allocation per command?

4. **Error handling model**: Panic on graphics failure, or degrade to headless?

Let me know which phase you want to start with, or if any phase needs expansion.