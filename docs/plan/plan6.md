# Plan: Replace UEFI Crate + Own Display Driver Stack

## Context

The `uefi` crate (v0.32) manages memory behind our back via `global_allocator`, abstracts away UEFI protocol access through layers of Rust generics, and has deprecated APIs we're already suppressing with `#![allow(deprecated)]`. At 256M RAM, OVMF's GOP allocation choices + our VirtIO-GPU SET_SCANOUT caused a black screen. The root problem: we don't own our memory or our display.

**Goal:** Replace the `uefi` crate with raw UEFI FFI (we talk to the firmware directly), and add kernel display drivers (bochs-display + fix VirtIO-GPU) so the kernel owns the display pipeline.

## Part 1: Raw UEFI Bootloader (replace `uefi` crate)

### New crate: `bootloader/uefi-raw/`

Minimal `#![no_std]` crate with raw UEFI type definitions — pure C FFI, no allocator, no magic.

**Types to define (~400 lines):**

```
EfiHandle, EfiStatus, EfiGuid, EfiTableHeader
EfiSystemTable        — pointers to BootServices, RuntimeServices, ConfigurationTable, ConOut
EfiBootServices       — function pointers: AllocatePages, GetMemoryMap, LocateProtocol,
                        OpenProtocol, ExitBootServices, SetWatchdogTimer
EfiSimpleTextOutput   — OutputString function pointer (for boot messages)
EfiGraphicsOutput     — QueryMode, SetMode, Blt, Mode (current mode + framebuffer base/size)
EfiGraphicsOutputMode — MaxMode, Mode, Info pointer, FrameBufferBase, FrameBufferSize
EfiGraphicsOutputModeInfo — HorizontalRes, VerticalRes, PixelFormat, PixelsPerScanLine
EfiPixelFormat        — PixelRedGreenBlueReserved8BitPerColor, PixelBlueGreenRedReserved8BitPerColor, etc.
EfiBltPixel           — Blue, Green, Red, Reserved (BGRA order)
EfiSimpleFileSystem   — OpenVolume function pointer
EfiFileProtocol       — Open, Close, Read, GetInfo function pointers
EfiFileInfo           — Size, FileSize, PhysicalSize, etc.
EfiMemoryDescriptor   — Type, PhysicalStart, VirtualStart, NumberOfPages, Attribute
EfiMemoryType         — enum (ConventionalMemory, LoaderData, BootServicesCode, etc.)
EfiConfigurationTable — VendorGuid, VendorTable pointer
```

**GUIDs to define:**
- `EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID`
- `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID`
- `EFI_FILE_INFO_GUID`
- `EFI_ACPI_TABLE_GUID` (ACPI 1.0)
- `EFI_ACPI2_TABLE_GUID` (ACPI 2.0)

**CStr16 helper:** Simple `&[u16]` wrapper + `const fn` to convert ASCII literals to UCS-2 at compile time (for file paths like `\\EFI\\OXIDE\\kernel.elf`).

**No allocator.** The bootloader uses `AllocatePages` directly for everything. No `alloc` crate, no `Vec`, no `format!`. Boot messages go through `ConOut->OutputString` or raw serial.

### Rewrite `bootloader/boot-uefi/src/main.rs`

**Entry point:** Replace `#[entry]` macro with raw:
```rust
#[no_mangle]
pub extern "efiapi" fn efi_main(image_handle: EfiHandle, system_table: *const EfiSystemTable) -> EfiStatus
```

**Function-by-function replacement:**

