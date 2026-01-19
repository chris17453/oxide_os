# EFFLUX Compatibility Runtimes Specification

**Version:** 1.0  
**Status:** Draft  
**License:** MIT  

---

## 0) Overview

EFFLUX provides sandboxed compatibility runtimes for legacy and scripting environments:

- **DOS 16-bit** — V86 mode for real-mode DOS programs
- **Python** — Native CPython port with sandboxing
- **Future** — Windows PE (Wine-like), Java, etc.

All runtimes operate in isolated sandboxes with controlled file access.

---

## 1) Sandbox Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Legacy/Script Application                              │
├─────────────────────────────────────────────────────────┤
│  Compatibility Runtime                                  │
│  ├── API Translation (DOS INT→syscall, etc.)           │
│  ├── Virtual Filesystem (sandbox view)                 │
│  └── Resource Limits                                   │
├─────────────────────────────────────────────────────────┤
│  Sandbox Container                                      │
│  ├── Namespace Isolation                               │
│  ├── Seccomp-like syscall filter                       │
│  └── COW Overlay Filesystem                            │
├─────────────────────────────────────────────────────────┤
│  EFFLUX Kernel                                          │
└─────────────────────────────────────────────────────────┘
```

---

## 2) Sandbox Isolation

### 2.1 File Access Modes

```rust
pub enum SandboxFileAccess {
    /// No file access
    None,
    
    /// Read-only access to specified paths
    ReadOnly(Vec<PathBuf>),
    
    /// Copy-on-write: reads from original, writes to overlay
    CopyOnWrite {
        lower: Vec<PathBuf>,    // Original files (read-only)
        upper: PathBuf,         // Overlay for writes
    },
    
    /// Full read-write to specified paths
    ReadWrite(Vec<PathBuf>),
}
```

### 2.2 Sandbox Configuration

```rust
pub struct SandboxConfig {
    pub name: String,
    pub runtime: RuntimeType,
    
    // Filesystem
    pub file_access: SandboxFileAccess,
    pub home_dir: PathBuf,
    pub tmp_dir: PathBuf,
    
    // Resources
    pub max_memory: usize,
    pub max_cpu_percent: u8,
    pub max_processes: u32,
    pub max_open_files: u32,
    pub max_disk_write: usize,
    
    // Network
    pub network_access: NetworkAccess,
    
    // Permissions
    pub allow_exec: bool,
    pub allow_ptrace: bool,
    pub allow_raw_io: bool,
    
    // Lifetime
    pub persist_overlay: bool,  // Keep writes after exit
    pub timeout: Option<Duration>,
}

pub enum NetworkAccess {
    None,
    LocalhostOnly,
    AllowList(Vec<String>),  // Domains/IPs
    Full,
}
```

### 2.3 COW Overlay Filesystem

```
Sandbox view:     /home/user/file.txt
                        │
                        ▼
                  ┌──────────┐
                  │  Overlay │
                  │  Layer   │──────► Write here
                  └──────────┘
                        │ (fallthrough if not found)
                        ▼
                  ┌──────────┐
                  │  Lower   │
                  │  Layer   │──────► Original (read-only)
                  └──────────┘

