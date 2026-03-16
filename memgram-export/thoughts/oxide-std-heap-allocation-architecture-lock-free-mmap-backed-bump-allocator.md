# oxide-std heap allocation architecture: lock-free mmap-backed bump allocator

📌 Pinned

| Field | Value |
|-------|-------|
| ID | `7e87f4d5b213` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-04T13:29:05.450333+00:00 |
| Accessed | 0 times |
| Keywords | heap-allocation, allocator, mmap, lock-free, bump-allocator, syscall, deadlock-risk, oxide-std |
| Files | `/home/nd/repos/Projects/oxide_os/rust-std/library/std/src/sys/alloc/oxide.rs`, `/home/nd/repos/Projects/oxide_os/userspace/libs/oxide-rt/src/alloc.rs`, `/home/nd/repos/Projects/oxide_os/userspace/libs/oxide-rt/src/syscall.rs`, `/home/nd/repos/Projects/oxide_os/kernel/syscall/syscall/src/memory.rs` |

## Content

## Oxide-std Heap Allocation Deep Dive

### GlobalAlloc Chain
1. Userspace Vec::with_capacity() and heap allocations use Rust's std GlobalAlloc
2. Std's System allocator is implemented at `rust-std/library/std/src/sys/alloc/oxide.rs`
3. System::alloc() directly calls `oxide_rt::alloc::mmap()` for every allocation
4. System::dealloc() calls `oxide_rt::alloc::munmap()`

### oxide_rt Allocator Implementation (`userspace/libs/oxide-rt/src/alloc.rs`)
**Two-tier design:**

#### Tier 1: Bootstrap Heap (256KB BSS)
- Static 256KB zero-initialized buffer in BSS (no mmap needed)
- Lock-free CAS loop with AtomicUsize for position tracking
- Used for early allocations before mmap is available
- Once full or if CAS fails, falls back to Tier 2

#### Tier 2: mmap Arenas (2MB each)
- Each new arena is a 2MB anonymous private page-mapped region
- Max 32 arenas allowed (64MB total heap cap)
- Lock-free CAS loop for position within current arena
- When current arena exhausted, `new_arena()` allocates a fresh 2MB arena via mmap

### Syscalls Used
**For mmap arena allocation: syscall #90 (MMAP)**
- Arguments: addr=0 (let kernel pick), len=2MB, prot=0x3 (PROT_READ|PROT_WRITE), flags=0x22 (MAP_PRIVATE|MAP_ANONYMOUS), fd=-1, offset=0
- Kernel handler: `kernel/syscall/syscall/src/memory.rs::sys_mmap()`

**For munmap (dealloc): syscall #91 (MUNMAP)**
- Kernel handler: `kernel/syscall/syscall/src/memory.rs::sys_munmap()`

### Key Characteristics

**No Locking:** Entirely lock-free using atomic CAS loops
- `AtomicUsize::compare_exchange()` for bootstrap heap position
- `AtomicUsize::compare_exchange_weak()` for arena position
- Ordering: Relaxed reads, SeqCst for CAS operations

**No Deallocation:** Bump allocator strategy
- `dealloc()` is a no-op (comment: "We never free. Deal with it.")
- Memory is freed only via `munmap()` when explicitly called by user
- Fragmentation is inevitable for long-running processes

**Potential Deadlock Risks:** NONE in the allocator itself
1. No locks are held during mmap/munmap syscalls
2. CAS loops are bounded (if CAS fails, retries immediately)
3. No blocking syscalls in the allocator path
4. However, sys_mmap() in the kernel DOES acquire ProcessMeta lock (meta.lock()) for address_space operations — ISR contexts calling Vec::new() would deadlock!

### Deadlock Risk Assessment

**Risk Level: MODERATE (context-dependent)**

Safe contexts:
- Normal userspace syscalls (ring 3)
- Userspace threads (no ISR context)

Dangerous contexts:
- ISR handlers in userspace (but oxide OS doesn't support ISR callbacks in userspace, only kernel ISRs)
- Unlikely in normal oxide-std usage

**Real Risk: ProcessMeta lock contention**
When sys_mmap() is called, it acquires `meta.lock()` (line 126):
```rust
{
    let mut m = meta.lock();
    let allocator = mm();
    match m.address_space.allocate_pages(...) { ... }
}
```
This lock can be contended in multithreaded programs. However, lock() is blocking (not try_lock), so it's a fairness issue, not deadlock.
