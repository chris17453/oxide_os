# Container Namespace Model

## Overview

OXIDE OS implements Linux-compatible namespaces for process isolation, enabling lightweight containerization without requiring a full container runtime. This document describes the namespace architecture and implementation strategy.

## Namespace Types

### Supported Namespaces

1. **PID Namespace** (CLONE_NEWPID)
   - Isolates process ID number space
   - Container init becomes PID 1 inside namespace
   - Cannot see or signal processes outside namespace

2. **Mount Namespace** (CLONE_NEWNS)
   - Isolates filesystem mount points
   - Private view of filesystem hierarchy
   - Essential for container root filesystem isolation

3. **Network Namespace** (CLONE_NEWNET)
   - Isolates network devices, IP addresses, routing tables
   - Each namespace has its own loopback device
   - Requires veth pairs or bridge for inter-namespace communication

4. **UTS Namespace** (CLONE_NEWUTS)
   - Isolates hostname and domain name
   - Allows containers to have distinct hostnames

5. **IPC Namespace** (CLONE_NEWIPC)
   - Isolates System V IPC objects (message queues, semaphores, shared memory)
   - Prevents interference between containers

6. **User Namespace** (CLONE_NEWUSER)
   - Isolates user and group IDs
   - Enables unprivileged containers (map UID 0 inside to non-root outside)
   - Most complex namespace type

7. **Cgroup Namespace** (CLONE_NEWCGROUP)
   - Isolates cgroup hierarchy view
   - Prevents containers from escaping resource limits

## Architecture

### Namespace Structure

```rust
/// Namespace reference - shared between processes
pub struct Namespace {
    /// Namespace type
    ns_type: NamespaceType,
    /// Unique namespace ID
    ns_id: u64,
    /// Reference count
    refcount: AtomicUsize,
    /// Type-specific data
    data: NamespaceData,
}

enum NamespaceData {
    Pid(PidNamespace),
    Mount(MountNamespace),
    Net(NetNamespace),
    Uts(UtsNamespace),
    Ipc(IpcNamespace),
    User(UserNamespace),
    Cgroup(CgroupNamespace),
}
```

### Process Namespace Tracking

Each process (ProcessMeta) contains namespace pointers:

```rust
pub struct ProcessMeta {
    // ... existing fields ...
    
    /// Namespace memberships
    pub namespaces: NamespaceSet,
}

pub struct NamespaceSet {
    pub pid: Arc<Namespace>,
    pub mount: Arc<Namespace>,
    pub net: Arc<Namespace>,
    pub uts: Arc<Namespace>,
    pub ipc: Arc<Namespace>,
    pub user: Arc<Namespace>,
    pub cgroup: Arc<Namespace>,
}
```

### Initialization

At boot, kernel creates initial namespaces:

```rust
pub fn init_namespaces() {
    // Create root namespaces
    let init_pid = Namespace::new_pid();
    let init_mount = Namespace::new_mount();
    let init_net = Namespace::new_net();
    let init_uts = Namespace::new_uts("oxide");
    let init_ipc = Namespace::new_ipc();
    let init_user = Namespace::new_user();
    let init_cgroup = Namespace::new_cgroup();
    
    // Store in global init namespace set
    unsafe {
        INIT_NAMESPACES = Some(NamespaceSet {
            pid: Arc::new(init_pid),
            mount: Arc::new(init_mount),
            net: Arc::new(init_net),
            uts: Arc::new(init_uts),
            ipc: Arc::new(init_ipc),
            user: Arc::new(init_user),
            cgroup: Arc::new(init_cgroup),
        });
    }
}
```

## Syscall Implementation

### unshare(flags)

Creates new namespaces for calling process without spawning child:

```rust
pub fn sys_unshare(flags: i32) -> i64 {
    let current = get_current_meta()?;
    let mut new_ns = current.lock().namespaces.clone();
    
    if flags & CLONE_NEWPID != 0 {
        new_ns.pid = Arc::new(Namespace::new_pid());
    }
    if flags & CLONE_NEWNS != 0 {
        new_ns.mount = Arc::new(Namespace::new_mount_from(&current.lock().namespaces.mount));
    }
    // ... other namespace types ...
    
    current.lock().namespaces = new_ns;
    Ok(0)
}
```

### setns(fd, nstype)

Joins existing namespace via /proc/<pid>/ns/<type> fd:

```rust
pub fn sys_setns(fd: i32, nstype: i32) -> i64 {
    // 1. Resolve fd to Namespace object
    let file = get_current_meta()?.lock().fd_table.get(fd)?;
    let namespace = file.as_namespace()?;
    
    // 2. Verify nstype matches (or 0 = any)
    if nstype != 0 && nstype != namespace.ns_type as i32 {
        return Err(errno::EINVAL);
    }
    
    // 3. Update current process namespace membership
    let current = get_current_meta()?;
    match namespace.ns_type {
        NamespaceType::Pid => current.lock().namespaces.pid = namespace.clone(),
        NamespaceType::Mount => current.lock().namespaces.mount = namespace.clone(),
        // ... other types ...
    }
    
    Ok(0)
}
```

### clone3(args)

Extended clone with namespace and pidfd support:

```rust
pub fn sys_clone3(args_ptr: u64, size: usize) -> i64 {
    let args = read_clone3_args(args_ptr, size)?;
    
    // Create child with requested namespace isolation
    let child_ns = if args.flags & CLONE_NEWPID != 0 {
        create_new_namespaces(args.flags)
    } else {
        parent.namespaces.clone()
    };
    
    // Create child process
    let child_pid = fork_with_namespaces(child_ns)?;
    
    // Write pidfd if requested
    if args.pidfd != 0 {
        let pidfd = create_pidfd(child_pid)?;
        write_to_user(args.pidfd, &pidfd)?;
    }
    
    Ok(child_pid)
}
```

## PID Namespace Details

### PID Translation

PIDs are namespace-local. Translation needed for:
- getpid() returns PID within namespace
- kill(pid) operates within namespace
- /proc shows only namespace-local PIDs

```rust
pub struct PidNamespace {
    /// Parent namespace (None for init)
    parent: Option<Arc<Namespace>>,
    /// PID allocator for this namespace
    pid_allocator: Mutex<PidAllocator>,
    /// Map: global PID -> namespace-local PID
    pid_map: Mutex<HashMap<Pid, Pid>>,
}

impl PidNamespace {
    /// Allocate new PID within this namespace
    pub fn alloc_pid(&self, global_pid: Pid) -> Pid {
        let local = self.pid_allocator.lock().alloc();
        self.pid_map.lock().insert(global_pid, local);
        local
    }
    
    /// Translate global PID to namespace-local
    pub fn translate(&self, global_pid: Pid) -> Option<Pid> {
        self.pid_map.lock().get(&global_pid).copied()
    }
}
```

### Init Process

PID 1 in each namespace:
- Cannot be killed by normal signals
- Reaps orphaned processes within namespace
- Namespace dies when init exits

## Mount Namespace Details

### Private Mount Table

Each mount namespace has independent mount table:

```rust
pub struct MountNamespace {
    /// Mount table (path -> mounted filesystem)
    mounts: Mutex<BTreeMap<String, Arc<dyn Filesystem>>>,
    /// Root inode
    root: Arc<dyn VnodeOps>,
}

impl MountNamespace {
    /// Clone mount namespace (for CLONE_NEWNS)
    pub fn clone_from(parent: &MountNamespace) -> Self {
        let mounts = parent.mounts.lock().clone();
        MountNamespace {
            mounts: Mutex::new(mounts),
            root: parent.root.clone(),
        }
    }
}
```

### Path Resolution

VFS path resolution checks current mount namespace:

```rust
pub fn lookup(path: &str) -> VfsResult<Arc<dyn VnodeOps>> {
    let ns = get_current_meta()?.lock().namespaces.mount.clone();
    ns.lookup_internal(path)
}
```

## Network Namespace Details

### Isolated Network Stack

Each network namespace has:
- Separate network devices
- Independent routing table
- Own IP addresses
- Private firewall rules

```rust
pub struct NetNamespace {
    /// Network interfaces
    interfaces: Mutex<Vec<Arc<NetworkInterface>>>,
    /// Routing table
    routes: Mutex<RoutingTable>,
    /// Firewall rules
    firewall: Arc<Firewall>,
}
```

### Loopback Device

Each namespace gets its own loopback (lo):