Storage:
/var/efflux/sandbox/<sandbox-id>/
├── upper/          # Writes go here
├── work/           # Overlay workdir
└── merged/         # Union mount point
```

---

## 3) DOS 16-bit Runtime (V86 Mode)

### 3.1 Overview

x86_64 does not support V86 mode directly. Options:

| Approach | Performance | Complexity | Chosen |
|----------|-------------|------------|--------|
| Full emulation (DOSBox) | Slow | High | No |
| V86 in 32-bit compat | Fast | Medium | **Yes** |
| Hardware VM (VT-x) | Fast | High | Future |

**Implementation:** Use 32-bit compatibility mode with V86 task for 16-bit real mode code.

### 3.2 Architecture

```
┌─────────────────────────────────────────────────────────┐
│  DOS Program (16-bit real mode)                         │
├─────────────────────────────────────────────────────────┤
│  V86 Monitor (32-bit kernel mode)                       │
│  ├── Trap INT instructions                             │
│  ├── Emulate sensitive instructions                    │
│  └── Translate DOS calls to EFFLUX syscalls            │
├─────────────────────────────────────────────────────────┤
│  DOS Environment                                        │
│  ├── Virtual memory map (640K conventional)            │
│  ├── Emulated devices (video, keyboard, disk)          │
│  └── DOS data structures (PSP, MCB, etc.)              │
├─────────────────────────────────────────────────────────┤
│  EFFLUX Kernel                                          │
└─────────────────────────────────────────────────────────┘
```

### 3.3 Memory Map

```
0x00000 - 0x003FF   Interrupt Vector Table (IVT)
0x00400 - 0x004FF   BIOS Data Area (BDA)
0x00500 - 0x9FFFF   Conventional Memory (640K)
0xA0000 - 0xBFFFF   Video Memory (VGA)
0xC0000 - 0xEFFFF   ROM Area (unused/emulated)
0xF0000 - 0xFFFFF   BIOS ROM (emulated)
```

### 3.4 Interrupt Handling

DOS programs use INT instructions for system calls:

| INT | Handler | Translation |
|-----|---------|-------------|
| 0x10 | Video BIOS | → framebuffer ops |
| 0x13 | Disk BIOS | → file ops |
| 0x16 | Keyboard BIOS | → input events |
| 0x21 | DOS | → file/process ops |
| 0x33 | Mouse | → mouse input |

### 3.5 INT 21h (DOS) Function Translation

```rust
pub fn handle_int21(regs: &mut V86Regs, sandbox: &Sandbox) -> Result<()> {
    match regs.ah {
        // Character I/O
        0x01 => dos_read_char_echo(regs, sandbox),
        0x02 => dos_write_char(regs, sandbox),
        0x09 => dos_write_string(regs, sandbox),
        0x0A => dos_buffered_input(regs, sandbox),
        
        // File operations
        0x3C => dos_create_file(regs, sandbox),
        0x3D => dos_open_file(regs, sandbox),
        0x3E => dos_close_file(regs, sandbox),
        0x3F => dos_read_file(regs, sandbox),
        0x40 => dos_write_file(regs, sandbox),
        0x41 => dos_delete_file(regs, sandbox),
        0x42 => dos_seek_file(regs, sandbox),
        0x43 => dos_get_set_attr(regs, sandbox),
        
        // Directory operations
        0x39 => dos_mkdir(regs, sandbox),
        0x3A => dos_rmdir(regs, sandbox),
        0x3B => dos_chdir(regs, sandbox),
        0x47 => dos_getcwd(regs, sandbox),
        0x4E => dos_find_first(regs, sandbox),
        0x4F => dos_find_next(regs, sandbox),
        
        // Process control
        0x00 => dos_terminate(regs, sandbox),
        0x4B => dos_exec(regs, sandbox),
        0x4C => dos_exit(regs, sandbox),
        0x4D => dos_get_return_code(regs, sandbox),
        
        // Memory management
        0x48 => dos_alloc_memory(regs, sandbox),
        0x49 => dos_free_memory(regs, sandbox),
        0x4A => dos_resize_memory(regs, sandbox),
        
        // Misc
        0x25 => dos_set_vector(regs, sandbox),
        0x2A => dos_get_date(regs, sandbox),
        0x2C => dos_get_time(regs, sandbox),
        0x30 => dos_get_version(regs, sandbox),
        0x35 => dos_get_vector(regs, sandbox),
        
        _ => Err(Error::UnsupportedDosFunction(regs.ah)),
    }
}
```

### 3.6 Path Translation

DOS paths translated to sandbox paths:

```
DOS:      C:\GAMES\DOOM\DOOM.EXE
Sandbox:  /sandbox/dos/drives/c/games/doom/doom.exe
Actual:   /var/efflux/sandbox/<id>/upper/drives/c/games/doom/doom.exe
          (or lower layer if not modified)
