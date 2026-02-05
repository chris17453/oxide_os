# Cgroup and Seccomp Design

## Overview

This document outlines the design for cgroups v2 (control groups) and seccomp (secure computing mode) in OXIDE OS. Both are essential container primitives for resource management and syscall filtering.

## Cgroups v2

### Purpose

Control groups provide resource management and monitoring for groups of processes:
- **CPU**: Time slice allocation, real-time scheduling
- **Memory**: Limits, OOM control, statistics
- **I/O**: Bandwidth limits, latency targets
- **PIDs**: Maximum process count per cgroup

### Architecture

#### Hierarchy

Cgroups form a single unified hierarchy (v2 model):

```
/sys/fs/cgroup/
├── cgroup.controllers      (available controllers)
├── cgroup.subtree_control  (enabled controllers for children)
├── system.slice/
│   ├── sshd.service/
│   │   ├── cpu.weight
│   │   ├── memory.max
│   │   └── pids.max
│   └── systemd.service/
└── user.slice/
    └── user-1000.slice/
        ├── session-1.scope/
        └── session-2.scope/
```

#### Cgroup Structure

```rust
pub struct Cgroup {
    /// Cgroup name
    name: String,
    /// Parent cgroup
    parent: Option<Arc<Cgroup>>,
    /// Child cgroups
    children: Mutex<Vec<Arc<Cgroup>>>,
    /// Processes in this cgroup
    processes: Mutex<HashSet<Pid>>,
    /// Controllers and their state
    controllers: Mutex<HashMap<Controller, ControllerState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Controller {
    Cpu,
    Memory,
    Io,
    Pids,
}

pub enum ControllerState {
    Cpu(CpuController),
    Memory(MemoryController),
    Io(IoController),
    Pids(PidsController),
}
```

### Controllers

#### CPU Controller

```rust
pub struct CpuController {
    /// Relative weight (1-10000, default 100)
    weight: u64,
    /// Maximum burst (microseconds)
    max_burst: u64,
    /// Period (microseconds)
    period: u64,
    /// Quota per period (microseconds)
    quota: i64,
    /// Statistics
    stats: CpuStats,
}

pub struct CpuStats {
    /// Total CPU time used (microseconds)
    usage_usec: AtomicU64,
    /// User CPU time
    user_usec: AtomicU64,
    /// System CPU time
    system_usec: AtomicU64,
    /// Number of periods throttled
    nr_throttled: AtomicU64,
    /// Total throttled time
    throttled_usec: AtomicU64,
}
```

**Files:**
- `cpu.weight` - Relative scheduling weight (default 100)
- `cpu.max` - "quota period" (e.g., "100000 1000000" = 10% CPU)
- `cpu.stat` - Statistics (usage, throttling)

#### Memory Controller

```rust
pub struct MemoryController {
    /// Hard memory limit (bytes)
    max: Option<u64>,
    /// Soft memory limit (bytes)
    high: Option<u64>,
    /// Swap limit (bytes)
    swap_max: Option<u64>,
    /// OOM killer configuration
    oom_group: bool,
    /// Statistics
    stats: MemoryStats,
}

pub struct MemoryStats {
    /// Current memory usage
    current: AtomicU64,
    /// Peak memory usage
    peak: AtomicU64,
    /// Anonymous memory
    anon: AtomicU64,
    /// File-backed memory
    file: AtomicU64,
    /// Page faults
    pgfault: AtomicU64,
    /// Major page faults
    pgmajfault: AtomicU64,
}
```

**Files:**
- `memory.max` - Hard limit (OOM if exceeded)
- `memory.high` - Soft limit (throttle if exceeded)
- `memory.current` - Current usage
- `memory.stat` - Detailed statistics

#### I/O Controller

```rust
pub struct IoController {
    /// Per-device bandwidth limits
    limits: HashMap<DeviceId, IoBandwidthLimit>,
    /// Latency targets
    latency: HashMap<DeviceId, Duration>,
    /// Statistics
    stats: IoStats,
}

pub struct IoBandwidthLimit {
    /// Read bytes per second
    rbps: u64,
    /// Write bytes per second
    wbps: u64,
    /// Read IOPS
    riops: u64,
    /// Write IOPS
    wiops: u64,
}
```

**Files:**
- `io.max` - "major:minor rbps=N wbps=N riops=N wiops=N"
- `io.latency` - Target latency per device
- `io.stat` - I/O statistics per device

