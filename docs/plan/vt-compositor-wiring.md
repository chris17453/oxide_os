# VT ↔ Compositor Full Wiring Plan

## Architecture: Virtual Framebuffers

**Core principle:** Apps NEVER touch hardware framebuffers. Every app gets a
virtual framebuffer (VFB) — a RAM buffer sized to its VT viewport. The
compositor is the sole owner of all hardware framebuffers and blits VFBs into
the correct positions on the correct physical displays.

```
┌─────────────────────────────────────────────────────┐
│  Userspace                                          │
│                                                     │
│  App A (VT0)          App B (VT1)      App C (VT2)  │
│  opens /dev/fb0       opens /dev/fb0   opens /dev/fb0│
│       │                    │                │       │
│       ▼                    ▼                ▼       │
│  ┌─────────┐         ┌─────────┐      ┌─────────┐  │
│  │ VFB 0   │         │ VFB 1   │      │ VFB 2   │  │
│  │ 636x800 │         │ 636x800 │      │ 1280x800│  │
│  │ (RAM)   │         │ (RAM)   │      │ (RAM)   │  │
│  └────┬────┘         └────┬────┘      └────┬────┘  │
├───────┼────────────────────┼───────────────┼────────┤
│  Kernel: Compositor (sole hardware owner)           │
│       │                    │               │        │
│       ▼                    ▼               │ (not   │
│  ┌──────────────────────────────┐          │visible)│
│  │ Hardware FB 0 (MMIO)        │          │        │
│  │ Monitor 0: 1280x800        │          │        │
│  │ ┌──────────┬──────────┐    │          │        │
│  │ │ VT0 tile │ VT1 tile │    │          │        │
│  │ │ @(0,0)   │ @(638,0) │    │          │        │
│  │ └──────────┴──────────┘    │          │        │
│  └─────────────────────────────┘          │        │
│                                           │        │
│  ┌──────────────────────────────┐         │        │
│  │ Hardware FB 1 (MMIO)        │◄────────┘        │
│  │ Monitor 1: 1920x1080       │ (future)          │
│  └──────────────────────────────┘                   │
└─────────────────────────────────────────────────────┘
```

**Key properties:**
- `/dev/fb0` resolves to caller's VT virtual framebuffer, not hardware
- Each VT has exactly one VFB, sized to its viewport's usable area
- VFBs exist even for off-screen VTs (apps can write, content ready when visible)
- Compositor blits visible VFBs → hardware FBs at 30Hz per display
- A VT can appear on multiple displays (compositor blits same VFB to multiple HW FBs)
- Hardware FBs are compositor-internal, never exposed to userspace
- All VFB writes are RAM speed (fast). All MMIO writes are compositor-only (30Hz)

**Multi-monitor (future):**
- Each physical display has a hardware FB owned by compositor
- Compositor manages a layout per display (which VTs are tiled where)
- Same VT can be visible on multiple displays simultaneously
- VFB is sized to the LARGEST viewport across all displays showing it
- No new device nodes needed — fb0 is always "your VT's buffer"

---

## Phase 0: Viewport Geometry as Source of Truth

**Problem:** Backing buffers are allocated at full hardware FB size regardless of
actual viewport dimensions. A VT in VSplit gets a 1280x800 buffer when it only
occupies 636x800. No structured representation of what's visible where.

**Changes:**

1. **Add `ViewportGeometry` struct to compositor** (layout.rs):
   ```
   ViewportGeometry {
       // Position on hardware FB (compositor-internal, apps don't see this)
       screen_x, screen_y,

       // Full viewport including chrome
       total_width, total_height,

       // Chrome dimensions (borders, title bar, scrollbar)
       border_top, border_bottom, border_left, border_right,

       // Content area = total - chrome (this is what apps see)
       usable_width, usable_height,

       // Text grid derived from usable area ÷ font size
       text_cols, text_rows,
   }
   ```
   Computed from layout mode + hardware FB dimensions + font metrics.

