# OXIDE OS Vim Compatibility Analysis v2

**Date:** 2026-02-01  
**Status:** Several issues identified - vim partially functional

---

## Executive Summary

Vim requires a **full Unix-like environment** including proper terminal handling, signal delivery, memory management, and file operations. OXIDE OS has most syscalls implemented but several critical features are **missing or incomplete**.

| Priority | Category | Issues | Impact on Vim |
|----------|----------|--------|---------------|
| 🔴 CRITICAL | 4 | Signal handlers, libc fcntl, SIGWINCH, brk | Crashes, no resize |
| 🟠 HIGH | 4 | TIOCSCTTY, Job control, select EINTR | Job control broken |
| 🟡 MEDIUM | 4 | flock, mmap file, tmpfile | Minor features |
| 🟢 LOW | 3 | setrlimit, utime, misc | Non-essential |

**Good news:** Kernel has `sys_fcntl()` implemented! But libc doesn't use it.

---

## CRITICAL ISSUES 🔴 (Must Fix for Vim)

### 1. Signal Handler Delivery Not Working 🔴

**Problem:** Signals are queued but user-space handlers are never invoked.

**Location:** `kernel/src/scheduler.rs` lines 316-361

**Current state:**
```rust
// Signal delivery ONLY handles Terminate/CoreDump:
match result {
    SignalResult::Terminate | SignalResult::CoreDump => {
        // Exit process - WORKS
    }
    _ => {
        // Ignore, Stop, Continue, UserHandler - not yet implemented
    }
}
```

**What vim needs:**
- SIGWINCH (28) - terminal resize → redraw screen
- SIGINT (2) - Ctrl+C → interrupt current operation  
- SIGTSTP (20) - Ctrl+Z → suspend vim
- SIGCONT (18) - fg → resume vim
- SIGALRM (14) - timer for autosave

**What's missing:**
1. `SignalResult::UserHandler` case not implemented
2. Signal frame not pushed onto user stack
3. `sigreturn` syscall doesn't restore context properly
4. Signal restorer trampoline not set up

**Fix required:**
```rust
// In kernel/src/scheduler.rs after line 355:
SignalResult::UserHandler { handler, signo, info, flags, handler_mask } => {
    // Setup signal frame on user stack
    let (new_rip, new_rsp, frame) = setup_signal_handler(
        handler, signo, info, flags, restorer,
        signal_mask, frame.rip, frame.rsp, frame.rflags, &regs
    );
    
    // Write frame to user stack
    unsafe { core::ptr::write(new_rsp as *mut SignalFrame, frame); }
    
    // Modify return context to invoke handler
    frame.rip = new_rip;
    frame.rsp = new_rsp;
    frame.rdi = signo as u64;  // First arg: signal number
    
    // Apply handler mask
    meta.signal_mask = handler_mask;
}
```

---

### 2. libc fcntl() Doesn't Use Kernel Syscall 🔴

**Problem:** Kernel has `sys_fcntl()` implemented but libc doesn't call it!

**Kernel implementation exists:** `crates/syscall/syscall/src/vfs.rs` line 1327
```rust
pub fn sys_fcntl(fd: i32, cmd: i32, arg: u64) -> i64 {
    const F_GETFL: i32 = 3;
    const F_SETFL: i32 = 4;
    // ... FULL IMPLEMENTATION with O_NONBLOCK support!
}
```

**But libc is a stub:** `userspace/libc/src/c_exports.rs` line 2085
```rust
pub unsafe extern "C" fn fcntl(fd: i32, cmd: i32, arg: i64) -> i32 {
    match cmd {
        3 => 0,       // F_GETFL -> STUB returns 0!
        4 => 0,       // F_SETFL -> STUB does nothing!
        ...
    }
}
```

**Impact:** Vim can't set non-blocking I/O on terminal.

**Fix required:** (1 line change!)
```rust
pub unsafe extern "C" fn fcntl(fd: i32, cmd: i32, arg: i64) -> i32 {
    syscall::syscall3(syscall::nr::FCNTL, fd as usize, cmd as usize, arg as usize) as i32
}
```

---

### 3. SIGWINCH Not Sent on Terminal Resize 🔴

**Problem:** TIOCSWINSZ ioctl sets window size but doesn't send SIGWINCH.

**Location:** `crates/tty/tty/src/tty.rs` line 285-287

**Current state:**
```rust
TIOCSWINSZ => {
    let winsize = unsafe { *ptr };
    self.set_winsize(winsize);
    // Should send SIGWINCH to foreground process group  <-- COMMENT ONLY!
    Ok(0)
}
```