| Current (uefi crate) | New (raw FFI) |
|---|---|
| `uefi::helpers::init()` | Store `system_table` pointer in static |
| `system_table_boot()` | Read the stored static pointer |
| `bs.get_handle_for_protocol::<GOP>()` | `(bs.LocateProtocol)(&GOP_GUID, null, &mut proto)` |
| `bs.open_protocol_exclusive::<GOP>(h)` | Already have proto pointer from LocateProtocol |
| `gop.current_mode_info()` | Read `(*gop).Mode` → `(*mode).Info` |
| `gop.frame_buffer()` | Read `(*gop).Mode` → `FrameBufferBase` + `FrameBufferSize` |
| `gop.modes()` | Loop `0..(*gop).Mode.MaxMode`, call `(gop.QueryMode)(i, &size, &info)` |
| `gop.blt(BltOp::VideoFill)` | `(gop.Blt)(gop, &pixel, EfiBltVideoFill, ...)` |
| `bs.allocate_pages(AnyPages, LoaderData, n)` | `(bs.AllocatePages)(AllocateAnyPages, LoaderData, n, &mut addr)` |
| `bs.memory_map(LoaderData)` | `(bs.GetMemoryMap)(&mut size, buf, &mut key, &mut desc_size, &mut ver)` |
| `st.exit_boot_services(LoaderData)` | `(bs.ExitBootServices)(image_handle, map_key)` |
| `fs.open_volume()` | `(sfs.OpenVolume)(sfs, &mut root)` |
| `root.open(path, Read, empty)` | `(root.Open)(root, &mut file, path, EFI_FILE_MODE_READ, 0)` |
| `file.get_info::<FileInfo>(buf)` | `(file.GetInfo)(file, &FILE_INFO_GUID, &mut size, buf)` |
| `file.read(&mut data)` | `(file.Read)(file, &mut size, buf)` |
| `st.config_table()` | Read `(*st).ConfigurationTable` array, iterate `NumberOfTableEntries` |
| `st.stdout().write_str(s)` | `(conout.OutputString)(conout, ucs2_buf)` |
| `cstr16!("\\EFI\\...")` | `const UCS2_PATH: &[u16] = &[0x5C, 0x45, 0x46, ...]` or helper macro |
| `alloc::vec![0u8; size]` | `AllocatePages` + raw pointer (no alloc crate) |

**Memory strategy:** All allocations via `AllocatePages` with `LoaderData` type. No heap allocator. Kernel/initramfs loaded into page-aligned buffers allocated directly from UEFI.

**Paging (paging.rs):** Replace 2 calls: `system_table_boot()` → stored static, `bs.allocate_pages()` → raw `AllocatePages` call. Rest is pure pointer math (unchanged).

### Files

| Action | Path |
|--------|------|
| **Create** | `bootloader/uefi-raw/Cargo.toml` |
| **Create** | `bootloader/uefi-raw/src/lib.rs` — all UEFI FFI types |
| **Create** | `bootloader/uefi-raw/src/protocols.rs` — GOP, SimpleFileSystem, File protocol structs |
| **Create** | `bootloader/uefi-raw/src/guid.rs` — GUID type + protocol GUIDs |
| **Create** | `bootloader/uefi-raw/src/cstr16.rs` — UCS-2 string helpers |
| **Rewrite** | `bootloader/boot-uefi/Cargo.toml` — drop `uefi` dep, add `uefi-raw` |
| **Rewrite** | `bootloader/boot-uefi/src/main.rs` — raw FFI calls |
| **Modify** | `bootloader/boot-uefi/src/paging.rs` — 2 call sites |
| **Modify** | `Cargo.toml` (workspace) — add `bootloader/uefi-raw` member |

---

## Part 2: Bochs Display Driver (kernel)

### New crate: `kernel/drivers/gpu/bochs-display/`

PCI device `1234:1111` — the device behind QEMU's `-vga std`.

**How it works:**
- BAR0 = linear framebuffer (write pixels, they appear on screen)
- DISPI registers at I/O ports `0x01CE` (index) / `0x01CF` (data)
- Mode set: disable → write xres/yres/bpp → enable with LFB flag

**Key registers:**
```
INDEX_ID=0x00  INDEX_XRES=0x01  INDEX_YRES=0x02  INDEX_BPP=0x03
INDEX_ENABLE=0x04  INDEX_VIRT_WIDTH=0x06  INDEX_VIRT_HEIGHT=0x07
ENABLE_DISABLED=0x00  ENABLE_ENABLED=0x01  ENABLE_LFB=0x40
```

