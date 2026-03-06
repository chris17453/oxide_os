# OXIDE OS — Tiling VT Compositor

> *"Six virtual terminals walk into a framebuffer. Only one could be seen — until now."*
> — GlassSignal

## 1. Problem Statement

Today, OXIDE OS has 6 virtual terminals (VT0–VT5) but only one can render at a time. The terminal emulator writes glyphs directly to the hardware framebuffer. Graphics apps (`gwbasic`, future SDL/framebuffer apps) write to `/dev/fb0` which is also the raw hardware framebuffer. There is:

- **No isolation** — graphics apps clobber the terminal and vice versa
- **No simultaneous display** — you see VT0 OR VT1, never both
- **No per-VT graphics mode** — no distinction between "this VT runs a terminal" and "this VT runs a graphics app"
- **No splitscreen** — one VT = entire screen, always

## 2. Goals

1. **Per-VT backing buffers** — every VT gets its own pixel buffer in RAM. Terminal VTs render text there. Graphics VTs receive `/dev/fb0` writes there.
2. **Tiling layout engine** — display 1, 2, or 4 VTs simultaneously on screen with keyboard-driven layout switching.
3. **Zero impact on Linux cross-compiled apps** — `/dev/fb0` and `ioctl(FBIOGET_VSCREENINFO)` continue to work identically. Apps don't know they're tiled.
4. **VT mode flag** — each VT is either `Text` (terminal emulator active) or `Graphics` (raw framebuffer mode). Mode switches cleanly.
5. **Clean VT switching** — Alt+Fn switches focus. Graphics VT state survives unfocus. Terminal VT state survives unfocus. Switching is instant (buffer swap, no re-render).

## 3. Non-Goals (for this phase)