**What vim needs:** SIGWINCH signal when terminal is resized to redraw.

**Fix required:**
```rust
TIOCSWINSZ => {
    let winsize = unsafe { *ptr };
    let old_size = self.get_winsize();
    self.set_winsize(winsize);
    
    // Send SIGWINCH if size changed
    if winsize.ws_row != old_size.ws_row || winsize.ws_col != old_size.ws_col {
        let fg_pgid = self.get_foreground_pgid();
        if fg_pgid > 0 {
            if let Some(callback) = SIGNAL_PGRP_CALLBACK.load(Ordering::Relaxed) {
                callback(fg_pgid, signal::SIGWINCH);
            }
        }
    }
    Ok(0)
}
```

---

### 4. brk() Returns Error 🔴

**Problem:** brk() always returns ENOMEM, breaking malloc implementations.

**Location:** `crates/syscall/syscall/src/memory.rs` line 405-416

**Current state:**
```rust
pub fn sys_brk(addr: u64) -> i64 {
    if addr == 0 {
        return 0;  // Query returns 0 (invalid!)
    }
    errno::ENOMEM  // Always fails
}
```

**What vim needs:** Working memory allocation for buffers, undo history, etc.

**Impact:** Vim can't allocate memory → immediate crash or OOM.

**Note:** musl libc and some allocators can fall back to mmap(), so this may not crash vim immediately, but will cause issues with some allocators.

**Fix required:**
```rust
pub fn sys_brk(addr: u64) -> i64 {
    with_current_meta_mut(|meta| {
        if addr == 0 {
            return meta.brk as i64;
        }
        
        // Extend heap if needed
        let new_brk = addr.max(meta.brk);
        // ... allocate pages from meta.brk to new_brk ...
        
        meta.brk = new_brk;
        new_brk as i64
    }).unwrap_or(errno::ENOMEM)
}
```

---

## HIGH PRIORITY ISSUES 🟠

### 5. TIOCSCTTY / TIOCNOTTY Not Implemented 🟠

**Problem:** Can't set or release controlling terminal.

**Location:** `crates/tty/tty/src/tty.rs` - missing from ioctl handler (line 329 returns NotSupported)

**Constants defined:** `crates/tty/tty/src/termios.rs` lines 287, 307
```rust
pub const TIOCSCTTY: u64 = 0x540E; // Set controlling terminal
pub const TIOCNOTTY: u64 = 0x5422; // Release controlling terminal
```

**What vim needs:** Proper job control (Ctrl+Z, fg, bg).

**Fix:** Add to TTY ioctl handler:
```rust
TIOCSCTTY => {
    // arg: if non-zero, steal from other session (requires CAP_SYS_ADMIN)
    let pid = current_pid();
    self.set_session(pid);
    with_current_meta_mut(|m| m.ctty = Some(self.device_id()));
    Ok(0)
}
TIOCNOTTY => {
    with_current_meta_mut(|m| m.ctty = None);
    Ok(0)
}
```

---

### 6. Job Control (Stop/Continue) Not Working 🟠

**Problem:** SIGTSTP/SIGSTOP don't actually stop processes.

**Location:** `kernel/src/scheduler.rs` line 353-355

**Current state:**
```rust
SignalResult::Stop => {
    // Not implemented - process keeps running
}
SignalResult::Continue => {
    // Not implemented
}
```

**What vim needs:** Ctrl+Z suspends, `fg` resumes.

**Fix:** Implement task state transitions:
```rust
SignalResult::Stop => {
    drop(meta);
    sched::set_task_state(current_pid, TaskState::Stopped);
    // Send SIGCHLD to parent
    if let Some(ppid) = sched::get_task_ppid(current_pid) {
        send_signal_to_pid(ppid, SIGCHLD, current_pid, 0);
    }
    sched::set_need_resched();
}
SignalResult::Continue => {
    sched::set_task_state(current_pid, TaskState::Running);
}
```

---

### 7. select()/poll() Don't Return EINTR 🟠

**Problem:** select/poll don't return EINTR when signals arrive.

**Location:** `crates/syscall/syscall/src/poll.rs`

**Current state:** Loops until timeout or data ready, ignoring signals.

**What vim needs:** Wake from select when SIGWINCH/SIGINT arrives.

**Fix:** Check for pending signals in the poll loop:
```rust
// In poll loop:
if meta.has_pending_signals() {
    // Return results so far, or EINTR if no results
    return if ready > 0 { ready } else { errno::EINTR };
}
```

