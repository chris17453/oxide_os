# OXIDE OS Performance, Security & Stability Audit Plan

> Full audit completed March 2026. ~100 findings across 6 subsystems.
> This document is the master plan for remediation.
>
> **STATUS: ALL TIERS COMPLETE (P0-P3) — 24 remediation items implemented.**
> Build verified clean after all changes.

---

## Priority Tiers

| Tier | Criteria | Timeline |
|------|----------|----------|
| **P0 — CRITICAL** | Data corruption, memory leaks that cause OOM, kernel takeover vectors | Immediate |
| **P1 — HIGH** | Crashes, SMP races, hang vectors, missing safety checks | Next |
| **P2 — MEDIUM** | Performance bottlenecks, missing hardening, correctness gaps | After P1 |
| **P3 — LOW** | Code quality, defense-in-depth, future-proofing | Ongoing |

---

## P0 — CRITICAL (Fix First)

### 0.1 — Physical Frame Leak on Process Exit
- **Problem:** `UserAddressSpace` has no `Drop` impl. Every process exit leaks ALL physical frames (code, stack, heap, page tables). OOM is inevitable.
- **Files:** `kernel/proc/proc/src/address_space.rs`, `kernel/src/scheduler.rs:146`
- **Fix:** Implement `Drop for UserAddressSpace` that iterates `allocated_frames` and calls `mm().free_frame()` for each. Also walk page tables to free PT pages. Hook into `remove_process()` path.
- **Validation:** Boot, run 50+ processes, check buddy allocator free count returns to baseline.

### 0.2 — Per-CPU GDT/TSS (SMP Stack Corruption)
- **Problem:** Single global GDT/TSS shared by all CPUs. `TSS.RSP0` (used by hardware interrupts from usermode) is a single value — 4 CPUs overwrite each other's kernel stack pointer.
- **Files:** `kernel/arch/arch-x86_64/src/gdt.rs:221-224`
- **Fix:** Create `static GDT_ARRAY: [Gdt; MAX_CPUS]` and `static TSS_ARRAY: [TaskStateSegment; MAX_CPUS]`. Each CPU loads its own GDT+TSS during init. `set_kernel_stack(cpu_id, rsp)` writes to `TSS_ARRAY[cpu_id].rsp[0]`. APs call `gdt::init_cpu(cpu_id)` instead of `gdt::init()`.
- **Pattern:** Same as `SYSCALL_USER_CONTEXTS` per-CPU array pattern.
- **Validation:** Boot with `-smp 4`, trigger rapid user->kernel transitions (signals, page faults) on all CPUs simultaneously.

### 0.3 — `copy_path_from_user` Returns Reference to Userspace (TOCTOU)
- **Problem:** Returns `&'static str` pointing into userspace memory. Malicious thread can change path after validation. Affects ALL path-based syscalls (~25 syscalls).
- **Files:** `kernel/syscall/syscall/src/vfs.rs:81-99`
- **Fix:** Change to return `String` (kernel-owned copy):
  ```rust
  pub fn copy_path_from_user(path_ptr: u64, path_len: usize) -> Option<String> {
      // validation unchanged...
      let path_bytes = unsafe { core::slice::from_raw_parts(path_ptr as *const u8, path_len) };
      core::str::from_utf8(path_bytes).ok().map(|s| String::from(s))
  }
  ```
- **Impact:** All callers already use the result as `&str` via deref coercion — minimal caller changes.
- **Validation:** Multi-threaded test that races path modification against syscalls.

### 0.4 — `ARCH_SET_GS` Corrupts Kernel Per-CPU Data
- **Problem:** Writes to `IA32_GS_BASE` (0xC0000101) which, during a syscall (after swapgs), holds the *kernel's* GS base. Corrupts per-CPU data.
- **Files:** `kernel/syscall/syscall/src/lib.rs:2853-2878`
- **Fix:** Write to `IA32_KERNEL_GS_BASE` (0xC0000102) instead — that's where the *user's* GS value lives during a syscall. The next `swapgs` (on sysret) will move it into the active GS_BASE.
  ```rust
  arch_prctl_op::ARCH_SET_GS => {
      unsafe { core::arch::asm!(
          "mov ecx, 0xC0000102",  // KERNEL_GS_BASE (holds user GS during syscall)
          "mov eax, {val_lo:e}",
          "mov edx, {val_hi:e}",
          "wrmsr",
          val_lo = in(reg) (addr as u32),
          val_hi = in(reg) ((addr >> 32) as u32),
          out("ecx") _,
      ); }
  }
  ```
