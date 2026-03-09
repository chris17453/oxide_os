# OXIDE OS Performance Modifications

## Completed

### Row-Batched MMIO Writes (Scrollbar/Compositor/Vkbd Deadlock Fix)
- **`kernel/tty/compositor/src/lib.rs`** — `fill_hw_rect_static()`: builds pixel row in 1KB stack buffer, copies entire rows to MMIO instead of per-pixel writes. 16× speedup for 16px-wide scrollbar rendering. Eliminates timer ISR deadlock from scrollbar MMIO pixel writes.
- **`kernel/tty/compositor/src/lib.rs`** — `fill_hw_rect()`: deduplicated to call `fill_hw_rect_static`.
- **`kernel/tty/vkbd/src/lib.rs`** — `fill_rect()`: same row-batching optimization.

### Terminal Write Pipeline Profiling
- **`kernel/perf/src/lib.rs`** — Added counters: `term_write_calls/bytes/cycles`, `term_glyph_renders/cycles`, `term_bulk_renders/rows`, `term_flushes/flush_cycles`, `term_scrolls/scroll_cycles`.
- **`kernel/tty/terminal/src/renderer.rs`** — Instrumented `render_cell`, `scroll_up_pixels`, `flush_fb` with rdtsc timing.
- **`kernel/perf/src/stats.rs`** — Stats display shows time breakdown: `glyphs% / scroll% / flush% / other%`.

### Scrollback Push — Zero-Copy Row Access (C1)
- **`userspace/libs/vte/src/buffer.rs`** — Added `ScreenBuffer::row_slice()` returning `&[Cell]` (no heap alloc). Added `ScrollbackBuffer::push_slice()` accepting `&[Cell]` (single alloc into VecDeque).
- **`userspace/libs/vte/src/handler.rs`** — `linefeed()` now borrows row as slice via `row_slice()` + `push_slice()` instead of `get_row().to_vec()`. Eliminates ~3.2KB heap clone per newline. For `find /` with 10K lines = 32MB of heap churn eliminated.

### Path Resolution — Borrow Instead of Clone (C3)
- **`kernel/syscall/syscall/src/vfs.rs`** — `resolve_path()` borrows cwd as `&str` inside meta lock instead of `cwd.clone()`. Absolute paths skip the ProcessMeta lock entirely. `normalize_path_direct()` uses a stack-local `[usize; 64]` array for component tracking instead of `Vec<&str>`. `normalize_path_relative()` processes cwd+path in one pass. Reduces 3-4 heap allocations to 1 per path syscall.

### getcwd — Zero Allocation (M3)
- **`kernel/syscall/syscall/src/dir.rs`** — `sys_getcwd()` copies cwd to user space inside the meta lock closure. Old pattern cloned the String just to copy bytes.

### TTY Read Waiters — Move Instead of Clone (C5)
- **`kernel/tty/tty/src/tty.rs`** — Replaced `read_waiters.clone()` + `clear()` with `core::mem::take()`. Moves the Vec out, replaces with empty — zero heap allocations per keystroke.

### Compositor Dirty Flag Separation
- **`kernel/tty/compositor/src/lib.rs`** — Separated `SCROLLBAR_DIRTY` from VT content dirty. Scrollbar hover no longer triggers full VT blit (~1.4MB saved per hover event).
- **`kernel/tty/vkbd/src/lib.rs`** — Vkbd hover repaints only 2 keys (old + new) instead of all 100.

### Pre-Allocated Scroll Composite Buffer
- **`kernel/tty/terminal/src/lib.rs`** — Added `scroll_composite: ScreenBuffer` field to `TerminalEmulator`. Reused every frame instead of `ScreenBuffer::new()` (126KB heap alloc from timer ISR = deadlock on heap spinlock).

---

## Remaining — Priority Order

### CRITICAL — Actively Killing `find /` Performance

