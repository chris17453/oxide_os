# VIM Syscall Issues Analysis - Phase 2

**Status**: vim still broken after Phase 1 fixes - can't accept keys properly in insert mode

## CRITICAL ISSUES FOUND: 7

### 🔴 CRITICAL #1: ioctl Return Value Truncation **[BLOCKS VIM]**
**Location**: `userspace/libc/src/c_exports.rs:2093`

```rust
pub unsafe extern "C" fn ioctl(fd: i32, request: u64, arg: u64) -> i32 {
    syscall::sys_ioctl(fd, request, arg)  // Returns i64 but cast to i32!
}
```

**Problem**: Kernel returns 64-bit value, libc wrapper truncates to 32 bits.
**Impact**: tcgetattr(), tcsetattr(), and ALL ioctl operations broken.
**Fix**: Change return type to match kernel, or properly handle 64-bit returns.

---

### 🔴 CRITICAL #2: sys_write_vfs Wrong Error Code
**Location**: `crates/syscall/syscall/src/vfs.rs:424`

**Problem**: Returns `ESRCH` (No such process) instead of `EBADF` (Bad file descriptor).
**Impact**: Vim's error handling breaks, expects EBADF for invalid FDs.
**Fix**: Change `errno::ESRCH` to `errno::EBADF` (one line fix).

---

### 🔴 CRITICAL #3: isatty() Broken by ioctl Truncation
**Location**: `userspace/libc/src/c_exports.rs:1094-1098`

**Problem**: isatty() uses ioctl internally, inherits truncation bug.
**Impact**: Vim can't detect if stdout is a TTY, breaks interactive mode.
**Fix**: Fixed automatically when #1 is fixed.

---

### 🔴 CRITICAL #4: fcntl O_NONBLOCK Silent Failure
**Location**: `crates/syscall/syscall/src/vfs.rs:1375`

```rust
let _ = file.ioctl(request, arg);  // Error silently discarded!
```

**Problem**: Errors from TTY ioctl are ignored, returns success even if failed.
**Impact**: Vim hangs when expecting async I/O.
**Fix**: Propagate error or at least log it.

---

### ⚠️ MEDIUM #5: O_NONBLOCK vs VMIN/VTIME Conflict
**Location**: `crates/tty/tty/src/tty.rs:384-425`

**Problem**: O_NONBLOCK and VMIN=0/VTIME=0 both trigger non-blocking, may return 0 (EOF) instead of EAGAIN.
**Impact**: Vim confused by EOF-like behavior.
**Fix**: Prioritize O_NONBLOCK to always return EAGAIN.

---

### ⚠️ MEDIUM #6: poll/select Incomplete TTY Checking
**Location**: `crates/syscall/syscall/src/poll.rs:74-87`

**Problem**: check_fd_ready() doesn't account for O_NONBLOCK state.
**Impact**: poll() may block when shouldn't.
**Fix**: Check O_NONBLOCK in readiness logic.

---

### ⚠️ MEDIUM #7: tcsetattr Ignores action Parameter
**Location**: `userspace/libc/src/c_exports.rs:3295`

**Problem**: `_action` parameter (TCSANOW, TCSADRAIN, TCSAFLUSH) completely ignored.
**Impact**: Vim expects different behaviors, gets immediate set always.
**Fix**: Implement TCSADRAIN (drain output) and TCSAFLUSH (flush input).

---

## FIX STATUS

✅ **FIXED #1 ioctl truncation** - userspace/libc/src/c_exports.rs:2094
   - Added explicit `as i32` cast

✅ **FIXED #2 write errno** - crates/syscall/syscall/src/vfs.rs:424
   - Changed ESRCH to EBADF

✅ **FIXED #4 fcntl silent fail** - crates/syscall/syscall/src/vfs.rs:1375
   - Propagate ioctl errors instead of ignoring

✅ **FIXED #5 O_NONBLOCK logic** - ALREADY CORRECT
   - Code already checks O_NONBLOCK first

✅ **FIXED #6 poll/select** - crates/syscall/syscall/src/poll.rs:74-105
   - Check FIONREAD for TTYs to see if data available

✅ **FIXED #7 tcsetattr action** - userspace/libc/src/c_exports.rs:3296
   - Map action to correct ioctl (TCSETS/TCSETSW/TCSETSF)

## What Works ✅

- TCGETS, TCSETS, TCSETSF, TCSETSW ioctl handlers
- TIOCGWINSZ, TIOCSWINSZ window size
- TIOCGPGRP, TIOCSPGRP process groups
- Signal delivery (SIGINT, SIGQUIT, SIGTSTP, SIGWINCH)
- fcntl F_GETFL, F_SETFL framework
- VMIN/VTIME read timeout support
- Basic poll() and select() frameworks

## Phase 1 Fixes (Already Done)

1. ✅ fcntl() libc wrapper
2. ✅ Signal UserHandler delivery
3. ✅ SIGWINCH on terminal resize
4. ✅ brk() heap allocation