- **Validation:** Userspace program calling `arch_prctl(ARCH_SET_GS, ...)` must not crash kernel.

---

## P1 — HIGH (Fix Next)

### 1.1 — Fork COW TLB Shootdown
- **Problem:** `clone_address_space_cow()` uses local `flush_tlb(virt)` per page. Other CPUs retain writable TLB entries, bypassing COW protection.
- **Files:** `kernel/proc/proc/src/fork.rs:267`
- **Fix:** Replace `flush_tlb(virt)` with `smp::tlb_shootdown(virt, 1)` for each COW-marked page. Better: batch all COW pages and do a single shootdown range (or `flush_tlb_all_cpus()` if >32 pages).
- **Also fix:** `do_exec()` (exec.rs:503) — add `smp::flush_tlb_all_cpus()` after CR3 switch for CLONE_VM scenarios.

### 1.2 — Kernel Stack Guard Pages
- **Problem:** No guard page below kernel stacks. Stack overflow silently corrupts memory.
- **Files:** `kernel/src/process.rs:263` (stack allocation), `kernel/proc/proc/src/fork.rs` (clone)
- **Fix:** Allocate one extra page below each kernel stack. Map it in the page table as NOT_PRESENT. On overflow, the CPU triggers a page fault (which uses IST1 stack), allowing the kernel to kill the offending task instead of silently corrupting memory.
- **Consideration:** Requires virtual mapping for kernel stacks (currently uses physical direct map). Alternative: just allocate the guard page and never map it — access via direct physical map to unmapped frame triggers #PF.

### 1.3 — STAC/CLAC Audit + SFMASK Fix
- **Problem:** 20+ syscalls access user memory without STAC/CLAC. SFMASK value 0x4700 doesn't include AC bit.
- **Files:** `kernel/arch/arch-x86_64/src/syscall.rs:25`, plus ~20 locations across `syscall/src/*.rs` and `process.rs`
- **Fix (two-part):**
  1. Fix SFMASK: `const SFMASK_VALUE: u64 = 0x44700;` (add AC=0x40000)
  2. Audit every user memory access in syscall code. Create helper functions:
     ```rust
     fn with_user_access<F, R>(f: F) -> R where F: FnOnce() -> R {
         unsafe { core::arch::asm!("stac"); }
         let result = f();
         unsafe { core::arch::asm!("clac"); }
         result
     }
     ```
  3. Wrap all user memory reads/writes in `with_user_access()`. Keep the STAC window minimal.
- **Affected syscalls:** sys_fstat, sys_stat, sys_lstat, sys_pipe, sys_sigaction, sys_sigprocmask, sys_sigpending, sys_sigsuspend, sys_setsockopt, sys_getsockopt, sys_getsockname, sys_getpeername, sys_bind, sys_connect, write_sockaddr_in, parse_sockaddr_in, kernel_exec (path/argv/envp), sys_setitimer, sys_getitimer, sys_arch_prctl, sys_fw_add_rule, sys_fw_get_conntrack, clear_child_tid
- **Validation:** Enable SMAP in CR4 (init.rs:144-151, uncomment), boot, run userspace programs. Every SMAP violation = GPF that identifies missing STAC.

### 1.4 — GPF Handler: Kill Process, Don't Halt System
- **Problem:** General Protection Fault handler halts entire system even for user-mode faults.
- **Files:** `kernel/arch/arch-x86_64/src/exceptions.rs:940-942`
- **Fix:** Check if fault was from user mode (CS & 3 == 3). If user-mode, send SIGSEGV to the process. If kernel-mode, panic with diagnostic output.
  ```rust
  extern "C" fn handle_general_protection(frame: *const InterruptFrame, error: u64) {
      let frame = unsafe { &*frame };
      if frame.cs & 3 != 0 {
          // User-mode GPF — kill process
          kill_faulting_process(frame, 11); // SIGSEGV
      } else {
          // Kernel-mode GPF — fatal
          serial_panic_dump(frame, error);
          loop { unsafe { core::arch::asm!("hlt"); } }
      }
  }
  ```