#### PIDs Controller

```rust
pub struct PidsController {
    /// Maximum number of processes
    max: Option<usize>,
    /// Current process count
    current: AtomicUsize,
}
```

**Files:**
- `pids.max` - Maximum PIDs (or "max" for unlimited)
- `pids.current` - Current PID count

### Process Assignment

```rust
/// Move process into cgroup
pub fn cgroup_attach(cgroup: &Cgroup, pid: Pid) -> Result<()> {
    // 1. Remove from old cgroup
    if let Some(old) = get_process_cgroup(pid) {
        old.processes.lock().remove(&pid);
    }
    
    // 2. Check limits (e.g., pids.max)
    if let Some(pids_ctrl) = cgroup.controllers.lock().get(&Controller::Pids) {
        if let ControllerState::Pids(pids) = pids_ctrl {
            if let Some(max) = pids.max {
                if cgroup.processes.lock().len() >= max {
                    return Err(Error::Busy);
                }
            }
        }
    }
    
    // 3. Add to new cgroup
    cgroup.processes.lock().insert(pid);
    
    // 4. Update process metadata
    get_process(pid)?.lock().cgroup = Arc::downgrade(cgroup);
    
    Ok(())
}
```

### Enforcement

#### CPU Enforcement

Scheduler checks cgroup CPU limits:

```rust
pub fn schedule() {
    let current = current_task();
    let cgroup = current.cgroup();
    
    // Check CPU quota
    if let Some(cpu_ctrl) = cgroup.get_controller(Controller::Cpu) {
        if cpu_ctrl.quota_exceeded() {
            // Throttle: don't schedule until next period
            current.set_throttled(true);
            return schedule_next();
        }
    }
    
    // Use cgroup weight for scheduling priority
    let weight = cgroup.cpu_weight().unwrap_or(100);
    current.set_priority_from_weight(weight);
}
```

#### Memory Enforcement

Page allocation checks memory limits:

```rust
pub fn alloc_page() -> Result<Page> {
    let current = current_task();
    let cgroup = current.cgroup();
    
    if let Some(mem_ctrl) = cgroup.get_controller(Controller::Memory) {
        let current_usage = mem_ctrl.stats.current.load(Ordering::Relaxed);
        
        // Check hard limit
        if let Some(max) = mem_ctrl.max {
            if current_usage + PAGE_SIZE > max {
                // OOM: kill process or return error
                return Err(Error::OutOfMemory);
            }
        }
        
        // Check soft limit (throttle)
        if let Some(high) = mem_ctrl.high {
            if current_usage + PAGE_SIZE > high {
                // Trigger reclaim
                mem_reclaim(cgroup);
            }
        }
    }
    
    // Allocate page and account to cgroup
    let page = physical_alloc()?;
    cgroup.memory_charge(PAGE_SIZE);
    Ok(page)
}
```

### Cgroup Filesystem

Mounted at `/sys/fs/cgroup`:

```rust
pub struct CgroupFs {
    root: Arc<Cgroup>,
}

impl Filesystem for CgroupFs {
    fn lookup(&self, path: &str) -> Result<Arc<dyn VnodeOps>> {
        // Navigate cgroup hierarchy
        let cgroup = self.root.find_child(path)?;
        Ok(Arc::new(CgroupDir::new(cgroup)))
    }
}

pub struct CgroupDir {
    cgroup: Arc<Cgroup>,
}

impl VnodeOps for CgroupDir {
    fn readdir(&self) -> Result<Vec<DirEntry>> {
        let mut entries = Vec::new();
        
        // Add controller files
        entries.push(DirEntry::file("cgroup.controllers"));
        entries.push(DirEntry::file("cgroup.procs"));
        
        // Add controller-specific files
        for controller in self.cgroup.enabled_controllers() {
            match controller {
                Controller::Cpu => {
                    entries.push(DirEntry::file("cpu.weight"));
                    entries.push(DirEntry::file("cpu.max"));
                    entries.push(DirEntry::file("cpu.stat"));
                }
                // ... other controllers ...
            }
        }
        
        // Add child cgroups
        for child in self.cgroup.children.lock().iter() {
            entries.push(DirEntry::dir(&child.name));
        }
        
        Ok(entries)
    }
}
```

## Seccomp

### Purpose

Seccomp (secure computing mode) filters syscalls to reduce kernel attack surface:
- Whitelist allowed syscalls
- Block dangerous syscalls
- Log suspicious activity
- Enforce argument constraints

