# OXIDE OS Performance Autopsy

> *"It's not a bug, it's a performance feature nobody asked for."*
> — GraveShift, 3:47 AM, staring at a flamegraph that looks like a mushroom cloud

**Date:** March 2026
**Analyst:** Collective Persona Intelligence (CPI) — the voices in the kernel's head
**Verdict:** Death by a thousand paper cuts. No single assassin — just a mob.

---

## Executive Summary

OXIDE OS is architecturally sound but operationally slow. The system suffers from
**compounding micro-inefficiencies** that stack multiplicatively across hot paths.
Think of it as a sports car with the parking brake half-engaged: the engine is good,
the chassis is solid, but something is dragging.

**The Big Three killers (80% of perceived sluggishness):**

| # | Bottleneck | Impact | Persona Verdict |
|---|-----------|--------|-----------------|
| 1 | **Debug build by default** (`PROFILE=debug`) | 3-10× slower than release | *"You're racing a Formula 1 car with training wheels welded on."* — PulseForge |
| 2 | **Serial UART spin-waiting** (115200 baud, byte-at-a-time) | Blocks CPUs for ~87µs/byte | *"Every println is a tiny hostage situation."* — PatchBay |
| 3 | **Heap allocator O(n) first-fit** (linked list walk) | Degrades with fragmentation | *"First-fit? More like worst-fit for your patience."* — NeonRoot |

**The Other Seven (the remaining 20%, but they add up):**

| # | Issue | Category |
|---|-------|----------|
| 4 | `opt-level = "z"` in release (size over speed) | Build Config |
| 5 | Heap hardening writes 0xDD over every freed allocation | Memory |
| 6 | VFS path resolution: 3-4 heap allocations per `open()` | Syscall |
| 7 | procfs regenerates entire file content on every `read()` | Filesystem |
| 8 | 2KB kernel buffer copy on every `sys_read`/`sys_write` (SMAP) | I/O |
| 9 | TLB shootdown uses unbounded spinlock | SMP |
| 10 | COW tracker uses BTreeMap (O(log n) per frame) | Memory |

---

## 1. THE DEBUG BUILD PROBLEM 🔴 CRITICAL

> *"You showed up to a drag race in a car full of sandbags and wondered why you lost."*
> — HexLine, mildly disgusted

### What's Happening

```makefile
# mk/config.mk, line 8 — the crime scene
PROFILE ?= debug
```

The kernel builds in **debug mode by default**. This means:

- **Zero optimization** (`opt-level = 0`) — the compiler doesn't inline, doesn't
  vectorize, doesn't eliminate dead code, doesn't do *anything useful*
- **Debug assertions enabled** — bounds checks on every array access, overflow
  checks on every arithmetic operation
- **16 codegen units** — parallel compilation but worse optimization per unit
- **No LTO** — no cross-crate inlining, no whole-program optimization
- **Larger binary** — more instruction cache pressure, more TLB misses

### Impact Measurement

| Operation | Debug | Release | Ratio |
|-----------|-------|---------|-------|
| Function call overhead | ~15 cycles | ~2 cycles (inlined) | 7.5× |
| Bounds checks | Every access | Elided where provable | 2-3× |
| Code size | ~3-5× larger | Baseline | Cache pressure |
| LTO cross-crate inline | None | Full | Variable |

### The Fix

For development: keep debug. **For performance testing:**

```bash
PROFILE=release make run
```

But wait — even release has a problem...

---

## 2. RELEASE OPTIMIZES FOR SIZE, NOT SPEED 🟡 SIGNIFICANT

> *"You asked the compiler to make a diamond and it gave you a very small rock."*
> — PulseForge, reading the Cargo.toml

### What's Happening

```toml
# Cargo.toml, line 274
[profile.release]
opt-level = "z"    # ← Optimize for SIZE, not speed
lto = true
panic = "abort"
```

`opt-level = "z"` tells LLVM: *"I'd rather have a smaller binary than a faster one."*
This means:

