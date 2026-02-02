# Scheduling & Process Management

## Crates

| Crate | Purpose |
|-------|---------|
| `sched-traits` | Scheduler trait abstractions |
| `sched` | Round-robin scheduler implementation |
| `proc-traits` | Process/thread trait abstractions |
| `proc` | Process management (fork, exec, wait, exit) |
| `smp` | Symmetric multiprocessing support |

## Architecture

The scheduler uses a round-robin algorithm with timer-driven preemption.
Each CPU core runs its own scheduling loop via the `smp` crate.

Processes are managed by the `proc` crate: `fork()` creates a child with
CoW memory, `exec()` loads a new ELF binary, `wait()` reaps zombie children.
Signal delivery is handled by the `signal` crate.
