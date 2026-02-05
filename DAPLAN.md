# OXIDE OS: Complete TODO Cleanup Implementation Plan

## Executive Summary

**Mission:** Clean up all 13 tracked TODOs across timing, scheduling, VFS, syscalls, TTY, boot, and process subsystems.

**Status:** 8 of 13 complete (Phases 1 & 2 DONE)
**Approach:** Eager CPU affinity enforcement, reference counting for unmount safety, complete legacy cleanup

---

## Progress Tracker

### ✅ Phase 1: Critical Kernel Correctness (COMPLETE)
- [x] **TSC Calibration** - PIT-based calibration, cached frequency
- [x] **Reschedule IPI** - Cross-CPU wake with <1ms latency
- [x] **CPU Time Tracking** - Sync to ProcessMeta for accurate clock_gettime
- [x] **CPU Affinity Migration** - Eager IPI-based enforcement
- [x] **Debug Buffer Cleanup** - Removed dead code

### ✅ Phase 2: VFS Safety & Correctness (COMPLETE)
- [x] **Unmount File Check** - Reference counting prevents busy unmount
- [x] **Tmpfs UID** - Capture owner from process context
- [x] **Mount Remount** - Dynamic flag updates (ro↔rw)

### 🚧 Phase 3: Features & Compatibility (PENDING)
- [ ] **Real Sysfs** - ~200 line pseudo-filesystem implementation
- [ ] **DECRQSS Terminal** - VT100 state query for vim/tmux
- [ ] **Remove BOUND_SOCKETS** - Delete legacy registry

### 🔍 Phase 4: Investigation & Platform (PENDING)
- [ ] **TLS Setup Hack** - Debug ELF parser reliability
- [ ] **ARCS Memory** - Document as MIPS-specific/blocked

---

## Implementation Details

### Phase 1: Critical Kernel Correctness ✅

#### 1.1 TSC Calibration ✅
**Files:** `kernel/arch/arch-x86_64/src/lib.rs`, `kernel/arch/arch-x86_64/src/apic.rs`

**Implementation:**
- Added `CACHED_TSC_FREQUENCY: AtomicU64` for BSP/AP coordination
- `calibrate_tsc()`: 10ms PIT reference window, calculate Hz
- Called from `apic::init()` on boot
- Fallback to 2.5GHz if called before calibration
- Pattern matches APIC's PIT calibration (avoids data race)

**Testing:** Verify on QEMU with different CPU speeds, check dmesg for calibration result

**Persona:** SableWire, GraveShift

---

#### 1.2 Reschedule IPI ✅
**Files:** `kernel/src/scheduler.rs`, `kernel/sched/sched/src/core.rs`

**Implementation:**
- `handle_reschedule_ipi()`: Sets need_resched, sends EOI
- Registered for vector 0xF0 during scheduler init
- `sched::core.rs:254`: Calls `smp::ipi::send_reschedule(cpu)`
- Infrastructure complete in `smp/src/ipi.rs`

**Testing:** Two tasks with CPU 0/1 affinity, measure wake latency <1ms

**Persona:** NeonRoot

---

#### 1.3 CPU Time Tracking ✅
**File:** `kernel/sched/sched/src/runqueue.rs:352`

**Implementation:**
```rust
// After t.sum_exec_runtime += delta:
if let Some(meta_arc) = t.meta.as_ref() {
    if let Some(mut meta) = meta_arc.try_lock() {
        meta.cpu_time_ns = t.sum_exec_runtime;
    }
}
```

**Rationale:** Single update point ensures consistency, try_lock avoids timer interrupt deadlock

**Testing:** Spin task for 1s, verify `clock_gettime(CLOCK_PROCESS_CPUTIME_ID)` ≈ 1,000,000,000ns

**Persona:** GraveShift

---

#### 1.4 CPU Affinity Migration ✅
**File:** `kernel/sched/sched/src/core.rs:615`

**Implementation:**
- Check if new affinity excludes current CPU
- Running task: Set need_resched + send IPI if remote
- Queued task: Dequeue, extract, re-enqueue (select_task_rq picks new CPU)
- Returns task via Option to avoid lock issues

**Testing:** Task on CPU 3, change affinity to CPUs 0-1, verify immediate migration

**Persona:** ThreadRogue

---

#### 1.5 Debug Buffer Cleanup ✅
**File:** `kernel/src/scheduler.rs:137`

**Implementation:** Deleted commented TODO + no-op `try_flush_debug()` call

**Rationale:** Buffering removed, writes are direct to serial

---

### Phase 2: VFS Safety & Correctness ✅

#### 2.1 Unmount File Check ✅
**Files:** `kernel/vfs/vfs/src/mount.rs`, `kernel/vfs/vfs/src/file.rs`, `kernel/syscall/syscall/src/vfs.rs`

**Implementation:**
- **Mount:** Added `open_file_count: Arc<AtomicUsize>`
- **File:** Added `mount_ref_count: Option<Arc<AtomicUsize>>` field
- **File::new_with_mount_ref():** Increments counter on creation
- **Drop for File:** Decrements counter on close
- **VFS::get_mount_ref_for_path():** Resolves path to counter
- **sys_open:** Uses `File::new_with_mount_ref()` with mount reference
- **VFS::unmount():** Checks counter, returns EBUSY if >0