2. **Per-VT visibility state in compositor:**
   ```
   vt_geometries: [Option<ViewportGeometry>; MAX_VTS]
   ```
   `Some(geometry)` = VT is visible on screen with these dimensions.
   `None` = VT is off-screen (not tiled in current layout).

3. **`recompute_geometries()` called on every layout change:**
   - Fullscreen → 1 geometry (focused VT), rest None
   - HSplit → 2 geometries (top/bottom VTs), rest None
   - VSplit → 2 geometries (left/right VTs), rest None
   - Quad → 4 geometries, rest None
   - VT assignment to positions determined by focus order

4. **Public query API (lock-free where possible):**
   - `get_vt_geometry(vt_num) -> Option<ViewportGeometry>`
   - `is_vt_visible(vt_num) -> bool`
   - `get_vfb_dimensions(vt_num) -> (width, height)` — for fb0 ioctl

**Files:** `kernel/tty/compositor/src/layout.rs`, `kernel/tty/compositor/src/lib.rs`

**Exit criteria:** `recompute_geometries()` returns correct values for all layout
modes. Unit testable. No rendering changes yet.

---

## Phase 1: Dynamic Virtual Framebuffer Resize

**Problem:** Backing buffers are fixed at creation size. Layout changes don't
resize them. A fullscreen VT shrunk to half-width still has a full-size buffer.

**Changes:**

1. **Add `resize(new_width, new_height)` to `BackingFramebuffer` (the VFB):**
   - Allocate new contiguous frames from buddy allocator
   - Copy old content (clipped to min of old/new dimensions) row by row
   - Free old frames
   - Update stored width/height/stride/size
   - If allocation fails, keep old buffer (graceful degradation — app sees stale
     dimensions until next successful resize)

2. **Compositor triggers VFB resize on layout change:**
   After `recompute_geometries()`, for each VT:
   - Geometry changed (visible, new dimensions) → resize VFB to usable_width × usable_height
   - Geometry appeared (was None, now Some) → lazy-allocate VFB at correct size
   - Geometry disappeared (was Some, now None) → **keep VFB at last size**
     (app may still be writing; content preserved for when VT returns)
   - Geometry unchanged → no-op

3. **Off-screen VTs keep their VFBs and last-known dimensions:**
   - Terminal emulator continues running, apps continue writing
   - No blitting to hardware (compositor skips invisible VTs)
   - When VT becomes visible again with DIFFERENT dimensions → resize then
   - When VT becomes visible with SAME dimensions → just start blitting

**Files:** `kernel/tty/compositor/src/backing_fb.rs`, `kernel/tty/compositor/src/lib.rs`

**Exit criteria:** Fullscreen → VSplit resizes VT0 VFB from 1280x800 to ~636x800.
Alt+Enter back to Fullscreen resizes to 1280x800. Off-screen VT keeps its buffer.

---

## Phase 2: Terminal Emulator Resize

**Problem:** Terminal emulators have fixed cols/rows set at creation. Resizing
the VFB doesn't update the text grid — terminal still thinks 160x50 in a
half-width viewport.

**Changes:**

1. **Add `resize(new_cols, new_rows, new_fb)` to `TerminalEmulator`:**
   - Update cols/rows
   - Allocate new cell grid, copy existing content clipped to new dimensions
     (like xterm resize — text reflows or truncates)
   - Update renderer's framebuffer reference to resized VFB
   - Clamp cursor position to new bounds
   - Full repaint of visible content to new VFB

2. **Compositor triggers terminal resize after VFB resize:**
   After Phase 1 resize completes:
   `terminal::resize_vt(vt_num, geometry.text_cols, geometry.text_rows, new_fb_ref)`

3. **Off-screen VTs deferred:**
   Don't resize terminal for invisible VTs. When VT becomes visible and needs
   resize, do it then (VFB resize + terminal resize as one operation).

