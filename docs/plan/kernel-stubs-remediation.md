# OXIDE Kernel Stubs & TODO Remediation Plan

> Audit completed March 2026. 16 genuine stubs found in x86_64 kernel subsystems.
> aarch64/mips64 arch stubs (44 total) tracked separately — those ports aren't active.
>
> **STATUS: PLANNING**

---

## Priority Tiers

| Tier | Criteria | Timeline |
|------|----------|----------|
| **P0 — CRITICAL** | Missing functionality that breaks real workloads or causes silent data loss | Immediate |
| **P1 — HIGH** | Features that block common POSIX programs or cause incorrect behavior | Next |
| **P2 — MEDIUM** | Missing cleanup paths, incomplete subsystems, observability gaps | After P1 |
| **P3 — LOW** | Nice-to-have, infrastructure stubs, non-blocking placeholders | Ongoing |

---

## P0 — CRITICAL (Fix First)

### 0.1 — VirtIO Device Removal: No Cleanup on Detach

- **Problem:** Three VirtIO drivers have empty `remove()` methods. If a device is hot-unplugged or the driver is unloaded, virtqueues leak, DMA buffers leak, and global device registries retain stale entries. Under memory pressure this accelerates OOM.
- **Files:**
  - `kernel/drivers/audio/virtio-snd/src/lib.rs:774`
  - `kernel/drivers/gpu/virtio-gpu/src/lib.rs:1044`
  - `kernel/drivers/input/virtio-input/src/lib.rs:649`
- **Fix:** Each `remove()` must:
  1. Remove device from the global device list (`VIRTIO_*_DEVICES`)
  2. Destroy virtqueues (free DMA ring buffers via `mm().free_contiguous()`)
  3. Reset device (write 0 to VirtIO status register)
  4. Free any interrupt registrations
- **Validation:** QEMU device hot-unplug test; verify buddy allocator free count returns to baseline after detach.

### 0.2 — ext4 Extent Tree Cannot Grow

- **Problem:** ext4 cannot expand extent trees beyond the initial allocation. `allocate_extent()` returns `Ext4Error::NoSpace` when the tree needs splitting. Files that grow past the initial extent set will fail with ENOSPC even if disk space is available. Indirect block fallback also returns `UnsupportedFeature`.
- **Files:**
  - `kernel/fs/ext4/src/extent.rs:178` (indirect block fallback)
  - `kernel/fs/ext4/src/extent.rs:391` (extent tree growth)
- **Fix:**
  1. Implement extent tree splitting: when leaf is full, allocate new block, split entries, update parent index node
  2. Implement depth increase: when root index is full, push root down one level
  3. Indirect blocks can stay unimplemented (modern ext4 always uses extents) but should return a clearer error
- **Validation:** Create file > 4 extents; verify writes succeed and data reads back correctly. `fsck` after unmount.

---

## P1 — HIGH

### 1.1 — ITIMER_VIRTUAL and ITIMER_PROF Return ENOSYS

- **Problem:** `setitimer()`/`getitimer()` for `ITIMER_VIRTUAL` (user CPU time) and `ITIMER_PROF` (user+sys CPU time) return `-ENOSYS`. Programs using `SIGVTALRM`/`SIGPROF` (profilers, runtimes with GC timers) silently fail.
- **Files:** `kernel/syscall/syscall/src/lib.rs:2476,2518`
- **Fix:**
  1. Track per-process user_time and sys_time counters (increment in scheduler tick based on ring level)
  2. `ITIMER_VIRTUAL`: decrement on user ticks, deliver `SIGVTALRM` at expiry
  3. `ITIMER_PROF`: decrement on user+sys ticks, deliver `SIGPROF` at expiry
  4. Store timer state in `ProcessMeta` alongside existing `ITIMER_REAL`
- **Validation:** Write test program that sets ITIMER_VIRTUAL, spins in userspace, verifies SIGVTALRM fires.

### 1.2 — MAP_PRIVATE Copies All Pages (No COW)

