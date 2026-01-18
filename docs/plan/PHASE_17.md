# Phase 17: Containers

**Stage:** 4 - Advanced
**Status:** Not Started
**Dependencies:** Phase 9 (SMP)

---

## Goal

Implement Linux-compatible namespaces and cgroups for container isolation.

---

## Deliverables

| Item | Status |
|------|--------|
| PID namespaces | [ ] |
| Mount namespaces | [ ] |
| Network namespaces | [ ] |
| User namespaces | [ ] |
| UTS namespaces | [ ] |
| Cgroups v2 (CPU, memory) | [ ] |
| Seccomp syscall filtering | [ ] |

---

## Architecture Status

| Arch | Namespaces | Cgroups | Seccomp | Done |
|------|------------|---------|---------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] |

---

## Namespace Types

| Type | Flag | Isolates |
|------|------|----------|
| PID | CLONE_NEWPID | Process IDs |
| Mount | CLONE_NEWNS | Mount points |
| Network | CLONE_NEWNET | Network stack |
| User | CLONE_NEWUSER | User/group IDs |
| UTS | CLONE_NEWUTS | Hostname |
| IPC | CLONE_NEWIPC | IPC resources |
| Cgroup | CLONE_NEWCGROUP | Cgroup root |

---

## Container Architecture

```
┌──────────────────────────────────────────────────┐
│                   Host System                    │
│                                                  │
│  ┌────────────────┐    ┌────────────────┐       │
│  │  Container A   │    │  Container B   │       │
│  │                │    │                │       │
│  │  PID NS: 1     │    │  PID NS: 2     │       │
│  │  ┌──────────┐  │    │  ┌──────────┐  │       │
│  │  │ init (1) │  │    │  │ init (1) │  │       │
│  │  │  └─ sh   │  │    │  │  └─ app  │  │       │
│  │  └──────────┘  │    │  └──────────┘  │       │
│  │                │    │                │       │
│  │  Mount NS:     │    │  Mount NS:     │       │
│  │  /rootfs-a     │    │  /rootfs-b     │       │
│  │                │    │                │       │
│  │  Net NS:       │    │  Net NS:       │       │
│  │  veth0         │    │  veth1         │       │
│  └────────────────┘    └────────────────┘       │
│                                                  │
│  Cgroups:                                        │
│  /sys/fs/cgroup/container-a (CPU: 50%, Mem: 1G) │
│  /sys/fs/cgroup/container-b (CPU: 25%, Mem: 512M)│
└──────────────────────────────────────────────────┘
```

---

## Syscalls to Implement

| Number | Name | Args | Return |
|--------|------|------|--------|
| 80 | sys_unshare | flags | 0 or -errno |
| 81 | sys_setns | fd, nstype | 0 or -errno |
| 82 | sys_clone3 | args, size | pid or -errno |
| 83 | sys_pivot_root | new_root, put_old | 0 or -errno |
| 84 | sys_seccomp | op, flags, args | varies |

---

## PID Namespace

```rust
pub struct PidNamespace {
    /// Parent namespace (None for root)
    parent: Option<Arc<PidNamespace>>,

    /// PID allocator for this namespace
    pid_alloc: PidAllocator,

    /// Init process (PID 1 in this namespace)
    init: Option<Weak<Process>>,

    /// Mapping: local PID -> global PID
    local_to_global: BTreeMap<Pid, Pid>,
}

// Process sees PID 1 inside container
// Host sees the actual PID (e.g., 12345)
```

---

## Mount Namespace

```rust
pub struct MountNamespace {
    /// Root mount point
    root: Arc<Mount>,

    /// All mount points in this namespace
    mounts: Vec<Arc<Mount>>,

    /// Mount ID allocator
    mount_id_alloc: AtomicU64,
}

// Each container has its own view of the filesystem
// pivot_root switches to new root filesystem
```

---

## Cgroups v2

```
/sys/fs/cgroup/
├── cgroup.controllers      # Available controllers
├── cgroup.subtree_control  # Enabled for children
├── container-a/
│   ├── cgroup.procs        # PIDs in this cgroup
│   ├── cpu.max             # CPU limit (quota period)
│   ├── memory.max          # Memory limit
│   ├── memory.current      # Current usage
│   └── ...
└── container-b/
    └── ...

cpu.max format: "quota period" (e.g., "50000 100000" = 50%)
memory.max format: bytes (e.g., "1073741824" = 1GB)
```

---

## Cgroup Structure

```rust
pub struct Cgroup {
    /// Path in cgroup hierarchy
    path: PathBuf,

    /// Parent cgroup
    parent: Option<Weak<Cgroup>>,

    /// Processes in this cgroup
    processes: Mutex<Vec<Weak<Process>>>,

    /// CPU controller
    cpu: Option<CpuController>,

    /// Memory controller
    memory: Option<MemoryController>,
}

pub struct CpuController {
    quota_us: AtomicU64,    // Quota in microseconds
    period_us: AtomicU64,   // Period in microseconds
    usage_us: AtomicU64,    // Current usage
}

pub struct MemoryController {
    max_bytes: AtomicU64,       // Limit
    current_bytes: AtomicU64,   // Current usage
    oom_kill_count: AtomicU64,  // OOM kills
}
```

---

## Seccomp

```rust
// Seccomp BPF filter
pub struct SeccompFilter {
    /// BPF program
    program: Vec<BpfInsn>,

    /// Default action
    default_action: SeccompAction,
}

pub enum SeccompAction {
    Allow,
    Kill,
    Trap,       // Send SIGSYS
    Errno(i32), // Return errno
    Trace,      // Notify tracer
    Log,        // Log and allow
}

// Filter checks:
// - Syscall number
// - Architecture
// - Arguments (limited)
```

---

## Key Files

```
crates/container/efflux-namespace/src/
├── lib.rs
├── pid.rs             # PID namespace
├── mount.rs           # Mount namespace
├── net.rs             # Network namespace
├── user.rs            # User namespace
└── uts.rs             # UTS namespace

crates/container/efflux-cgroup/src/
├── lib.rs
├── v2.rs              # Cgroups v2 implementation
├── cpu.rs             # CPU controller
├── memory.rs          # Memory controller
└── fs.rs              # cgroupfs

crates/container/efflux-seccomp/src/
├── lib.rs
├── filter.rs          # BPF filter
└── bpf.rs             # BPF interpreter
```

---

## Exit Criteria

- [ ] unshare() creates new namespaces
- [ ] PID namespace shows PID 1 for container init
- [ ] Mount namespace isolates filesystem view
- [ ] pivot_root changes root filesystem
- [ ] Cgroups limit CPU usage
- [ ] Cgroups limit memory (OOM kill works)
- [ ] Seccomp blocks syscalls
- [ ] Works on all 8 architectures

---

## Test: Simple Container

```c
int container_main(void *arg) {
    // Inside new namespaces
    sethostname("container", 9);

    // Mount proc
    mount("proc", "/proc", "proc", 0, NULL);

    // Pivot to new root
    pivot_root("/rootfs", "/rootfs/old");
    umount2("/old", MNT_DETACH);

    // Check PID
    printf("PID inside container: %d\n", getpid());  // Should be 1

    execl("/bin/sh", "sh", NULL);
    return 1;
}

int main() {
    char stack[8192];

    int flags = CLONE_NEWPID | CLONE_NEWNS | CLONE_NEWUTS | SIGCHLD;
    pid_t pid = clone(container_main, stack + 8192, flags, NULL);

    printf("Container PID from host: %d\n", pid);
    waitpid(pid, NULL, 0);
    return 0;
}
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 17 of EFFLUX Implementation*