```rust
impl NetNamespace {
    pub fn new() -> Self {
        let mut interfaces = Vec::new();
        interfaces.push(Arc::new(NetworkInterface::loopback()));
        NetNamespace {
            interfaces: Mutex::new(interfaces),
            routes: Mutex::new(RoutingTable::new()),
            firewall: Arc::new(Firewall::new()),
        }
    }
}
```

## User Namespace Details

### UID/GID Mapping

Most powerful for unprivileged containers:

```rust
pub struct UserNamespace {
    /// Parent namespace
    parent: Option<Arc<Namespace>>,
    /// UID mappings (inside -> outside)
    uid_map: Mutex<Vec<UidMapEntry>>,
    /// GID mappings (inside -> outside)
    gid_map: Mutex<Vec<GidMapEntry>>,
    /// Owner UID in parent namespace
    owner_uid: u32,
}

pub struct UidMapEntry {
    inside_start: u32,
    outside_start: u32,
    count: u32,
}
```

### Capability Sets

User namespace provides capability isolation:
- Root inside namespace has capabilities within that namespace
- No capabilities in parent namespace

## /proc Integration

### Namespace Files

Each process exposes namespace via /proc:

```
/proc/<pid>/ns/
├── pid      -> pid:[4026531836]
├── mnt      -> mnt:[4026531840]
├── net      -> net:[4026531956]
├── uts      -> uts:[4026531838]
├── ipc      -> ipc:[4026531839]
├── user     -> user:[4026531837]
└── cgroup   -> cgroup:[4026531835]
```

Implementation:

```rust
// In procfs
pub fn ns_readlink(pid: Pid, ns_type: &str) -> String {
    let proc = get_process(pid)?;
    let ns = match ns_type {
        "pid" => &proc.namespaces.pid,
        "mnt" => &proc.namespaces.mount,
        // ...
    };
    format!("{}:[{}]", ns_type, ns.ns_id)
}
```

### Opening Namespace Files

Opening creates reference to namespace:

```rust
pub fn ns_open(pid: Pid, ns_type: &str) -> Result<Arc<File>> {
    let proc = get_process(pid)?;
    let ns = get_namespace(proc, ns_type)?;
    
    // Create special file that holds namespace reference
    Ok(Arc::new(File::Namespace(ns)))
}
```

## Security Considerations

### Permission Checks

- unshare() requires CAP_SYS_ADMIN (except CLONE_NEWUSER)
- setns() requires CAP_SYS_ADMIN or same user namespace
- User namespace: anyone can create, but limited nesting

### Resource Limits

- Maximum namespace nesting depth: 32
- Maximum namespaces per process: 64
- Maximum total namespaces: 65536

### Escape Prevention

- PID 1 cannot be killed from outside namespace
- Mount namespace prevents ../ escapes via pivot_root
- Network namespace isolates packet visibility
- User namespace cannot escalate parent privileges

## Implementation Phases

### Phase 1: Core Infrastructure (Week 1-2)
- [ ] Create kernel/container/namespace crate
- [ ] Define Namespace and NamespaceSet structures
- [ ] Add namespace fields to ProcessMeta
- [ ] Initialize root namespaces at boot

### Phase 2: PID Namespace (Week 3)
- [ ] Implement PID translation
- [ ] Handle init process special cases
- [ ] Update getpid()/kill() to use namespace PIDs

### Phase 3: Mount Namespace (Week 4)
- [ ] Per-namespace mount tables
- [ ] Update VFS path resolution
- [ ] Implement mount propagation

### Phase 4: Syscalls (Week 5)
- [ ] Implement unshare()
- [ ] Implement setns()
- [ ] Update clone() for clone3()

### Phase 5: /proc Integration (Week 6)
- [ ] Add /proc/<pid>/ns/ entries
- [ ] Implement namespace file opening
- [ ] Add pidfd support

### Phase 6: Advanced Namespaces (Week 7-8)
- [ ] Network namespace
- [ ] User namespace with UID mapping
- [ ] IPC and cgroup namespaces

## Testing Strategy

### Unit Tests
- Namespace creation and reference counting
- PID translation
- UID/GID mapping

### Integration Tests
- unshare() creates isolated process
- setns() joins existing namespace
- /proc/<pid>/ns/ links work correctly

### Container Tests
- Run busybox in isolated namespace
- Verify filesystem isolation
- Verify network isolation
- Test nested containers

## References

- Linux namespaces(7) man page
- Linux kernel namespaces implementation
- Docker/runc namespace usage
- LXC container documentation
