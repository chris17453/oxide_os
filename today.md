# Debugging Session — 2025-02-13

## What We Fixed
1. **Initramfs missing critical binaries** — getty, login, servicemgr were only on ext4 rootfs.
   If ext4 mount at /mnt/root failed (timing), init stayed on initramfs and couldn't exec getty.
   Fixed in `mk/initramfs.mk` — getty, login, servicemgr now included.

2. **switch_root validation too weak** — only checked `/mnt/root/sbin/init`.
   Now validates `/etc/passwd`, `/bin/getty`, `/bin/login`, `/bin/esh` before pivoting.
   Fixed in `userspace/system/init/src/main.rs`.

3. **GDB debugging infrastructure** — added `-s` (GDB server port 1234) to both run-fedora
   and run-rhel targets. Added `make attach` for one-command backtrace dump.
   Fixed in `mk/qemu.mk`.

## What's Still Broken — The Intermittent Boot Hang
**Symptom:** ~50% of boots get to login prompt. ~50% hang at "[init] Getty started" forever.

**What we know:**
- The kernel boots fine every time. Initramfs loads, ext4 mounts at /mnt/root.
- Init runs, does switch_root (pivots to ext4), forks getty.
- Getty's `setup_terminal()` calls `open2("/dev/console", O_RDWR)` — this is where it hangs.
- 260 context switches at 125s uptime = processes ARE running (init in wait(), getty blocked).
- Terminal renders: 0 (getty never writes to console because it's stuck in open).
- No kernel exception/panic/fault messages in serial.

**Root cause hypothesis — devfs mount_move race or VFS bug:**
- switch_root does `mount_move("/dev", "/mnt/root/dev")` then `pivot_root()`
- After pivot, `/dev` should be the moved devfs
- Getty opens `/dev/console` — if the devfs move didn't propagate correctly in VFS,
  the open could block or fail
- This would explain intermittency: timing-dependent VFS state after pivot_root

**What to investigate next:**
1. Add serial trace to `switch_root()` — print return codes of each mount_move and pivot_root
2. Add serial trace to getty — print before/after open2("/dev/console")
3. Check kernel's `mount_move` and `pivot_root` implementations for VFS consistency bugs
4. Consider: does the kernel's open() for ConsoleDevice block? Check if there's a
   blocking path in VtDevice::open() or the TTY open logic
5. Nuclear option: skip switch_root entirely (comment out the call) and verify boots are
   100% reliable on initramfs alone — this isolates whether pivot_root is the culprit
6. Check if `open2()` (O_RDWR without O_NOCTTY) triggers CTTY acquisition that blocks

**Files changed this session:**
- `mk/initramfs.mk` — added getty, login, servicemgr to initramfs
- `mk/qemu.mk` — added -s GDB flag and make attach target
- `userspace/system/init/src/main.rs` — hardened switch_root validation

**Key diagnostic tools:**
- `make attach` — dumps all CPU backtraces via GDB (QEMU must be running with -s)
- `vncdo -s localhost::5999 capture screenshot.png` — captures actual framebuffer (MCP QEMU)
- `vncdo -s localhost::5900 capture screenshot.png` — captures framebuffer (make run)
- Serial log at `target/serial.log` (MCP) or stdio (make run)