### Modes

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeccompMode {
    /// No filtering
    Disabled,
    /// Strict mode: only read, write, exit, sigreturn
    Strict,
    /// Filter mode: BPF program decides
    Filter,
}
```

#### Strict Mode

Oldest mode, extremely restrictive:
- Allowed: read, write, exit, sigreturn
- All others: SIGKILL

```rust
const STRICT_ALLOWED: &[u64] = &[
    nr::READ,
    nr::WRITE,
    nr::EXIT,
    nr::SIGRETURN,
];

pub fn seccomp_strict_check(syscall_nr: u64) -> bool {
    STRICT_ALLOWED.contains(&syscall_nr)
}
```

#### Filter Mode (BPF)

Modern mode using BPF programs:

```rust
pub struct SeccompFilter {
    /// BPF program
    program: BpfProgram,
    /// Program length (instructions)
    len: usize,
}

pub struct BpfProgram {
    /// BPF instructions
    insns: Vec<BpfInsn>,
}

/// BPF instruction (sock_filter compatible)
#[repr(C)]
pub struct BpfInsn {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}
```

### Filter Actions

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeccompAction {
    /// Allow syscall
    Allow,
    /// Kill process with SIGSYS
    Kill,
    /// Return errno to caller
    Errno(i32),
    /// Send SIGSYS signal but don't kill
    Trap,
    /// Log and allow
    Log,
    /// Trace (for debugging)
    Trace,
}
```

### BPF Program Structure

Seccomp BPF operates on seccomp_data:

```rust
#[repr(C)]
pub struct SeccompData {
    /// Syscall number
    nr: u32,
    /// Architecture (AUDIT_ARCH_X86_64)
    arch: u32,
    /// Instruction pointer
    instruction_pointer: u64,
    /// Syscall arguments (6 args)
    args: [u64; 6],
}
```

Example filter (allow only read/write/exit):

```rust
pub fn example_filter() -> SeccompFilter {
    // Load syscall number into accumulator
    BPF_STMT(BPF_LD | BPF_W | BPF_ABS, offsetof!(SeccompData, nr)),
    
    // Check if nr == READ
    BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, nr::READ, 0, 1),
    BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW),
    
    // Check if nr == WRITE
    BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, nr::WRITE, 0, 1),
    BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW),
    
    // Check if nr == EXIT
    BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, nr::EXIT, 0, 1),
    BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW),
    
    // Default: kill
    BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_KILL),
}
```

### Installation

Via prctl():

```rust
pub fn sys_prctl(option: i32, arg2: u64, ...) -> i64 {
    match option {
        PR_SET_NO_NEW_PRIVS => {
            // Must set no_new_privs before seccomp
            current().no_new_privs = true;
            Ok(0)
        }
        PR_SET_SECCOMP => {
            let mode = arg2 as i32;
            match mode {
                SECCOMP_MODE_STRICT => {
                    current().seccomp_mode = SeccompMode::Strict;
                    Ok(0)
                }
                SECCOMP_MODE_FILTER => {
                    // arg3 = BPF program pointer
                    let prog = read_bpf_program(arg3)?;
                    install_seccomp_filter(prog)?;
                    Ok(0)
                }
                _ => Err(errno::EINVAL),
            }
        }
        PR_GET_SECCOMP => {
            Ok(current().seccomp_mode as i64)
        }
        _ => Err(errno::EINVAL),
    }
}
```

### Enforcement

On every syscall:

```rust
pub fn syscall_dispatch(nr: u64, args: &[u64; 6]) -> i64 {
    let current = current_task();
    
    // Check seccomp
    if current.seccomp_mode != SeccompMode::Disabled {
        let action = seccomp_check(current, nr, args);
        match action {
            SeccompAction::Allow => { /* continue */ }
            SeccompAction::Kill => {
                signal::send_signal(current, SIGSYS);
                sys_exit_group(-1);
            }
            SeccompAction::Errno(err) => {
                return -err as i64;
            }
            SeccompAction::Trap => {
                signal::send_signal(current, SIGSYS);
                return -errno::EACCES;
            }
            SeccompAction::Log => {
                os_log::println!("[SECCOMP] Process {} syscall {} logged", 
                    current.pid, nr);
            }
            SeccompAction::Trace => {
                // Ptrace notification (future work)
            }
        }
    }
    
    // Normal dispatch
    dispatch_syscall(nr, args)
}

fn seccomp_check(task: &Task, nr: u64, args: &[u64; 6]) -> SeccompAction {
    match task.seccomp_mode {
        SeccompMode::Disabled => SeccompAction::Allow,
        SeccompMode::Strict => {
            if STRICT_ALLOWED.contains(&nr) {
                SeccompAction::Allow
            } else {
                SeccompAction::Kill
            }
        }
        SeccompMode::Filter => {
            // Run BPF program
            let data = SeccompData {
                nr: nr as u32,
                arch: AUDIT_ARCH_X86_64,
                instruction_pointer: task.rip,
                args: *args,
            };
            
            let ret = run_bpf_program(&task.seccomp_filter, &data);
            decode_seccomp_ret(ret)
        }
    }
}
```

