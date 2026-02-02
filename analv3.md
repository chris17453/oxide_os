# OXIDE OS — Implementation Plan v3
## Items 2-4: SMP Boot, Signal Stop/Continue, ext4 Timestamps

---

## Item 2: SMP/AP Boot — ACPI MADT Enumeration

### Current State
- INIT-SIPI-SIPI protocol **fully implemented** in `kernel/smp/smp/src/cpu.rs::boot_ap()`
- AP trampoline (real→protected→long mode) **complete** in `arch-x86_64/src/ap_boot.s` + `ap_boot.rs`
- AP Rust entry + scheduler init **complete** in `kernel/src/smp_init.rs`
- IPI infrastructure, per-CPU data, TLB shootdown all **complete**
- **BLOCKER:** CPU enumeration is **hardcoded** — only boots CPU 1 with APIC ID 1
- **BLOCKER:** Bootloader does NOT pass RSDP pointer to kernel
- **BLOCKER:** No ACPI table parsing exists

### Implementation Steps

#### 2a. Add RSDP to BootInfo
- **File:** `kernel/boot/boot-proto/src/lib.rs`
- Add `rsdp_physical_address: u64` field to `BootInfo` struct (0 = not found)

#### 2b. UEFI RSDP Discovery
- **File:** `bootloader/boot-uefi/src/main.rs`
- Before exiting boot services, search UEFI SystemTable ConfigurationTable for ACPI 2.0 GUID (`8868e871-e4f1-11d3-bc22-0080c73c8881`) then ACPI 1.0 GUID
- Store physical address in BootInfo

#### 2c. ACPI MADT Parser
- **New crate:** `kernel/acpi/acpi/` (or module in existing code)
- Parse RSDP → RSDT/XSDT → find MADT (signature "APIC")
- Extract: Local APIC entries (type 0) → APIC ID + processor ID + enabled flag
- Extract: I/O APIC entries (type 1) → base address
- Return `Vec`-free iterator/array of discovered CPUs (max 256)

#### 2d. Dynamic CPU Enumeration in init.rs
- **File:** `kernel/src/init.rs` (lines ~463-559)
- Replace hardcoded `register_cpu(1, 1, false)` with MADT enumeration loop
- For each enabled LAPIC entry (excluding BSP): `register_cpu(id, apic_id, false)` then `boot_ap()`
- BSP identified by comparing APIC ID from `cpuid` or MADT flags

#### 2e. Verification
- Boot log should show discovered CPU count from MADT
- Each AP should reach `ap_entry_rust()` and go online

### Files Changed
| File | Change |
|------|--------|
| `kernel/boot/boot-proto/src/lib.rs` | Add rsdp field |
| `bootloader/boot-uefi/src/main.rs` | RSDP discovery |
| `kernel/acpi/acpi/` (new crate) | MADT parser |
| `kernel/src/init.rs` | Dynamic enumeration |
| `kernel/Cargo.toml` | Add acpi dep |
| `Cargo.toml` (workspace) | Add acpi member |

---

## Item 3: Process Signals — Stop/Continue

### Current State
- All 64 signals defined; delivery pipeline works for Terminate/UserHandler/Ignore
- `DefaultAction::Stop` defined for SIGSTOP/SIGTSTP/SIGTTIN/SIGTTOU
- `DefaultAction::Continue` defined for SIGCONT
- `TASK_STOPPED` state defined in sched-traits but **never set or checked**
- `scheduler.rs:417-419`: `// TODO: Stop/Continue not yet implemented`
- ProcessMeta has no `stop_signal` or job control fields
- wait() has WUNTRACED parsed but not acted on

### Implementation Steps

#### 3a. Extend ProcessMeta
- **File:** `kernel/proc/proc/src/meta.rs`
- Add `stop_signal: Option<u8>` — which signal stopped the process
- Add `continued: bool` — set on SIGCONT, cleared when parent reads it

#### 3b. Signal Delivery for Stop Signals
- **File:** `kernel/src/scheduler.rs` (signal delivery section ~lines 316-428)
- When DefaultAction::Stop: set process state to TASK_STOPPED, record stop_signal, notify parent with SIGCHLD
- When SIGCONT received: if TASK_STOPPED, transition to TASK_READY, set continued=true, notify parent with SIGCHLD
- SIGSTOP is non-catchable (like SIGKILL) — skip sigaction check

#### 3c. Scheduler Stop State Handling
- **File:** `kernel/sched/sched/src/lib.rs` (or wherever schedule() lives)
- TASK_STOPPED tasks must NOT be selected for scheduling (skip in run queue)
- On SIGCONT: re-enqueue to run queue

