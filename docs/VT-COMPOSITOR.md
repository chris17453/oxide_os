# Virtual Terminals & Tiling Compositor

OXIDE OS has a kernel-level tiling compositor with up to 6 virtual terminals (VTs). Backing pixel buffers (~4MB each) are **allocated on demand** — only VT0 gets a buffer at boot. Split the screen or switch VTs and the compositor creates new buffers automatically. The compositor owns the hardware framebuffer exclusively and blits VT buffers into viewport rectangles on the physical display.

---

## Architecture

```
+-------------------+     +-------------------+     +-------------------+
|  VT0 (4MB buf)    |     |  VT1 (4MB buf)    |     |  VT2 (4MB buf)    |
|  Terminal emu      |     |  Terminal emu      |     |  Terminal emu      |
|  getty/login/esh   |     |  (idle)            |     |  (idle)            |
+-------------------+     +-------------------+     +-------------------+
         |                          |                          |
         +-------------+------------+-----------+--------------+
                        |
                   COMPOSITOR
                   (blits visible VTs to hw fb)
                        |
               +--------v--------+
               | Hardware FB      |
               | 1280x800 MMIO   |
               +-----------------+
```

Each VT has:
- A **backing framebuffer** (~4MB, lazy-allocated from buddy allocator physical frames on first use)
- A **TTY instance** with its own line discipline and input ring buffer
- A **VT number** (0-5, mapped to Ctrl+Alt+F1 through Ctrl+Alt+F6)

The compositor supports 4 layout modes with a 2px dark gray border between tiles and a cyan focus highlight on the active tile.

---

## Keyboard Shortcuts

### VT Switching (works in all layouts)

| Shortcut | Action |
|----------|--------|
| `Ctrl+Alt+F1` | Switch to VT0 |
| `Ctrl+Alt+F2` | Switch to VT1 |
| `Ctrl+Alt+F3` | Switch to VT2 |
| `Ctrl+Alt+F4` | Switch to VT3 |
| `Ctrl+Alt+F5` | Switch to VT4 |
| `Ctrl+Alt+F6` | Switch to VT5 |

### Tiling Layout

| Shortcut | Action |
|----------|--------|
| `Ctrl+Alt+Enter` | Toggle fullscreen / last split layout |
| `Ctrl+Alt+H` | Horizontal split (top/bottom, 2 VTs) |
| `Ctrl+Alt+V` | Vertical split (left/right, 2 VTs) |
| `Ctrl+Alt+Q` | Quad layout (2x2 grid, 4 VTs) |
| `Ctrl+Alt+Tab` | Cycle focus to next visible tile |

### Layout Diagrams

**Fullscreen** (default) — one VT fills the entire screen:
```
+---------------------------+
|                           |
|           VT0             |
|                           |
+---------------------------+
```

**Horizontal Split** (`Ctrl+Alt+H`) — two VTs stacked:
```
+---------------------------+
|           VT0             |
+------ 2px border ---------+
|           VT1             |
+---------------------------+
```

**Vertical Split** (`Ctrl+Alt+V`) — two VTs side by side:
```
+-----------+||+-----------+
|           ||||           |
|    VT0    ||||    VT1    |
|           ||||           |
+-----------+||+-----------+
             2px
```

**Quad** (`Ctrl+Alt+Q`) — four VTs in a 2x2 grid:
```
+-----------+||+-----------+
|    VT0    ||||    VT1    |
+--------- 2px -----------+
|    VT2    ||||    VT3    |
+-----------+||+-----------+
```

The cyan border highlights which tile has keyboard focus. Press `Ctrl+Alt+Tab` to cycle focus between visible tiles.

---

## How VT Switching Works

1. **Keyboard IRQ** fires, `input::kbd` detects Ctrl+Alt+F*n*
2. `vt::switch_to(n)` updates `ACTIVE_VT` (the global that routes keyboard input)
3. `compositor::focus_vt(n)` swaps the focused tile to show VT *n*
4. `terminal::update_framebuffer()` points the terminal renderer at VT *n*'s backing buffer
5. Compositor triggers a full redraw, blitting all visible VT buffers to the hardware fb

In split/quad mode, `Ctrl+Alt+F*n*` brings VT *n* into the **focused tile** (the one with the cyan border). The other tiles keep showing their assigned VTs. Use `Ctrl+Alt+Tab` to move focus between tiles without switching VTs.

