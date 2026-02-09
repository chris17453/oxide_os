# FUCKINGHELL.md — Every Uncommitted Change Since HEAD (122db4f)

All changes from the last commit (Feb 8, 10:14 PM) to the current dirty worktree.
30 files changed, 278 insertions(+), 2399 deletions(-).

---

## CATEGORY 1: REAL FIXES (KEEP THESE)

### 1. TTY Echo Lock Ordering Fix — `kernel/tty/tty/src/tty.rs` (+15 lines)
**Problem:** `Tty::input()` called `driver.write()` while holding the LDISC lock.
Echo → VtTtyDriver::write → console_write → terminal::write → TERMINAL mutex.
Timer ISR holds TERMINAL for cursor blink → deadlock on Tab key.

**Fix:** Collect echo data into a Vec while lock is held, write AFTER releasing.
Same pattern as `Tty::write()`.

**Status:** Correct fix. Prevents Tab deadlock. **KEEP.**

---

### 2. Tab Echo Expansion — `kernel/tty/tty/src/ldisc.rs` (+14 lines)
**Problem:** Raw tab byte (0x09) sent to terminal as echo caused visual chaos.

**Fix:** Expand tab to `8 - (column % 8)` spaces in `input_char()`, track column position.

**Status:** Correct fix. Standard Unix behavior. **KEEP.**

---

### 3. Lock-Free Ring Outside Mutex — `kernel/tty/vt/src/lib.rs` (+113/-113 = rewrite)
**Problem:** `LockFreeRing` was inside `Mutex<VtState>`. `push_input()` from ISR called
`try_lock()` to reach it. When `read()` held the lock (waiting for input), IRQ
couldn't push → keystrokes silently dropped. A "lock-free" buffer behind a lock.

**Fix:** Moved `input_rings: [LockFreeRing; NUM_VTS]` to `VtManager` directly (outside mutex).
`push_input()` accesses `self.input_rings[active]` — zero locks.
All references to ring buffer updated throughout the file.

**Status:** Correct fix. Eliminates keystroke drops. **KEEP.**

---

### 4. ISR-Safe Debug Output → Serial — `kernel/src/console.rs` (+40/-49)
**Problem:** `write_byte_unsafe()` / `write_str_unsafe()` were pushing bytes to VT INPUT buffer.
Kernel debug messages ("APIC-CAL Starting calibration...") appeared as typed keystrokes.
Getty tried to authenticate debug spew as usernames.

**Fix:** Route ISR-safe debug output to serial port (`arch::serial::write_byte_unsafe`).
Debug spew goes to COM1, not to the user's terminal.

**Status:** Correct fix. Eliminates ghost input. **KEEP.**

---

### 5. Perf Stats Gated Behind `debug-perf` — `kernel/arch/arch-x86_64/src/exceptions.rs` (+34 lines)
**Problem:** `print_perf_stats()` runs every 500 ticks (~5s) in timer ISR.
1500 bytes of UTF-8 box art at 115200 baud = ~130ms with interrupts disabled.
PERF-WARN message (53 bytes) creates self-amplifying feedback cascade.

**Fix:** Both `print_perf_stats` and `PERF-WARN` gated behind `#[cfg(feature = "debug-perf")]`.
Feature added to `kernel/Cargo.toml` and `arch-x86_64/Cargo.toml`.
NOT included in `debug-all` — must be explicitly enabled.

**Status:** Correct fix. Counter recording still always-on (cheap atomics). **KEEP.**

---

## CATEGORY 2: TRACE CLEANUP (REMOVING RAW SERIAL SPAM)

### 6. Buddy Allocator Trace Removal — `kernel/mm/mm-core/src/buddy.rs` (-542 lines)
Removed massive blocks of raw serial trace output (unbounded `while inb(0x3FD)` loops)
from `add_free_block()`, `remove_free_block()`, and related functions.
These traces logged every alloc/free in a specific physical address range (0xc400000-0xc500000).
Added compact `buddy_fatal_serial()` / `buddy_fatal_hex()` helpers for corruption panics.
**The allocator LOGIC is unchanged — only traces removed.**