#### 3d. Wait Support for Stopped Children
- **File:** `kernel/proc/proc/src/wait.rs`
- When WUNTRACED set: also report children that have stopped (return status with stop signal)
- When WCONTINUED set: report children that have continued
- Encode status: stopped = `(stop_signal << 8) | 0x7f`, continued = `0xffff`

#### 3e. sys_kill Integration
- **File:** `kernel/syscall/syscall/src/signal.rs`
- No special handling needed — stop/continue logic triggers in scheduler's signal delivery

### Files Changed
| File | Change |
|------|--------|
| `kernel/proc/proc/src/meta.rs` | stop_signal, continued fields |
| `kernel/src/scheduler.rs` | Stop/Continue state transitions |
| `kernel/sched/sched/src/lib.rs` | Skip stopped tasks in scheduler |
| `kernel/proc/proc/src/wait.rs` | WUNTRACED/WCONTINUED reporting |

---

## Item 4: ext4 Timestamps & UID/GID

### Current State
- Inode struct has all timestamp fields (atime, mtime, ctime, crtime)
- `new_inode()` hardcodes `let now = 0u32` — all files created with epoch 0
- `touch_mtime()`, `touch_ctime()`, `touch_atime()` methods exist but **never called**
- UID/GID hardcoded to 0 in `vnode.rs` file creation
- Time source exists: `syscall::time::get_realtime()` works but is **private**
- ext4 can't depend on syscall (circular dep via vfs)
- `BOOT_TIME_SECS` defaults to Jan 1, 2024 (1704067200)

### Implementation Steps

#### 4a. os_core Time Bridge
- **File:** `kernel/core/os_core/src/lib.rs`
- Add time module with:
  ```rust
  static WALL_CLOCK_FN: AtomicPtr<()> = AtomicPtr::new(null_mut());
  pub fn register_wall_clock(f: fn() -> u64) { ... }
  pub fn wall_clock_secs() -> u32 { ... }  // returns unix timestamp
  ```
- Fallback: return 0 if no clock registered

#### 4b. Register Time Source in Kernel Init
- **File:** `kernel/syscall/syscall/src/time.rs`
- Make `get_realtime()` pub or create a wrapper
- **File:** `kernel/src/init.rs`
- After timer init: `os_core::time::register_wall_clock(syscall::time::wall_clock_secs)`

#### 4c. ext4 Uses os_core Time
- **File:** `kernel/fs/ext4/Cargo.toml` — add `os_core` dependency
- **File:** `kernel/fs/ext4/src/inode.rs`
- `new_inode()`: replace `let now = 0u32` with `os_core::time::wall_clock_secs()`

#### 4d. Timestamp Updates on Operations
- **File:** `kernel/fs/ext4/src/vnode.rs`
- Write path: call `touch_mtime()` + `touch_ctime()` after successful write
- Truncate: call `touch_mtime()` + `touch_ctime()`
- Rename: call `touch_ctime()` on moved inode, `touch_mtime()` on parent dirs
- Read: call `touch_atime()` (or skip if noatime mount option later)
- Metadata change: call `touch_ctime()`

#### 4e. UID/GID from Current Process
- **File:** `kernel/fs/ext4/src/vnode.rs`
- File/dir creation: get UID/GID from current process instead of hardcoded 0
- Need process accessor: `proc::current_uid()` / `proc::current_gid()` or similar

#### 4f. set_times() VnodeOps Implementation
- **File:** `kernel/fs/ext4/src/vnode.rs`
- Implement `set_times()` in VnodeOps trait impl (for utimensat/utime syscalls)

### Files Changed
| File | Change |
|------|--------|
| `kernel/core/os_core/src/lib.rs` | Time bridge module |
| `kernel/syscall/syscall/src/time.rs` | Expose wall_clock_secs() |
| `kernel/src/init.rs` | Register time source |
| `kernel/fs/ext4/Cargo.toml` | Add os_core dep |
| `kernel/fs/ext4/src/inode.rs` | Real timestamps in new_inode() |
| `kernel/fs/ext4/src/vnode.rs` | touch_* calls, UID/GID, set_times() |

---

## Commit Plan
- [ ] **Commit 1:** ACPI MADT parser + RSDP in BootInfo + UEFI discovery
- [ ] **Commit 2:** Dynamic SMP boot from MADT enumeration
- [ ] **Commit 3:** Process Stop/Continue signal handling
- [ ] **Commit 4:** ext4 timestamps via os_core time bridge
- [ ] **Commit 5:** ext4 UID/GID from process + set_times()

## Risks
- **SMP:** QEMU may present different MADT than real hardware; test with `-smp 4`
- **Signals:** Stop/Continue interacts with terminal foreground process group (not implementing full job control yet)
- **ext4 time:** Boot time is approximate (Jan 1, 2024 default); RTC read would be more accurate but out of scope