---

## Current Limitations

### Single Terminal Emulator
There is one global `TerminalEmulator` instance. It processes ANSI escape codes and renders glyphs. When you switch VTs, the terminal renderer swaps its target framebuffer to the new VT's backing buffer. This means:

- The **active VT** gets full terminal rendering (cursor, colors, scrollback)
- **Inactive VTs** that are visible in split mode show their last rendered state but don't update in real time — output from background processes on inactive VTs is **dropped** (the VtTtyDriver skips writes for non-active VTs)

This is a known limitation. The fix requires one terminal emulator per VT (tracked in the compositor plan).

### Multi-Login Sessions
`init` spawns a **getty on every active VT** (`/dev/tty1` through `/dev/ttyN`). Each getty runs in its own session (`setsid`) with its own controlling terminal. When a getty exits (user logs out), init respawns it after a 2-second cooldown.

Switching to VT2 (`Ctrl+Alt+F2`) shows an independent login prompt. Each VT is a separate session.

### No Per-VT Terminal State
Because there's one terminal emulator, switching VTs doesn't preserve terminal state (cursor position, colors, scroll position) per-VT. The terminal state belongs to whatever VT is active. Switching away and back may show stale pixel data in the backing buffer from the last time that VT was active.

---

## Trying It Out

### Split Screen (what works now)

1. Boot normally: `make run` or `make run-release`
2. You'll see the login prompt on VT0 (fullscreen)
3. Press `Ctrl+Alt+V` for vertical split — left side shows VT0 (login), right side shows VT1 (blank)
4. Press `Ctrl+Alt+H` for horizontal split — top/bottom
5. Press `Ctrl+Alt+Q` for quad — four panes
6. Press `Ctrl+Alt+Enter` to toggle back to fullscreen
7. Press `Ctrl+Alt+Tab` to cycle focus between visible tiles (watch the cyan border move)

### VT Switching

1. Press `Ctrl+Alt+F2` to switch to VT1 (independent login prompt)
2. Press `Ctrl+Alt+F1` to switch back to VT0
3. Log in independently on each VT

### What You'll See

- Each VT has its own login prompt (independent sessions)
- In split mode, you can see multiple VTs with their own sessions
- The cyan border shows which tile has keyboard focus

---

## Source Files

| File | What it does |
|------|-------------|
| `kernel/tty/compositor/src/lib.rs` | Compositor: blit loop, dirty tracking, layout dispatch |
| `kernel/tty/compositor/src/backing_fb.rs` | Per-VT pixel buffer (buddy allocator frames) |
| `kernel/tty/compositor/src/layout.rs` | Layout geometry (fullscreen, hsplit, vsplit, quad) |
| `kernel/tty/vt/src/lib.rs` | VT manager: 6 VTs, input routing, TTY instances |
| `kernel/tty/terminal/src/lib.rs` | Terminal emulator (ANSI parser, glyph renderer) |
| `kernel/tty/terminal/src/renderer.rs` | Pixel rendering (back buffer, dirty regions) |
| `kernel/input/input/src/kbd.rs` | Keyboard handler: Ctrl+Alt+F1-F6, Ctrl+Alt+H/V/Q/Enter/Tab |
| `kernel/src/init.rs` | Boot wiring: compositor init, VT device registration, callbacks |
| `userspace/system/init/src/main.rs` | PID 1: spawns getty on each VT, reaps/respawns |

---

## On-Demand Allocation

At boot, only VT0's backing buffer (~4MB) is allocated. When you:
- **Split the screen** (`Ctrl+Alt+V`, `Ctrl+Alt+H`, `Ctrl+Alt+Q`) — newly visible VTs get buffers allocated automatically
- **Switch VTs** (`Ctrl+Alt+F2`) — the target VT's buffer is allocated on first access

This saves ~15MB of physical memory at boot compared to pre-allocating all 6 buffers. The maximum VT count is set by `MAX_VTS` in `kernel/tty/compositor/src/layout.rs` (default: 6).

---

## Future Work

- **Per-VT terminal emulator** — each VT gets its own TerminalEmulator instance so inactive VTs buffer output and preserve state
- **Graphics mode** — VTs can switch to raw framebuffer mode for /dev/fb0 apps (DOOM, etc.)
- **Status bar** — show VT number, layout mode, and system info at screen edge
