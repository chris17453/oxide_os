# EFFLUX Operating System — Master Specification

**Version:** 1.0
**Status:** Draft
**Project Name:** EFFLUX
**License:** MIT
**Targets:** x86_64, i686, AArch64, ARM32, MIPS64, MIPS32, RISC-V 64/32
**Boot:** UEFI, BIOS, ARCS (SGI), OpenSBI, U-Boot, bare
**Kernel Model:** Monolithic + Loadable Modules  

---

## 0) Project Principles

1. **Arch-agnostic core.** All arch-specific code lives in `kernel/arch/{x86_64,i686,aarch64,arm,mips64,mips32,riscv64,riscv32}/`. Core kernel code uses traits.
2. **No piecemeal integration.** Each phase has defined interfaces. Later phases consume earlier interfaces without rewriting.
3. **Test before proceed.** Each phase has exit criteria. Don't start phase N+1 until phase N passes.
4. **Rust everywhere.** Kernel, drivers, libc, userland. No C except where absolutely unavoidable (firmware blobs).
5. **POSIX-ish, not POSIX.** Source compatible. Clean syscall interface. Apps recompile.
6. **Loadable modules.** Drivers load/unload at runtime without kernel rebuild.
7. **AI-native design.** Semantic search, embeddings, and intelligent metadata are first-class citizens.
8. **Security by default.** File signing, encryption, and trust verification built into the filesystem.

---

## 1) Target Platforms

### Primary (64-bit)

| Platform | Arch | Boot | Test Environment |
|----------|------|------|------------------|
| QEMU x86_64 | x86_64 | UEFI, BIOS+PXE | Primary dev target |
| QEMU aarch64 virt | AArch64 | UEFI | Primary dev target |
| QEMU RISC-V 64 virt | riscv64 | OpenSBI | Primary dev target |
| QEMU MIPS64 malta | mips64 | YAMON/bare | Primary dev target |
| Bare metal x86_64 | x86_64 | UEFI, PXE | Secondary |
| Raspberry Pi 3/4/5 | AArch64 | UEFI (via U-Boot) | Secondary |
| SGI Indy/Indigo2/O2 | MIPS64 | ARCS | Secondary (retro) |
| SGI Octane/Origin | MIPS64 | ARCS | Secondary (retro) |
| Loongson 3A | MIPS64 | PMON/UEFI | Secondary |

### Secondary (32-bit)

| Platform | Arch | Boot | Test Environment |
|----------|------|------|------------------|
| QEMU i386 | i686 | BIOS, UEFI | Secondary |
| QEMU ARM virt | ARM32 | U-Boot | Secondary |
| QEMU RISC-V 32 | riscv32 | OpenSBI | Secondary |
| QEMU MIPS malta | mips32 | YAMON/bare | Secondary |
| Legacy PC hardware | i686 | BIOS | Compatibility |
| Embedded ARM boards | ARM32 | U-Boot/bare | Embedded |
| MIPS routers (OpenWrt) | mips32 | bare | Embedded |

### Tertiary (No-MMU / Embedded)

| Platform | Arch | Boot | Notes |
|----------|------|------|-------|
| ESP32-S3 | Xtensa | bare | No MMU, compile-time `nommu` feature |
| Cortex-M4/M7 | ARM32 | bare | MPU only, no MMU |
| RISC-V embedded | riscv32 | bare | Optional PMP |

**Dropped:** ARMv6 (too old), 16-bit (8086, Z80)

---

## 2) Repository Structure