**Files:** `kernel/tty/terminal/src/lib.rs`, `kernel/tty/terminal/src/renderer.rs`

**Exit criteria:** VSplit → VT0 terminal reports 79x50. Fullscreen → 160x50.
Text content preserved across resize (clipped, not lost).

---

## Phase 3: Per-VT Winsize + SIGWINCH

**Problem:** All VTs share a single global winsize. Apps can't discover their
actual dimensions. No notification on resize.

**Changes:**

1. **Per-VT winsize derived from geometry:**
   After terminal resize (Phase 2), set the VT's TTY winsize:
   ```
   tty.set_winsize(Winsize {
       ws_row: geometry.text_rows,
       ws_col: geometry.text_cols,
       ws_xpixel: geometry.usable_width,
       ws_ypixel: geometry.usable_height,
   })
   ```

2. **SIGWINCH delivery on layout change:**
   After updating winsize, send SIGWINCH to the VT's foreground process group.
   Only for VTs whose dimensions actually changed (don't spam unchanged VTs).

3. **TIOCGWINSZ already works** — reads from per-VT TTY winsize. With correct
   per-VT values, apps get the right answer automatically.

4. **Remove `set_all_winsize()` global setter** — replace with per-VT init
   using geometry from compositor.

5. **Off-screen VTs retain last winsize** — no SIGWINCH for invisible VTs.
   When VT becomes visible with new dimensions, resize + SIGWINCH at that point.

**Files:** `kernel/tty/vt/src/lib.rs`, `kernel/tty/tty/src/tty.rs`, `kernel/src/init.rs`

**Exit criteria:** `stty size` in VSplit reports correct half-width cols.
Toggling layout triggers SIGWINCH. htop/vim reflow to new dimensions.

---

## Phase 4: fb0 → Virtual Framebuffer Redirection

**Problem:** `/dev/fb0` writes go directly to hardware framebuffer, bypassing
compositor and VT isolation.

**Design:** `/dev/fb0` is a single Linux-compatible device path. Under the hood
it resolves to the caller's VT virtual framebuffer. The app sees a framebuffer
at (0,0) with its viewport's usable dimensions. It doesn't know where it is on
screen, which monitor it's on, or how many displays exist. Hardware FBs are
compositor-internal and never exposed.

This is the same pattern as `/dev/tty` — single path, per-process behavior.

**Changes:**

1. **fb0 resolves to caller's VT on every operation:**
   - Determine calling process's controlling TTY → VT number → that VT's VFB
   - Fallback chain: controlling TTY → active VT → VT0
   - Resolution happens per-syscall (process may switch VTs between calls)

2. **fb0 read/write target the VFB:**
   - `write(fd, buf, len)` at offset → copy into VFB RAM at offset
   - `read(fd, buf, len)` at offset → copy from VFB RAM at offset
   - Bounds checked against VFB size (not hardware FB size)
   - Write marks VT dirty (one atomic store)

3. **fb0 ioctl returns per-VT info:**
   - `FBIOGET_VSCREENINFO` → VFB dimensions (usable_width × usable_height), BPP
   - `FBIOGET_FSCREENINFO` → VFB physical address, stride, size
   - Two apps on different VTs get different answers — correct behavior

4. **Off-screen VTs:**
   - fb0 still works — writes go into VFB, reads return VFB content
   - ioctl returns last-known dimensions
   - When VT becomes visible, content already there

**Files:** `kernel/vfs/devfs/src/devices.rs`, `kernel/vfs/devfs/src/lib.rs`

**Exit criteria:** Graphics app on VT0 in VSplit paints only its half. VT1's
terminal unaffected. FBIOGET_VSCREENINFO returns half-width resolution.

---

## Phase 5: KDSETMODE (Text ↔ Graphics per VT)

**Problem:** No way for an app to say "I'm doing raw pixel graphics, stop
rendering terminal text on this VT."

**Changes:**

1. **Per-VT mode flag:**
   ```
   enum VtMode { Text, Graphics }
   ```
   Stored in VtState, defaults to Text.

2. **KDSETMODE ioctl on /dev/ttyN:**
   - `KD_TEXT (0x00)` → terminal emulator renders text to VFB
   - `KD_GRAPHICS (0x01)` → terminal emulator stops rendering, fb0 owns VFB

3. **Terminal write path checks mode:**
   If mode == Graphics: buffer text in terminal's screen buffer (preserving
   content for switch back to Text) but do NOT render glyphs to VFB.

