# OXIDE Project Structure

**Status:** Draft
**Principle:** Everything is a crate. Kernel is minimal glue.

---

## Design Goals

1. **Modular** - Every component is a separate crate
2. **Swappable** - Crates implement traits, can be replaced
3. **`#![no_std]`** - Zero dependencies on hosted environment
4. **Hierarchical** - Logical grouping in directories
5. **Buildable** - Clear path from source to bootable image

---

## Repository Layout

```
oxide/
├── Cargo.toml                      # Workspace root
├── rust-toolchain.toml             # Nightly + targets
├── .cargo/
│   └── config.toml                 # Build settings, targets
│
├── kernel/                         # KERNEL BINARY (minimal)
│   ├── Cargo.toml
│   └── src/
│       └── main.rs                 # Wires crates together
│
├── crates/                         # ALL COMPONENT CRATES
│   │
│   ├── core/                       # Core utilities
│   │   ├── core/            # Sync primitives, collections
│   │   ├── alloc/           # Global allocator interface
│   │   └── log/             # Logging framework
│   │
│   ├── arch/                       # Architecture layer
│   │   ├── arch-traits/     # Arch trait definitions
│   │   ├── arch-x86_64/
│   │   ├── arch-i686/
│   │   ├── arch-aarch64/
│   │   ├── arch-arm/
│   │   ├── arch-mips64/
│   │   ├── arch-mips32/
│   │   ├── arch-riscv64/
│   │   └── arch-riscv32/
│   │
│   ├── mm/                         # Memory management
│   │   ├── mm-traits/       # MM trait definitions
│   │   ├── mm-buddy/        # Buddy allocator
│   │   ├── mm-slab/         # Slab allocator
│   │   ├── mm-vmm/          # Virtual memory manager
│   │   └── mm-heap/         # Kernel heap
│   │
│   ├── sched/                      # Scheduler
│   │   ├── sched-traits/    # Scheduler traits
│   │   ├── sched-rr/        # Round-robin scheduler
│   │   └── sched-cfs/       # CFS-like scheduler (future)
│   │
│   ├── process/                    # Process management
│   │   ├── process/         # Process/thread structures
│   │   ├── elf/             # ELF loader
│   │   └── signal/          # Signal handling
│   │
│   ├── syscall/                    # Syscall layer
│   │   ├── syscall-traits/  # Syscall interface
│   │   └── syscall/         # Syscall dispatch + handlers
│   │
│   ├── vfs/                        # Virtual filesystem
│   │   ├── vfs-traits/      # VFS traits (Vnode, Filesystem)
│   │   └── vfs/             # VFS implementation
│   │
│   ├── fs/                         # Filesystem implementations
│   │   ├── fs-oxidefs/     # Native filesystem
│   │   ├── fs-fat32/        # FAT32
│   │   ├── fs-tmpfs/        # RAM filesystem
│   │   ├── fs-devfs/        # Device filesystem
│   │   ├── fs-procfs/       # Process filesystem
│   │   └── fs-initramfs/    # Initial ramdisk (cpio)
│   │
│   ├── drivers/                    # Device drivers
│   │   ├── driver-traits/   # Driver traits
│   │   ├── serial/
│   │   │   ├── driver-uart-8250/
│   │   │   └── driver-uart-pl011/
│   │   ├── block/
│   │   │   ├── driver-virtio-blk/
│   │   │   ├── driver-nvme/
│   │   │   └── driver-ahci/
│   │   ├── net/
│   │   │   ├── driver-virtio-net/
│   │   │   └── driver-e1000/
│   │   ├── input/
│   │   │   ├── driver-ps2/
│   │   │   └── driver-virtio-input/
│   │   ├── gpu/
│   │   │   ├── driver-virtio-gpu/
│   │   │   └── driver-framebuffer/
│   │   ├── usb/
│   │   │   └── driver-xhci/
│   │   └── timer/
│   │       ├── driver-apic-timer/
│   │       ├── driver-hpet/
│   │       └── driver-arm-timer/
│   │
│   ├── net/                        # Network stack
│   │   ├── net-traits/      # Network traits
│   │   ├── net-stack/       # TCP/IP stack
│   │   └── net-socket/      # Socket API
│   │
│   ├── ipc/                        # IPC mechanisms
│   │   ├── ipc-pipe/        # Pipes
│   │   ├── ipc-socket/      # Unix sockets
│   │   └── ipc-shm/         # Shared memory
│   │
│   ├── tty/                        # Terminal
│   │   ├── tty/             # TTY subsystem
│   │   └── pty/             # PTY pairs
│   │
│   ├── security/                   # Security subsystem
│   │   ├── crypto/          # Crypto primitives
│   │   ├── trust/           # Trust store, certs
│   │   └── quarantine/      # Quarantine system
│   │
│   ├── ai/                         # AI subsystem
│   │   ├── embeddings/      # Embedding generation
│   │   └── search/          # Vector search
│   │
│   └── module/                     # Loadable modules
│       └── module/          # Module loader
│
├── bootloader/                     # BOOTLOADER CRATES
│   ├── boot-common/         # Shared boot code
│   ├── boot-uefi/           # UEFI bootloader
│   ├── boot-bios/           # Legacy BIOS
│   ├── boot-opensbi/        # RISC-V OpenSBI
│   ├── boot-arcs/           # MIPS ARCS (SGI)
│   └── boot-uboot/          # ARM U-Boot
│
├── libc/                           # CUSTOM LIBC
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── stdio.rs
│       ├── stdlib.rs
│       ├── string.rs
│       ├── unistd.rs
│       └── ...
│
├── apps/                           # USER APPLICATIONS
│   ├── init/                       # PID 1
│   ├── login/                      # Login program
│   ├── shell/                      # Shell
│   ├── coreutils/                  # ls, cat, cp, mv, etc.
│   │   ├── Cargo.toml              # Workspace for all utils
│   │   └── src/bin/
│   │       ├── ls.rs
│   │       ├── cat.rs
│   │       └── ...
│   ├── tools/               # OXIDE-specific tools
│   │   ├── trust/                  # Trust management CLI
│   │   ├── sign/                   # File signing
│   │   └── search/                 # Semantic search CLI
│   └── tests/                      # Test applications
│       ├── hello/                  # Minimal test
│       ├── fork-test/
│       └── ...
│
├── tools/                          # BUILD TOOLS (host)
│   ├── mkfs-oxide/                # Create oxidefs image
│   ├── mkimage/                    # Create boot image
│   ├── qemu-runner/                # QEMU test runner
│   └── ci/                         # CI scripts
│
├── policies/                       # BUILD POLICIES
│   ├── default.toml                # Default build config
│   ├── minimal.toml                # Minimal image
│   ├── desktop.toml                # Full desktop
│   ├── server.toml                 # Server config
│   └── embedded.toml               # Embedded/constrained
│
├── build/                          # BUILD OUTPUT & SCRIPTS
│   ├── scripts/
│   │   ├── build-kernel.sh
│   │   ├── build-image.sh
│   │   └── run-qemu.sh
│   ├── targets/                    # Target specs (.json)
│   │   ├── x86_64-oxide.json
│   │   ├── aarch64-oxide.json
│   │   └── ...
│   └── images/                     # Output images (gitignored)
│
└── docs/                           # DOCUMENTATION
    ├── plan/                       # Implementation plan
    ├── arch/                       # Arch-specific docs
    └── *.md                        # Specifications
```

