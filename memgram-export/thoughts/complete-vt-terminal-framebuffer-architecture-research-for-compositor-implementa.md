# Complete VT/Terminal/Framebuffer architecture research for compositor implementation

📌 Pinned

| Field | Value |
|-------|-------|
| ID | `8818999c7b9d` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-06T20:40:34.702275+00:00 |
| Accessed | 0 times |
| Keywords | vt, terminal, framebuffer, compositor, architecture, keyboard, rendering, devfs |
| Session | [719ec7dd6fc5](../sessions/research-vt-terminal-framebuffer-and-compositor-architecture-for-understanding-h.md) |

## Content

## Architecture Summary

### VT Module (kernel/tty/vt/src/lib.rs)

**Key Structs:**
- `VtManager` - manages 6 VT devices behind Mutex lock
  - `vts: [Mutex<VtState>; NUM_VTS]` - per-VT TTY state (lines 94-97)
  - `input_rings: [LockFreeRing; NUM_VTS]` - lock-free input ring buffers (lines 96-97)
- `VtState` - holds Arc<Tty> + vt_num (lines 70-75)
- Global `ACTIVE_VT: spin::RwLock<usize>` (line 513) - tracks which VT is active

**Key Functions:**
- `VtManager::switch_to(vt_num)` - uses try_write() on ACTIVE_VT for ISR safety (lines 190-221)
- `VtManager::push_input(ch)` - push bytes to active VT's lock-free ring (lines 230-315)
- `vt::init() -> Arc<VtManager>` - initializes manager (lines 543-554)
- `vt::get_manager() -> Option<&'static VtManager>` - lock-free access via AtomicPtr (lines 557-564)
- `vt::push_input_global(ch)` - IRQ entry point (lines 577-581)

**Callbacks Set During Init (kernel/src/init.rs lines 101-107):**
- `set_console_write_callback()` - terminal output
- `set_vt_switch_callback(terminal_vt_switch_callback)` - notifies terminal on VT switch (line 113)
- `set_signal_pgrp_callback()` - signal delivery
- `set_signal_pending_callback()` - signal checking
- `set_yield_callback()` - blocking read yield point

**Important:** ACTIVE_VT uses try_write/try_read from ISR context to avoid deadlock.

### Terminal Module (kernel/tty/terminal/src/lib.rs)

**Key Structs:**
- `TerminalEmulator` - main terminal state machine (lines 87-128)
  - `parser: Parser` - VT100 state machine
  - `primary/alternate` - ScreenBuffer (dual-buffering)
  - `scrollback: ScrollbackBuffer` - scroll history
  - `renderer: Renderer` - pixel output
  - `cols, rows` - terminal dimensions in characters
  - `cell_width, cell_height` - pixel dimensions per cell
  
**Global Statics (lines 1779-1782):**
- `TERMINAL: Mutex<Option<TerminalEmulator>>` - global terminal instance
- `TERMINAL_INITIALIZED: AtomicBool` - init flag for ISR-safe check
- `MOUSE_INPUT: Mutex<MouseInputState>` - ISR-facing mirror of mouse state (line 1846) with separate tiny lock

**Public API (all global functions):**
- `terminal::init(fb: Arc<dyn Framebuffer>)` - initialize terminal (lines 1916-1923)
- `terminal::is_initialized() -> bool` - lock-free check (lines 1926-1928)
- `terminal::update_framebuffer(fb)` - hot-swap fb after GPU driver init (lines 1935-1968)
- `terminal::write(data: &[u8])` - write bytes with per-glyph rendering (lines 1982-2035)
- `terminal::try_flush()` - non-blocking flush (uses try_lock)
- `terminal::tick()` - called from timer ISR for cursor blink
- `terminal::set_response_callback()` - register callback for terminal query responses

**Lock Order (critical for ISR safety):**
1. TERMINAL lock (held during write for glyph rendering)
2. MOUSE_INPUT lock (acquired AFTER releasing TERMINAL, never during)
- ISR only touches MOUSE_INPUT via try_lock, never TERMINAL

### Renderer (kernel/tty/terminal/src/renderer.rs)

**Key Structs:**
- `Renderer` (lines 92-124)
  - `fb: Arc<dyn Framebuffer>` - target framebuffer
  - `back_buffer: Option<Vec<u8>>` - software buffer for double-buffering (~3MB at 1024×768)
  - `dirty: DirtyRegion` - track which rows need redraw
  - `blit_y_min/max: Cell<u32>` - blit region tracking
- `DirtyRegion` - tracks which rows are dirty (lines 20-87)

**Key Methods:**
- `Renderer::new(fb)` - constructor, allocates back_buffer (lines 128-165)
- `Renderer::update_framebuffer(fb)` - hot-swap (lines 171-190)
- `Renderer::render_cell(buffer, row, col)` - render one glyph to back_buffer
- `Renderer::scroll_up_pixels(count, bg_color)` - pixel memmove scroll
- `Renderer::flush_fb()` - blit dirty scanlines from back_buffer to hardware fb
- `Renderer::set_pixel()` / `fill_rect()` - primitive drawing

**Important:** Back_buffer already exists — compositor can leverage this. Renderer doesn't know about compositing.

### Framebuffer Module (kernel/graphics/fb/src/framebuffer.rs, kernel/graphics/fb/src/lib.rs)

**Key Traits/Structs:**
- `Framebuffer` trait (lines 24-38 in framebuffer.rs)
  - `fn width() -> u32`
  - `fn height() -> u32`
  - `fn stride() -> u32` (bytes per scanline)
  - `fn buffer() -> *mut u8` - RAW POINTER to pixel buffer
  - `fn size() -> usize` - total size in bytes
  - `fn format() -> PixelFormat`
  - `set_pixel(x, y, color)` / `fill_rect()` / `get_pixel()` etc.