4. **Compositor is mode-agnostic:**
   Always blits VFB to hardware regardless of mode. In text mode the terminal
   fills VFB. In graphics mode the app fills VFB via fb0. Compositor doesn't care.

5. **Mode switch triggers repaint:**
   KD_GRAPHICS → clear VFB (app starts with blank canvas)
   KD_TEXT → terminal does full repaint from screen buffer to VFB

**Files:** `kernel/tty/vt/src/lib.rs`, `kernel/tty/tty/src/tty.rs`,
`kernel/tty/terminal/src/lib.rs`

**Exit criteria:** App sets KD_GRAPHICS, writes pixels via fb0, no terminal text
collision. KD_TEXT restores terminal content.

---

## Phase 6: Compositor Chrome

**Problem:** No visual separation between tiled VTs. No focus indicator.

**Changes:**

1. **Borders between viewports:**
   - 2px lines between tiles, drawn on hardware FB AFTER VFB blits
   - Chrome is compositor-only — never touches VFBs
   - Border pixels deducted from viewport total → usable area calculation

2. **Focus indicator:**
   - Focused/active VT gets highlight border (cyan/white)
   - Unfocused visible VTs get dim border (dark gray)

3. **Title bar (future):**
   - Optional thin bar per viewport: VT number + running process
   - Height deducted from usable area in ViewportGeometry
   - Rendered by compositor after VFB blit

4. **Scrollbar (future):**
   - Thin bar in right chrome region showing scrollback position
   - Width deducted from usable area in ViewportGeometry

**Files:** `kernel/tty/compositor/src/lib.rs`, `kernel/tty/compositor/src/layout.rs`

**Exit criteria:** VSplit shows visible border. Focused VT highlighted.

---

## Hardware Acceleration Design Notes

The VFB architecture is explicitly designed to survive GPU acceleration without
changing the app-facing interface or compositor contract.

**Today (software compositing):**
```
App → write() → VFB (buddy-allocated RAM) → compositor memcpy → hardware FB (MMIO)
```

**Future (GPU-accelerated compositing):**
```
App → write() → VFB (GPU-allocated VRAM) → compositor GPU blit cmd → GPU DMA → display
```

Apps don't change. They still write to their VFB at (0,0). The compositor
submits GPU blit commands instead of `copy_nonoverlapping`. Same dirty flags,
same 30Hz tick, different backend.

**What changes (compositor-internal only):**

1. **VFB backing memory moves from RAM to VRAM:**
   GPU driver allocates per-VT surfaces instead of buddy allocator frames.
   Apps write via mapped VRAM (or DMA upload). Blitting is GPU→GPU copy —
   no bus crossing, massively faster.

2. **Compositor blit becomes a GPU command:**
   Instead of row-by-row memcpy into MMIO, submit `TRANSFER_FROM_HOST_2D`,
   scanout commands, or equivalent. GPU handles viewport positioning, scaling,
   rotation — things that are expensive in software but free in hardware.

3. **VirtIO-GPU already supports this:**
   Per-VT GPU resources (`RESOURCE_CREATE_2D`), per-VT transfers
   (`TRANSFER_TO_HOST_2D`), per-VT scanout. We'd create N resources instead
   of one global one.

4. **Chrome/borders become GPU overlays:**
   Hardware cursor, overlay planes, or GPU-rendered borders instead of
   pixel-by-pixel compositor drawing.

**Design requirement for Phase 1:**

