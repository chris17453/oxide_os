# Autonomous GDB Debugging for OXIDE OS

— ColdCipher: Because debugging at 3 AM requires programmatic control,
not fumbling through GDB commands while your eyes glaze over.

## Overview

This debugging infrastructure allows Claude Code (or any autonomous agent) to debug the OXIDE kernel without human intervention. The system provides:

- **Programmatic GDB control** via Python scripts
- **Crash capture** that runs until fault/panic and dumps state
- **Boot verification** to quickly check if the kernel starts
- **Command execution** for targeted debugging queries
- **Interactive REPL** for exploratory debugging

## Quick Start

### For Humans

```bash
# Run and attach GDB interactively (old way)
make run

# Start QEMU with GDB server, no auto-attach
make debug-server

# Capture any crash/panic automatically
make debug-capture

# Quick boot sanity check
make debug-boot-check

# Execute a single GDB command
make debug-exec CMD="bt"
make debug-exec CMD="info registers"

# Interactive autonomous REPL
make debug-repl
```

### For Agents

```bash
# All-in-one script for autonomous debugging
./scripts/debug-kernel.sh capture           # Capture crash
./scripts/debug-kernel.sh boot              # Boot check
./scripts/debug-kernel.sh exec "bt"         # Execute command
./scripts/debug-kernel.sh repl              # Interactive

# Or use Python controller directly
./scripts/gdb-autonomous.py --panic         # Analyze panic
./scripts/gdb-autonomous.py --backtrace     # Get backtrace
./scripts/gdb-autonomous.py --exec "bt"     # Execute command
./scripts/gdb-autonomous.py --repl          # REPL mode
```

## Integration with Claude Code

The agent can use these tools autonomously via the Bash tool. Here's the workflow:

### Step 1: Reproduce the Issue

```bash
# Build and capture crash
make build && ./scripts/debug-kernel.sh capture
```

This will run until a crash/panic occurs and dump full state to `target/crash-capture.log`.

### Step 2: Analyze Output

Read the crash log:

```bash
cat target/crash-capture.log
```

Look for:
- Panic messages
- Backtrace showing call stack
- Fault addresses (RIP, RSP, etc.)
- Register state (especially RIP, RSP, RBP)

### Step 3: Investigate Root Cause

Based on crash analysis, execute targeted queries:

```bash
# Get detailed backtrace
./scripts/debug-kernel.sh exec "bt full"

# Examine fault address
./scripts/debug-kernel.sh exec "x/32x \$rip"

# Check specific variable
./scripts/debug-kernel.sh exec "p some_variable"
```

### Step 4: Fix and Verify

1. Fix the code based on findings
2. Rebuild: `make build`
3. Verify fix: `./scripts/debug-kernel.sh boot`
4. Full test: `./scripts/debug-kernel.sh capture`

## Output Files

All debugging sessions create log files in `target/`:

- `target/crash-capture.log` - Full crash dump
- `target/boot-check.log` - Boot verification results
- `target/qemu.log` - QEMU debug log (interrupts, faults, etc.)
- `target/serial.log` - Serial port output

## Troubleshooting

### GDB Won't Connect

```bash
# Check if QEMU is running with GDB server
ps aux | grep qemu

# Check if port 1234 is open
nc -zv localhost 1234

# Kill stale QEMU and retry
make kill-qemu
```

### Timeout Errors

Increase timeout in the script or Makefile:

```bash
# In debug-kernel.sh, increase timeout
timeout 120 gdb ...  # 2 minutes instead of 60s
```

### OVMF Not Found

Install OVMF firmware:

```bash
# Fedora
sudo dnf install edk2-ovmf

# RHEL
sudo dnf install edk2-ovmf
```

## Advanced Usage

### Custom Breakpoints

Create a breakpoint script:

```gdb
# breakpoints.gdb
target remote :1234

# Break on specific function
break kernel::my_function
commands
    silent
    printf "Hit my_function\n"
    bt 5
    info locals
    continue
end

# Break on condition
break kernel::scheduler::schedule if task->priority > 10
commands
    silent
    printf "High priority task: %d\n", task->priority
    continue
end

continue
```

Execute:

```bash
./scripts/debug-kernel.sh script breakpoints.gdb
```

### Watchpoints

Watch memory for changes:

```gdb
# watch.gdb
target remote :1234

# Watch a variable
watch some_global_var

# Watch memory address
watch *0xffff800000123000

continue
```

### Thread Debugging

Examine all threads:

```bash
./scripts/debug-kernel.sh exec "info threads"
./scripts/debug-kernel.sh exec "thread apply all bt"
```

Switch between threads:

```bash
./scripts/debug-kernel.sh exec "thread 2"
./scripts/debug-kernel.sh exec "bt"
```

## Architecture Notes

### Why Not Interactive GDB?

Interactive GDB requires human input, which blocks autonomous debugging. This system provides:

- **Batch mode** execution with structured output
- **Scripted** breakpoints and commands
- **Programmatic** control via Python API
- **Non-blocking** operation

### Why Python Controller?

The Python controller (`gdb-autonomous.py`) provides:

- Structured output parsing
- Retry logic for connection
- Timeout handling
- Easy integration with other tools
- Rich API for complex queries

### Why Shell Wrapper?

The shell wrapper (`debug-kernel.sh`) handles:

- QEMU lifecycle management
- Cleanup on exit/interrupt
- Log file management
- Simple command-line interface

## Tips for Autonomous Debugging

1. **Always capture first** - Run crash capture to get full state before targeted queries

2. **Use boot check for quick feedback** - Verify changes boot before full test

3. **Parse backtraces systematically** - Look for:
   - Function names (what was executing)
   - File:line numbers (exact location)
   - Arguments (function inputs)

4. **Check register state** - Common issues:
   - RIP = instruction pointer (where crash occurred)
   - RSP = stack pointer (stack corruption if wrong)
   - RAX/RDI/RSI = function args/return values

5. **Examine memory around fault** - Use `x/32i $rip-32` to see instructions

6. **Look for patterns** - Same crash repeatedly? Same backtrace? Focus there.

7. **Verify fixes work** - Always run boot check after changes

## Files Reference

### Scripts

- `scripts/gdb-autonomous.py` - Python GDB controller
- `scripts/debug-kernel.sh` - Shell wrapper for debugging
- `scripts/gdb-init-kernel.gdb` - GDB initialization
- `scripts/gdb-capture-crash.gdb` - Crash capture script
- `scripts/gdb-check-boot.gdb` - Boot verification script

### Makefiles

- `mk/qemu.mk` - QEMU and debug targets
- `mk/config.mk` - GDB configuration variables
- `mk/help.mk` - Help documentation

### Documentation

- `docs/AUTONOMOUS-DEBUGGING.md` - This file
- `docs/DEBUGGING.md` - General debugging guide

## See Also

- `docs/DEBUGGING.md` - Manual debugging with GDB
- `docs/DRIVES.md` - Boot and filesystem flow
- `AGENTS.md` - Agent-specific instructions

---

— ColdCipher: Debug smart, not hard. The kernel doesn't care about your feelings,
but at least now you can debug it without losing your mind at 3 AM.