```

```rust
pub fn translate_dos_path(dos_path: &str, sandbox: &Sandbox) -> Result<PathBuf> {
    let (drive, path) = parse_dos_path(dos_path)?;
    let drive_mount = sandbox.get_drive_mount(drive)?;
    
    // Convert backslashes, lowercase
    let unix_path = path.replace('\\', "/").to_lowercase();
    
    // Check sandbox permissions
    let full_path = drive_mount.join(&unix_path);
    sandbox.check_access(&full_path)?;
    
    Ok(full_path)
}
```

### 3.7 Video Emulation

| Mode | Resolution | Type | Implementation |
|------|------------|------|----------------|
| 0x03 | 80x25 | Text | Terminal emulation |
| 0x13 | 320x200 | 256-color | Framebuffer |
| VESA | Various | Various | Framebuffer |

Text mode maps to PTY. Graphics modes render to virtual framebuffer, composited to display.

### 3.8 CLI

```bash
# Run DOS program
efflux dos run DOOM.EXE [args...]

# Run with specific drive mappings
efflux dos run --drive C:/path/to/c --drive D:/path/to/d GAME.EXE

# Configure sandbox
efflux dos config --memory 16M --sound-blaster

# List available DOS environments
efflux dos list

# Create DOS environment
efflux dos create --name "Games" --base freedos
```

---

## 4) Python Runtime

### 4.1 Overview

Native CPython port compiled for EFFLUX, running in sandbox.

**NOT emulated** — full native performance, just restricted.

### 4.2 Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Python Script                                          │
├─────────────────────────────────────────────────────────┤
│  CPython Interpreter (native EFFLUX build)             │
│  ├── Standard library                                  │
│  ├── pip packages (sandboxed)                          │
│  └── EFFLUX-specific modules                           │
├─────────────────────────────────────────────────────────┤
│  Sandbox Container                                      │
│  ├── Restricted syscalls                               │
│  ├── COW filesystem                                    │
│  └── Resource limits                                   │
├─────────────────────────────────────────────────────────┤
│  EFFLUX Kernel                                          │
└─────────────────────────────────────────────────────────┘
```

### 4.3 Sandbox Restrictions

```rust
pub fn python_sandbox_policy() -> SandboxConfig {
    SandboxConfig {
        name: "python".into(),
        runtime: RuntimeType::Python,
        
        file_access: SandboxFileAccess::CopyOnWrite {
            lower: vec![
                PathBuf::from("/usr/lib/python3"),
                PathBuf::from("/usr/share/python3"),
            ],
            upper: PathBuf::from("/var/efflux/sandbox/python/upper"),
        },
        
        max_memory: 512 * 1024 * 1024,  // 512 MB
        max_cpu_percent: 100,
        max_processes: 16,
        max_open_files: 256,
        max_disk_write: 100 * 1024 * 1024,  // 100 MB
        
        network_access: NetworkAccess::None,  // Default no network
        
        allow_exec: false,
        allow_ptrace: false,
        allow_raw_io: false,
        
        persist_overlay: false,
        timeout: None,
    }
}
```

### 4.4 Syscall Filter

Allowed syscalls for Python sandbox:

```rust
pub fn python_syscall_whitelist() -> Vec<Syscall> {
    vec![
        // Basic I/O
        Syscall::Read,
        Syscall::Write,
        Syscall::Open,
        Syscall::Close,
        Syscall::Stat,
        Syscall::Fstat,
        Syscall::Lstat,
        Syscall::Lseek,
        
        // Memory
        Syscall::Mmap,
        Syscall::Munmap,
        Syscall::Mprotect,
        Syscall::Brk,
        
        // Process (limited)
        Syscall::Getpid,
        Syscall::Exit,
        Syscall::ExitGroup,
        
        // Time
        Syscall::ClockGettime,
        Syscall::Nanosleep,
        
        // Signals (limited)
        Syscall::RtSigaction,
        Syscall::RtSigprocmask,
        
        // Directory
        Syscall::Getcwd,
        Syscall::Getdents64,
        
        // File metadata
        Syscall::Access,
        Syscall::Readlink,
        
        // Threading
        Syscall::Clone,  // With restrictions
        Syscall::Futex,
        Syscall::SetTidAddress,
        
        // Misc
        Syscall::Getrandom,
        Syscall::Uname,
    ]
}
```