```
efflux/
├── kernel/
│   ├── arch/
│   │   ├── mod.rs              # Arch trait definitions (Mmu, Tlb, Context, etc.)
│   │   ├── x86_64/             # 64-bit x86
│   │   │   ├── boot.rs         # UEFI/BIOS entry
│   │   │   ├── mm.rs           # 4/5-level page tables
│   │   │   ├── interrupt.rs    # IDT, APIC, IOAPIC
│   │   │   ├── context.rs      # Context switch
│   │   │   ├── syscall.rs      # syscall/sysret
│   │   │   └── timer.rs        # APIC timer, HPET
│   │   ├── i686/               # 32-bit x86
│   │   │   ├── boot.rs         # BIOS/UEFI entry
│   │   │   ├── mm.rs           # 2-level / PAE page tables
│   │   │   ├── interrupt.rs    # IDT, PIC/APIC
│   │   │   └── ...
│   │   ├── aarch64/            # 64-bit ARM
│   │   │   ├── boot.rs
│   │   │   ├── mm.rs           # TTBR0/TTBR1, 4KB/16KB/64KB granules
│   │   │   ├── interrupt.rs    # GIC, exception vectors
│   │   │   ├── context.rs
│   │   │   ├── syscall.rs      # svc handler
│   │   │   └── timer.rs        # Generic timer
│   │   ├── arm/                # 32-bit ARM
│   │   │   ├── boot.rs
│   │   │   ├── mm.rs           # 2-level page tables
│   │   │   └── ...
│   │   ├── mips64/             # 64-bit MIPS (SGI, Loongson)
│   │   │   ├── boot.rs         # ARCS/YAMON/bare entry
│   │   │   ├── mm.rs           # Software TLB refill
│   │   │   ├── cp0.rs          # Coprocessor 0 (TLB, exceptions)
│   │   │   ├── interrupt.rs    # MIPS interrupt controller
│   │   │   └── platform/       # Platform-specific (SGI, Malta, Loongson)
│   │   │       ├── sgi.rs      # SGI Indy/O2/Octane
│   │   │       └── malta.rs    # QEMU Malta
│   │   ├── mips32/             # 32-bit MIPS
│   │   │   ├── boot.rs
│   │   │   ├── mm.rs           # Software TLB
│   │   │   └── ...
│   │   ├── riscv64/            # 64-bit RISC-V
│   │   │   ├── boot.rs         # OpenSBI entry
│   │   │   ├── mm.rs           # Sv39/Sv48/Sv57
│   │   │   ├── interrupt.rs    # PLIC, CLINT
│   │   │   └── ...
│   │   └── riscv32/            # 32-bit RISC-V
│   │       ├── boot.rs
│   │       ├── mm.rs           # Sv32
│   │       └── ...
│   ├── core/
│   │   ├── mm/                 # Memory management
│   │   ├── sched/              # Scheduler
│   │   ├── ipc/                # IPC primitives
│   │   ├── syscall/            # Syscall dispatch
│   │   ├── vfs/                # Virtual filesystem
│   │   ├── net/                # Network stack
│   │   ├── module/             # Module loader
│   │   ├── crypto/             # Encryption, signing, hashing
│   │   ├── trust/              # Certificate management, trust store
│   │   └── security/           # Capabilities, namespaces
│   ├── drivers/
│   │   ├── tty/                # TTY, PTY, serial
│   │   ├── input/              # Keyboard, mouse, HID
│   │   ├── gpu/                # Framebuffer, virtio-gpu, Intel, AMD, NVIDIA
│   │   ├── storage/            # NVMe, virtio-blk, AHCI
│   │   ├── net/                # virtio-net, Intel e1000, etc.
│   │   ├── audio/              # virtio-snd, HDA
│   │   └── usb/                # USB host controller, HID
│   └── lib.rs
├── fs/
│   ├── effluxfs/               # Native EFFLUX filesystem
│   ├── fat32/                  # FAT32 driver
│   ├── devfs/                  # Device filesystem
│   ├── procfs/                 # Process filesystem
│   └── tmpfs/                  # RAM filesystem
├── libc/                       # Custom libc
├── userland/
│   ├── init/
│   ├── login/
│   ├── shell/
│   ├── coreutils/
│   └── tools/           # Trust, signing, quarantine CLI
├── indexer/                    # AI indexing daemon
│   ├── embeddings/             # Embedding models (Candle)
│   ├── search/                 # Vector search, HNSW
│   └── daemon/                 # indexd
├── bootloader/
│   ├── uefi/                   # UEFI bootloader (x86_64, aarch64)
│   ├── bios/                   # Legacy BIOS (i686, x86_64)
│   ├── opensbi/                # OpenSBI payload (riscv)
│   ├── arcs/                   # ARCS loader (mips64 SGI)
│   └── uboot/                  # U-Boot integration (arm, embedded)
└── tools/
    ├── mkfs.efflux/            # Filesystem creation
    ├── fsck.efflux/            # Filesystem check
    └── pxe/                    # PXE boot server config
```

---

## 3) Arch Abstraction Traits

All arch-specific code implements these traits. Core kernel only uses trait methods.

```rust
pub trait Arch {
    fn name() -> &'static str;
    fn page_size() -> usize;
    fn kernel_base() -> usize;
}

pub trait Mmu {
    type PageTable;
    type Entry;
    
    fn new_page_table() -> Self::PageTable;
    fn map_page(pt: &mut Self::PageTable, va: usize, pa: usize, flags: MapFlags) -> Result<()>;
    fn unmap_page(pt: &mut Self::PageTable, va: usize) -> Result<()>;
    fn protect_page(pt: &mut Self::PageTable, va: usize, flags: MapFlags) -> Result<()>;
    fn walk(pt: &Self::PageTable, va: usize) -> Option<(usize, MapFlags)>;
    fn activate(pt: &Self::PageTable);
    fn invalidate_page(va: usize);
    fn invalidate_all();
}

pub trait InterruptController {
    fn init();
    fn enable_irq(irq: u32);
    fn disable_irq(irq: u32);
    fn ack_irq(irq: u32);
    fn send_ipi(cpu: u32, vector: u8);
    fn set_irq_handler(irq: u32, handler: fn());
}

pub trait Timer {
    fn init(freq_hz: u32);
    fn current_ticks() -> u64;
    fn set_oneshot(ticks: u64);
    fn ack();
}

pub trait Context {
    type Registers;
    
    fn new_kernel(entry: fn(), stack: usize) -> Self::Registers;
    fn new_user(entry: usize, stack: usize, arg: usize) -> Self::Registers;
    fn switch(old: &mut Self::Registers, new: &Self::Registers);
}

pub trait Syscall {
    fn init();
    fn return_to_user(regs: &Context::Registers) -> !;
}

pub trait Serial {
    fn init();
    fn putc(c: u8);
    fn getc() -> Option<u8>;
}
```

---

## 4) Phase 0: Boot + Serial Output

**Goal:** Boot to Rust, print to serial on both arches.

### 4.1 Deliverables
- Custom UEFI bootloader loads kernel ELF
- Kernel entry point runs on BSP
- Serial output functional
- Basic panic handler prints and halts

### 4.2 x86_64 Specifics
- UEFI GOP for early framebuffer (optional)
- UEFI memory map passed to kernel
- Kernel loaded at high address (e.g., `0xFFFF_8000_0010_0000`)
- COM1 (`0x3F8`) for serial

### 4.3 AArch64 Specifics
- UEFI memory map passed to kernel
- Kernel loaded at high address (TTBR1 range)
- PL011 UART for serial (QEMU virt: `0x0900_0000`)

### 4.4 PXE Boot
- UEFI PXE loads kernel from TFTP
- Same kernel image works for disk and PXE

### 4.5 Exit Criteria
- [ ] Boots on QEMU x86_64 (UEFI)
- [ ] Boots on QEMU aarch64 (UEFI)
- [ ] Serial "Hello from EFFLUX" on both
- [ ] Panic prints backtrace and halts
- [ ] PXE boot works on x86_64