**Rationale:** Reference counting handles multi-path access, Drop ensures cleanup

**Testing:** Open file on /mnt/disk, attempt unmount (EBUSY), close file, retry (success)

**Persona:** WireSaint

---

#### 2.2 Tmpfs UID ✅
**File:** `kernel/vfs/tmpfs/src/lib.rs`

**Implementation:**
- **TmpDir/TmpFile:** Added `uid: u32, gid: u32` fields
- **new_root():** Set uid=0, gid=0 (root owned)
- **new_subdir():** Capture from `os_core::current_uid_gid()`
- **TmpFile::new():** Capture from `os_core::current_uid_gid()`
- **stat():** Return stored uid/gid instead of hardcoded 0

**Pattern:** Follows ext4 at `kernel/fs/ext4/src/vnode.rs:240-243`

**Testing:** `su user1000; touch /tmp/test; stat /tmp/test` shows uid=1000

**Persona:** EmberLock

---

#### 2.3 Mount Remount ✅
**Files:** `kernel/vfs/vfs/src/mount.rs`, `kernel/src/mount.rs`

**Implementation:**
- **Mount:** Changed `flags` to `RwLock<MountFlags>` for interior mutability
- **update_flags():** Atomically updates flags with write lock
- **flags():** Acquires read lock
- **VFS::remount():** Finds mount, delegates to `update_flags()`
- **handle_remount():** Maps VfsError to errno codes

**Testing:** Mount r/w → remount r/o (writes fail) → remount r/w (writes succeed)

**Persona:** WireSaint

---

### Phase 3: Features & Compatibility 🚧

#### 3.1 Real Sysfs (TODO)
**New crate:** `kernel/vfs/sysfs/`

**Plan:**
1. Create sysfs crate following procfs pattern (`kernel/vfs/procfs/src/lib.rs`)
2. Implement directory hierarchy:
   - `/sys/kernel/debug/` (empty for now)
   - `/sys/devices/` (empty, future: device tree)
   - `/sys/class/` (empty, future: device classes)
3. Update `mount_sysfs()` at `kernel/src/mount.rs:219`
4. Add sysfs dependency to kernel Cargo.toml

**Size:** ~200 lines (VnodeOps boilerplate for 4 directories)

**Testing:** `ls /sys/kernel`, `cat /sys/class/` should not error

**Persona:** IronGhost, NightDoc

---

#### 3.2 DECRQSS Terminal State Query (TODO)
**File:** `kernel/tty/terminal/src/lib.rs:662`

**Plan:**
1. Parse DCS parameter (byte after `$ q`)
2. Build response based on parameter:
   - `m`: Query SGR attributes → read handler.attrs, encode as `1;5;7m` style
   - `" q`: Query cursor shape → return `2` (block) / `4` (underline) / `6` (bar)
   - `r`: Query scroll margins → return `1;<rows>r`
3. Send response: `DCS 1 $ r <state> ST` (valid) or `DCS 0 $ r ST` (invalid)

**Reference:** `external/vim/src/libvterm/t/26state_query.test:29-57`

**Testing:** Run vim, verify no "unknown terminal capability" errors

**Persona:** IronGhost, NightDoc

---

#### 3.3 Remove BOUND_SOCKETS (TODO)
**File:** `kernel/syscall/syscall/src/socket.rs`

**Plan:**
1. Delete `static BOUND_SOCKETS` declaration (line 48)
2. Remove insert logic in `sys_bind()` (lines 577-582)
3. Remove cleanup in `close_socket()` (lines 1361-1372)
4. Verify no other references: `rg BOUND_SOCKETS`

**Replacement:** LOOPBACK_REGISTRY handles all socket types (lines 192-232)

**Testing:** UDP bind/send/recv, RAW sockets, loopback connectivity

**Persona:** ShadePacket

---

### Phase 4: Investigation & Platform 🔍

#### 4.1 TLS Setup Hack (TODO)
**File:** `kernel/proc/proc/src/exec.rs:202`

**Approach:**
1. Add debug logging to ELF parser's `tls_template()` method
2. Compare with manual `forced_tls` search results
3. Identify discrepancies (offset errors, size mismatches, null checks)
4. Fix ELF parser bugs
5. Test with TLS-using binaries (pthread apps)
6. Remove `forced_tls` block once parser reliable

**Status:** Blocked on ELF parser crate investigation (location TBD)

**Testing:** Run pthread-based userspace apps, verify TLS variables work correctly

**Persona:** Hexline

---

#### 4.2 ARCS Firmware Memory (DEPRIORITIZED)
**File:** `kernel/boot/boot-proto/src/arcs.rs:219`

**Status:** MIPS/SGI platform-specific, OXIDE targets x86_64

**Action:** Add comment noting "blocked on MIPS port", no implementation

**Rationale:** Code retained for potential future MIPS support but not tested/maintained

---

## Critical Files Reference