---

## Crate Hierarchy

### Dependency Flow

```
kernel (binary)
    │
    ├── arch-{arch}          # Selected architecture
    │   └── arch-traits
    │
    ├── mm-*                  # Memory management
    │   └── mm-traits
    │
    ├── sched-*               # Scheduler
    │   └── sched-traits
    │
    ├── vfs                   # VFS
    │   └── vfs-traits
    │
    ├── fs-*                  # Filesystems
    │
    ├── driver-*              # Drivers
    │   └── driver-traits
    │
    ├── syscall               # Syscalls
    │
    └── core                  # Core utilities
        └── alloc
```

### Trait Crates

Every subsystem has a `-traits` crate defining interfaces:

| Trait Crate | Defines |
|-------------|---------|
| arch-traits | `Arch`, `Mmu`, `Tlb`, `InterruptController`, `Timer`, `Context` |
| mm-traits | `FrameAllocator`, `PageTableOps`, `HeapAllocator` |
| sched-traits | `Scheduler`, `Thread`, `RunQueue` |
| vfs-traits | `Filesystem`, `Vnode`, `FileOps` |
| driver-traits | `Driver`, `BlockDevice`, `NetworkDevice`, `CharDevice` |
| net-traits | `NetworkStack`, `Socket` |

---

## `#![no_std]` Rules