---

## 5) Phase 1: Memory Management

**Goal:** Virtual memory, kernel heap, frame allocator.

*Refer to existing VM spec for full details. Summary here.*

### 5.1 Deliverables
- Physical frame allocator (bump → buddy)
- Kernel page tables (higher-half + direct map)
- Kernel heap (slab allocator)
- MMU enabled on both arches

### 5.2 Address Space Layout

#### x86_64
| Range | Usage |
|-------|-------|
| `0x0000_0000_0000_0000 - 0x0000_7FFF_FFFF_FFFF` | User |
| `0xFFFF_8000_0000_0000 - 0xFFFF_8000_xxxx_xxxx` | Direct map |
| `0xFFFF_FFFF_8000_0000 - 0xFFFF_FFFF_xxxx_xxxx` | Kernel image |
| `0xFFFF_FFFF_C000_0000 - 0xFFFF_FFFF_xxxx_xxxx` | Vmalloc |

#### AArch64
| Range | Usage |
|-------|-------|
| `0x0000_0000_0000_0000 - 0x0000_FFFF_FFFF_FFFF` | User (TTBR0) |
| `0xFFFF_0000_0000_0000 - 0xFFFF_xxxx_xxxx_xxxx` | Kernel (TTBR1) |

### 5.3 Exit Criteria
- [ ] Frame allocator functional
- [ ] Kernel heap allocations work
- [ ] Direct map covers all RAM
- [ ] Page faults handled (panic for now)
- [ ] Works on both arches

---

## 6) Phase 2: Interrupts + Timer + Scheduler

**Goal:** Preemptive multitasking in kernel mode.

### 6.1 Deliverables
- Interrupt handlers installed
- Timer interrupt fires
- Kernel threads
- Preemptive round-robin scheduler
- Per-CPU run queues (ready for SMP)

### 6.2 Interrupt Handling

#### x86_64
- IDT with 256 entries
- APIC for local interrupts
- IOAPIC for external interrupts
- IST for double-fault, NMI, MCE

#### AArch64
- Exception vector table at `VBAR_EL1`
- GIC v2/v3 for interrupt distribution
- IRQ, FIQ, SError, Sync exception handling

### 6.3 Scheduler Design

```rust
pub struct Thread {
    pub tid: u64,
    pub state: ThreadState,
    pub priority: u8,
    pub cpu_affinity: Option<u32>,
    pub kernel_stack: usize,
    pub context: arch::Registers,
    pub address_space: Option<Arc<AddressSpace>>,
}

pub enum ThreadState {
    Running,
    Ready,
    Blocked,
    Zombie,
}
```

- Priority levels: 0-15 (0 = highest)
- Round-robin within priority
- Per-CPU run queues
- Work stealing for load balance (SMP phase)
- Time slice: 10ms default