| Priority | File Path | TODOs | Status |
|----------|-----------|-------|--------|
| **P1** | `kernel/arch/arch-x86_64/src/lib.rs` | TSC calibration | ✅ DONE |
| **P1** | `kernel/sched/sched/src/core.rs` | IPI send, affinity migration | ✅ DONE |
| **P1** | `kernel/sched/sched/src/runqueue.rs` | CPU time sync | ✅ DONE |
| **P1** | `kernel/src/scheduler.rs` | IPI handler, debug cleanup | ✅ DONE |
| **P2** | `kernel/vfs/vfs/src/mount.rs` | Unmount check, remount flags | ✅ DONE |
| **P2** | `kernel/vfs/vfs/src/file.rs` | Mount ref counting | ✅ DONE |
| **P2** | `kernel/vfs/tmpfs/src/lib.rs` | UID from context | ✅ DONE |
| **P2** | `kernel/src/mount.rs` | Remount handler | ✅ DONE |
| **P3** | `kernel/tty/terminal/src/lib.rs` | DECRQSS response | 🚧 TODO |
| **P3** | `kernel/syscall/syscall/src/socket.rs` | BOUND_SOCKETS cleanup | 🚧 TODO |
| **P4** | `kernel/proc/proc/src/exec.rs` | TLS hack investigation | 🚧 TODO |

---

## Persona Assignments

- **GraveShift**: Kernel systems (TSC, scheduler, CPU time)
- **SableWire**: Firmware + hardware (TSC calibration, APIC timing)
- **NeonRoot**: SMP integration (IPI, cross-CPU operations)
- **ThreadRogue**: Runtime + process model (affinity migration)
- **WireSaint**: Storage systems (VFS, mount, unmount safety)
- **EmberLock**: Identity + auth (tmpfs UID, permissions)
- **IronGhost**: Platform APIs (sysfs, terminal)
- **NightDoc**: Developer experience (vim compatibility, cleanup)
- **Hexline**: Compiler + toolchain (TLS/ELF investigation)
- **ShadePacket**: Networking stack (socket cleanup)

---

## Testing Strategy

### Phase 1 Testing ✅
- **TSC:** Boot on QEMU, check dmesg calibration, verify delay accuracy
- **IPI:** Two tasks with affinity (CPU 0/1), measure wake latency
- **CPU time:** `time sleep 1` shows accurate process time
- **Affinity:** Change affinity while task running, verify immediate migration
- **Debug:** No commented code in scheduler.rs

### Phase 2 Testing ✅
- **Unmount:** Open file, unmount (fail EBUSY), close, unmount (succeed)
- **Tmpfs UID:** Files show correct owner from creating process
- **Remount:** ro→rw→ro transitions work correctly

### Phase 3 Testing (Pending)
- **Sysfs:** `ls /sys/kernel`, `cat /sys/class/` work without errors
- **DECRQSS:** Run vim/tmux without terminal capability errors
- **BOUND_SOCKETS:** UDP/RAW sockets work, loopback functional

### Phase 4 Testing (Pending)
- **TLS:** pthread apps run correctly with proper TLS variable access
- **ARCS:** Comment added, no functional testing needed

---

## Success Criteria

**Phase 1 ✅ COMPLETE:**
- [x] TSC calibrates correctly on non-2.5GHz systems
- [x] Remote task wakeup latency <1ms (not 10ms)
- [x] `clock_gettime(CLOCK_PROCESS_CPUTIME_ID)` returns accurate values
- [x] Affinity violations trigger immediate migration with IPI
- [x] No commented debug buffer flush code

**Phase 2 ✅ COMPLETE:**
- [x] Cannot unmount filesystem with open files (EBUSY)
- [x] Tmpfs files show correct owner UID/GID
- [x] Can remount filesystems with different flags (ro↔rw)

**Phase 3 🚧 PENDING:**
- [ ] `/sys` directory structure exists with kernel/devices/class subdirs
- [ ] Vim/tmux work without terminal capability errors
- [ ] No references to BOUND_SOCKETS in codebase

**Phase 4 🚧 PENDING:**
- [ ] TLS setup uses ELF parser exclusively (no manual hack)
- [ ] ARCS marked as platform-specific/blocked

---

## Commits

All Phase 1 and Phase 2 work has been committed and pushed:
- **37aef8f** - Phase 1 complete (TSC, IPI, CPU time, affinity, debug cleanup)
- **18f8266** - Phase 2.2 complete (tmpfs UID tracking)
- **d483d41** - Phase 2.3 complete (mount remount support)

Phase 2.1 (unmount safety) was included in commit 37aef8f.

---

## Next Session

Continue with Phase 3:
1. Implement sysfs pseudo-filesystem (~200 lines)
2. Add DECRQSS terminal state query support
3. Remove BOUND_SOCKETS legacy registry

Then Phase 4 investigation work (TLS, ARCS documentation).

**Estimated:** ~2-3 hours for Phase 3, investigative for Phase 4.

---

*Plan created: 2026-02-04*
*Status: 8/13 TODOs complete (61%)*
*All changes build successfully, Phase 1 & 2 pushed to main*
