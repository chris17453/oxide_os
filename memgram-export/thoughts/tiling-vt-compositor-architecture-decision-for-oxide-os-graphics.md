# Tiling VT Compositor architecture decision for OXIDE OS graphics

📌 Pinned

| Field | Value |
|-------|-------|
| ID | `a43a81a26510` |
| Type | decision |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-06T14:21:04.405327+00:00 |
| Accessed | 0 times |
| Keywords | compositor, tiling, vt, framebuffer, graphics, display, splitscreen, backingbuffer |
| Files | `docs/plan/TILING-VT-COMPOSITOR.md`, `kernel/tty/vt/src/lib.rs`, `kernel/tty/terminal/src/lib.rs`, `kernel/tty/terminal/src/renderer.rs`, `kernel/vfs/devfs/src/devices.rs`, `kernel/graphics/fb/src/lib.rs` |
| Session | [7dbaf6fcc81d](../sessions/comprehensive-performance-analysis-of-oxide-os-identify-bottlenecks-hot-paths-an.md) |

## Content

Decided on a kernel-level tiling VT compositor for OXIDE OS display management. Each VT gets its own per-VT backing pixel buffer (allocated from buddy frame allocator, ~4MB each). A compositor layer owns the hardware framebuffer exclusively and blits VT buffers into viewport rectangles. Supports fullscreen, hsplit, vsplit, quad layouts. /dev/fb0 writes redirect to the calling process's VT buffer (transparent to apps). Terminal renderer targets VT buffer instead of hardware fb. Full Linux fbdev compatibility preserved — zero app changes needed. 4 phases: (1) backing buffers + basic redirect, (2) tiling layout, (3) graphics VT mode + fb0 redirect, (4) polish. Plan doc at docs/plan/TILING-VT-COMPOSITOR.md