### 6.4 Exit Criteria
- [ ] Timer interrupt fires at 100Hz+
- [ ] Multiple kernel threads run concurrently
- [ ] Context switch works
- [ ] Preemption works (thread doesn't hog CPU)
- [ ] Works on both arches

---

## 7) Phase 3: User Mode + Syscalls

**Goal:** Ring 3 execution, syscall interface.

### 7.1 Deliverables
- User address space creation
- ELF loader (static binaries)
- Ring 0 → Ring 3 transition
- Syscall entry/exit
- Basic syscalls: exit, write (serial)

### 7.2 Syscall Interface

#### x86_64
- `syscall` instruction
- RAX = syscall number
- RDI, RSI, RDX, R10, R8, R9 = args
- RAX = return value

#### AArch64
- `svc #0` instruction
- X8 = syscall number
- X0-X5 = args
- X0 = return value

### 7.3 Initial Syscalls

| Number | Name | Args | Description |
|--------|------|------|-------------|
| 0 | sys_exit | status | Exit process |
| 1 | sys_write | fd, buf, len | Write to fd |
| 2 | sys_read | fd, buf, len | Read from fd |

### 7.4 Exit Criteria
- [ ] User process runs in ring 3
- [ ] Syscall traps to kernel
- [ ] sys_exit terminates process
- [ ] sys_write to stdout works
- [ ] Works on both arches

---

## 8) Phase 4: Process Model + fork/exec

**Goal:** Full process lifecycle.

### 8.1 Deliverables
- Process structure with PID
- fork() with COW
- exec() loads new binary
- wait()/waitpid()
- Process groups, sessions

### 8.2 Process Structure

```rust
pub struct Process {
    pub pid: u64,
    pub ppid: u64,
    pub pgid: u64,
    pub sid: u64,
    pub state: ProcessState,
    pub address_space: Arc<AddressSpace>,
    pub threads: Vec<Arc<Thread>>,
    pub open_files: FileTable,
    pub cwd: PathBuf,
    pub uid: u32,
    pub gid: u32,
    pub exit_status: Option<i32>,
    pub children: Vec<u64>,
}
```

### 8.3 Syscalls Added

| Name | Description |
|------|-------------|
| sys_fork | Create child process (COW) |
| sys_exec | Replace process image |
| sys_wait | Wait for child |
| sys_getpid | Get PID |
| sys_getppid | Get parent PID |
| sys_setsid | Create session |
| sys_setpgid | Set process group |

### 8.4 Exit Criteria
- [ ] fork() creates child with COW
- [ ] exec() loads and runs new binary
- [ ] wait() reaps zombies
- [ ] Process hierarchy works
- [ ] Works on both arches

---

## 9) Phase 5: VFS + Initial Filesystems

**Goal:** Unified filesystem interface.

### 9.1 Deliverables
- VFS layer with vnode abstraction
- devfs (/dev)
- tmpfs (RAM-based)
- initramfs (cpio) loaded at boot
- procfs (/proc) - basic

### 9.2 VFS Design

```rust
pub trait Filesystem: Send + Sync {
    fn name(&self) -> &str;
    fn mount(&self, source: Option<&Path>, flags: MountFlags) -> Result<Arc<dyn Vnode>>;
}

pub trait Vnode: Send + Sync {
    fn stat(&self) -> Result<Stat>;
    fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize>;
    fn write(&self, offset: u64, buf: &[u8]) -> Result<usize>;
    fn lookup(&self, name: &str) -> Result<Arc<dyn Vnode>>;
    fn create(&self, name: &str, mode: Mode) -> Result<Arc<dyn Vnode>>;
    fn mkdir(&self, name: &str, mode: Mode) -> Result<Arc<dyn Vnode>>;
    fn unlink(&self, name: &str) -> Result<()>;
    fn readdir(&self) -> Result<Vec<DirEntry>>;
    fn ioctl(&self, cmd: u64, arg: usize) -> Result<usize>;
    fn mmap(&self, offset: u64, len: usize, prot: Prot) -> Result<*mut u8>;
}
```

### 9.3 Syscalls Added

| Name | Description |
|------|-------------|
| sys_open | Open file |
| sys_close | Close fd |
| sys_read | Read from fd |
| sys_write | Write to fd |
| sys_lseek | Seek in file |
| sys_stat | Get file status |
| sys_fstat | fstat |
| sys_mkdir | Create directory |
| sys_rmdir | Remove directory |
| sys_unlink | Remove file |
| sys_chdir | Change directory |
| sys_getcwd | Get current directory |
| sys_mount | Mount filesystem |
| sys_umount | Unmount filesystem |
| sys_dup | Duplicate fd |
| sys_dup2 | Duplicate fd to specific fd |
| sys_pipe | Create pipe |

### 9.4 Initial Device Nodes

| Path | Type | Description |
|------|------|-------------|
| /dev/null | char | Discard sink |
| /dev/zero | char | Zero source |
| /dev/random | char | Random bytes |
| /dev/console | char | System console |
| /dev/tty | char | Controlling TTY |

### 9.5 Exit Criteria
- [ ] open/read/write/close work
- [ ] initramfs mounts at boot
- [ ] /dev/null, /dev/zero work
- [ ] tmpfs mounts and works
- [ ] /proc/self/maps exists
- [ ] Works on both arches

---

## 10) Phase 6: TTY + PTY + Sessions

**Goal:** Terminal handling for interactive use.

### 10.1 Deliverables
- TTY line discipline
- PTY master/slave pairs
- Job control (SIGTSTP, SIGCONT, etc.)
- Session/foreground process group

### 10.2 TTY Structure

```rust
pub struct Tty {
    pub index: u32,
    pub termios: Termios,
    pub input_buffer: VecDeque<u8>,
    pub output_buffer: VecDeque<u8>,
    pub session: Option<u64>,      // SID
    pub foreground_pg: Option<u64>, // PGID
    pub winsize: Winsize,
}

pub struct Pty {
    pub master: Arc<PtyMaster>,
    pub slave: Arc<PtySlave>,
}
```

### 10.3 Syscalls Added

| Name | Description |
|------|-------------|
| sys_ioctl | TTY ioctls (TCGETS, TCSETS, TIOCGWINSZ, etc.) |
| sys_openpty | Open PTY pair |
| sys_tcgetpgrp | Get foreground PGID |
| sys_tcsetpgrp | Set foreground PGID |

### 10.4 Exit Criteria
- [ ] Serial console works as TTY
- [ ] PTY allocation works
- [ ] Line editing (canonical mode) works
- [ ] Ctrl+C sends SIGINT
- [ ] Works on both arches

---

## 11) Phase 7: Signals

**Goal:** POSIX-style signal delivery.

### 11.1 Deliverables
- Signal generation and delivery
- Signal handlers
- Signal masks
- sigaction, sigprocmask, kill

### 11.2 Signals Supported

| Signal | Default | Description |
|--------|---------|-------------|
| SIGHUP | Term | Hangup |
| SIGINT | Term | Interrupt |
| SIGQUIT | Core | Quit |
| SIGILL | Core | Illegal instruction |
| SIGTRAP | Core | Trace trap |
| SIGABRT | Core | Abort |
| SIGBUS | Core | Bus error |
| SIGFPE | Core | Floating point exception |
| SIGKILL | Term | Kill (uncatchable) |
| SIGSEGV | Core | Segmentation fault |
| SIGPIPE | Term | Broken pipe |
| SIGALRM | Term | Alarm clock |
| SIGTERM | Term | Termination |
| SIGCHLD | Ignore | Child status changed |
| SIGCONT | Cont | Continue |
| SIGSTOP | Stop | Stop (uncatchable) |
| SIGTSTP | Stop | Terminal stop |
| SIGTTIN | Stop | Background read |
| SIGTTOU | Stop | Background write |
| SIGUSR1 | Term | User defined |
| SIGUSR2 | Term | User defined |

### 11.3 Syscalls Added

| Name | Description |
|------|-------------|
| sys_kill | Send signal |
| sys_sigaction | Set signal handler |
| sys_sigprocmask | Block signals |
| sys_sigreturn | Return from signal |
| sys_pause | Wait for signal |
| sys_alarm | Set alarm |

### 11.4 Exit Criteria
- [ ] SIGKILL terminates process
- [ ] SIGINT delivered on Ctrl+C
- [ ] Custom signal handlers work
- [ ] Signal mask works
- [ ] Works on both arches

---

## 12) Phase 8: Libc + Init + Shell

**Goal:** Self-sufficient userland.

### 12.1 Deliverables
- Custom libc (musl-inspired API)
- init process (PID 1)
- login program
- Basic shell
- Core utilities: ls, cat, echo, mkdir, rm, cp, mv, pwd, env

### 12.2 Libc Components

| Header | Functions |
|--------|-----------|
| stdio.h | printf, scanf, fopen, fclose, fread, fwrite, fgets, fputs |
| stdlib.h | malloc, free, realloc, exit, getenv, setenv, atoi, rand |
| string.h | strlen, strcpy, strncpy, strcmp, strncmp, memcpy, memset, memmove |
| unistd.h | read, write, open, close, fork, exec*, pipe, dup, dup2, chdir, getcwd, sleep, getpid |
| fcntl.h | open flags, fcntl |
| sys/stat.h | stat, fstat, mkdir, chmod |
| sys/wait.h | wait, waitpid |
| signal.h | signal, sigaction, kill, raise |
| errno.h | errno, error codes |
| dirent.h | opendir, readdir, closedir |
| termios.h | tcgetattr, tcsetattr, cfmakeraw |
| sys/ioctl.h | ioctl |
| sys/mman.h | mmap, munmap, mprotect |
| time.h | time, gettimeofday, nanosleep |
| pthread.h | pthread_create, pthread_join, pthread_mutex_*, pthread_cond_* |

### 12.3 Init Behavior
1. Mount /proc, /dev, /tmp
2. Open /dev/console for stdin/stdout/stderr
3. Spawn getty on each configured TTY
4. Reap orphaned zombies
5. Handle shutdown signals

### 12.4 Exit Criteria
- [ ] libc links and works
- [ ] init runs as PID 1
- [ ] login prompts and authenticates
- [ ] shell runs commands
- [ ] coreutils work
- [ ] Works on both arches

---

## 13) Phase 9: SMP

**Goal:** Multi-core support.

### 13.1 Deliverables
- AP (application processor) boot
- Per-CPU data structures
- Per-CPU run queues
- TLB shootdowns
- Spinlocks, RwLocks with proper memory ordering
- Work stealing scheduler

### 13.2 x86_64 SMP Boot
1. BSP parses ACPI MADT for AP info
2. BSP allocates per-CPU stacks
3. BSP sends INIT-SIPI-SIPI to each AP
4. APs start in real mode, transition to long mode
5. APs enter scheduler

### 13.3 AArch64 SMP Boot
1. Parse device tree for CPU info
2. Use PSCI or spin-table method
3. APs start at EL1 (or EL2 if needed)
4. APs enter scheduler

### 13.4 Synchronization Primitives

```rust
pub struct Spinlock<T> { ... }
pub struct RwLock<T> { ... }
pub struct Mutex<T> { ... }         // Blocking
pub struct Condvar { ... }
pub struct Semaphore { ... }
pub struct AtomicRefCount { ... }
```

### 13.5 Exit Criteria
- [ ] All cores boot
- [ ] Threads run on all cores
- [ ] Lock contention handled correctly
- [ ] TLB shootdowns work
- [ ] No data races (MIRI/loom testing)
- [ ] Works on both arches

---

## 14) Phase 10: Loadable Kernel Modules

**Goal:** Dynamic driver loading.

### 14.1 Deliverables
- Module binary format (ELF relocatable)
- Module loader
- Symbol resolution (kernel exports)
- Module init/exit hooks
- insmod/rmmod utilities

### 14.2 Module Structure

```rust
#[no_mangle]
pub static MODULE_INFO: ModuleInfo = ModuleInfo {
    name: "example_driver",
    version: "1.0.0",
    author: "RustOS",
    license: "MIT",
    init: module_init,
    exit: module_exit,
    dependencies: &["pci"],
};

fn module_init() -> Result<()> {
    // Register driver
    Ok(())
}

fn module_exit() {
    // Unregister driver
}
```

### 14.3 Kernel Symbol Export

```rust
#[export_symbol]
pub fn register_driver(driver: &Driver) -> Result<()> { ... }
```

### 14.4 Exit Criteria
- [ ] Modules load at runtime
- [ ] Modules unload cleanly
- [ ] Symbol resolution works
- [ ] Dependency ordering works
- [ ] Works on both arches

---

## 15) Phase 11: Block Storage + Real Filesystem

**Goal:** Persistent storage with native efflux.fs and FAT32 support.

### 15.1 Deliverables
- Block device interface
- Partition table parsing (GPT)
- virtio-blk driver (QEMU)
- NVMe driver
- AHCI/SATA driver
- efflux.fs driver (native filesystem)
- FAT32 driver (boot partition, compatibility)

### 15.2 Block Device Interface

```rust
pub trait BlockDevice: Send + Sync {
    fn block_size(&self) -> usize;
    fn block_count(&self) -> u64;
    fn read_blocks(&self, start: u64, buf: &mut [u8]) -> Result<()>;
    fn write_blocks(&self, start: u64, buf: &[u8]) -> Result<()>;
    fn flush(&self) -> Result<()>;
}
```

### 15.3 Filesystem Strategy

| Filesystem | Use case |
|------------|----------|
| efflux.fs | Root, data partitions, full features |
| FAT32 | EFI system partition, USB compatibility |

### 15.4 Mount Alias System

Traditional Unix paths with optional drive letter and named aliases:

```bash
# Mount with auto-assigned letter (starts at C:)
mount /dev/nvme0n1p2 /

# Mount with explicit letter
mount /dev/nvme0n1p3 /home letter=D

# Mount with named alias
mount /dev/sda1 /mnt/backup name=BACKUP

# Access methods (all equivalent):
/mnt/backup/file.txt
D:/file.txt
BACKUP:/file.txt
```

**Reserved:** A: and B: (legacy floppy), assignments start at C:

### 15.5 Exit Criteria
- [ ] virtio-blk works in QEMU
- [ ] GPT partition parsing works
- [ ] efflux.fs mounts and works
- [ ] FAT32 mounts and works
- [ ] Mount aliases work
- [ ] Works on both arches

---

## 16) Phase 12: Network Stack

**Goal:** TCP/IP networking.

### 16.1 Deliverables
- Network device interface
- virtio-net driver (QEMU)
- Intel e1000 driver (common hardware)
- smoltcp integration (or port)
- Socket API

### 16.2 Network Device Interface

```rust
pub trait NetworkDevice: Send + Sync {
    fn mac_address(&self) -> [u8; 6];
    fn mtu(&self) -> usize;
    fn send(&self, packet: &[u8]) -> Result<()>;
    fn recv(&self, buf: &mut [u8]) -> Result<usize>;
    fn set_rx_callback(&self, cb: fn(&[u8]));
}
```

### 16.3 Socket Syscalls

| Name | Description |
|------|-------------|
| sys_socket | Create socket |
| sys_bind | Bind to address |
| sys_listen | Listen for connections |
| sys_accept | Accept connection |
| sys_connect | Connect to server |
| sys_send | Send data |
| sys_recv | Receive data |
| sys_sendto | Send to address |
| sys_recvfrom | Receive with address |
| sys_setsockopt | Set socket option |
| sys_getsockopt | Get socket option |
| sys_shutdown | Shutdown connection |

### 16.4 Exit Criteria
- [ ] virtio-net works
- [ ] DHCP gets IP address
- [ ] TCP connections work
- [ ] UDP works
- [ ] DNS resolution works
- [ ] Works on both arches

---

## 17) Phase 13: Input Devices

**Goal:** Keyboard, mouse, HID support.

### 17.1 Deliverables
- PS/2 keyboard driver (x86)
- PS/2 mouse driver (x86)
- virtio-input driver
- USB HID driver (phase 17)
- Input event subsystem

### 17.2 Input Event Interface

```rust
pub struct InputEvent {
    pub timestamp: u64,
    pub device_id: u32,
    pub event_type: InputEventType,
}

pub enum InputEventType {
    KeyPress(Keycode),
    KeyRelease(Keycode),
    MouseMove { dx: i32, dy: i32 },
    MouseButton { button: u8, pressed: bool },
    MouseScroll { dx: i32, dy: i32 },
}
```

### 17.3 Exit Criteria
- [ ] Keyboard input works
- [ ] Mouse input works
- [ ] Events delivered to foreground process
- [ ] Works on both arches (virtio-input on ARM)

---

## 18) Phase 14: Graphics

**Goal:** Framebuffer and GPU support.

### 18.1 Deliverables
- UEFI GOP framebuffer
- virtio-gpu driver
- DRM-like interface
- Simple compositor (optional)

### 18.2 GPU Driver Roadmap

| Phase | Driver | Priority |
|-------|--------|----------|
| 14a | UEFI GOP framebuffer | Must have |
| 14b | virtio-gpu | Must have (QEMU) |
| 14c | Intel integrated (i915) | High |
| 14d | AMD (amdgpu) | Medium |
| 14e | NVIDIA (nouveau or proprietary) | Low |

### 18.3 Framebuffer Interface

```rust
pub trait Framebuffer: Send + Sync {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn pitch(&self) -> u32;
    fn bpp(&self) -> u8;
    fn buffer(&self) -> &mut [u8];
    fn flush(&self);
}
```

### 18.4 GPU Interface (DRM-like)

```rust
pub trait GpuDevice: Send + Sync {
    fn create_buffer(&self, width: u32, height: u32, format: PixelFormat) -> Result<BufferHandle>;
    fn destroy_buffer(&self, handle: BufferHandle);
    fn map_buffer(&self, handle: BufferHandle) -> Result<*mut u8>;
    fn set_scanout(&self, buffer: BufferHandle);
    fn cursor_set(&self, buffer: BufferHandle, hot_x: u32, hot_y: u32);
    fn cursor_move(&self, x: u32, y: u32);
}
```

### 18.5 Exit Criteria
- [ ] Framebuffer console works
- [ ] virtio-gpu scanout works
- [ ] Multiple resolutions supported
- [ ] Works on both arches

---

## 19) Phase 15: Audio

**Goal:** Sound output.

### 19.1 Deliverables
- Audio device interface
- virtio-snd driver
- Intel HDA driver (optional)
- Simple mixer

### 19.2 Audio Interface

```rust
pub trait AudioDevice: Send + Sync {
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> u8;
    fn format(&self) -> AudioFormat;
    fn write(&self, samples: &[u8]) -> Result<usize>;
    fn set_volume(&self, channel: u8, volume: u8);
}
```

### 19.3 Exit Criteria
- [ ] Audio playback works
- [ ] Volume control works
- [ ] Works on both arches (virtio-snd)

---

## 20) Phase 16: USB

**Goal:** USB host controller support.

### 20.1 Deliverables
- xHCI driver
- USB device enumeration
- USB mass storage
- USB HID (keyboard, mouse)

### 20.2 Exit Criteria
- [ ] USB devices enumerate
- [ ] USB keyboard works
- [ ] USB storage mounts
- [ ] Works on both arches

---

## 21) Phase 17: Containers

**Goal:** Process isolation primitives.

### 21.1 Deliverables
- PID namespaces
- Mount namespaces
- Network namespaces
- User namespaces
- Cgroups (CPU, memory limits)
- Seccomp-like syscall filtering

### 21.2 Syscalls Added

| Name | Description |
|------|-------------|
| sys_unshare | Create new namespaces |
| sys_setns | Join namespace |
| sys_clone3 | Clone with namespace flags |
| sys_pivot_root | Change root filesystem |
| sys_cgroup_* | Cgroup management |
| sys_seccomp | Syscall filtering |

### 21.3 Exit Criteria
- [ ] PID namespace isolation works
- [ ] Mount namespace isolation works
- [ ] Memory limits enforced
- [ ] Container can't escape

---

## 22) Phase 18: Hypervisor

**Goal:** Run VMs inside RustOS.

### 22.1 Deliverables
- VT-x support (x86_64)
- EL2 support (AArch64)
- VMCS/VMCB management
- Nested page tables (EPT/Stage 2)
- virtio device emulation
- Basic VM lifecycle (create, run, pause, destroy)

### 22.2 Hypervisor Interface

```rust
pub trait Hypervisor {
    fn create_vm(&self) -> Result<VmHandle>;
    fn destroy_vm(&self, vm: VmHandle);
    fn create_vcpu(&self, vm: VmHandle) -> Result<VcpuHandle>;
    fn run_vcpu(&self, vcpu: VcpuHandle) -> Result<VmExit>;
    fn set_memory(&self, vm: VmHandle, slot: u32, gpa: u64, size: u64, hva: *mut u8);
}

pub enum VmExit {
    Io { port: u16, is_write: bool, data: u64 },
    Mmio { addr: u64, is_write: bool, data: u64 },
    Halt,
    Shutdown,
    // ...
}
```

### 22.3 Exit Criteria
- [ ] Create and destroy VM
- [ ] Guest boots to serial output
- [ ] virtio devices work in guest
- [ ] Works on both arches

---

## 23) Phase 19: Self-Hosting

**Goal:** Compile Rust on EFFLUX.

### 23.1 Deliverables
- Port LLVM
- Port rustc
- Port cargo
- Compile kernel on itself

### 23.2 Dependencies
- Full libc with threading
- mmap/mprotect working
- Large file support
- /tmp with lots of space

### 23.3 Exit Criteria
- [ ] rustc compiles hello world
- [ ] cargo build works
- [ ] Kernel compiles on itself

---

## 24) Phase 20: AI Indexing + Semantic Search

**Goal:** AI-native file metadata and search.

### 24.1 Deliverables
- indexd daemon
- Embedding generation (Candle runtime)
- Vector search index (HNSW)
- Extended metadata on efflux.fs
- Overlay metadata for non-native filesystems
- Search API

### 24.2 Embedding Tiers

| Tier | Dimensions | Use case | Storage |
|------|------------|----------|---------|
| micro | 128 | Filenames, short text | 512 bytes |
| small | 384 | General files | 1.5 KB |
| medium | 768 | Documents, code | 3 KB |
| large | 1536 | High-fidelity search | 6 KB |

### 24.3 Extended Metadata (efflux.fs)

```rust
pub struct ExtendedMeta {
    pub mime_type: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub embedding_tier: EmbeddingTier,
    pub embedding: Option<Vec<f32>>,
    pub source: Option<String>,
    pub created_by: String,
    pub ai_summary: Option<String>,
    pub relationships: Vec<(PathBuf, RelationType)>,
    pub confidence: f32,
}
```

### 24.4 Overlay Store (non-efflux.fs)

```
/var/efflux/meta/
  └── <mount-uuid>/
      ├── index.db        # SQLite metadata
      ├── vectors.bin     # Packed embeddings
      └── pending/        # Indexing queue
```

### 24.5 Search API

```rust
// IPC to indexd
pub enum SearchQuery {
    Text(String),                           // Full-text search
    Semantic(String),                       // Vector similarity
    Similar(PathBuf),                       // Find similar to file
    Tags(Vec<String>),                      // Tag match
    Combined { text: String, semantic: String, tags: Vec<String> },
}

pub struct SearchResult {
    pub path: PathBuf,
    pub score: f32,
    pub snippet: Option<String>,
}
```

### 24.6 Exit Criteria
- [ ] indexd runs and indexes files
- [ ] Embeddings generated for text files
- [ ] Semantic search returns relevant results
- [ ] Works with efflux.fs and overlay
- [ ] Works on both arches

---

## 25) Phase 21: Security + Trust System

**Goal:** File signing, encryption, trust management.

### 25.1 Deliverables
- Certificate management (X.509)
- File signing (Ed25519)
- File encryption (AES-256-GCM / ChaCha20-Poly1305)
- Trust store
- Quarantine system
- Trust sharing (peer-to-peer)

### 25.2 Trust Hierarchy

```
EFFLUX Root CA (built into OS)
    ├── Vendor CAs (third-party apps, drivers)
    └── User CAs (personal certificates)
```

### 25.3 File Integrity Metadata

```rust
pub struct FileIntegrity {
    pub hash_algo: HashAlgo,           // BLAKE3, SHA-256
    pub content_hash: [u8; 32],
    pub signature: Option<Vec<u8>>,
    pub signer_cert: Option<Vec<u8>>,
    pub trust_level: TrustLevel,
    pub signed_at: u64,
    pub flags: IntegrityFlags,
}

pub enum TrustLevel {
    System,     // EFFLUX root CA
    Vendor,     // Vendor sub-CA
    User,       // User cert
    Untrusted,  // No signature
}
```

### 25.4 Quarantine System

```
/var/efflux/quarantine/
  └── <file-uuid>/
      ├── content.bin       # Encrypted, non-executable
      ├── metadata.json
      ├── signature.bin
      ├── signer_cert.pem
      └── source.txt
```

### 25.5 Trust Sharing

- QR code (in-person)
- NFC tap
- USB file
- mDNS discovery (local network)
- Manual fingerprint verification

### 25.6 CLI Tools

```bash
efflux trust list|add|remove|revoke|export|share|discover
efflux sign <file>
efflux verify <file>
efflux seal <file>
efflux inspect <file>
efflux quarantine list|inspect|accept|reject
efflux encrypt <file>
efflux decrypt <file>
```

### 25.7 Exit Criteria
- [ ] File signing and verification works
- [ ] File encryption works
- [ ] Trust store manages certs
- [ ] Quarantine flow works
- [ ] Cert sharing between machines works
- [ ] Works on both arches

---

## 26) Testing Strategy

### 26.1 Per-Phase Tests

Each phase includes:
- Unit tests (in-kernel, `#[cfg(test)]`)
- Integration tests (userland test binaries)
- QEMU automated boot tests

### 26.2 CI Pipeline

```
1. cargo build --target x86_64-unknown-none
2. cargo build --target aarch64-unknown-none
3. Run QEMU x86_64 boot test
4. Run QEMU aarch64 boot test
5. Run test suite in QEMU
6. Run MIRI for unsafe code
```

### 26.3 Test Categories

| Category | Description |
|----------|-------------|
| boot | Kernel boots to init |
| mm | Memory allocation, paging, COW |
| sched | Thread creation, preemption |
| syscall | All syscalls exercised |
| vfs | File operations |
| net | TCP/UDP connectivity |
| driver | Each driver has tests |
| crypto | Signing, encryption, hashing |
| trust | Certificate operations |
| search | Indexing and search |

---

## 27) Documentation Requirements

Each component requires:
- DESIGN.md: Architecture decisions
- API.md: Public interfaces
- INTERNALS.md: Implementation details
- TESTING.md: How to test

---

## 28) Milestones Summary

| Phase | Milestone | Exit Criteria |
|-------|-----------|---------------|
| 0 | Boot | Serial output on both arches |
| 1 | Memory | Kernel heap works |
| 2 | Scheduler | Preemptive kernel threads |
| 3 | User mode | Ring 3 process runs |
| 4 | Processes | fork/exec work |
| 5 | VFS | Files work |
| 6 | TTY | Interactive terminal |
| 7 | Signals | Signal handlers work |
| 8 | Userland | Shell runs |
| 9 | SMP | Multi-core works |
| 10 | Modules | Drivers load dynamically |
| 11 | Storage | efflux.fs + FAT32 work |
| 12 | Network | TCP/IP works |
| 13 | Input | Keyboard/mouse work |
| 14 | Graphics | Framebuffer/GPU works |
| 15 | Audio | Sound plays |
| 16 | USB | USB devices work |
| 17 | Containers | Isolation works |
| 18 | Hypervisor | VMs run |
| 19 | Self-host | Compile Rust on EFFLUX |
| 20 | AI Search | Semantic search works |
| 21 | Security | Signing + encryption work |
| 22 | Async I/O | epoll/kqueue equivalent works |
| 23 | External Media | USB/network shares handled safely |
| 24 | Compat Runtimes | DOS V86, Python sandboxed |
| 25 | Full libc | Source compat with Linux apps |

---

## 29) Resolved Decisions

| Item | Decision |
|------|----------|
| Project name | EFFLUX |
| License | MIT |
| Kernel model | Monolithic + loadable modules |
| Targets | x86_64, AArch64 |
| Boot | Custom UEFI, PXE |
| Compatibility | Source compat, POSIX-ish, custom libc |
| Native filesystem | efflux.fs |
| Boot filesystem | FAT32 |
| Drive letters | A:/B: reserved, start at C: |
| Network stack | smoltcp |
| GPU roadmap | virtio → Intel → AMD → NVIDIA |
| Embedding runtime | Candle (Rust-native) |
| Embedding tiers | 128/384/768/1536 selectable |
| Signing algorithm | Ed25519 |
| Encryption | AES-256-GCM / ChaCha20-Poly1305 |
| Hashing | BLAKE3 |
| Certificates | X.509 |
| Trust model | PKI with local CRL |
| DOS emulation | V86 mode (fast, native) |
| Python runtime | Native CPython port, sandboxed |
| External files | Read-only until promoted |
| Network shares | Read-only, same as USB policy |

---

## 30) Related Specifications

| Document | Description |
|----------|-------------|
| MEMORY_SPEC.md | Physical/virtual memory, buddy, slab, CoW, no-MMU |
| EFFLUXFS_SPEC.md | efflux.fs on-disk format |
| SECURITY_SPEC.md | Security and trust system |
| LIBC_SPEC.md | C library API |
| SCHEDULER_SPEC.md | Scheduler design |
| VFS_SPEC.md | Virtual filesystem |
| ASYNC_IO_SPEC.md | Async I/O (epoll-like) |
| IPC_SPEC.md | Inter-process communication |
| MODULE_SPEC.md | Loadable kernel modules |
| COMPAT_SPEC.md | Compatibility runtimes (DOS, Python) |
| EXTERNAL_MEDIA_SPEC.md | External media handling |
| PROCFS_SPEC.md | /proc filesystem |
| DEVFS_SPEC.md | /dev filesystem |
| GRAPHICS_SPEC.md | Graphics, GPU, display server, Canvas API |
| SESSION_SPEC.md | Session types, display routing, backend architecture |
| NETWORK_SPEC.md | TCP/IP stack, drivers, sockets, routing, firewall |
| INPUT_SPEC.md | Keyboard, mouse, HID, layouts, Unicode, codepages |

---

## 31) Next Steps

1. Review and approve this spec
2. Review efflux.fs spec
3. Review security spec
4. Review all component specs
5. Set up repository structure
6. Begin Phase 0 implementation

---

*End of EFFLUX Master Specification*