**Status:** Safe cleanup. Traces were ISR-unsafe (unbounded serial spin). **KEEP.**

---

### 7. Page Mapper Trace Removal — `kernel/mm/mm-paging/src/mapper.rs` (-98 lines)
Removed raw serial traces from `map_page()` that logged PTE writes in address range
0xc400000-0xc500000. Same unbounded `while inb(0x3FD)` pattern.
**The mapping LOGIC is unchanged — only traces removed.**

**Status:** Safe cleanup. **KEEP.**

---

### 8. Address Space Trace Removal — `kernel/proc/proc/src/address_space.rs` (-222 lines)
Removed raw serial traces from `allocate_pages()` that logged frame allocation progress,
PTE writes, and post-allocation verification in address range 0xc400000-0xc500000.
**The address space LOGIC is unchanged — only traces removed.**

**Status:** Safe cleanup. **KEEP.**

---

### 9. Init ELF-COPY Trace Removal — `kernel/src/init.rs` (-26 lines)
Removed raw serial trace from ELF segment loading that logged physical addresses
in range 0xc400000-0xc500000. Same unbounded serial pattern.

**Status:** Safe cleanup. **KEEP.**

---

### 10. Userspace init Debug Removal — `userspace/system/init/src/main.rs` (-36 lines)
Removed DEBUG printlns throughout init (fork, exec, PID logging).
Removed syscall 999 screen dump debug call.

**Status:** Safe cleanup (reduces serial spam). **KEEP.**

---

## CATEGORY 3: BUILD/QEMU CONFIG CHANGES

### 11. QEMU Run Targets Rewrite — `mk/qemu.mk` (-211 lines)
- `make run` now calls `run-rhel` / `run-fedora` (was `run-rhel-debug` / `run-fedora-debug`)
- Deleted `run-fedora-debug`, `run-rhel-debug`, `DEBUG_QEMU_ARGS`
- Deleted `debug-server`, `debug-capture`, `debug-boot-check`, `debug-exec`, `debug-repl`
- New run-rhel: `-smp 4`, `-cpu max,+invtsc`, `-m 256M`, `-device virtio-gpu-pci`, `-vnc :0`
- Old debug: `-smp 1`, `-cpu qemu64,+smap,+smep`, `-m 512M`, `-serial file:`, GDB attach
- **Removed `-vga std`** (was not in debug config either — both used virtio-gpu-pci only)

**Status:** Functional change. No more auto-GDB, different QEMU params.
**REVIEW CAREFULLY** — `-smp 4` vs old `-smp 1` could expose SMP races.

---

### 12. Config GDB Vars Removed — `mk/config.mk` (-4 lines)
Removed `GDB ?= gdb`, `GDB_AUTO ?= 1`, `GDB_CMDS` variables.
`RUN_KERNEL_FEATURES` was already empty in HEAD (NOT changed).

**Status:** Matches qemu.mk cleanup. **KEEP if keeping qemu.mk changes.**

---

### 13. Help Target Updated — `mk/help.mk` (-8 lines)
Removed references to deleted debug targets.

**Status:** Matches qemu.mk cleanup. **KEEP.**

---

### 14. QEMU MCP Tool Updated — `tools/qemu-mcp/index.js` (+6 lines)
Added `-device virtio-keyboard-pci` and `-device virtio-tablet-pci` to MCP QEMU args.
Removed `-vga std` from MCP args.

**Status:** Aligns MCP tool with new QEMU config. **KEEP.**

---

## CATEGORY 4: DELETED FILES

### 15. docs/AUTONOMOUS-DEBUGGING.md — DELETED (-303 lines)
Full GDB automation documentation.

### 16. scripts/debug-kernel.sh — DELETED (-215 lines)
Autonomous debug shell wrapper.

### 17. scripts/gdb-autonomous.py — DELETED (-373 lines)
Python GDB controller for autonomous debugging.

### 18. scripts/gdb-capture-crash.gdb — DELETED (-60 lines)
GDB crash capture script.

### 19. scripts/gdb-check-boot.gdb — DELETED (-24 lines)
GDB boot check script.

### 20. scripts/gdb-init-kernel.gdb — DELETED (-68 lines)
GDB kernel init script.