- **Problem:** `mmap(MAP_PRIVATE)` eagerly copies all pages instead of using copy-on-write. This wastes physical memory proportional to the mapping size. Every `MAP_PRIVATE` file mapping doubles its memory cost.
- **Files:** `kernel/syscall/syscall/src/memory.rs:203`
- **Fix:** Mark source pages read-only + COW (same mechanism as fork COW). On write fault, allocate + copy single page. The COW infrastructure already exists from fork — reuse `mm-cow` crate.
- **Validation:** mmap a 4MB file MAP_PRIVATE; verify physical page count is ~0 until writes occur; verify writes are private (don't affect underlying file).

### 1.3 — IPv6 Not Implemented

- **Problem:** IPv6 packets are dropped/errored in the TCP/IP stack. `AF_INET6` sockets cannot be created. As more userspace programs default to IPv6, this causes silent connection failures.
- **Files:**
  - `kernel/net/tcpip/src/lib.rs:131`
  - `kernel/net/tcpip/src/filter.rs:36`
- **Fix:** Phase this — full IPv6 is large. Minimum viable:
  1. Parse IPv6 headers in packet receive path
  2. ICMPv6 neighbor discovery (equivalent of ARP)
  3. `AF_INET6` socket creation with `sockaddr_in6`
  4. IPv6 TCP/UDP send/receive through existing transport layer
  5. Routing filter support for IPv6
- **Validation:** `ping6 ::1` (loopback), then `ping6` to QEMU host via SLIRP.

---

## P2 — MEDIUM

### 2.1 — /proc/loadavg Always Reports [0,0,0]

- **Problem:** `/proc/loadavg` returns `0.00 0.00 0.00` regardless of system load. Tools like `uptime`, `top`, `htop` show zero load. Misleading for capacity monitoring.
- **Files:** `kernel/vfs/procfs/src/lib.rs:1644`
- **Fix:**
  1. Add `LOAD_AVG: [AtomicU64; 3]` global (1min, 5min, 15min) as fixed-point (×2048)
  2. In scheduler tick (every ~4ms on BSP), count runnable tasks across all CPUs
  3. Apply exponential decay: `load = load * decay + active * (2048 - decay)` where decay factors are `1884/2048` (1min), `2014/2048` (5min), `2037/2048` (15min) — standard Linux EXP_ constants
  4. Read from procfs handler, format as `%.2f`
- **Validation:** Run CPU-bound stress test; verify load climbs to expected values over 1 minute.

### 2.2 — Driver Probe Failure Not Logged

- **Problem:** When a driver fails to probe, the error is silently swallowed. Makes device initialization debugging painful.
- **Files:** `kernel/drivers/driver-core/src/registry.rs:127,134,161`
- **Fix:** Replace TODO comments with `os_log::warn!()` calls. The logging infrastructure exists — these are just missing callsites.
- **Validation:** Intentionally misconfigure a PCI BAR; verify warning appears on serial console.

### 2.3 — Socket Future Operations Stub

- **Problem:** Advanced socket operations (SO_REUSEPORT, multicast, raw sockets) hit a stub that does nothing.
- **Files:** `kernel/syscall/syscall/src/socket.rs:1767`
- **Fix:** Triage which operations matter most for real programs. At minimum:
  1. `SO_REUSEPORT` — needed for multi-worker servers
  2. `SO_KEEPALIVE` — needed for long-lived connections
  3. Return `-ENOPROTOOPT` for genuinely unsupported options instead of silent success
- **Validation:** Run a server with SO_REUSEPORT; verify two sockets can bind same port.

### 2.4 — Dynamic Linker Placeholder

- **Problem:** `dlopen()` creates a fake handle without loading any ELF. Programs using runtime dynamic linking get garbage.
- **Files:** `kernel/libc-support/dl/src/lib.rs:173`
- **Fix:** Implement minimal dlopen:
  1. Find and read ELF from filesystem
  2. Map LOAD segments into process address space
  3. Process relocations (R_X86_64_RELATIVE, R_X86_64_GLOB_DAT, R_X86_64_JUMP_SLOT)
  4. Run DT_INIT / DT_INIT_ARRAY
  5. `dlsym()` searches DT_SYMTAB
- **Note:** This is a large feature. Can be deferred if no current userspace programs need it.
- **Validation:** dlopen a .so, call a function from it via dlsym, verify correct return value.

---

## P3 — LOW

### 3.1 — Module Loader Bridge Stub

- **Problem:** `load_driver_module()` returns `Err(())`. Dynamic kernel module loading doesn't work.
- **Files:** `kernel/src/module_driver_bridge.rs:84`
- **Fix:** Wire up to the existing `kernel/module/module/` ELF loader. Resolve kernel symbols, map module code, call module init function.
- **Validation:** Build a trivial kernel module, load it at runtime, verify init function runs.

### 3.2 — AI Embedding Model is Hash Mock

- **Problem:** The embedding model in `kernel/ai/embed/` uses a hash function instead of a real transformer. Semantic search quality is poor.
- **Files:** `kernel/ai/embed/src/model.rs:86`
- **Fix:** This is intentionally a mock — real inference in kernel space is a research project. Keep as-is unless AI-powered features become a priority.
- **Validation:** N/A — mock is acceptable for current use.

### 3.3 — Deprecated `add_process()` in Scheduler

- **Problem:** Empty function kept for "legacy compatibility" but nothing calls it.
- **Files:** `kernel/src/scheduler.rs:167-174`
- **Fix:** Delete it. If nothing calls it, it's dead code.
- **Validation:** `cargo build` succeeds; grep confirms no callers.

---

## Arch Port Stubs (aarch64 + mips64) — Tracked Separately

These 44 stubs are expected for inactive architecture ports. They become P0 when those ports are activated.

| Arch | Category | Count | What's needed |
|------|----------|-------|---------------|
| aarch64 | Serial (PL011 UART) | 3 | MMIO UART driver |
| aarch64 | Timing (CNTPCT_EL0, MIDR_EL1) | 2 | System register reads |
| aarch64 | Memory barriers (DMB) | 3 | Inline asm: dmb ish/ishld/ishst |
| aarch64 | Exceptions (VBAR_EL1) | 2 | Vector table setup |
| aarch64 | Syscalls (SVC) | 2 | SVC handler + entry point |
| aarch64 | SMP (PSCI + GIC) | 7 | PSCI CPU_ON, GIC SGI, generic timer |
| mips64 | Serial (Zilog 8530) | 3 | SCC UART driver |
| mips64 | Timing (CP0 Count/PRId) | 2 | CP0 register reads |
| mips64 | Memory barriers (SYNC) | 3 | Inline asm: sync |
| mips64 | Exceptions (EBase) | 2 | CP0 EBase setup |
| mips64 | Syscalls | 2 | Syscall handler + entry point |
| mips64 | SMP (SGI HUB) | 8 | HUB IPC, CP0 Count delays |

---

## Change Checklist (per item)

- [ ] Read relevant code and understand current behavior
- [ ] Implement fix with minimal diff
- [ ] Run `make build` — clean compile
- [ ] Test the specific fix
- [ ] Update this plan with status
- [ ] One commit per fix (imperative subject)