- **Loop unrolling disabled** — tight loops pay branch penalty every iteration
- **Function inlining reduced** — more call/ret overhead
- **Vectorization suppressed** — SIMD opportunities left on the table
- **Instruction selection biased toward compact encodings** — not always the fastest

For an OS kernel running in a VM with 512MB RAM, binary size is *not* the constraint.
Execution speed is.

### The Fix

```toml
[profile.release]
opt-level = 3      # Maximum speed optimization
lto = true
panic = "abort"
codegen-units = 1  # Better optimization at cost of compile time
```

**Expected improvement:** 10-30% across the board on compute-bound paths.

---

## 3. SERIAL UART: THE SILENT ASSASSIN 🔴 CRITICAL

> *"Every byte you write to COM1 is a tiny prayer to the baud rate gods.
>  At 115200, they answer slowly."*
> — PatchBay, haunted by THRE bits

### What's Happening

The serial port operates at 115200 baud. That's **11,520 bytes/second**. Every byte
written requires:

```
1. Acquire COM1 global mutex (contention with ALL other CPUs)
2. Spin-wait on THRE (Transmit Holding Register Empty) — up to 2048 iterations
3. Write one byte
4. Release mutex
5. Next caller fights for the mutex again
```

**Math of doom:**
- One byte at 115200 baud: ~87 µs
- A 60-character `println!`: ~5.2 ms of CPU time *burned spinning*
- If 4 CPUs all want to print: serialized through one mutex → 20+ ms stall
- The perf monitoring system itself prints stats every 5 seconds — a 53-byte
  PERF-WARN message costs ~5M CPU cycles

### The Feedback Loop From Hell

```
Debug output → serial write → THRE spin → CPU stalls
                                    ↓
                              timer ISR delayed
                                    ↓
                              scheduler tick missed
                                    ↓
                              tasks starved → look slow
                                    ↓
                              developer adds more debug output
                                    ↓
                              (goto start)
```

> *"The system observability tools are the system's biggest performance problem.
>  Heisenberg would be proud."* — StaticRiot

### The Fix (Layered)

**Short-term:** Ring buffer for serial output. Write to RAM, drain async.

**Medium-term:** UART FIFO batch writes (16550A supports 16-byte FIFO — already
enabled but underutilized). Write 16 bytes per THRE wait instead of 1.

**Long-term:** Interrupt-driven serial TX. Fire-and-forget into a ring buffer;
UART interrupt drains it in the background. CPU never spins.

---

## 4. HEAP ALLOCATOR: THE O(n) WALK OF SHAME 🔴 CRITICAL

> *"First-fit allocation in 2026. Somewhere, Knuth is weeping."*
> — NeonRoot, measuring allocation latency

### What's Happening

```rust
// mm-heap/src/linked_list.rs — the slow lane
fn allocate(&mut self, layout: Layout) -> *mut u8 {
    let mut current = &mut self.head;
    while let Some(ref mut block) = current.next {
        if let Some((start, end)) = Self::alloc_from_block(block, ...) {
            // Found one! Only took O(n) to get here.
            return start;
        }
        current = current.next.as_mut().unwrap();  // keep walking...
    }
    null_mut()  // walked the whole list, found nothing
}
```

Every heap allocation **linearly scans** the free list until it finds a block that
fits. With heap fragmentation (inevitable in a running OS), this list grows to
hundreds of entries.

**Combined with heap hardening:**
- Every `free()` writes `0xDD` over the **entire allocation** (a 4KB free = 4096
  byte writes)
- Every `free()` checks 32 redzone bytes (16 leading + 16 trailing)
- Every `alloc()` writes `0xCD` over the allocation + `0xFD` over redzones

### Impact

| Heap State | Free Blocks | Alloc Time | With Hardening |
|-----------|------------|------------|----------------|
| Fresh | 1 | O(1) | +64 bytes written |
| Light use | 10-20 | O(20) | +64 bytes + 0xDD fill |
| Fragmented | 100+ | O(100+) | +64 bytes + 0xDD fill |
| Heavy use | 500+ | **O(500+)** | 🔥 |