**All kernel crates must be `#![no_std]`**

### Allowed

```rust
#![no_std]

extern crate alloc;  // After heap is initialized

use alloc::vec::Vec;
use alloc::boxed::Box;
use alloc::string::String;
use core::*;
```

### Forbidden

```rust
// NEVER in kernel crates
use std::*;
```

### Build Tools Exception

Crates in `tools/` run on host and may use `std`:

```rust
// tools/mkfs-oxide/src/main.rs
use std::fs::File;  // OK - runs on host
```

---

## Feature Flags

Crates use features for optional functionality:

```toml
# crates/mm/mm-buddy/Cargo.toml
[features]
default = []
smp = []           # SMP-safe allocator
stats = []         # Allocation statistics
debug = []         # Debug assertions
```

Kernel selects features:

```toml
# kernel/Cargo.toml
[dependencies]
mm-buddy = { path = "../crates/mm/mm-buddy", features = ["smp"] }
```

---

## Swappable Components

The kernel can swap implementations by changing dependencies:

```toml
# kernel/Cargo.toml

# Option A: Round-robin scheduler
sched = { package = "sched-rr", path = "../crates/sched/sched-rr" }

# Option B: CFS scheduler (swap this in)
# sched = { package = "sched-cfs", path = "../crates/sched/sched-cfs" }
```

Both implement `sched-traits::Scheduler`.

---

## Architecture Selection

Arch is selected at build time via target:

```toml
# kernel/Cargo.toml

[target.'cfg(target_arch = "x86_64")'.dependencies]
arch = { package = "arch-x86_64", path = "../crates/arch/arch-x86_64" }

[target.'cfg(target_arch = "aarch64")'.dependencies]
arch = { package = "arch-aarch64", path = "../crates/arch/arch-aarch64" }

# ... other architectures
```

---

## Kernel Binary

The kernel crate is minimal - just wires components:

```rust
// kernel/src/main.rs
#![no_std]
#![no_main]

extern crate alloc;

use arch as arch;
use mm_buddy as frame_alloc;
use mm_slab as heap;
use sched as sched;
use vfs as vfs;
use syscall as syscall;

#[no_mangle]
pub extern "C" fn kernel_main(boot_info: &arch::BootInfo) -> ! {
    // 1. Early init (serial, logging)
    arch::early_init(boot_info);

    // 2. Memory
    frame_alloc::init(boot_info.memory_map());
    heap::init();

    // 3. Interrupts + Timer
    arch::interrupts::init();
    arch::timer::init(100); // 100 Hz

    // 4. Scheduler
    sched::init();

    // 5. VFS
    vfs::init();

    // 6. Syscalls
    syscall::init();

    // 7. Start init process
    // ...

    sched::run() // Never returns
}
```

---

## Apps Structure

User applications link against `libc`:

```toml
# apps/shell/Cargo.toml
[package]
name = "shell"

[dependencies]
libc = { path = "../../libc" }

[[bin]]
name = "sh"
```

Coreutils are a single crate with multiple binaries:

```toml
# apps/coreutils/Cargo.toml
[package]
name = "coreutils"

[[bin]]
name = "ls"
path = "src/bin/ls.rs"

[[bin]]
name = "cat"
path = "src/bin/cat.rs"

# ... etc
```

---

## Next Steps

1. Create workspace `Cargo.toml`
2. Create initial crate structure
3. Set up build targets
4. See [BUILD_PLAN.md](BUILD_PLAN.md) for image creation

---

*OXIDE Project Structure*