---

### 8. mmap() File Mapping May Not Work 🟠

**Problem:** MAP_PRIVATE file mappings might not read file content correctly.

**Location:** `crates/syscall/syscall/src/memory.rs` line 51

**Current state:** File read happens but errors may be silently ignored.

**What vim needs:** Memory-mapped files for fast buffer loading.

**Impact:** Large files load slowly or incorrectly.

---

## MEDIUM PRIORITY ISSUES 🟡

### 9. flock() Not Working 🟡

**Problem:** flock() is a stub.

**Location:** `userspace/libc/src/c_exports.rs` line 2097

```rust
pub unsafe extern "C" fn flock(_fd: i32, _operation: i32) -> i32 {
    0  // Always succeeds but does nothing
}
```

**What vim needs:** Lock swap files to prevent multiple editors.

---

### 10. access() Syscall Missing 🟡

**Problem:** No direct access() syscall, only faccessat().

**Impact:** Vim checks file permissions before opening. faccessat() exists and should work for most cases.

---

### 11. getpwnam() / getpwuid() 🟡

**Problem:** User database not fully implemented.

**What vim needs:** Expand `~` in paths, show username.

---

### 12. Environment Variables 🟡

**Problem:** Some expected vars may be missing.

**What vim needs:**
- `HOME` - for ~/.vimrc
- `TERM` - for termcap/terminfo (should be set)
- `SHELL` - for :! commands
- `PATH` - for external commands
- `LANG`/`LC_*` - for UTF-8

---

## LOW PRIORITY ISSUES 🟢

### 13. getrlimit() / setrlimit() 🟢

**Problem:** Resource limits not implemented.

**What vim needs:** Check max open files, stack size.

---

### 14. utime() / utimes() 🟢

**Problem:** Setting file timestamps may not be complete.

**What vim needs:** Preserve modification time on :w.

**Status:** utimes/futimes syscalls exist.

---

### 15. readlink() Edge Cases 🟢

**Problem:** May not handle all symlink cases.

**What vim needs:** Resolve symlinks in paths.

---

## SYSCALL IMPLEMENTATION STATUS

### Fully Implemented ✅

| Syscall | Status | Notes |
|---------|--------|-------|
| read/write | ✅ | Working |
| open/close | ✅ | Working |
| lseek | ✅ | Working |
| stat/fstat/lstat | ✅ | Working |
| dup/dup2/dup3 | ✅ | Working |
| pipe/pipe2 | ✅ | Working |
| fork | ✅ | Working |
| execve | ✅ | Working |
| exit/exit_group | ✅ | Working |
| wait/waitpid/wait4 | ✅ | Working |
| getpid/getppid | ✅ | Working |
| setpgid/getpgid | ✅ | Working |
| setsid/getsid | ✅ | Working |
| kill | ✅ | Sends signals |
| sigaction | ✅ | Sets handlers |
| sigprocmask | ✅ | Sets mask |
| mmap (anon) | ✅ | Working |
| munmap | ✅ | Working |
| mprotect | ✅ | Working |
| poll/ppoll | ✅ | Working |
| select/pselect6 | ✅ | Working |
| ioctl | ✅ | Most codes |
| getcwd/chdir | ✅ | Working |
| mkdir/rmdir | ✅ | Working |
| unlink/rename | ✅ | Working |
| chmod/chown | ✅ | Working |
| clock_gettime | ✅ | Working |
| nanosleep | ✅ | Working |
| uname | ✅ | Working |
| **fcntl (kernel)** | ✅ | **Implemented!** |
| faccessat | ✅ | Working |
| utimes/futimes | ✅ | Working |
| fsync/fdatasync | ✅ | Working |

### Partially Implemented ⚠️

| Syscall | Status | Issue |
|---------|--------|-------|
| Signal delivery | ⚠️ | UserHandler not invoked |
| brk | ⚠️ | Always fails |
| mmap (file) | ⚠️ | May not read correctly |
| fcntl (libc) | ⚠️ | Stub doesn't use kernel |
| flock | ⚠️ | Stub only |

### Not Implemented ❌

| Syscall | Priority | Notes |
|---------|----------|-------|
| TIOCSCTTY | 🟠 | Controlling terminal |
| TIOCNOTTY | 🟠 | Release terminal |
| getrlimit | 🟢 | Resource limits |
| setrlimit | 🟢 | Resource limits |

---

## TERMINAL REQUIREMENTS FOR VIM

### Termios Features