VFB allocation MUST go through a trait so the backing can be swapped without
touching compositor logic:

```rust
trait VfbAllocator {
    /// Allocate a virtual framebuffer of given dimensions
    fn alloc(width: u32, height: u32, format: PixelFormat) -> Option<Box<dyn Framebuffer>>;

    /// Resize an existing VFB (may reallocate)
    fn resize(fb: &mut dyn Framebuffer, new_width: u32, new_height: u32) -> bool;

    /// Free a VFB
    fn free(fb: Box<dyn Framebuffer>);
}
```

Two implementations:
- `BuddyVfbAllocator` — current: allocates contiguous physical frames from buddy allocator
- `GpuVfbAllocator` (future) — allocates GPU resources via VirtIO-GPU or native driver

Compositor holds a `&dyn VfbAllocator` and never knows which backend is active.
GPU driver registration swaps the allocator at runtime when a capable GPU is
detected.

**What the architecture already handles (no changes needed):**
- Apps never touch hardware → GPU swap is invisible to userspace
- Compositor is sole hardware owner → swap backend freely
- VFBs are trait objects → backed by RAM, VRAM, or GPU surfaces
- Dirty tracking is backend-agnostic → drives GPU command submission same as memcpy
- Off-screen VTs with VRAM-backed VFBs just don't get scanout commands

---

## Non-Goals (Explicit)

- **No floating windows** — tiling compositor only (for now)
- **No GPU-accelerated compositing yet** — memcpy for current resolutions, but
  architecture supports GPU swap via VfbAllocator trait (see above)
- **No per-pixel dirty tracking** — full VFB blit at 30Hz is ~5MB/s per VT
- **No double-buffered VFBs** — single buffer + 30Hz tick is sufficient
- **No mmap for fb0** — read/write only (mmap needs page-fault dirty tracking)
- **No direct hardware FB access from userspace** — compositor owns all hardware

---

## Dependency Graph

```
Phase 0 (ViewportGeometry)
    ↓
Phase 1 (Dynamic VFB Resize)
    ↓
Phase 2 (Terminal Emulator Resize)
    ↓
Phase 3 (Per-VT Winsize + SIGWINCH)
    ↓                ↓
Phase 4 (fb0 → VFB)  Phase 5 (KDSETMODE)
    ↓                ↓
Phase 6 (Chrome/Borders)
```

Phases 4 and 5 are independent but both require 0-3.
Phase 6 is cosmetic, can happen anytime after Phase 0.

---

## Performance Notes

**Zero new hot-path cost:**
- VFB writes are RAM writes (faster than current direct MMIO writes to hw FB)
- Compositor tick already exists at 30Hz, already copies buffers to hardware
- Dirty tracking already atomic, lock-free
- VFB resize is cold-path only (layout changes, not per-frame)
- KDSETMODE is a flag check in terminal write path (one branch)

**Memory budget (worst case, 6 VTs at 1280x800x32bpp):**
- 6 × ~4MB VFBs = ~24MB from buddy allocator
- Currently: VT0 always allocated (~4MB), VT1-5 lazy (~4MB each on first use)
- No change in allocation strategy, just correct sizing

---

## Files Modified (Summary)

| File | Phases |
|------|--------|
| `kernel/tty/compositor/src/layout.rs` | 0, 6 |
| `kernel/tty/compositor/src/lib.rs` | 0, 1, 2, 3, 6 |
| `kernel/tty/compositor/src/backing_fb.rs` | 1 |
| `kernel/tty/terminal/src/lib.rs` | 2, 5 |
| `kernel/tty/terminal/src/renderer.rs` | 2 |
| `kernel/tty/vt/src/lib.rs` | 3, 5 |
| `kernel/tty/tty/src/tty.rs` | 3, 5 |
| `kernel/vfs/devfs/src/devices.rs` | 4 |
| `kernel/vfs/devfs/src/lib.rs` | 4 |
| `kernel/src/init.rs` | 3 |