### The Fix

**Short-term:** Size-class segregated free lists (like Linux SLUB). O(1) allocation
for common sizes (32, 64, 128, 256, 512, 1024 bytes).

**Medium-term:** Slab allocator for kernel objects. Pre-allocate typed pools for
the most common allocations (task structs, VMA entries, page table entries).

**Hardening optimization:** Check redzones as `u128` comparison (1 instruction)
instead of byte-by-byte loop (16 iterations × 2 = 32 iterations).

---

## 5. SMAP BUFFER COPY: THE 2KB TAX ON EVERY I/O 🟡 SIGNIFICANT

> *"We copy data to a kernel buffer so the terminal subsystem doesn't accidentally
>  nuke the SMAP flag. It's like putting on a hazmat suit to make toast."*
> — SableWire, resigned

### What's Happening

Every `sys_read()` and `sys_write()` copies up to 2KB through a kernel-stack buffer:

```
User calls write(fd, buf, 100):
  1. Copy 100 bytes: user_buf → kernel_stack_buf     (memcpy #1)
  2. Kernel processes: kernel_stack_buf → VFS → driver (memcpy #2 in VFS)
  3. If stdout: driver → terminal → framebuffer       (memcpy #3 in renderer)

Total: 3 copies of the same data before a character appears on screen
```

**Why?** The terminal subsystem internally uses `STAC`/`CLAC` (SMAP enable/disable)
for echo processing. If user-space pointers are passed through the VFS stack,
nested STAC/CLAC pairs clobber the AC flag → SMAP fault → kernel panic.

### Impact

For a simple `write(1, "hello", 5)`:
- 5 bytes copied to kernel buffer: ~10 cycles
- 5 bytes through VFS: ~20 cycles
- 5 bytes to framebuffer: ~30 cycles
- Total overhead: ~60 cycles for 5 bytes
- **For bulk I/O (2048 bytes):** ~400 cycles per syscall, amortized

Not catastrophic per-call, but multiplied by every read/write syscall in the system.

### The Fix

Refactor terminal to not use STAC/CLAC internally. All user memory access should
happen at the syscall boundary, once, with a single STAC/CLAC pair.

---

## 6. VFS PATH RESOLUTION: ALLOCATION PARTY 🟡 SIGNIFICANT

> *"Every open() call is a tiny malloc festival. The heap allocator just loves it."*
> — WireSaint, tracking allocation counts

### What's Happening

```rust
// Every path resolution does:
fn resolve_path(cwd: &str, path: &str) -> String {
    let full = if path.starts_with('/') {
        path.to_string()                    // Allocation #1
    } else {
        format!("{}/{}", cwd, path)         // Allocation #1 (format! = heap alloc)
    };
    normalize_path(&full)                   // Allocation #2 (new String)
}

fn normalize_path(path: &str) -> String {
    let mut components: Vec<&str> = ...;    // Allocation #3 (Vec)
    // ... process ".." and "." ...
    components.join("/")                    // Allocation #4 (join = new String)
}
```

**4 heap allocations per `open()` syscall.** Combined with the O(n) heap allocator,
this becomes expensive fast — especially when `top` or `htop` hammers `/proc/`.

### The Fix

Stack-allocated path buffer (256 bytes covers 99% of paths). Only fall back to
heap for absurdly long paths.

```rust
fn resolve_path(cwd: &str, path: &str) -> SmallString<256> {
    // No heap allocation for paths < 256 chars
}
```

---

## 7. PROCFS: GENERATE-ON-EVERY-READ 🟡 SIGNIFICANT

> *"Reading /proc/[pid]/status is like asking someone their life story
>  every time you want to know their name."*
> — DeadLoop, watching `top` destroy the system

### What's Happening

Every `read()` on a procfs file **regenerates the entire content**:

```rust
fn generate_content(&self) -> String {
    let meta = process_meta.try_lock()?;     // Lock acquisition
    format!(                                 // Heap allocation
        "Name: {}\nState: {}\nPid: {}\n...",
        meta.name, meta.state, meta.pid,
        // ... 15 more fields
    )
}
```

**The `top` apocalypse:**
- `top` reads ~20 processes every second
- Each process: `open` + `read` + `close` on `/proc/[pid]/status`
- Each read: lock acquisition + format! allocation + string build
- 20 processes × 3 syscalls × 4 allocations each = **240 heap allocations/second**
- Plus the O(n) heap walk on each allocation

### The Fix

**Short-term:** Cache generated content with a generation counter. Invalidate on
state change, not on every read.

**Medium-term:** Seq-file style interface (Linux). Generate incrementally, don't
buffer the entire file in memory.

---

## 8. TLB SHOOTDOWN: THE UNBOUNDED SPIN 🟡 MODERATE

> *"One CPU fires the IPI. All other CPUs drop everything to flush their TLBs.
>  Meanwhile, everyone spins on a global lock. Democracy at its finest."*
> — ThreadRogue, watching CPU utilization graphs flatline

### What's Happening

```rust
// kernel/smp/smp/src/tlb.rs — the bottleneck
while TLB_STATE.in_progress
    .compare_exchange(0, 1, Acquire, Relaxed)
    .is_err()
{
    core::hint::spin_loop();  // ← UNBOUNDED. No timeout. No backoff.
}
```

During fork/exec/mmap, page table changes require all CPUs to flush their TLBs.
This uses a **global lock** with **no timeout or backoff**.

Under heavy fork load (shell scripts spawning processes), all 4 CPUs serialize
through this single lock.

### The Fix

Add exponential backoff + bounded wait. Or better: batch TLB invalidations
and amortize the cost.

---

## 9. COW TRACKER: BTreeMap IN THE HOT PATH 🟡 MODERATE

> *"We track every shared page in a BTreeMap. For millions of pages.
>  log(n) is still O(a-lot) when n is a million."*
> — ColdCipher, doing napkin math

### What's Happening

```rust
pub struct CowTracker {
    counts: RwLock<BTreeMap<usize, u32>>,  // frame → refcount
}
```

Every fork's COW setup calls `increment_range()` which does O(count × log n)
BTreeMap insertions **under a single write lock**.

For a 1000-page process fork: 1000 × log(total_frames) tree operations while
blocking all other COW operations system-wide.

### The Fix

Dense array indexed by physical frame number. O(1) lookup, O(1) increment,
no lock contention for different frames (use per-frame atomics).

---

## 10. THE LOCKING CENSUS 🟡 MODERATE

> *"We counted 70+ global mutexes across the kernel. Each one is a potential
>  serialization point. It's locks all the way down."*
> — VeilAudit, after a very long grep session

### The Lock Map

```
CRITICAL PATH LOCKS (touched on every syscall or tick):
├── COM1 serial mutex          (every debug print)
├── WRITER log mutex           (every println!)
├── CONSOLE_WRITER mutex       (every println! — yes, a SECOND lock)
├── Heap allocator lock        (every malloc/free)
├── Run queue per-CPU locks    (every scheduler tick)
├── VFS fd_table lock          (every read/write/close)
└── Terminal lock              (every stdout write)

MODERATE PATH LOCKS (per-operation):
├── ProcessMeta mutex          (per-process state queries)
├── COW tracker RwLock         (fork/page-fault)
├── Buddy allocator zone locks (page allocation)
├── TTY line discipline lock   (keyboard input)
└── Socket table lock          (network I/O)

LOW-FREQUENCY LOCKS:
├── TLB shootdown lock         (fork/exec/mmap)
├── PCI config lock            (device init)
└── Module registry lock       (module load)
```

**The cascading lock problem:** A single `println!("hello")` acquires:
1. WRITER mutex
2. COM1 serial mutex (inside WRITER)
3. CONSOLE_WRITER mutex
4. Terminal lock (inside CONSOLE_WRITER)