### 4.5 EFFLUX Python Modules

```python
# efflux.sandbox - Query sandbox environment
import efflux.sandbox
print(efflux.sandbox.is_sandboxed())    # True
print(efflux.sandbox.get_limits())      # {'memory': 536870912, ...}
print(efflux.sandbox.request_network()) # Request network access (prompts user)

# efflux.trust - File trust operations
import efflux.trust
efflux.trust.verify('/path/to/file')    # Verify signature
efflux.trust.is_trusted('/path/to/file') # Check trust level

# efflux.search - AI search
import efflux.search
results = efflux.search.semantic("documents about quarterly reports")
results = efflux.search.similar("/path/to/file")
```

### 4.6 CLI

```bash
# Run Python script (sandboxed)
efflux python script.py [args...]

# Run with network access
efflux python --network script.py

# Run with file access
efflux python --read /data --write /output script.py

# Interactive REPL
efflux python

# Install packages (to sandbox)
efflux python -m pip install numpy

# Run unsandboxed (requires trust)
efflux python --no-sandbox script.py
```

### 4.7 Trust Elevation

Scripts can request elevated permissions:

```python
#!/usr/bin/env python
# efflux:require network
# efflux:require read /etc/config
# efflux:require write /var/data

import efflux.sandbox
if not efflux.sandbox.check_permissions():
    print("This script requires additional permissions")
    sys.exit(1)
```

User prompted to approve before execution.

---

## 5) External File Handling

### 5.1 File Source Detection

```rust
pub enum FileSource {
    Local,                          // Created on this system
    Network { host: String },       // Network share
    Removable { device: String },   // USB, etc.
    Download { url: String },       // Downloaded file
    Unknown,
}

pub fn detect_file_source(path: &Path) -> FileSource {
    // Check mount point type
    // Check extended attributes
    // Check download metadata
}
```

### 5.2 Automatic Access Modes

| Source | Default Mode | Elevation Path |
|--------|--------------|----------------|
| Local (trusted) | Full access | N/A |
| Local (untrusted) | Read-only | User approval |
| USB drive | Read-only | User approval |
| Network share | Read-only | User approval |
| Download | Quarantine | Inspect → Accept |

### 5.3 Mount Policies

```rust
pub struct MountPolicy {
    pub source_type: FileSource,
    pub default_mode: AccessMode,
    pub allow_elevation: bool,
    pub require_signature: bool,
    pub auto_scan: bool,
}

pub fn default_mount_policies() -> Vec<MountPolicy> {
    vec![
        MountPolicy {
            source_type: FileSource::Removable { device: "*".into() },
            default_mode: AccessMode::ReadOnly,
            allow_elevation: true,
            require_signature: false,
            auto_scan: true,  // Scan for threats
        },
        MountPolicy {
            source_type: FileSource::Network { host: "*".into() },
            default_mode: AccessMode::ReadOnly,
            allow_elevation: true,
            require_signature: false,
            auto_scan: false,
        },
    ]
}
```

### 5.4 Access Elevation Flow

```
┌─────────────────────────────────────────────────────────┐
│  User attempts write to external file                   │
└───────────────────────┬─────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────┐
│  Check current access mode                              │
│  Mode = ReadOnly                                        │
└───────────────────────┬─────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────┐
│  Prompt user:                                           │
│  "Allow write access to USB drive 'SANDISK'?"          │
│  [Allow Once] [Allow Session] [Allow Always] [Deny]    │
└───────────────────────┬─────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────┐
│  Store decision, update access mode                     │
└─────────────────────────────────────────────────────────┘
```

### 5.5 Executable Handling

External executables require explicit approval:

```rust
pub fn check_external_exec(path: &Path) -> ExecDecision {
    let source = detect_file_source(path);
    let trust = verify_signature(path);
    
    match (source, trust) {
        (FileSource::Local, VerifyResult::Valid { trust_level: TrustLevel::System, .. }) => {
            ExecDecision::Allow
        }
        (FileSource::Removable { .. }, _) => {
            ExecDecision::Prompt {
                message: "Execute program from external drive?",
                allow_remember: false,  // Always prompt for removable
            }
        }
        (FileSource::Download { .. }, VerifyResult::UnknownSigner) => {
            ExecDecision::Quarantine
        }
        _ => ExecDecision::Prompt {
            message: "Execute untrusted program?",
            allow_remember: true,
        }
    }
}
```

---

## 6) Syscalls for Sandboxing

```rust
// Create sandbox
pub fn sys_sandbox_create(config: *const SandboxConfig) -> Result<SandboxId>;

// Enter sandbox
pub fn sys_sandbox_enter(id: SandboxId) -> Result<()>;

// Exit sandbox
pub fn sys_sandbox_exit(id: SandboxId) -> Result<()>;

// Destroy sandbox
pub fn sys_sandbox_destroy(id: SandboxId) -> Result<()>;

// Query sandbox status
pub fn sys_sandbox_status(id: SandboxId, status: *mut SandboxStatus) -> Result<()>;

// Request permission elevation
pub fn sys_sandbox_request_perm(perm: SandboxPermission) -> Result<bool>;

// Mount in sandbox
pub fn sys_sandbox_mount(id: SandboxId, source: *const u8, target: *const u8,
                         mode: AccessMode) -> Result<()>;
```

---

## 7) CLI Tools

### efflux sandbox

```bash
# List sandboxes
efflux sandbox list

# Create sandbox
efflux sandbox create --name "dev" --config /etc/efflux/sandbox/dev.toml

# Run in sandbox
efflux sandbox run --name "dev" -- /bin/bash

# Show sandbox status
efflux sandbox status "dev"

# Destroy sandbox
efflux sandbox destroy "dev"

# Persist/discard overlay
efflux sandbox persist "dev"
efflux sandbox discard "dev"
```

### efflux mount (with policies)

```bash
# Mount with read-only (default for external)
efflux mount /dev/sdb1 /mnt/usb

# Mount with explicit mode
efflux mount --mode rw /dev/sdb1 /mnt/usb

# Mount network share
efflux mount //server/share /mnt/share --mode ro

# Show mount policies
efflux mount policy list

# Set policy
efflux mount policy set removable --default-mode ro
```

---

## 8) Implementation Phases

### Phase 1: Sandbox Framework
- [ ] Namespace isolation
- [ ] Syscall filtering
- [ ] COW overlay filesystem
- [ ] Resource limits

### Phase 2: External Media
- [ ] Source detection
- [ ] Mount policies
- [ ] Access elevation UI
- [ ] Executable blocking

### Phase 3: DOS Runtime
- [ ] V86 monitor
- [ ] INT 21h translation
- [ ] Basic video (text mode)
- [ ] File access through sandbox

### Phase 4: Python Runtime
- [ ] CPython port
- [ ] Sandbox integration
- [ ] EFFLUX modules
- [ ] pip support

### Phase 5: Advanced DOS
- [ ] Graphics modes
- [ ] Sound (Sound Blaster emulation)
- [ ] Mouse
- [ ] Extended memory (XMS/EMS)

---

## 9) Exit Criteria

### Sandbox
- [ ] Processes isolated in namespaces
- [ ] Syscalls filtered correctly
- [ ] COW overlay works
- [ ] Resource limits enforced

### DOS
- [ ] DOS games run (e.g., DOOM, Commander Keen)
- [ ] File access restricted to sandbox
- [ ] Text and graphics modes work

### Python
- [ ] Python scripts run natively
- [ ] pip packages install in sandbox
- [ ] Network blocked by default
- [ ] Elevation flow works

### External Media
- [ ] USB mounts read-only by default
- [ ] User can elevate to read-write
- [ ] Executables blocked until approved

---

*End of EFFLUX Compatibility Runtimes Specification*