### BPF Interpreter

```rust
pub fn run_bpf_program(prog: &BpfProgram, data: &SeccompData) -> u32 {
    let mut A: u32 = 0; // Accumulator
    let mut X: u32 = 0; // Index register
    let mut pc: usize = 0; // Program counter
    
    while pc < prog.insns.len() {
        let insn = &prog.insns[pc];
        
        match insn.code & 0xFF {
            BPF_LD | BPF_W | BPF_ABS => {
                // Load word at offset k
                A = load_word(data, insn.k);
            }
            BPF_JMP | BPF_JEQ | BPF_K => {
                // Jump if A == k
                if A == insn.k {
                    pc += insn.jt as usize;
                } else {
                    pc += insn.jf as usize;
                }
                continue;
            }
            BPF_RET | BPF_K => {
                // Return constant
                return insn.k;
            }
            _ => {
                // Invalid instruction
                return SECCOMP_RET_KILL;
            }
        }
        
        pc += 1;
    }
    
    // Ran off end of program
    SECCOMP_RET_KILL
}
```

## Integration

### Container Workflow

```rust
// 1. Create cgroup
mkdir("/sys/fs/cgroup/container1")?;
write("/sys/fs/cgroup/container1/memory.max", "512M")?;
write("/sys/fs/cgroup/container1/pids.max", "100")?;

// 2. Move process into cgroup
write("/sys/fs/cgroup/container1/cgroup.procs", format!("{}", getpid()))?;

// 3. Set up seccomp
prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)?;
let filter = create_container_filter();
prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &filter)?;

// 4. Drop privileges
setuid(1000)?;

// 5. Execute container init
execve("/bin/init", args, env)?;
```

## Security Considerations

### Seccomp Bypass Prevention

- BPF program validation before installation
- No_new_privs required before filter
- Filters are inherited and cannot be removed
- BPF programs run in kernel (no untrusted code)

### Cgroup Escape Prevention

- Processes cannot write to cgroup.procs without privileges
- Child cgroups cannot exceed parent limits
- Cgroup namespace hides parent cgroups

## Implementation Phases

### Phase 1: Cgroup Core (Week 1-2)
- [ ] Create kernel/container/cgroup crate
- [ ] Implement cgroup hierarchy
- [ ] Add cgroup field to ProcessMeta
- [ ] Mount cgroupfs at /sys/fs/cgroup

### Phase 2: Controllers (Week 3-4)
- [ ] Implement PIDs controller
- [ ] Implement CPU controller (weight + quota)
- [ ] Implement memory controller (max + high)
- [ ] Basic I/O controller

### Phase 3: Seccomp Strict (Week 5)
- [ ] Add seccomp_mode to ProcessMeta
- [ ] Implement strict mode
- [ ] Add prctl(PR_SET_SECCOMP)

### Phase 4: Seccomp Filter (Week 6-7)
- [ ] BPF instruction decoder
- [ ] BPF interpreter
- [ ] Filter installation via prctl()

### Phase 5: Integration (Week 8)
- [ ] Container test suite
- [ ] Performance tuning
- [ ] Documentation

## Testing

### Cgroup Tests
- Process movement between cgroups
- Memory limit enforcement (OOM on exceed)
- CPU quota throttling
- PID limit enforcement

### Seccomp Tests
- Strict mode blocks all but 4 syscalls
- Filter mode allows whitelisted syscalls
- Invalid filters rejected
- Filters persist across exec

## References

- Linux cgroups v2 documentation
- Linux seccomp(2) man page
- Docker seccomp profiles
- Kubernetes pod security policies
