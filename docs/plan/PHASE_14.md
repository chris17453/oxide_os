# Phase 14: Graphics

**Stage:** 3 - Hardware
**Status:** Not Started
**Dependencies:** Phase 8 (Libc + Userland)

---

## Goal

Implement framebuffer graphics with text console.

---

## Deliverables

| Item | Status |
|------|--------|
| UEFI GOP framebuffer | [ ] |
| virtio-gpu driver | [ ] |
| Framebuffer abstraction | [ ] |
| Text console on framebuffer | [ ] |
| Resolution switching | [ ] |
| Basic 2D operations | [ ] |
| Font rendering | [ ] |

---

## Architecture Status

| Arch | Framebuffer | virtio-gpu | Console | Done |
|------|-------------|------------|---------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] |

---

## Framebuffer Interface

```rust
pub trait Framebuffer: Send + Sync {
    /// Get framebuffer dimensions
    fn width(&self) -> u32;
    fn height(&self) -> u32;

    /// Get pixel format
    fn format(&self) -> PixelFormat;

    /// Get stride (bytes per row)
    fn stride(&self) -> u32;

    /// Get raw framebuffer pointer
    fn buffer(&self) -> *mut u8;

    /// Set a pixel
    fn set_pixel(&self, x: u32, y: u32, color: Color);

    /// Fill rectangle
    fn fill_rect(&self, x: u32, y: u32, w: u32, h: u32, color: Color);

    /// Copy rectangle (blit)
    fn copy_rect(&self, src_x: u32, src_y: u32, dst_x: u32, dst_y: u32, w: u32, h: u32);

    /// Flush to display (for double buffering)
    fn flush(&self);
}

#[derive(Clone, Copy)]
pub enum PixelFormat {
    RGB888,      // 24-bit RGB
    RGBA8888,    // 32-bit RGBA
    BGR888,      // 24-bit BGR
    BGRA8888,    // 32-bit BGRA (common on x86)
}

#[derive(Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
```

---

## Graphics Stack

```
┌─────────────────────────────┐
│      Application            │
│   (/dev/fb0, ioctl)         │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│    Framebuffer Core         │
│  - Mode management          │
│  - /dev/fb0                 │
│  - mmap support             │
└──────────────┬──────────────┘
               │
    ┌──────────┴──────────┐
    ▼                     ▼
┌────────────┐    ┌────────────┐
│  UEFI GOP  │    │ virtio-gpu │
│ (bootloader│    │            │
│  provided) │    │            │
└────────────┘    └────────────┘
```

---

## UEFI GOP Framebuffer

```rust
// Passed from bootloader via BootInfo
pub struct GopFramebuffer {
    pub base: PhysAddr,         // Physical address of framebuffer
    pub size: usize,            // Size in bytes
    pub width: u32,             // Horizontal resolution
    pub height: u32,            // Vertical resolution
    pub stride: u32,            // Pixels per scanline
    pub format: PixelFormat,    // Pixel format
}
```

---

## virtio-gpu

```rust
// virtio-gpu control commands
const VIRTIO_GPU_CMD_GET_DISPLAY_INFO: u32 = 0x100;
const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: u32 = 0x101;
const VIRTIO_GPU_CMD_SET_SCANOUT: u32 = 0x103;
const VIRTIO_GPU_CMD_RESOURCE_FLUSH: u32 = 0x104;
const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: u32 = 0x105;
const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING: u32 = 0x106;

// Workflow:
// 1. RESOURCE_CREATE_2D - Create framebuffer resource
// 2. RESOURCE_ATTACH_BACKING - Attach guest memory
// 3. SET_SCANOUT - Connect resource to display
// 4. TRANSFER_TO_HOST_2D - Copy data to host
// 5. RESOURCE_FLUSH - Display the update
```

---

## Text Console

```rust
pub struct FbConsole {
    fb: Arc<dyn Framebuffer>,
    font: &'static Font,
    cols: u32,
    rows: u32,
    cursor_x: u32,
    cursor_y: u32,
    fg_color: Color,
    bg_color: Color,
    buffer: Vec<Cell>,  // Character buffer
}

pub struct Cell {
    ch: char,
    fg: Color,
    bg: Color,
    dirty: bool,
}

impl FbConsole {
    fn putchar(&mut self, ch: char);
    fn scroll(&mut self);
    fn clear(&mut self);
    fn set_cursor(&mut self, x: u32, y: u32);
}
```

---

## Font Format

```rust
// PSF2 font format (Linux console font)
#[repr(C)]
struct Psf2Header {
    magic: [u8; 4],     // 0x72 0xb5 0x4a 0x86
    version: u32,       // 0
    header_size: u32,   // Usually 32
    flags: u32,         // 0x01 = has unicode table
    num_glyphs: u32,    // Number of glyphs
    bytes_per_glyph: u32,
    height: u32,        // Glyph height in pixels
    width: u32,         // Glyph width in pixels
}

// Glyph data follows header
// Each glyph is (height * ceil(width/8)) bytes
```

---

## Key Files

```
crates/graphics/efflux-fb/src/
├── lib.rs
├── framebuffer.rs     # Framebuffer trait
├── console.rs         # Text console
├── font.rs            # Font loading (PSF2)
└── color.rs           # Color handling

crates/drivers/gpu/efflux-virtio-gpu/src/
├── lib.rs
├── commands.rs        # GPU commands
└── resource.rs        # Resource management
```

---

## Framebuffer ioctls

| Name | Description |
|------|-------------|
| FBIOGET_VSCREENINFO | Get variable screen info |
| FBIOPUT_VSCREENINFO | Set variable screen info |
| FBIOGET_FSCREENINFO | Get fixed screen info |
| FBIOPAN_DISPLAY | Pan display |

---

## Exit Criteria

- [ ] Framebuffer from bootloader works
- [ ] Text console renders characters
- [ ] Scrolling works
- [ ] virtio-gpu displays graphics
- [ ] /dev/fb0 accessible from userspace
- [ ] Resolution can be queried
- [ ] Works on all 8 architectures

---

## Test Program

```c
int main() {
    int fd = open("/dev/fb0", O_RDWR);
    if (fd < 0) {
        perror("open");
        return 1;
    }

    struct fb_var_screeninfo vinfo;
    struct fb_fix_screeninfo finfo;

    ioctl(fd, FBIOGET_FSCREENINFO, &finfo);
    ioctl(fd, FBIOGET_VSCREENINFO, &vinfo);

    printf("Resolution: %dx%d, %d bpp\n",
           vinfo.xres, vinfo.yres, vinfo.bits_per_pixel);

    size_t size = finfo.line_length * vinfo.yres;
    uint8_t *fb = mmap(NULL, size, PROT_READ | PROT_WRITE,
                       MAP_SHARED, fd, 0);

    // Draw red rectangle
    for (int y = 100; y < 200; y++) {
        for (int x = 100; x < 300; x++) {
            int offset = y * finfo.line_length + x * 4;
            fb[offset + 0] = 0x00;  // B
            fb[offset + 1] = 0x00;  // G
            fb[offset + 2] = 0xFF;  // R
            fb[offset + 3] = 0x00;  // A
        }
    }

    munmap(fb, size);
    close(fd);
    return 0;
}
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 14 of EFFLUX Implementation*