- `FramebufferInfo` struct (lines 8-21)
  - `base: usize` - virtual address
  - `width, height, stride` - dimensions
  - `format: PixelFormat`

- `LinearFramebuffer` - concrete impl wrapping hardware fb (in lib.rs line 25)

**Global Statics (kernel/graphics/fb/src/lib.rs):**
- `FRAMEBUFFER: Mutex<Option<Arc<dyn Framebuffer>>>` (line 55) - singleton instance
- `FB_PHYS_BASE: Mutex<u64>` (line 87) - physical address for /dev/fb0
- `FLUSH_CALLBACK: AtomicPtr<()>` (line 66) - GPU flush hook (VirtIO-GPU sets this)

**Public API:**
- `fb::init_from_boot(boot_fb, phys_map_base, video_modes)` - initialize from bootloader (lines 112-151)
- `fb::is_initialized() -> bool`
- `fb::get_fb_info() -> Option<FbDeviceInfo>` - returns physical/virtual addresses + metadata
- `fb::set_flush_callback(cb: fn(u32,u32,u32,u32))` - register GPU flush (line 71)
- `fb::call_flush_callback()` - call registered flush callback (lines 77-84)

**Key Property:** `buffer()` returns a raw *mut u8 pointer to the MMIO framebuffer. Compositor will wrap VT buffers as Framebuffer trait objects instead.

### DevFS /dev/fb0 (kernel/vfs/devfs/src/devices.rs)

**Current Implementation:**
- FbDevice struct (not shown, presumed char device)
- `write()` - direct memcpy to hardware framebuffer via `fb::get_fb_info().base`
- `read()` - reads from hardware framebuffer
- `ioctl(FBIOGET_VSCREENINFO)` - returns physical framebuffer resolution
- `ioctl(FBIOGET_FSCREENINFO)` - returns physical framebuffer info

**After Compositor:** Will redirect to VT's backing buffer and return viewport dimensions.

### Keyboard Handler (kernel/input/input/src/kbd.rs)

**Key Functions:**
- `process_key_event(keycode, pressed) -> KeyAction` (lines 131-200)
  - Handles modifier tracking (SHIFT, CTRL, ALT) via AtomicBool (lines 50-56)
  - Alt+F1..F6 → calls VT_SWITCH_CALLBACK (lines 291-309)
  - Ctrl+key → control codes (0x01-0x1A)
  - Returns LED changes for driver feedback

**Callbacks (set during init):**
- `set_console_callback(fn(&[u8]))` - push bytes to VT
- `set_vt_switch_callback(fn(usize))` - request VT switch

**VT Switch Flow:** kbd → VT_SWITCH_CALLBACK → vt::switch_to() → vt::ACTIVE_VT.try_write() → terminal_vt_switch_callback()

### Init Sequence (kernel/src/init.rs)

**Terminal Init Chain (approximate):**
1. `arch::serial_init()` - early debug output
2. `fb::init_from_boot(boot_info)` - create LinearFramebuffer, store in FRAMEBUFFER global
3. `terminal::init(fb)` - create TerminalEmulator with hardware fb
4. `vt::init()` - create VtManager
5. `set_console_write_callback()` - wire vt write to terminal output
6. `set_vt_switch_callback()` - wire keyboard Alt+Fn to terminal redraw

**Key Insight:** Terminal is initialized with `Arc<dyn Framebuffer>`, which could be:
- Current: hardware fb from `FRAMEBUFFER.lock()`
- After compositor: wrapper pointing to VT0 backing buffer

### Workspace (Cargo.toml members)

Relevant crates:
- `kernel/tty/vt` - VT manager
- `kernel/tty/terminal` - terminal emulator
- `kernel/graphics/fb` - framebuffer abstraction
- `kernel/vfs/devfs` - device filesystem
- `kernel/input/input` - keyboard/mouse
- `kernel/src/` - kernel main (has init.rs)

**No existing compositor crate.** Will be NEW: `kernel/tty/compositor`

## Compositor Implementation Hooks

From the plan (TILING-VT-COMPOSITOR.md):

**Phase 1 (Foundation):**
- Create `kernel/tty/compositor/src/lib.rs` with:
  - `VtBackingBuffer` struct
  - `Compositor` struct with buffer array
  - `init()` function called after `fb::init_from_boot()`
- Create `VtBackingFramebuffer: impl Framebuffer` wrapper
- Redirect terminal renderer to use VT0 backing fb
- Basic single-VT blit in compositor

**Phase 2 (Multi-VT):**
- `LayoutManager` for tiling layouts
- Dirty-flag-based compositing
- Keyboard shortcuts (Alt+H/V/Q/Enter/Tab)
- Hook into timer tick for blit

**Phase 3 (Graphics Mode):**
- VtMode enum (Text vs Graphics)
- Redirect /dev/fb0 to VT buffer
- Process → VT mapping via controlling TTY

**Phase 4 (Polish):**
- Focus indicators, status bar, crash recovery

## Key Design Patterns

1. **Lock-free Access Pattern:** Use AtomicPtr + Ordering::Acquire/Release for statics that ISR reads
2. **Try-lock in ISR Context:** Never block in ISR; use try_lock/try_write/try_read
3. **Deferred Compositing:** Mark dirty flags in syscall context, composite on timer tick or VT switch
4. **Lock Order:** TERMINAL → MOUSE_INPUT (never reversed)
5. **Double-Buffering Strategy:** Render to back_buffer in RAM, blit to MMIO once (fbcon style)

