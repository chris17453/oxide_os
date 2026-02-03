# OXIDE OS Fixup List
**Date:** 2026-02-02
**Purpose:** Track required fixes across the system

---

## Kernel Syscalls Needed

### High Priority

| Syscall | Affected Utils | Description |
|---------|----------------|-------------|
| `setpriority` | nice | Set process scheduling priority |
| `getpriority` | nice | Get process scheduling priority |
| `alarm` | timeout | Schedule SIGALRM delivery |
| `timer_create` | timeout | POSIX interval timers (alternative to alarm) |

### Medium Priority

| Syscall | Affected Utils | Description |
|---------|----------------|-------------|
| `timer_settime` | timeout | Arm/disarm POSIX timer |
| `timer_delete` | timeout | Delete POSIX timer |

---

## Procfs Enhancements Needed

| Path | Affected Utils | Description |
|------|----------------|-------------|
| `/proc/[pid]/cmdline` | pgrep, pkill | Full command line arguments |
| `/proc/[pid]/status` | pgrep, pkill | Process status info |
| `/proc/[pid]/comm` | pgrep, pkill | Command name |

---

## Coreutils Fixes

### Code Changes Required

| Utility | Issue | Priority | Effort |
|---------|-------|----------|--------|
| diff | Add unified format (-u) | Medium | 2-3 hours |
| diff | Add context format (-c) | Low | 2-3 hours |
| more | Add search functionality (/) | Medium | 1-2 hours |
| tail | Multi-file -f support | Low | 2-3 hours |
| wget | DNS hostname resolution | Medium | Depends on libc |

### Blocked by Kernel

| Utility | Blocking Issue | Priority |
|---------|----------------|----------|
| nice | Needs setpriority syscall | High |
| timeout | Needs alarm or timer syscalls | High |
| pgrep | Needs /proc/[pid]/cmdline | Medium |
| pkill | Needs /proc/[pid]/cmdline | Medium |
| nohup | Needs better signal handling | Low |

---

## Libc Fixes

### Incomplete Implementations

| Module | Issue | Priority |
|--------|-------|----------|
| zlib | Only CRC32, no compress/decompress | Medium |
| dns | No gethostbyname/getaddrinfo | Medium |
| locale | Minimal stub | Low |
| wchar | Partial implementation | Low |

### Debug Code Removed

- [x] `[LIBC_DEBUG]` prints in syscall.rs
- [x] `[START_DEBUG]` prints in start.rs
- [x] `[LS_DEBUG]` prints in ls.rs

---

## Kernel Gaps (from analv2.md)

### P0 - Critical

| Issue | Status | Notes |
|-------|--------|-------|
| Thread creation (clone with CLONE_VM) | ❌ | Returns ENOSYS |
| SMP/Multi-core | ❌ | AP boot fails |

### P1 - High

| Issue | Status | Notes |
|-------|--------|-------|
| ext4 timestamps | ⚠️ | Uses 0 |
| ext4 UID/GID | ⚠️ | Uses 0 |
| TCP non-loopback | ⚠️ | Stub |

### P2 - Medium

| Issue | Status | Notes |
|-------|--------|-------|
| Per-process CPU time | ❌ | TODO in code |
| PRIO_PGRP/PRIO_USER | ❌ | Returns ENOSYS |
| USB device support | ⚠️ | Framework only |

---

## Build System

- [x] Copy .bas files to initramfs
- [x] Debug features disabled in Makefile
- [ ] Add pkg-config files for cross-compilation
- [ ] Add CMake toolchain file

---

## Documentation Needed

- [ ] Syscall ABI specification
- [ ] Driver development guide  
- [ ] Memory layout diagram
- [ ] Boot sequence documentation
- [ ] Debugging guide

---

## Quick Wins (Can Fix Today)

1. **Implement setpriority/getpriority syscalls**
   - Location: `kernel/syscall/syscall/src/lib.rs`
   - Fixes: `nice` utility
   - Effort: 1-2 hours

2. **Implement alarm syscall**
   - Location: `kernel/syscall/syscall/src/time.rs`
   - Fixes: `timeout` utility
   - Effort: 2-3 hours

3. **Add /proc/[pid]/cmdline**
   - Location: `kernel/vfs/procfs/`
   - Fixes: `pgrep`, `pkill`
   - Effort: 1-2 hours

4. **Add unified diff format**
   - Location: `userspace/coreutils/src/bin/diff.rs`
   - Effort: 2-3 hours

5. **Add search to more**
   - Location: `userspace/coreutils/src/bin/more.rs`
   - Effort: 1-2 hours

---

## Testing Checklist

After fixes, verify:

- [ ] `nice -n 10 sleep 5` works
- [ ] `timeout 5 sleep 10` kills after 5s
- [ ] `pgrep init` finds init process
- [ ] `pkill -9 someproc` kills by name
- [ ] `diff -u file1 file2` shows unified diff
- [ ] `more file` then `/pattern` searches
- [ ] `gwbasic /usr/share/gwbasic/hello.bas` runs
- [ ] `gwbasic` then `SCREEN 1` enters graphics mode
