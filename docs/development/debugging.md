# Debugging OXIDE OS

## Debug Feature Flags

Debug output is gated via Cargo feature flags on the kernel crate. Never delete
debug macros — gate them with features instead.

### Available Channels

| Flag | What It Traces |
|------|---------------|
| `debug-input` | Input event path (keyboard, mouse) |
| `debug-mouse` | Mouse events specifically |
| `debug-sched` | Scheduler context switches |
| `debug-fork` | Process fork/exec operations |
| `debug-lock` | Lock contention |
| `debug-syscall` | Syscall entry/exit |
| `debug-syscall-perf` | Slow syscalls (>100K cycles) |
| `debug-tty-read` | TTY read operations |
| `debug-all` | All channels enabled |

### Usage

```bash
# Enable specific channels
make run RUN_KERNEL_FEATURES="debug-syscall debug-sched"

# Enable everything
make run RUN_KERNEL_FEATURES="debug-all"

# Build-time features (baked into kernel binary)
make build KERNEL_FEATURES="debug-fork debug-lock"
```

## Debug Macros

Use macros from `kernel/src/debug.rs`:

- `debug_*!()` — standard debug output for each channel
- `debug_sched_unsafe!()` — ISR-safe scheduler debug (no locks)

Never use raw serial writes — always go through the debug macro system.

## Serial Output

QEMU serial output goes to `target/serial.log`:

```bash
make run                    # Serial output on stdio
cat target/serial.log       # After a test run
make test                   # Automated: checks serial.log for OXIDE banner
```

## GDB

```bash
# Terminal 1: start QEMU with GDB server
make run QEMU_EXTRA="-s -S"

# Terminal 2: connect GDB
gdb target/x86_64-unknown-none/debug/oxide-kernel
(gdb) target remote :1234
(gdb) continue
```
