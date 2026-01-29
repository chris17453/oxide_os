# Current Status: Python Execution Issues

## What Works
- ✅ **TLS (Thread-Local Storage)** - Fixed by NOT loading FS segment after WRMSR
- ✅ **Kernel heap** - Increased to 32MB for large executables
- ✅ **Userspace heap** - Increased to 64MB
- ✅ **Syscall numbers** - Fixed critical mismatch between libc and kernel
- ✅ **SETEUID/SETEGID** - Implemented syscalls 20 and 21
- ✅ **setlocale()** - Now accepts all locales (returns "C")
- ✅ **Python loads and executes** - No more page faults or kernel panics

## Current Problem: SIGHUP Killing All Processes

**CRITICAL**: All child processes are being killed by signal 1 (SIGHUP):
```
[init] Python test FAILED (status=1)
[init] Reaped process 4 (killed by signal 1)
[init] Reaped process 5 (killed by signal 1)
[init] Reaped process 6 (killed by signal 1)
... (continues for all children)
```

Python starts executing but is immediately killed by SIGHUP before it can complete.

### What Python Shows Before Death
```
Error setting LC_CTYPE, skipping C locale coercion (x2)
object address  : 0xbb83c0
object refcount : 1
object type     : 0x986dc0
object type name: %s
object repr     : ("Missed attribute 'n_fields' of type ")
lost sys.stderr
```

The "Missed attribute 'n_fields'" error might be Python's death throes, not the root cause.

## Root Cause Analysis Needed

### Hypothesis: Terminal/Session Signal Handling
- SIGHUP is typically sent when:
  1. Terminal hangs up
  2. Controlling terminal closes
  3. Session leader dies
  4. TTY is closed

### What to Investigate
1. **Check init's signal handling** - Is init forwarding SIGHUP to children?
2. **Check TTY/session setup** - Are child processes in correct session/pgrp?
3. **Check terminal state** - Is something closing the terminal?
4. **Check process groups** - Are processes in correct pgrp to avoid SIGHUP?

## Files Changed (Last Session)

### Syscall Number Fixes
- `userspace/libc/src/syscall.rs` - Fixed UID/GID syscall numbers (14-21, not 100-107)
- `crates/syscall/syscall/src/lib.rs` - Added SETEUID/SETEGID syscalls (20-21)

### Other Changes
- `userspace/libc/src/locale.rs` - setlocale() accepts all locales
- `userspace/libc/src/pwd.rs` - seteuid/setegid implementations
- `userspace/libc/src/c_exports.rs` - Removed debug tracing from vfprintf
- `toolchain/sysroot/lib/libm.a` - Created symlink to liboxide_libc.a

### Python Test
- `userspace/init/src/main.rs` - Testing: `python3 -I -S -c "import sys; print('Python', sys.version_info[:2])"`

## Next Steps
1. Investigate why all processes receive SIGHUP
2. Check init's signal mask and forwarding behavior
3. Check if Python/children need to be in separate session
4. May need to call setsid() before exec to create new session
5. Check if issue is specific to init's children or affects all processes

## Build Commands
```bash
make build                    # Rebuild kernel
make userspace               # Rebuild userspace
make toolchain               # Rebuild toolchain (if libc changes)
make cpython                 # Rebuild Python (if toolchain changes)
make create-rootfs           # Rebuild root filesystem
make run                     # Test in QEMU
```

## Git Status
Commits ready to push:
- Fix TLS support (don't load FS after WRMSR)
- Increase heap sizes (kernel 32MB, userspace 64MB)
- Fix syscall number mismatch and implement SETEUID/SETEGID
- Remove debug tracing

**PUSH REQUIRED** - These commits need to be pushed to remote.