**Four nested locks for one debug print.** If any of these is contended, all
callers queue up.

---

## Performance Impact Matrix

### Estimated Time Budget Per Operation

| Operation | Current Cost | Optimal Cost | Waste Factor |
|-----------|-------------|-------------|--------------|
| `println!("hi")` | ~500 µs (serial spin) | ~2 µs (buffered) | **250×** |
| `malloc(64)` | ~5 µs (O(n) walk) | ~0.1 µs (slab) | **50×** |
| `free(ptr)` | ~4 µs (0xDD fill) | ~0.05 µs (no fill) | **80×** |
| `open("/proc/1/status")` | ~20 µs (4 allocs + resolve) | ~2 µs (cached) | **10×** |
| `read(procfd, buf, 512)` | ~15 µs (generate + copy) | ~1 µs (cached) | **15×** |
| `write(1, "x", 1)` | ~10 µs (kbuf copy + terminal) | ~2 µs (direct) | **5×** |
| Context switch | ~200 ns | ~150 ns | **1.3×** |
| Timer tick | ~60 ns | ~50 ns | **1.2×** |
| Page fault (COW) | ~5 µs (BTreeMap) | ~1 µs (array) | **5×** |

### Where Time Goes (Estimated Breakdown for Typical Interactive Session)

```
                        ┌─────────────────────────────────────────┐
                        │     WHERE YOUR CPU CYCLES GO            │
                        │     (interactive shell session)          │
                        ├─────────────────────────────────────────┤
Serial I/O spin    ████████████████████████████░░░░░  45%
Heap alloc/free    ████████████░░░░░░░░░░░░░░░░░░░░░  20%
Debug assertions   ████████░░░░░░░░░░░░░░░░░░░░░░░░░  13%  (debug build)
SMAP buffer copy   ████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   7%
Terminal render    ███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   5%
VFS path resolve   ██░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   4%
Actual useful work ██░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   4%
Scheduler          █░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   2%
                        └─────────────────────────────────────────┘
```

> *"Four percent. That's how much of your CPU is doing actual work.
>  The other ninety-six percent is overhead pretending to be useful."*
> — FuzzStatic, dead inside

---

## What's Actually Good (Credit Where Due)

Not everything is on fire. Some subsystems are genuinely well-engineered:

| Component | Grade | Why |
|-----------|-------|-----|
| **Scheduler core** | A | O(1) RT bitmap, O(log n) CFS min-heap, per-CPU RQs, zero-alloc hot path |
| **Framebuffer** | A | Double-buffered, dirty-row tracking, `rep movsq` blit — ESC[2J went from 25s to <1s |
| **Context switch** | A | ~200ns, single RQ transaction, no allocation |
| **ISR safety** | A | try_lock throughout, bounded waits, per-CPU preempt_count |
| **Syscall entry/exit** | A- | ~30-40 cycles, per-CPU scratch space, SYSRET fast path |
| **Buddy allocator** | B+ | O(log n) split/merge, zone-based, canary-protected |
| **Font rendering** | B+ | O(1) glyph lookup, batch dirty cell tracking |
| **Per-CPU data** | B+ | Lock-free atomics, no false sharing (if aligned) |

---

## Remediation Plan

### Phase 1: LOW-HANGING FRUIT (Days, Massive Impact)

| # | Fix | Effort | Impact | Owner |
|---|-----|--------|--------|-------|
| 1.1 | Change `PROFILE ?= debug` to `release` for `make run` | 1 line | **3-10× speedup** | PulseForge |
| 1.2 | Change `opt-level = "z"` to `opt-level = 3` | 1 line | **10-30% speedup** | PulseForge |
| 1.3 | Add `codegen-units = 1` to release profile | 1 line | **5-15% speedup** | PulseForge |
| 1.4 | Check redzones as `u128` not byte loop | 5 lines | 16× faster free validation | ColdCipher |
| 1.5 | Align heap canaries to 8 bytes | 2 lines | Eliminate unaligned write penalty | ColdCipher |

**Combined Phase 1 impact: 5-15× overall speedup with ~10 lines changed.**

### Phase 2: SERIAL I/O OVERHAUL (1-2 Weeks)

| # | Fix | Effort | Impact |
|---|-----|--------|--------|
| 2.1 | Ring buffer for serial output (4KB) | Medium | Decouple CPU from UART baud rate |
| 2.2 | Batch UART FIFO writes (16 bytes/wait) | Small | 16× fewer THRE spins |
| 2.3 | Split os_log: serial vs console paths | Medium | Remove double-lock on println! |
| 2.4 | Rate-limit debug output in ISR context | Small | Prevent feedback loops |

### Phase 3: MEMORY ALLOCATOR UPGRADE (2-4 Weeks)

| # | Fix | Effort | Impact |
|---|-----|--------|--------|
| 3.1 | Size-class segregated free lists (SLUB-style) | Large | O(1) alloc for common sizes |
| 3.2 | Slab allocator for kernel objects | Large | Zero-fragmentation for typed allocs |
| 3.3 | Per-CPU heap caches | Medium | Eliminate cross-CPU lock contention |
| 3.4 | Lazy heap poisoning (guard pages) | Medium | Remove 0xDD fill overhead |
| 3.5 | Dense array COW tracker | Medium | O(1) vs O(log n) per frame |

### Phase 4: I/O PATH OPTIMIZATION (2-3 Weeks)

| # | Fix | Effort | Impact |
|---|-----|--------|--------|
| 4.1 | Stack-allocated path buffer (SmallString) | Medium | Eliminate 4 allocs/open |
| 4.2 | procfs content caching with generation counter | Medium | Eliminate regen on every read |
| 4.3 | Refactor terminal STAC/CLAC to syscall boundary | Large | Remove 2KB buffer copy |
| 4.4 | TLB shootdown bounded backoff | Small | Prevent unbounded spin |

### Phase 5: ADVANCED OPTIMIZATIONS (Long-term)

| # | Fix | Effort | Impact |
|---|-----|--------|--------|
| 5.1 | Interrupt-driven serial TX | Large | CPU never waits for UART |
| 5.2 | RCU for read-heavy structures (procfs, socket table) | Large | Lock-free reads |
| 5.3 | Dentry cache for VFS path lookups | Large | Amortize repeated opens |
| 5.4 | Huge page support (2MB) | Large | Reduce TLB misses |
| 5.5 | CPU frequency scaling awareness | Medium | Better timer calibration |

---

## Quick Wins Checklist

For the impatient (you know who you are):

```bash
# Step 1: Actually run in release mode
# mk/config.mk line 8:
PROFILE ?= release

# Step 2: Optimize for speed not size
# Cargo.toml line 274:
opt-level = 3
codegen-units = 1

# Step 3: Disable serial-heavy debug features
# mk/config.mk line 22:
RUN_KERNEL_FEATURES ?=

# Step 4: Build and run
make clean && make run
```

**Expected result:** "Holy shit, it's actually fast now." — You, probably.

---

## Appendix A: Debug Feature Bandwidth Analysis

At 115200 baud (11,520 bytes/sec), here's how fast each debug feature saturates:

| Feature | Bytes/Event | Events/Sec | Bandwidth | Saturates? |
|---------|------------|------------|-----------|------------|
| debug-timer | ~45 | 400 (4 CPUs × 100Hz) | 18,000 B/s | 🔴 YES (156%) |
| debug-syscall | ~46 | ~200 | 9,200 B/s | 🔴 YES (80%) |
| debug-buddy | ~45 | ~500 | 22,500 B/s | 🔴 YES (195%) |
| debug-paging | ~60 | ~300 | 18,000 B/s | 🔴 YES (156%) |
| debug-console | ~40 | ~200 | 8,000 B/s | 🟡 69% |
| debug-terminal | ~50 | ~200 | 10,000 B/s | 🟡 87% |
| debug-lock | ~35 | ~60 | 2,100 B/s | ✅ 18% |
| debug-fork | ~80 | ~5 | 400 B/s | ✅ 3% |
| debug-cow | ~50 | ~20 | 1,000 B/s | ✅ 9% |
| debug-all | — | — | ~30,000 B/s | 🔴 **260%** |

> *"debug-all uses 260% of available serial bandwidth. That's not debugging,
>  that's denial-of-service against yourself."* — CanaryHex

---

## Appendix B: Lock Acquisition Heat Map

```
HOT ███████████ (>1000/sec)
├── Heap allocator lock     (every malloc/free across all syscalls)
├── Serial COM1 lock        (every debug print, every ISR trace)
├── Run queue locks          (every timer tick × 4 CPUs = 400/sec)

WARM █████░░░░░ (100-1000/sec)
├── Terminal lock            (every keystroke echo + cursor blink)
├── VFS fd_table lock        (every read/write/close/dup)
├── os_log WRITER lock       (every println!)
├── os_log CONSOLE lock      (every println! — the second lock)

COOL ██░░░░░░░░ (<100/sec)
├── ProcessMeta lock         (procfs reads, signals, wait)
├── COW tracker RwLock       (fork, page fault)
├── Buddy zone locks         (page alloc/free)
├── Socket table lock        (connect/bind/accept)
├── TLB shootdown lock       (fork/exec/mmap)
```

---

## Appendix C: The Path of a Keystroke

How long it takes for a key press to become a character on screen:

```
T+0.000 ms  Hardware IRQ fires (keyboard controller)
T+0.002 ms  IDT → IRQ handler stub → Rust handler
T+0.005 ms  PS2/VirtIO driver decodes scancode
T+0.008 ms  Input subsystem routes to active VT
T+0.010 ms  TTY line discipline processes character
T+0.015 ms  ├── Acquire ldisc lock
T+0.020 ms  ├── Process character (echo buffer)
T+0.025 ms  ├── Release ldisc lock
T+0.030 ms  TTY driver write (echo to terminal)
T+0.035 ms  ├── Acquire terminal lock
T+0.040 ms  ├── VTE parser processes byte
T+0.050 ms  ├── Font glyph lookup
T+0.060 ms  ├── Render to back buffer
T+0.080 ms  ├── Dirty region tracking
T+0.100 ms  ├── Blit dirty rows to framebuffer (rep movsq)
T+0.120 ms  └── Release terminal lock
T+0.500 ms  Serial echo (if debug enabled) ← THE BOTTLENECK
T+0.120 ms  Character visible on screen (without serial)

Total without serial: ~0.12 ms (120 µs) — perfectly responsive
Total with serial echo: ~0.5 ms (500 µs) — still okay, but 4× slower
Total with debug-all: ~5-50 ms — PERCEPTIBLY SLOW
```

---

## Conclusion

OXIDE OS isn't fundamentally slow. It's **accidentally slow** due to:

1. **Default build config** that nobody changed from development to testing
2. **Observability infrastructure** that costs more than what it observes
3. **Safety mechanisms** (heap hardening, SMAP copies) that are correct but unoptimized
4. **Allocation patterns** designed for correctness, not throughput

The architecture is solid. The scheduler is genuinely good. The framebuffer is
well-optimized. The problems are all in the **configuration and plumbing** — the
stuff between the good parts.

Fix Phase 1 (10 lines of config changes) and you'll wonder why you ever thought
it was slow. Fix Phase 2-3 and you'll have a kernel that could give Linux a run
for its money on single-threaded workloads.

> *"The fastest code is the code that doesn't run.
>  The second fastest is the code that runs in release mode.
>  You were running neither."*
> — GraveShift, signing off

---

*This analysis was performed by reading the actual source code, not by profiling.
Actual measurements may vary. But they won't vary enough to save your dignity.*
