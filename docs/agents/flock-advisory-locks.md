# Advisory File Locking (flock) — Agent Rule

## Architecture

```
std::fs::File::lock()           ← Rust std PAL (sys/fs/oxide.rs)
  → oxide_rt::fs::flock(fd, op) ← userspace syscall wrapper
    → SYS_FLOCK (143)           ← syscall number
      → sys_flock()             ← kernel/syscall/syscall/src/vfs.rs
        → FlockRegistry         ← kernel/vfs/vfs/src/flock.rs
```

## Semantics (Linux-compatible BSD flock)

| Constant | Value | Meaning |
|----------|-------|---------|
| `LOCK_SH` | 1 | Shared (read) lock — multiple allowed |
| `LOCK_EX` | 2 | Exclusive (write) lock — single owner only |
| `LOCK_NB` | 4 | Non-blocking flag (OR'd with SH/EX) |
| `LOCK_UN` | 8 | Unlock |

## Key Rules

1. **Locks are per open file description (`Arc<File>`), NOT per fd.**
   - `dup()`/`fork()` share the same `Arc<File>`, so they share the lock.
   - Opening the same file twice creates two independent `Arc<File>` instances with different `owner_id`s.

2. **Last close releases the lock automatically.**
   - `File::drop()` calls `FLOCK_REGISTRY.unlock()`.
   - No leaked locks even if userspace forgets to unlock.

3. **Upgrade/downgrade in place.**
   - If the same `owner_id` already holds a lock, calling flock again changes the lock type (shared→exclusive or vice versa).
   - This matches Linux behavior.

4. **Blocking waits use HLT+kpo.**
   - Same pattern as `sys_poll`, `sys_select`, etc.
   - Signals break out with `-EINTR`.

5. **Non-blocking failures return `-EAGAIN` (errno 11).**
   - `VfsError::WouldBlock` maps to errno -11.

## Files

| File | Role |
|------|------|
| `kernel/vfs/vfs/src/flock.rs` | FlockRegistry, InodeId, lock state tracking |
| `kernel/vfs/vfs/src/file.rs` | `owner_id` field, Drop cleanup |
| `kernel/syscall/syscall/src/vfs.rs` | `sys_flock()` handler |
| `kernel/syscall/syscall/src/lib.rs` | `nr::FLOCK = 143`, dispatch arm |
| `userspace/libs/oxide-rt/src/nr.rs` | `FLOCK = 143` |
| `userspace/libs/oxide-rt/src/fs.rs` | `flock()` wrapper |
| `rust-std/library/std/src/sys/fs/oxide.rs` | Real lock/unlock/try_lock calls |