- Alpha blending / transparency between VTs
- Window dragging / floating layout
- GPU-accelerated compositing
- Mouse-driven tiling / resize
- Per-window (sub-VT) compositing (that's a future display server)

## 4. Current Architecture (Before)

```
┌──────────────────────────────────────────────────────┐
│                    Hardware MMIO Framebuffer          │
│              1024×768 × 4bpp = ~3MB VRAM             │
└─────────────────────────┬────────────────────────────┘
                          │
            ┌─────────────┴─────────────┐
            │                           │
    Terminal Renderer              /dev/fb0 write()
    (back_buffer → blit)          (direct memcpy)
            │                           │
    Active VT's text only        Any app, raw pixels
```

**Problems:**
- Terminal renderer and `/dev/fb0` both write to the **same physical framebuffer**
- Only ACTIVE_VT's terminal renders (inactive VTs silently skip)
- `/dev/fb0` writes bypass VT ownership entirely — any process can clobber the screen
- VT switch = full terminal re-render (slow, ~50ms for 128×48 grid)

### Key Data Points

| Resource | Value |
|----------|-------|
| Physical framebuffer | 1024×768×32bpp via UEFI GOP |
| Framebuffer size | 1024 × 768 × 4 = **3,145,728 bytes (~3MB)** |
| QEMU RAM | 512MB |
| VT count | 6 (VT0–VT5, mapped to tty1–tty6) |
| Font | 8×16 PSF2 (128 cols × 48 rows) |
| Terminal back buffer | Already exists (Vec\<u8\>, ~3MB per terminal) |
| Frame allocator | Buddy, MAX_ORDER=10, max block = 4MB |
| Alt+Fn | Handled in `input::kbd`, calls `vt::switch_to()` |

## 5. Target Architecture (After)

```
┌──────────────────────────────────────────────────────────────────┐
│                    Hardware MMIO Framebuffer                      │
│                  1024×768 × 4bpp = ~3MB VRAM                     │
└───────────────────────────┬──────────────────────────────────────┘
                            │
                    ┌───────┴───────┐
                    │  COMPOSITOR   │  ← NEW: blits VT buffers
                    │  (blit loop)  │     into viewport rectangles
                    └───────┬───────┘     on the physical fb
                            │
          ┌─────────┬───────┼───────┬──────────┐
          │         │       │       │          │
       VT0 buf  VT1 buf  VT2 buf  VT3 buf  VT4/5 buf
       (Text)   (Text)   (Gfx)    (Text)    (Text)
         │         │       │         │          │
     Terminal  Terminal  /dev/fb0  Terminal  Terminal
     Renderer  Renderer  writes   Renderer  Renderer
```

### Core Concepts

**VT Backing Buffer**: Each VT owns a contiguous pixel buffer allocated from the physical frame allocator at boot. Size = physical framebuffer resolution × 4bpp. All rendering (terminal glyphs, `/dev/fb0` writes) targets this buffer, never the hardware framebuffer directly.

**Compositor**: A kernel subsystem that owns the hardware framebuffer exclusively. It reads from VT backing buffers and blits them into viewport rectangles on the physical framebuffer. It is the **only** thing that writes to MMIO.

**Layout Manager**: Tracks which VTs are visible and their viewport rectangles. Supports four layouts: fullscreen (1 VT), horizontal split (2 VTs), vertical split (2 VTs), quad (4 VTs).

**VT Mode**: Each VT is either `Text` (terminal emulator manages the buffer) or `Graphics` (buffer is raw pixel memory accessible via `/dev/fb0`). Mode is set per-VT and persists across focus changes.

## 6. Detailed Design

### 6.1 VT Backing Buffer Allocation

```rust
// — NeonRoot: every VT gets its own pixel playground. No sharing, no drama.

/// Physical backing buffer for a single VT
pub struct VtBackingBuffer {
    /// Physical address of the buffer (from buddy allocator)
    phys_base: PhysAddr,
    /// Virtual address (via PHYS_MAP_BASE identity map)
    virt_ptr: *mut u8,
    /// Buffer dimensions (matches physical framebuffer at allocation time)
    width: u32,
    height: u32,
    stride: u32,
    bpp: u32,
    /// Total size in bytes
    size: usize,
}
```

**Allocation strategy**: At kernel init (after framebuffer is available), allocate 6 backing buffers from `mm_manager::mm().alloc_contiguous()`. Each buffer = `ceil(fb_size / PAGE_SIZE)` pages. At 1024×768×4bpp this is 769 pages = order 10 buddy block (4MB, with ~900KB slack).

**Memory budget**: 6 VTs × 4MB = **24MB**. With 512MB QEMU RAM, this is **4.7%** — trivially affordable.

**Initialization sequence**:
```
1. Boot → UEFI GOP → framebuffer info available
2. fb::init_from_boot() → LinearFramebuffer created
3. terminal::init(fb) → terminal emulator starts
4. NEW: compositor::init(fb_info) → allocate 6 backing buffers
5. NEW: redirect terminal renderer to VT0's backing buffer
6. NEW: redirect /dev/fb0 writes to process's VT backing buffer
7. NEW: compositor starts blit loop (on VT switch / dirty notification)
```

### 6.2 VT Mode Enum

```rust
// — BlackLatch: a VT is either talking or painting. Never both.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VtMode {
    /// Terminal emulator active — text rendering, ANSI parsing, scrollback
    Text,
    /// Raw graphics mode — /dev/fb0 writes go here, no terminal processing
    Graphics,
}
```

Added to `VtState`:
```rust
struct VtState {
    tty: Arc<Tty>,
    _vt_num: usize,
    mode: VtMode,              // NEW
    backing: *mut VtBackingBuffer, // NEW — pointer to compositor-managed buffer
}
```

**Mode transition**:
- `Text → Graphics`: Triggered by `ioctl(KDSETMODE, KD_GRAPHICS)` on `/dev/ttyN` (Linux-compatible), or by first `/dev/fb0` write from a process on that VT
- `Graphics → Text`: Triggered by `ioctl(KDSETMODE, KD_TEXT)` on `/dev/ttyN`, or when the graphics process exits
- On mode switch, compositor marks the VT's viewport dirty for full reblit

### 6.3 Layout Manager

```rust
// — GlassSignal: screen real estate is the only finite resource that matters.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    /// Single VT fills entire screen
    Fullscreen,
    /// Two VTs stacked vertically (top/bottom)
    HSplit,
    /// Two VTs side by side (left/right)
    VSplit,
    /// Four VTs in a 2×2 grid
    Quad,
}

/// A viewport rectangle in physical screen coordinates
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub struct LayoutManager {
    /// Current layout mode
    layout: Layout,
    /// Which VTs are assigned to which tile slots
    /// Slot 0 = primary (or top-left), Slot 1 = secondary (or top-right), etc.
    slots: [usize; 4],  // VT indices
    /// Focused slot (receives keyboard input)
    focused_slot: usize,
    /// Physical screen dimensions
    screen_width: u32,
    screen_height: u32,
}
```

**Viewport calculation**:
```
Fullscreen: slot[0] → (0, 0, 1024, 768)

HSplit:     slot[0] → (0, 0, 1024, 384)        — top half
            slot[1] → (0, 384, 1024, 384)       — bottom half

VSplit:     slot[0] → (0, 0, 512, 768)          — left half
            slot[1] → (512, 0, 512, 768)         — right half

Quad:       slot[0] → (0, 0, 512, 384)          — top-left
            slot[1] → (512, 0, 512, 384)         — top-right
            slot[2] → (0, 384, 512, 384)         — bottom-left
            slot[3] → (512, 384, 512, 384)       — bottom-right
```

**Scaling strategy for terminal VTs**: When a VT's viewport shrinks (e.g., fullscreen → quad), the terminal re-computes its grid dimensions: `cols = viewport.width / font.width`, `rows = viewport.height / font.height`. The VT backing buffer is always full-resolution; the compositor blits only the viewport-sized region from it. Alternatively (simpler): terminal renders at viewport resolution directly into the top-left corner of its backing buffer.

**Scaling strategy for graphics VTs**: Graphics apps are told their resolution via `ioctl(FBIOGET_VSCREENINFO)`. In fullscreen mode, they see 1024×768. In quad mode, they see 512×384. The app renders at that resolution into its backing buffer, compositor blits 1:1 (no scaling). If the app hard-codes resolution (like mandelbrot.bas at 320×200), the image appears in the top-left corner of the tile — no stretching, consistent with Linux fbdev behavior.

### 6.4 Compositor

```rust
// — SableWire: one ring to blit them all, one ring to find them,
//   one ring to bring them all, and on the framebuffer bind them.

pub struct Compositor {
    /// The real hardware framebuffer — ONLY the compositor touches this
    hw_fb: Arc<dyn Framebuffer>,
    /// Layout manager
    layout: LayoutManager,
    /// Per-VT backing buffers
    vt_buffers: [VtBackingBuffer; NUM_VTS],
    /// Per-VT dirty flags (set by terminal/fb0 writes, cleared by compositor)
    dirty: [AtomicBool; NUM_VTS],
    /// Border color for split mode dividers
    border_color: u32,
    /// Whether compositor needs a full redraw (layout change, VT switch)
    full_redraw: AtomicBool,
}
```

**Blit algorithm** (runs on VT switch, dirty notification, or layout change):
```
fn composite(&mut self) {
    let viewports = self.layout.compute_viewports();

    for (slot_idx, viewport) in viewports.iter().enumerate() {
        let vt_idx = self.layout.slots[slot_idx];
        
        if !self.full_redraw && !self.dirty[vt_idx].swap(false, Ordering::AcqRel) {
            continue;  // — SableWire: clean buffer, skip the blit. efficiency is king.
        }

        let src = &self.vt_buffers[vt_idx];
        
        // Blit from VT backing buffer → hardware framebuffer at viewport offset
        for row in 0..viewport.height {
            let src_offset = (row * src.stride) as usize;
            let dst_offset = ((viewport.y + row) * self.hw_fb.stride()
                            + viewport.x * (self.hw_fb.bpp() / 8)) as usize;
            let row_bytes = (viewport.width * (src.bpp / 8)) as usize;
            
            unsafe {
                core::ptr::copy_nonoverlapping(
                    src.virt_ptr.add(src_offset),
                    self.hw_fb.buffer().add(dst_offset),
                    row_bytes,
                );
            }
        }
    }

    // Draw borders between tiles in split modes
    if self.layout.layout != Layout::Fullscreen {
        self.draw_borders();
    }

    self.full_redraw.store(false, Ordering::Release);
}
```

**Trigger points** (when does compositing happen?):
1. **VT switch** (Alt+Fn) — full redraw, instant
2. **Terminal write** — after rendering glyphs to backing buffer, set dirty flag, compositor blits on next tick
3. **`/dev/fb0` write** — after memcpy to backing buffer, set dirty flag
4. **Layout change** — full redraw with new viewport rectangles
5. **Timer tick** (30Hz, existing `terminal::tick()`) — check dirty flags, composite if needed

**Performance**: At 1024×768×4bpp, a full-screen blit is ~3MB memcpy. On modern x86 with REP MOVSB, this takes ~0.3ms. Even quad mode (4 blits) is ~1.2ms total. At 30Hz tick rate, this is trivially fast.

### 6.5 /dev/fb0 Redirect

**Current** (`kernel/vfs/devfs/src/devices.rs`):
```rust
fn write(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
    let info = self.get_fb_info()?;
    let fb_ptr = info.base as *mut u8;  // ← HARDWARE framebuffer
    unsafe {
        core::ptr::copy_nonoverlapping(buf.as_ptr(), fb_ptr.add(offset), to_write);
    }
}
```

**After**:
```rust
fn write(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
    // — WireSaint: redirect to the calling process's VT backing buffer.
    // The app thinks it's writing to "the screen." Cute.
    let vt_num = current_task_vt();  // get VT from process's controlling TTY
    let vt_buf = compositor::get_vt_buffer(vt_num);

    // Set VT mode to Graphics on first write (auto-detect)
    compositor::set_vt_mode(vt_num, VtMode::Graphics);

    let buf_ptr = vt_buf.virt_ptr;
    unsafe {
        core::ptr::copy_nonoverlapping(buf.as_ptr(), buf_ptr.add(offset), to_write);
    }

    // Mark dirty for compositor
    compositor::mark_dirty(vt_num);
    Ok(to_write)
}
```

**ioctl redirect** — `FBIOGET_VSCREENINFO` returns the VT's **viewport size**, not the physical screen size:
```rust
FBIOGET_VSCREENINFO => {
    let vt_num = current_task_vt();
    let viewport = compositor::get_viewport(vt_num);
    // Return viewport dimensions, not physical fb dimensions
    var_info.xres = viewport.width;
    var_info.yres = viewport.height;
    var_info.xres_virtual = viewport.width;
    var_info.yres_virtual = viewport.height;
    // bpp, color offsets, etc. unchanged
}
```

**`current_task_vt()` implementation**: Look up the current task's controlling TTY → extract VT number. If the process has no controlling TTY (detached), default to VT0 or return an error.

### 6.6 Terminal Renderer Redirect

The terminal `Renderer` currently holds `fb: Arc<dyn Framebuffer>` which points to the hardware framebuffer. Change: the renderer's `fb` should point to a **wrapper** that targets the VT's backing buffer.

**Option A (minimal change)**: The Renderer already has `back_buffer: Option<Vec<u8>>` and blits to `fb` on flush. Replace the blit target: instead of blitting `back_buffer → hw_fb`, blit `back_buffer → vt_backing_buffer`. Then the compositor blits `vt_backing_buffer → hw_fb`.

This adds one extra memcpy but keeps the Renderer changes minimal. The Renderer doesn't need to know about compositing at all — it just needs a different blit destination.

**Option B (zero-copy)**: Eliminate the Renderer's `back_buffer` entirely. Make the Renderer render directly into the VT backing buffer. The compositor then blits from VT buffer to hardware. This saves one memcpy per frame but requires the Renderer to accept a raw pointer instead of `Arc<dyn Framebuffer>`.

**Recommendation**: Start with Option A (extra memcpy is <0.3ms, not a bottleneck). Optimize to Option B later if profiling shows it matters.

### 6.7 Keyboard Shortcuts

| Shortcut | Action | Implementation |
|----------|--------|----------------|
| Alt+F1–F6 | Focus VT 0–5 (existing) | Already implemented in `input::kbd` |
| Alt+Enter | Toggle fullscreen ↔ last split layout | New: `compositor::toggle_fullscreen()` |
| Alt+H | Horizontal split (focused VT + next VT) | New: `compositor::set_layout(HSplit)` |
| Alt+V | Vertical split (focused VT + next VT) | New: `compositor::set_layout(VSplit)` |
| Alt+Q | Quad split (4 VTs) | New: `compositor::set_layout(Quad)` |
| Alt+Tab | Cycle focus between visible tiles | New: `compositor::cycle_focus()` |
| Alt+Shift+F1–F6 | Assign VT to current tile slot | New: `compositor::assign_vt(slot, vt)` |

Focus determines which VT receives keyboard input. In fullscreen mode, focused VT = displayed VT (same as today). In split mode, a thin highlight border indicates the focused tile.

### 6.8 Process → VT Association

Every process needs a VT association so `/dev/fb0` writes go to the right buffer. This already partially exists via the **controlling TTY** mechanism:

```
Process → controlling TTY (/dev/tty1) → VT number (0)
```

**For `/dev/fb0`**: Look up `current_task().controlling_tty` → extract VT index. If no controlling TTY, use VT0 as fallback.

**For `fork()`**: Child inherits parent's controlling TTY → same VT. This is correct — a graphics app that forks still writes to the same VT buffer.

**For `setsid()` + detach**: Process loses controlling TTY. `/dev/fb0` writes should fail or go to VT0. This matches Linux behavior (detached processes can't write to the console).

## 7. Implementation Plan

### Phase 1: VT Backing Buffers (Foundation)

**Goal**: Allocate per-VT pixel buffers, redirect terminal renderer, prove basic operation.

| Task | File(s) | Description |
|------|---------|-------------|
| 1.1 Define `VtBackingBuffer` struct | `kernel/tty/compositor/src/lib.rs` (NEW) | Struct, allocation, deallocation |
| 1.2 Allocate 6 buffers at boot | `kernel/src/init.rs` | After `fb::init_from_boot()`, call `compositor::init()` |
| 1.3 Create `Compositor` struct | `kernel/tty/compositor/src/lib.rs` | Holds buffers, layout, dirty flags |
| 1.4 Implement `VtBackingFramebuffer` | `kernel/tty/compositor/src/backing_fb.rs` (NEW) | `impl Framebuffer for VtBackingFramebuffer` — wraps a VT buffer as a Framebuffer trait object |
| 1.5 Redirect terminal renderer | `kernel/tty/terminal/src/lib.rs` | `terminal::init()` receives VT0's backing fb instead of hardware fb |
| 1.6 Implement basic compositing | `kernel/tty/compositor/src/lib.rs` | Single-VT blit: copy VT0 buffer → hardware fb after each terminal write |
| 1.7 Hook compositor into VT switch | `kernel/src/init.rs` | `terminal_vt_switch_callback` calls `compositor::switch_focus()` then `compositor::composite()` |

**Exit criteria**: Terminal looks and works exactly as before, but rendering goes through VT0 backing buffer → compositor → hardware fb. Visual output identical. No split-screen yet.

### Phase 2: Multi-VT Display & Tiling

**Goal**: Display multiple VTs simultaneously with keyboard-driven layout.

| Task | File(s) | Description |
|------|---------|-------------|
| 2.1 Implement `LayoutManager` | `kernel/tty/compositor/src/layout.rs` (NEW) | Layout enum, viewport computation, slot assignment |
| 2.2 Multi-VT compositing | `kernel/tty/compositor/src/lib.rs` | Blit loop over all visible VT viewports |
| 2.3 Dirty-region compositing | `kernel/tty/compositor/src/lib.rs` | Only reblit VTs with dirty flag set |
| 2.4 Per-VT terminal sizing | `kernel/tty/terminal/src/lib.rs` | Terminal reads viewport size from compositor, adjusts cols/rows |
| 2.5 SIGWINCH on layout change | `kernel/tty/vt/src/lib.rs` | Send SIGWINCH to foreground process group when viewport resizes |
| 2.6 TIOCGWINSZ from viewport | `kernel/tty/vt/src/lib.rs` | Return viewport-derived cols/rows from winsize ioctl |
| 2.7 Border rendering | `kernel/tty/compositor/src/lib.rs` | Draw 1-2px divider lines between tiles, focused tile highlight |
| 2.8 Keyboard shortcuts | `kernel/input/input/src/kbd.rs` | Alt+H/V/Q/Enter/Tab handlers → `compositor::set_layout()` etc. |
| 2.9 Hook into timer tick | `kernel/src/init.rs` | Call `compositor::tick()` from existing 30Hz timer callback to flush dirty VTs |

**Exit criteria**: Can split screen into 2 or 4 tiles, each showing a different VT with independent terminal sessions. Alt+Fn switches focus. Shells resize correctly on layout change.

### Phase 3: Graphics VT Mode & /dev/fb0 Redirect

**Goal**: Graphics apps render to their VT's backing buffer. Text and graphics VTs coexist.

| Task | File(s) | Description |
|------|---------|-------------|
| 3.1 Add `VtMode` enum | `kernel/tty/vt/src/lib.rs` | `Text` / `Graphics` mode per VT |
| 3.2 Redirect `/dev/fb0` writes | `kernel/vfs/devfs/src/devices.rs` | Write to calling process's VT buffer, not hardware fb |
| 3.3 Redirect `/dev/fb0` ioctls | `kernel/vfs/devfs/src/devices.rs` | VSCREENINFO returns viewport size |
| 3.4 Redirect `/dev/fb0` reads | `kernel/vfs/devfs/src/devices.rs` | Read from VT buffer |
| 3.5 Auto-detect graphics mode | `kernel/vfs/devfs/src/devices.rs` | First `/dev/fb0` write sets VT to `Graphics` mode |
| 3.6 Mode restore on exit | `kernel/proc/` | When graphics process exits, reset VT to `Text` mode, trigger terminal re-render |
| 3.7 `KDSETMODE` ioctl | `kernel/tty/vt/src/lib.rs` | Explicit `KD_TEXT` / `KD_GRAPHICS` mode switch (Linux-compatible) |
| 3.8 Mark dirty on fb0 write | `kernel/vfs/devfs/src/devices.rs` | After write, set dirty flag → compositor blits on next tick |
| 3.9 Process → VT mapping | `kernel/proc/` | `current_task_vt()` function: task → controlling TTY → VT number |

**Exit criteria**: GW-BASIC `mandelbrot.bas` renders in one VT tile while bash runs in another. `/dev/fb0` writes appear in the correct tile. Switching layouts works with graphics VTs.

### Phase 4: Polish & Edge Cases

| Task | File(s) | Description |
|------|---------|-------------|
| 4.1 Focus indicator | compositor | Colored border on focused tile (1px bright line) |
| 4.2 VT status bar (optional) | compositor | Thin bar at bottom: "VT0:bash | VT1:bash | VT2:gwbasic | [HSplit]" |
| 4.3 Crash recovery | compositor | If a graphics app crashes, force VT back to Text mode, clear buffer |
| 4.4 Mouse cursor compositing | compositor | Draw mouse cursor on top of compositor output (already exists in fb module) |
| 4.5 VirtIO-GPU flush integration | compositor | Call `flush_callback` after compositing for GPU-backed displays |
| 4.6 Documentation | `docs/` | Update `DRIVES.md`, `DEBUGGING.md`, add `COMPOSITOR.md` |

## 8. File Map (New & Modified)

### New Files
```
kernel/tty/compositor/
├── Cargo.toml                     # New crate: oxide-compositor
├── src/
│   ├── lib.rs                     # Compositor struct, init, composite(), public API
│   ├── layout.rs                  # LayoutManager, Layout enum, Viewport, viewport math
│   └── backing_fb.rs              # VtBackingFramebuffer: impl Framebuffer for backing buffer
```

### Modified Files
```
kernel/Cargo.toml                  # Add oxide-compositor dependency
kernel/src/init.rs                 # Initialize compositor after fb, hook into VT switch + timer
kernel/tty/vt/src/lib.rs           # Add VtMode, backing buffer pointer, KDSETMODE ioctl
kernel/tty/terminal/src/lib.rs     # Accept compositor-provided fb, resize on viewport change
kernel/tty/terminal/src/renderer.rs # Blit to backing buffer instead of hardware fb
kernel/vfs/devfs/src/devices.rs    # Redirect /dev/fb0 read/write/ioctl to VT buffer
kernel/input/input/src/kbd.rs      # New keyboard shortcuts (Alt+H/V/Q/Enter/Tab)
kernel/graphics/fb/src/lib.rs      # Expose buffer() method for compositor direct access
```

## 9. Linux Compatibility Matrix

| Interface | Before | After | App Impact |
|-----------|--------|-------|------------|
| `open("/dev/fb0")` | Opens hardware fb | Opens VT-redirected fb | **None** — transparent |
| `write(/dev/fb0, offset, buf)` | Writes to MMIO | Writes to VT buffer | **None** — same API |
| `read(/dev/fb0, offset, buf)` | Reads from MMIO | Reads from VT buffer | **None** — same API |
| `ioctl(FBIOGET_VSCREENINFO)` | Returns physical resolution | Returns **viewport** resolution | **Positive** — app adapts to tile size |
| `ioctl(FBIOGET_FSCREENINFO)` | Returns physical fb info | Returns VT buffer info | **None** — `smem_len` matches buffer |
| `ioctl(TIOCGWINSZ)` | Returns full terminal size | Returns **viewport** terminal size | **Positive** — shells/apps adapt |
| `ioctl(KDSETMODE)` | Not implemented | Sets VT mode text/graphics | **New Linux-compatible feature** |
| Alt+F1–F6 | Switch active VT | Switch focused VT | **Same behavior** |
| `SIGWINCH` | Not sent on VT switch | Sent on layout change | **Positive** — apps can resize |

**Key guarantee**: Any Linux fbdev app that works today will work after this change without recompilation. Apps that query resolution will automatically adapt to tile sizes. Apps that hard-code resolution will render in the top-left of their tile (same as Linux behavior when an app renders at a smaller resolution than the display).

## 10. Memory Budget

| Allocation | Size | Count | Total |
|-----------|------|-------|-------|
| VT backing buffer (1024×768×4) | 3.1 MB | 6 | **18.8 MB** |
| Buddy allocator overhead (4MB blocks) | 0.9 MB waste per VT | 6 | **5.4 MB** |
| Terminal ScreenBuffer (cell data) | ~100 KB | 6 | **0.6 MB** |
| Terminal back_buffer (redundant after Phase 1 optimization) | 3.1 MB | 1-6 | **0–18.8 MB** |
| **Total (conservative)** | | | **~25 MB** |
| **Total (optimized, no double back_buffer)** | | | **~25 MB** |

With 512MB RAM, this is **~5%**. Acceptable.

**Optimization opportunity**: After Phase 1, the terminal Renderer's `back_buffer` is redundant with the VT backing buffer. Eliminating it saves 3MB per active terminal VT. This is Phase 1 Option B (zero-copy rendering).

## 11. Performance Characteristics

| Operation | Latency | Notes |
|-----------|---------|-------|
| Full-screen composite (1 VT) | ~0.3 ms | 3MB memcpy, REP MOVSB |
| Quad composite (4 VTs) | ~1.2 ms | 4 × 0.75MB memcpy (each VT quarter-screen) |
| Single dirty VT reblit | ~0.3 ms | Only blit the changed tile |
| VT switch (focus change) | ~0.5 ms | Reblit focused VT border + keyboard redirect |
| Layout change | ~1.5 ms | Recompute viewports + full composite + SIGWINCH |
| Terminal glyph render → visible | ~0.6 ms | Render to backing buffer (0.3ms) + composite (0.3ms) |

At 30Hz compositor tick rate, the blit budget per frame is 33ms. Even worst-case quad mode uses 1.2ms = **3.6% of frame budget**. Performance is not a concern.

## 12. Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| Double-buffering latency (extra memcpy) | Low | 0.3ms per frame, imperceptible |
| Memory pressure (24MB for buffers) | Low | 5% of 512MB; lazy allocation if needed |
| Terminal resize bugs on layout change | Medium | Thorough SIGWINCH + TIOCGWINSZ testing |
| Graphics app hard-codes resolution vs viewport | Low | This is the app's problem; same behavior as Linux |
| Race: compositor blitting while terminal writes | Medium | Dirty flags are atomic; compositor reads are non-destructive (memcpy from stable source) |
| ISR calling compositor (VT switch in keyboard ISR) | High | Compositor must be ISR-safe OR defer to workqueue/timer tick. Use atomic dirty flags + deferred compositing. |
| Font scaling in small viewports | Medium | Don't scale fonts — just show fewer cols/rows. Minimum viable tile = 40×12 (320×192px) |

## 13. Future Extensions (Not In Scope)

These become natural follow-ups once the compositor exists:

- **Transparency/alpha**: VT backing buffers get alpha channel, compositor does alpha blend
- **Window manager**: A userspace process on a graphics VT manages sub-windows within its tile
- **GPU compositing**: Use VirtIO-GPU 2D ops for hardware-accelerated blitting
- **Dynamic VT count**: Allocate VTs on demand instead of fixed 6
- **Drag-to-resize**: Mouse-driven tile boundary adjustment
- **Picture-in-picture**: Small overlay VT in corner of fullscreen VT
- **Display server**: Full Wayland-like compositor running as a userspace process on a graphics VT

## 14. Open Questions

1. **Should per-VT buffers be lazily allocated?** (Only allocate when VT is first used, to save memory when only 1-2 VTs are active.) Adds complexity but saves ~12MB for unused VTs.

2. **Should the compositor be its own kernel crate or part of the VT module?** Separate crate is cleaner but adds a dependency. Part of VT keeps it contained.

3. **What's the minimum tile size?** If the user goes quad on 1024×768, each tile is 512×384 = 64×24 terminal. That's usable but tight. Should we enforce a minimum?

4. **Should graphics VTs auto-revert to text mode on process exit, or stay in graphics mode?** Auto-revert is safer (prevents orphaned graphics state). Linux does auto-revert.

5. **How to handle mmap() for /dev/fb0?** Current implementation doesn't support mmap. If added later, the mmap'd region must point to the VT backing buffer, not hardware fb. Deferred — mmap is not currently implemented.

---

*Last updated: 2026-03-06*
*Author: GlassSignal & NeonRoot, OXIDE OS Kernel Team*
