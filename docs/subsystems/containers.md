# Container Subsystem

## Crates

| Crate | Purpose |
|-------|---------|
| `namespace` | Linux-style namespaces (PID, mount, network, user, IPC, UTS) |
| `cgroup` | Control groups v2 for resource management |
| `seccomp` | Syscall filtering via BPF programs |

## Architecture

OXIDE implements Linux-compatible container primitives for process isolation:

- **Namespaces** isolate PID trees, mount tables, network stacks, and user IDs
- **Cgroups** enforce CPU, memory, and I/O limits per process group
- **Seccomp** filters syscalls to reduce the kernel attack surface

These primitives can be composed to create lightweight containers without
a full container runtime.