| ID | Issue | Location | Impact |
|----|-------|----------|--------|
| C2 | `Arc::clone()` on every read/write/lseek/fstat syscall — atomic inc+dec per file I/O, 19 call sites | `syscall/vfs.rs`, `dir.rs`, `poll.rs`, `vfs_ext.rs`, `memory.rs` | Every `find /` readdir+stat+write = 3-4 atomic ops × thousands of files. Fix: hold fd_table lock guard, borrow `&Arc<File>` directly for non-blocking ops; clone only for blocking read/write. |
| C4 | `getdents` does 3× `copy_to_user()` per directory entry (header, name, null) | `syscall/dir.rs:264-297` | 300 user-space copies per 100-entry directory. Fix: build entries into kernel buffer, single `copy_to_user()` per batch. |
| C6 | Parser clones `Vec<i64>` params + `Vec<u8>` intermediates on every escape sequence dispatch | `vte/parser.rs:299,396-397,420-421,577-580` | Escape-heavy output (colored ls, prompts) = clone per sequence. Fix: refactor `Action` enum to use `&[i64]`/`&[u8]` slice references, process before parser state resets. |

### HIGH — ISR Latency Bombs

| ID | Issue | Location | Impact |
|----|-------|----------|--------|
| H1 | Unbounded serial spin in keyboard ISR — `serial_trace()` loops forever on UART THRE | `input/kbd.rs:20-24` | If UART stalls, keyboard ISR hangs forever. Fix: gate behind `debug-kbd` feature flag or add spin limit. |
| H2 | O(256×512×512) page table validation on fork/exec — triple-nested loop over ALL user page tables | `process.rs:89-167` | Every fork()+exec() scans potentially millions of entries. Fix: gate behind `debug-pagetables` feature flag. |
| H3 | Per-pixel MMIO writes in vkbd `draw_glyph()` — each glyph pixel is individual `copy_nonoverlapping` | `vkbd/lib.rs:846-866` | 100 key labels × ~80 pixels = 8000 MMIO writes. Fix: row-batch like `fill_rect` was fixed. |
| H4 | `draw_bitmap_italic()` uses `bb_set_pixel()` per pixel — no bpp==4 fast path, redundant `color.write_to()` per pixel | `terminal/renderer.rs:796-823` | Every italic glyph 10-50× slower than non-italic. Fix: add bpp==4/bpp==2 fast paths like `draw_bitmap_glyph()` has. |

### MEDIUM — Adds Up Over Thousands of Operations

| ID | Issue | Location | Impact |
|----|-------|----------|--------|
| M1 | TCP `TcpOptions` allocates `Vec<(u32,u32)>` per packet for SACK blocks (typically empty) | `net/tcpip/src/tcp.rs:61-124` | Heap alloc/free on every TCP packet. Fix: use `ArrayVec<[(u32,u32); 4]>`. |
| M2 | TCP send does `drain().collect()` into temporary Vec | `net/tcpip/src/tcp.rs:906` | Unnecessary copy on every TCP send. Fix: write directly from send_buf slice. |
| M4 | Mouse sequence responses allocate `Vec::new()` per event | `terminal/lib.rs:1470,1482,1498` | Heap alloc per mouse move. Fix: stack-allocated `[u8; 32]` buffer. |
| M5 | Keyboard debug traces not behind feature flag — first 20 key events do serial I/O unconditionally | `input/kbd.rs:147-157` | 600 bytes of serial from ISR on every boot. Fix: gate behind `debug-kbd`. |

### LOW — Correct But Could Be Tighter

| ID | Issue | Location | Impact |
|----|-------|----------|--------|
| L1 | VirtIO descriptor chain walk has no iteration limit | `virtio-core/virtqueue.rs:270-286` | Infinite loop on corrupted descriptor. Fix: add `queue_size` iteration guard. |
| L2 | `render_with_scrollback()` per-cell `set()` with bounds check | `terminal/lib.rs:1815-1840` | Redundant per-column validation. Fix: use direct slice copy. |
| L3 | Boot-time page frame marking is linear per-PFN | `init.rs:640-672` | One-time cost, acceptable. Fix: batch range marking. |
