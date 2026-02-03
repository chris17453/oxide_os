# OXIDE OS Debugging Guide

## Quick Start

**Just run this:**
```bash
make run
```

Debug output is **already enabled** in the Makefile with ALL features except timer spam:
- ✅ Syscalls, fork, pagefaults, scheduler, locks, console, etc.
- ❌ Timer debug (disabled - fires 1000x/sec)

**NEW:** Recursion-protected debug system prevents feedback loops!

Serial output goes to the console automatically.

---

## Disable Debugging

Edit `Makefile` line 25:
```makefile
RUN_KERNEL_FEATURES ?=
```

---

## Debug Features Available

Set in Makefile or pass as argument:

**Recommended (current default):**
- `debug-syscall` - Syscall tracing ✅
- `debug-fork` - Fork/exec/clone operations ✅
- `debug-pagefault` - All page faults ✅

**Useful but verbose:**
- `debug-cow` - COW page faults
- `debug-mmap` - Memory mapping
- `debug-proc` - Process management

**⚠️ DANGER: Extremely verbose (causes feedback loop slowdown):**
- `debug-all` - All debugging (TOO SLOW - don't use)
- `debug-timer` - Timer interrupts (fires constantly)
- `debug-sched` - Context switches (happens constantly)
- `debug-lock` - Lock contention (happens constantly)
- `debug-syscall-perf` - Slow syscalls (slows down syscalls)

Combine with commas: `debug-syscall,debug-fork`

---

## Commands

```bash
# Normal run (with debugging enabled by default)
make run

# Build only
make build

# Clean build
make clean && make run
```

---

## Output

All debug output goes to serial console (your terminal).

Look for:
- `[SYSCALL]` - System call entry/exit
- `[FORK]` - Process creation
- `[PAGE]` - Page faults
- `[SCHED]` - Scheduling decisions
- Kernel panic traces

---

## Crash Analysis

When kernel panics, you'll see:
1. Panic message
2. Register dump (RAX, RBX, RCX, etc.)
3. Stack trace
4. Recent syscalls (if debug-syscall enabled)

Save the full output for debugging.

---

**TL;DR: Just run `make run` - debugging is already on.**