### 21. scripts/test-autonomous-debug.sh — DELETED (-124 lines)
Test harness for autonomous debugging.

**Status for all:** Matches qemu.mk debug removal. **KEEP if removing debug targets.**

---

## CATEGORY 5: DIAGNOSTIC GARBAGE (REMOVE ALL OF THIS)

### 22. Red Rectangle in terminal_tick — `kernel/src/console.rs` (+5 lines)
Persistent red rectangle drawn at (50,50) to prove framebuffer is writable.
**REMOVE — diagnostic only.**

### 23. Serial 'T'/'S' Markers — `kernel/tty/terminal/src/lib.rs` (+8 lines)
Emits 'T' on serial for every `terminal::write()` call, 'S' if SYNCHRONIZED_OUTPUT active.
**REMOVE — diagnostic only.**

### 24. Forced Red Foreground — `kernel/tty/terminal/src/renderer.rs` (+1 line)
`fg_color = Color::new(255, 0, 0);` — forces all text to bright red.
**REMOVE — breaks text colors.**

### 25. Green Dot Markers — `kernel/tty/terminal/src/renderer.rs` (+2 lines)
`self.fb.fill_rect(px, py, 3, 3, Color::new(0, 255, 0));` at end of render_cell_inner.
**REMOVE — covers text.**

### 26. Glyph Data Quality Diagnostic — `kernel/tty/terminal/src/renderer.rs` (+16 lines)
Serial output of glyph length/nonzero count in draw_bitmap_glyph.
**REMOVE — diagnostic only.**

### 27. draw_bitmap_glyph Replaced With set_pixel — `kernel/tty/terminal/src/renderer.rs` (-50/+10 lines)
The entire optimized 4bpp/2bpp glyph renderer was replaced with a slow `set_pixel()` loop.
**MUST RESTORE original draw_bitmap_glyph code from HEAD.**

---

## CATEGORY 6: MINOR/DOCS

### 28. CLAUDE.md Agent Rules Updated (+2/-2 lines)
Added references to new docs/agents/ rules files (isr-output-serial-only, perf-warn-feedback-loop,
lockfree-ring-outside-mutex, tty-echo-lock-ordering). Removed AUTONOMOUS-DEBUGGING.md reference.

**Status:** Matches actual fixes. **KEEP.**

### 29. Serial Comment Updates — `kernel/arch/arch-x86_64/src/lib.rs` (+8/-8 lines)
Updated `serial_print!` / `serial_println!` macro doc comments to accurately describe
that they route through os_log → serial (not console).

**Status:** Documentation fix. **KEEP.**

### 30. Cargo.lock — (+1 line)
Added `os_log` dependency to `driver-core` crate.

**Status:** Build artifact. **KEEP.**

### 31. QEMU SMAP Docs — `docs/agents/qemu-cpu-smap-requirement.md` (+10/-10 lines)
Updated example QEMU args to match new run targets.

**Status:** Documentation fix. **KEEP.**

---

## SUMMARY: WHAT TO DO

| Action | Files | Lines |
|--------|-------|-------|
| **KEEP** (real fixes) | tty.rs, ldisc.rs, vt/lib.rs, console.rs, exceptions.rs | ~200 lines |
| **KEEP** (trace cleanup) | buddy.rs, mapper.rs, address_space.rs, init.rs, init/main.rs | -900 lines removed |
| **KEEP** (build config) | qemu.mk, config.mk, help.mk, qemu-mcp, CLAUDE.md | config changes |
| **KEEP** (deleted scripts) | 7 debug scripts/docs | -1167 lines removed |
| **REMOVE** (diagnostics) | renderer.rs, lib.rs, console.rs | ~80 lines of diag junk |
| **RESTORE** | renderer.rs draw_bitmap_glyph | restore original from HEAD |

### THE DISPLAY BUG
The rendering code (renderer.rs) was UNCHANGED from HEAD until I started adding diagnostics.
The diagnostics currently in renderer.rs (forced red fg, green dots, replaced glyph renderer)
are ALL mine and ALL need to be removed/restored. Once renderer.rs is back to HEAD state,
the original glyph rendering should work — it was working yesterday at commit time.