**Implementation (~200 lines):**
- `BochsDisplay` struct: bar0_phys, fb_virt, width, height, stride
- `from_pci()`: read BAR0, verify DISPI ID (`0xB0C0..0xB0CF`), compute virtual addr
- `set_mode()`: DISPI register sequence (disable → set → enable)
- `framebuffer_info()`: returns `FramebufferInfo` for fb subsystem
- `take_over_display()`: public fn for display manager
- `PciDriver` impl: probe vendor `0x1234` device `0x1111`
- No flush callback needed — direct MMIO framebuffer (same as GOP)

### Files

| Action | Path |
|--------|------|
| **Create** | `kernel/drivers/gpu/bochs-display/Cargo.toml` |
| **Create** | `kernel/drivers/gpu/bochs-display/src/lib.rs` |
| **Modify** | `Cargo.toml` (workspace) — add member |
| **Modify** | `kernel/Cargo.toml` — add dependency + `extern crate` |

---

## Part 3: VirtIO-GPU as Primary Display

### Changes to `kernel/drivers/gpu/virtio-gpu/src/lib.rs`

1. **Remove** the "skip if GOP exists" guard in `init_from_pci()` — always init the device
2. **Add** `pub fn take_over_display() -> Option<FramebufferInfo>`:
   - Calls `setup_framebuffer()` if not already done
   - Registers `gpu_flush_region` callback with fb subsystem
   - Returns `FramebufferInfo` pointing to the GPU's DMA-backed resource buffer
3. The driver's `setup_framebuffer()` already does: CREATE_2D → ATTACH_BACKING → SET_SCANOUT
4. After display takeover, `terminal::update_framebuffer()` repaints into the new buffer
5. `flush_region()` sends TRANSFER_TO_HOST_2D + RESOURCE_FLUSH (already implemented)

---

## Part 4: Display Takeover in Kernel Init

### Changes to `kernel/src/init.rs`

Add `display_takeover()` after `driver_core::probe_all_devices()`:

```
Priority order:
1. Bochs/VGA std (direct MMIO — fastest, no GPU commands needed)
2. VirtIO-GPU (needs TRANSFER_TO_HOST_2D per frame, but has dirty-region support)
3. Keep UEFI GOP (fallback — already working from bootloader)
```

**Sequence:**
- Try `bochs_display::take_over_display()` → if Some, call `fb::init()` + `terminal::update_framebuffer()`
- Else try `virtio_gpu::take_over_display()` → same
- Else keep GOP (log message, no action)

**Why bochs first:** Direct linear framebuffer = zero overhead. VirtIO-GPU requires 2 virtqueue commands per flush. For a hobby OS on QEMU, direct MMIO is simpler and faster. VirtIO-GPU is the fallback for configs without `-vga std`.

---

## Part 5: fb Subsystem Changes

### Changes to `kernel/graphics/fb/src/lib.rs`

- Add `pub fn replace_framebuffer(info: FramebufferInfo)` — updates global `FRAMEBUFFER` + `FB_PHYS_BASE`
- Or just make `init()` also update `FB_PHYS_BASE` (currently only set by `init_from_boot`)

### Changes to `kernel/tty/terminal/`

- Verify `terminal::update_framebuffer(Arc<dyn Framebuffer>)` exists or add it
- Must trigger full repaint to the new buffer so display isn't blank after switch

---

## Implementation Order

1. **uefi-raw crate** — type definitions only, can build/test independently
2. **Rewrite boot-uefi** — replace `uefi` dep with `uefi-raw`, rewrite main.rs + paging.rs
3. **Build + boot test** — verify kernel still boots with raw UEFI bootloader
4. **bochs-display driver** — new kernel crate
5. **VirtIO-GPU changes** — remove skip guard, add take_over_display()
6. **display_takeover()** — wire into init.rs
7. **Build + test** — verify display works at 256M with both `-vga std` and `-device virtio-gpu-pci`

## Verification

- `make build` compiles cleanly
- Boot with `make run` (512M, `-vga std` + virtio-gpu) — display works
- Boot with `make run-256m` (256M, `-vga std` + virtio-gpu) — display works (was broken before)
- Boot without `-vga std` (virtio-gpu only) — VirtIO-GPU takes over as primary
- Serial log shows `[DISPLAY]` messages indicating which driver was selected
- Keyboard works in all configurations