### 1.5 — COW BTreeMap Heap Allocation in Exception Path
- **Problem:** `CowTracker` uses `BTreeMap` which can allocate during `increment()`. If page fault occurs while heap lock is held, the COW increment deadlocks.
- **Files:** `kernel/mm/mm-cow/src/lib.rs:22-26`
- **Fix (two options):**
  - **Option A (simple):** Use a fixed-capacity array/hashmap for COW tracking. Pre-allocate slots.
  - **Option B (standard):** Use a separate lock for COW tracker (not the heap allocator's lock). Accept that nested allocation is rare and the heap lock is usually not held during page faults.
  - **Option C (Linux-style):** Use `struct page` metadata embedded in the frame allocator — no separate data structure needed. Each page has an atomic refcount. No heap allocation in the COW path.

### 1.6 — No OOM Recovery
- **Problem:** When memory runs out, heap allocator returns null, Rust panics. No process killing, no reclaim.
- **Files:** `kernel/mm/mm-heap/src/linked_list.rs:244`, `kernel/mm/mm-manager/src/lib.rs`
- **Fix (phased):**
  1. **Phase 1:** Return proper error codes from frame allocation failures. Propagate `ENOMEM` to userspace syscalls (mmap, fork, exec) instead of panicking.
  2. **Phase 2:** Implement basic OOM killer — find largest-RSS process, send SIGKILL, reclaim frames.
  3. **Phase 3:** Page cache eviction (when page cache exists).

### 1.7 — Debug Statics Data Race in Syscall Entry
- **Problem:** `SYSRET_DEBUG_RSP` and 8 other `static mut` globals written by all CPUs on every syscall.
- **Files:** `kernel/arch/arch-x86_64/src/syscall.rs:543-564`
- **Fix:** Either make them per-CPU arrays (indexed by CPU ID from GS), or remove them entirely (they're debug statics that are never read from code). Removing is cleaner.

---

## P2 — MEDIUM (Performance & Hardening)

### 2.1 — Consolidate Context Switch Lock Acquisitions
- **Problem:** 5 separate RQ lock acquire/release cycles per context switch.
- **Files:** `kernel/src/scheduler.rs:625-871`
- **Fix:** Create `sched::context_switch_transaction(old_pid, new_pid)` that:
  1. Acquires RQ lock once
  2. Saves old task's context + preemption state
  3. Sets new task as current
  4. Gets new task's switch info (cr3, fs_base, kernel_stack)
  5. Loads new task's preemption state
  6. Releases lock
  7. Returns a `SwitchInfo` struct for the arch-specific switch
- **Impact:** ~100-500ns savings per context switch. Fewer cache line bounces.

### 2.2 — SMP Load Balancing
- **Problem:** No task migration. 75% of CPU wasted with `-smp 4`.
- **Fix (phased):**
  1. **Phase 1 — Pull-based:** In idle loop, before HLT, check other CPUs' `nr_running`. If any > 1 and local == 0, steal a task. Use `try_with_rq` to avoid deadlock.
  2. **Phase 2 — Periodic:** In `scheduler_tick()` (every 100ms), check load imbalance. Migrate tasks from busiest to least-busy CPU.
  3. **Phase 3 — NUMA-aware:** (Future, when NUMA support exists)
- **Key data structure:** Per-CPU `nr_running` as `AtomicU32` for lockless load checking.

### 2.3 — File Permission Checks (DAC)
- **Problem:** Zero permission enforcement on any file operation.
- **Fix:** Implement Linux-style DAC:
  1. Add `check_permission(vnode, uid, gid, mode)` function to VFS layer
  2. Call it at the entry of every file-modifying syscall
  3. Check owner/group/other bits against calling process's euid/egid
  4. Root (uid 0) bypasses checks (or use capabilities later)
  5. `sys_kill` must check: sender's euid == target's uid OR sender is root
- **Files:** New `kernel/vfs/vfs/src/permission.rs`, modifications to `kernel/syscall/syscall/src/vfs.rs`, `dir.rs`, `signal.rs`

### 2.4 — Reduce `with_rq` Spin Limit
- **Problem:** 100M spin iterations (~200ms) before deadlock fallback.
- **Files:** `kernel/sched/sched/src/core.rs:130-155`
- **Fix:** Reduce to 10,000 iterations (~50us). If still contended, use `hlt`-based backoff:
  ```rust
  fn with_rq<F, R>(cpu: u32, f: F) -> Option<R> {
      for _ in 0..10_000 {
          if let Some(g) = RUN_QUEUES[cpu as usize].try_lock() {
              return Some(f(&mut *g));
          }
          core::hint::spin_loop();
      }
      // Bounded HLT backoff — wake on next interrupt
      for _ in 0..10 {
          unsafe { core::arch::asm!("sti", "hlt", options(nomem, nostack)); }
          if let Some(g) = RUN_QUEUES[cpu as usize].try_lock() {
              return Some(f(&mut *g));
          }
      }
      // Final fallback — blocking lock
      Some(f(&mut *RUN_QUEUES[cpu as usize].lock()))
  }
  ```

### 2.5 — Replace BTreeMap Task Storage with Flat Array
- **Problem:** BTreeMap for task storage has O(log n) lookups, heap allocation, poor cache locality.
- **Files:** `kernel/sched/sched/src/runqueue.rs:38`
- **Fix:** Replace with `[Option<Task>; MAX_TASKS]` indexed by PID (mod MAX_TASKS). Pid-to-slot via simple hash. O(1) lookup, zero allocation, cache-friendly.
- **Impact:** Faster `get_task()`, `get_task_mut()`, `set_task_context()` — all hot-path operations.

### 2.6 — Global Heap Lock Contention (SMP)
- **Problem:** Single `spin::Mutex` on heap allocator. All 4 CPUs contend.
- **Fix (phased):**
  1. **Phase 1 — Integrate slab allocator:** Wire up `mm-slab` for common allocation sizes (32, 64, 128, 256, 512, 1024 bytes). The slab code already exists but `slab_alloc()` returns `Err(OutOfMemory)` — implement real allocation.
  2. **Phase 2 — Per-CPU slab caches:** Each CPU has a local slab cache. Only fall back to the global heap for sizes that don't match slab classes.
  3. **Phase 3 — Replace linked-list allocator:** Move to a buddy-based heap or SLUB-style allocator.

### 2.7 — Fix Double Vruntime Charging
- **Problem:** Task charged TICK_NS in `scheduler_tick()`, then again in `pick_next_task()` (via the delta=0 floor) when both fire in the same tick.
- **Files:** `kernel/sched/sched/src/runqueue.rs:349-376`, `kernel/sched/sched/src/core.rs:529-546`
- **Fix:** After `scheduler_tick()` charges vruntime, set a flag on the task (`vruntime_charged_this_tick`). In `pick_next_task()`, skip the delta=0 floor if the flag is set. Clear the flag after `pick_next_task()`.

### 2.8 — `sys_connect` Spin Loop → HLT
- **Problem:** 10M-iteration spin_loop() in `sys_connect`, burns CPU for ~10 seconds.
- **Files:** `kernel/syscall/syscall/src/socket.rs:722-748`
- **Fix:** Replace with HLT+kpo pattern (same as sys_poll/sys_select):
  ```rust
  loop {
      if let Ok(()) = socket.connect(...) { return 0; }
      // Check signals
      if should_interrupt_for_signal(pid) { return -EINTR; }
      arch::allow_kernel_preempt();
      unsafe { core::arch::asm!("sti", "hlt", options(nomem, nostack)); }
      arch::disallow_kernel_preempt();
  }
  ```

### 2.9 — Preemption Flags: APIC ID vs Logical CPU ID
- **Problem:** `KERNEL_PREEMPT_OK` indexed by raw APIC ID, but scheduler uses logical CPU ID. Mismatch if APIC IDs are sparse.
- **Files:** `kernel/arch/arch-x86_64/src/lib.rs:924-927`
- **Fix:** Use `sched::this_cpu()` (logical ID) instead of `apic::id()`. Or maintain a mapping table `APIC_TO_LOGICAL[apic_id] -> cpu_id` and use it consistently.

### 2.10 — VFS Mount Point Prefix Matching
- **Problem:** `starts_with("/home")` matches `/homework`. Wrong filesystem resolved.
- **Files:** `kernel/vfs/vfs/src/mount.rs:237-248`
- **Fix:** After `starts_with`, check that the next character is `/` or the path equals the mount point exactly:
  ```rust
  if path == mount_point || path.starts_with(&format!("{}/", mount_point)) || mount_point == "/"
  ```

### 2.11 — procfs Content Caching (seq_file pattern)
- **Problem:** procfs regenerates content on every `stat()` and every `read()` chunk. `stat()` + `read()` = 2x work.
- **Files:** `kernel/vfs/procfs/src/lib.rs`
- **Fix:** Cache generated content per-open. On first `read()` or `stat()`, generate and cache. Subsequent reads use the cache. Clear cache on close. This is Linux's `seq_file` pattern.

### 2.12 — `sys_sigsuspend` / `sys_pause` Must Actually Block
- **Problem:** Both return immediately with `-EINTR` without blocking.
- **Files:** `kernel/syscall/syscall/src/signal.rs:223-266`
- **Fix:** Block using HLT+kpo loop, checking for pending signals each wake:
  ```rust
  fn sys_sigsuspend(mask_ptr: u64) -> i64 {
      // Set temporary mask
      loop {
          if has_pending_signal(pid) { restore old mask; return -EINTR; }
          arch::allow_kernel_preempt();
          unsafe { core::arch::asm!("sti", "hlt"); }
          arch::disallow_kernel_preempt();
      }
  }
  ```

---

## P3 — LOW (Defense in Depth & Quality)

### 3.1 — Enable SMAP in CR4
- Uncomment SMAP enable in `init.rs:144-151`. Requires P1.3 (STAC/CLAC audit) first.
- Removes an entire class of accidental user memory access bugs.

### 3.2 — Enable Heap Hardening by Default
- Switch from `LockedHeap` to `LockedHardenedHeap` in `globals.rs:16`.
- Adds redzones, canaries, and freed-memory poisoning. Catches buffer overflows and use-after-free.

### 3.3 — ASLR (User Stack + mmap Base)
- Randomize user stack position within the 8MB range.
- Randomize mmap base hint.
- Use RDRAND (or PIT/TSC-based entropy) for randomness.

### 3.4 — Network Driver Interrupt-Driven TX Reclaim
- Add interrupt handler for VirtIO-net TX completion.
- Batch RX processing (NAPI-style, up to 64 packets per poll).

### 3.5 — Terminal Lock Splitting
- Split TERMINAL mutex into: parser lock, screen buffer lock, renderer lock.
- Allows ISR to check mouse mode without waiting for a large write to finish.
- Lower priority because current try_lock pattern prevents deadlocks.

### 3.6 — flock Wakeup Notification
- When `FLOCK_REGISTRY.unlock()` releases a lock, wake blocked waiters directly instead of relying on timer-tick polling (10ms latency floor).

### 3.7 — Pipe Vec Clone → Swap
- Replace `buffer.write_waiters.clone()` + `clear()` with `core::mem::swap()` in `pipe.rs:188-194`.
- Eliminates heap allocation while holding pipe lock.

### 3.8 — Mark CSI `@`, `P`, `X` as Single-Row Dirty
- Currently trigger `mark_all_dirty()`. Only affect cursor row.
- Reduces unnecessary full-screen repaints for ICH/DCH/ECH sequences.

### 3.9 — O(N_CPUS) Task Lookup → Global PID Table
- Replace the "try this_cpu then scan all CPUs" pattern with a global `PID_TO_CPU: [AtomicU32; MAX_PIDS]` mapping.
- `get_task_meta(pid)` becomes: read `PID_TO_CPU[pid]`, then `with_rq(cpu, ...)`.
- O(1) instead of O(N_CPUS).

### 3.10 — Double Fault Handler Serial Output
- Add serial output before the HLT loop so crashes are visible in QEMU.
- Use `arch::serial::write_str_unsafe()` (no locks, safe for fatal paths).

### 3.11 — Unbounded Serial Spins in ISR Paths
- Fix `virtio-input/src/lib.rs:163-174` and `ps2/src/lib.rs:568-574`.
- Add spin limit matching `UART_TX_SPIN_LIMIT = 2048`.

---

## Code Structure Recommendations

### Recommendation 1: User Memory Access Layer
Create `kernel/syscall/syscall/src/uaccess.rs`:
```rust
/// Copy bytes from userspace into a kernel Vec.
pub fn copy_from_user(ptr: u64, len: usize) -> Result<Vec<u8>, Errno> { ... }

/// Copy bytes from kernel to userspace.
pub fn copy_to_user(dst: u64, src: &[u8]) -> Result<(), Errno> { ... }

/// Read a T from userspace.
pub fn get_user<T: Copy>(ptr: u64) -> Result<T, Errno> { ... }

/// Write a T to userspace.
pub fn put_user<T: Copy>(ptr: u64, val: T) -> Result<(), Errno> { ... }

/// Copy a path string from userspace into a kernel-owned String.
pub fn copy_path_from_user(ptr: u64, len: usize) -> Result<String, Errno> { ... }
```
All functions: validate pointer range, STAC before access, CLAC after, handle page faults gracefully. Every syscall uses these — no raw pointer derefs anywhere else.

### Recommendation 2: Per-CPU Data Abstraction
Create `kernel/arch/arch-x86_64/src/percpu.rs`:
```rust
pub struct PerCpu<T> {
    data: [T; MAX_CPUS],
}
impl<T> PerCpu<T> {
    pub fn this_cpu(&self) -> &T { &self.data[sched::this_cpu() as usize] }
    pub fn this_cpu_mut(&mut self) -> &mut T { &mut self.data[sched::this_cpu() as usize] }
    pub fn cpu(&self, id: u32) -> &T { &self.data[id as usize] }
}
```
Use for: GDT, TSS, preemption flags, KPO streak, syscall debug statics, future per-CPU slab caches.

### Recommendation 3: VFS Permission Layer
Create `kernel/vfs/vfs/src/permission.rs`:
```rust
pub fn check_access(vnode: &dyn VnodeOps, uid: u32, gid: u32, mode: AccessMode) -> VfsResult<()> { ... }
pub fn check_open(vnode: &dyn VnodeOps, uid: u32, gid: u32, flags: u32) -> VfsResult<()> { ... }
pub fn is_owner_or_root(vnode: &dyn VnodeOps, uid: u32) -> bool { ... }
pub fn requires_root(uid: u32) -> VfsResult<()> { ... }
```
Called at the top of every syscall that touches files.

### Recommendation 4: Context Switch Transaction
Refactor `scheduler.rs` context switch into a single locked transaction:
```rust
struct SwitchInfo {
    old_cr3: u64,
    new_cr3: u64,
    new_rsp: u64,
    new_rip: u64,
    new_fs_base: u64,
    new_kernel_stack: u64,
}

fn prepare_switch(old_pid: Pid, new_pid: Pid) -> Option<SwitchInfo> {
    sched::with_rq(cpu, |rq| {
        // All state reads/writes under one lock
        rq.save_preempt(old_pid, ...);
        rq.set_context(old_pid, ...);
        let info = rq.get_switch_info(new_pid);
        rq.set_current(new_pid);
        rq.load_preempt(new_pid);
        info
    })
}
```

---

## Build Target Verification

All build targets are correctly configured:

| Layer | Target | Status |
|-------|--------|--------|
| Kernel | `targets/x86_64-unknown-oxide.json` | Correct — `cfg(target_os = "oxide")` |
| Userspace (core) | `x86_64-unknown-none` | Correct — no-std coreutils |
| Userspace (std) | `targets/x86_64-unknown-oxide-user.json` | Correct — `os: "oxide"`, `-Zbuild-std=std` |
| Bootloader | `x86_64-unknown-uefi` | Correct |

No changes needed for build targets.

---

## Execution Order

```
Phase 1 (P0): Frame leak → Per-CPU GDT/TSS → copy_path fix → ARCH_SET_GS fix
Phase 2 (P1): COW TLB → Stack guards → STAC/CLAC audit → GPF handler → COW alloc fix → OOM handling → Debug statics
Phase 3 (P2): Lock consolidation → Load balancing → DAC permissions → Spin limits → BTreeMap→Array → Heap contention → Vruntime fix → connect() fix → APIC ID fix → Mount matching → procfs cache → sigsuspend
Phase 4 (P3): SMAP enable → Heap hardening → ASLR → Net driver → Terminal split → flock wakeup → Pipe swap → CSI dirty → PID table → DF handler → Serial spins
```

Each phase builds on the previous. P0 fixes prevent data corruption and OOM. P1 fixes prevent crashes and SMP races. P2 improves performance and adds security layers. P3 hardens and polishes.