| Feature | Status | Notes |
|---------|--------|-------|
| ICANON toggle | ✅ | Raw/cooked mode |
| ECHO toggle | ✅ | Echo on/off |
| ISIG toggle | ✅ | Signal generation |
| VMIN/VTIME | ⚠️ | Basic support |
| c_cc characters | ✅ | All defined |
| TCGETS/TCSETS | ✅ | Working |

### Terminal Escape Sequences (from term_analysis_v3.md)

| Feature | Status |
|---------|--------|
| Cursor movement | ✅ |
| Screen clear | ✅ |
| Colors (256/RGB) | ✅ |
| Alt screen | ✅ |
| Mouse (optional) | ✅ |
| Bracketed paste | ✅ |
| Cursor shapes | ✅ |

### Window Size

| Feature | Status | Notes |
|---------|--------|-------|
| TIOCGWINSZ | ✅ | Get size |
| TIOCSWINSZ | ⚠️ | Set size, no SIGWINCH |

---

## RECOMMENDED FIX ORDER

### Phase 1: Make Vim Work 🔴 (1-2 days)

1. **Fix libc fcntl()** - Just wire it to kernel syscall (1 line!)
2. **Fix signal delivery** - Implement UserHandler case in scheduler
3. **Send SIGWINCH** - Add to TIOCSWINSZ ioctl
4. **Fix brk() or verify mmap works** - Memory allocation

### Phase 2: Job Control 🟠 (1 day)

5. **Implement TIOCSCTTY/TIOCNOTTY** - Controlling terminal
6. **Implement Stop/Continue signals** - Job control
7. **Fix select() EINTR** - Signal interruption

### Phase 3: Polish 🟡 (1 day)

8. **Implement flock()** - File locking
9. **Verify mmap file** - Memory-mapped files
10. **Environment variables** - HOME, TERM, etc.

---

## QUICK WINS (Easy Fixes)

### 1. libc fcntl() - 1 line fix!

```rust
// userspace/libc/src/c_exports.rs line 2085
pub unsafe extern "C" fn fcntl(fd: i32, cmd: i32, arg: i64) -> i32 {
    syscall::syscall3(syscall::nr::FCNTL, fd as usize, cmd as usize, arg as usize) as i32
}
```

### 2. SIGWINCH on resize - ~10 lines

```rust
// crates/tty/tty/src/tty.rs in TIOCSWINSZ handler
let old_size = self.get_winsize();
self.set_winsize(winsize);
if winsize.ws_row != old_size.ws_row || winsize.ws_col != old_size.ws_col {
    let fg_pgid = self.get_foreground_pgid();
    if fg_pgid > 0 {
        if let Some(cb) = SIGNAL_PGRP_CALLBACK.load(Ordering::Relaxed) {
            cb(fg_pgid, 28); // SIGWINCH
        }
    }
}
```

---

## FILES TO MODIFY

| File | Changes Needed | Effort |
|------|----------------|--------|
| `userspace/libc/src/c_exports.rs` | fcntl() use syscall | 1 line |
| `crates/tty/tty/src/tty.rs` | SIGWINCH, TIOCSCTTY | ~30 lines |
| `kernel/src/scheduler.rs` | Signal UserHandler | ~50 lines |
| `crates/syscall/syscall/src/memory.rs` | brk() | ~30 lines |
| `crates/syscall/syscall/src/poll.rs` | EINTR on signal | ~10 lines |

---

## TEST COMMANDS

```bash
# Test if vim starts
vim

# Test basic editing
vim test.txt
# i (insert) → type text → ESC → :wq

# Test terminal resize
# Resize terminal window while vim is running
# Should redraw without corruption

# Test Ctrl+C
vim
# Ctrl+C should not crash

# Test Ctrl+Z
vim
# Ctrl+Z should suspend
# fg should resume

# Test arrow keys
vim
# Arrow keys should move cursor

# Test escape timing
vim
# ESC should exit insert mode immediately
```

---

## CONCLUSION

Vim requires **4 critical fixes** before it will work reliably:

1. 🔴 **libc fcntl()** - Just use the kernel syscall (1 line fix!)
2. 🔴 **Signal UserHandler delivery** - Handlers never invoked
3. 🔴 **SIGWINCH on resize** - Not sent when window size changes
4. 🔴 **brk() or mmap** - Memory allocation must work

After these fixes, vim should be functional for basic editing. Additional fixes (job control, file locking) will improve stability.

**The kernel has more implemented than the libc exposes!**

**Estimated effort:** 
- Phase 1: 1-2 days
- Phase 2: 1 day  
- Phase 3: 1 day
