# EFFLUX Project Structure

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
efflux/
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
│   │   ├── efflux-core/            # Sync primitives, collections
│   │   ├── efflux-alloc/           # Global allocator interface
│   │   └── efflux-log/             # Logging framework
│   │
│   ├── arch/                       # Architecture layer
│   │   ├── efflux-arch-traits/     # Arch trait definitions
│   │   ├── efflux-arch-x86_64/
│   │   ├── efflux-arch-i686/
│   │   ├── efflux-arch-aarch64/
│   │   ├── efflux-arch-arm/
│   │   ├── efflux-arch-mips64/
│   │   ├── efflux-arch-mips32/
│   │   ├── efflux-arch-riscv64/
│   │   └── efflux-arch-riscv32/
│   │
│   ├── mm/                         # Memory management
│   │   ├── efflux-mm-traits/       # MM trait definitions
│   │   ├── efflux-mm-buddy/        # Buddy allocator
│   │   ├── efflux-mm-slab/         # Slab allocator
│   │   ├── efflux-mm-vmm/          # Virtual memory manager
│   │   └── efflux-mm-heap/         # Kernel heap
│   │
│   ├── sched/                      # Scheduler
│   │   ├── efflux-sched-traits/    # Scheduler traits
│   │   ├── efflux-sched-rr/        # Round-robin scheduler
│   │   └── efflux-sched-cfs/       # CFS-like scheduler (future)
│   │
│   ├── process/                    # Process management
│   │   ├── efflux-process/         # Process/thread structures
│   │   ├── efflux-elf/             # ELF loader
│   │   └── efflux-signal/          # Signal handling
│   │
│   ├── syscall/                    # Syscall layer
│   │   ├── efflux-syscall-traits/  # Syscall interface
│   │   └── efflux-syscall/         # Syscall dispatch + handlers
│   │
│   ├── vfs/                        # Virtual filesystem
│   │   ├── efflux-vfs-traits/      # VFS traits (Vnode, Filesystem)
│   │   └── efflux-vfs/             # VFS implementation
│   │
│   ├── fs/                         # Filesystem implementations
│   │   ├── efflux-fs-effluxfs/     # Native filesystem
│   │   ├── efflux-fs-fat32/        # FAT32
│   │   ├── efflux-fs-tmpfs/        # RAM filesystem
│   │   ├── efflux-fs-devfs/        # Device filesystem
│   │   ├── efflux-fs-procfs/       # Process filesystem
│   │   └── efflux-fs-initramfs/    # Initial ramdisk (cpio)
│   │
│   ├── drivers/                    # Device drivers
│   │   ├── efflux-driver-traits/   # Driver traits
│   │   ├── serial/
│   │   │   ├── efflux-driver-uart-8250/
│   │   │   └── efflux-driver-uart-pl011/
│   │   ├── block/
│   │   │   ├── efflux-driver-virtio-blk/
│   │   │   ├── efflux-driver-nvme/
│   │   │   └── efflux-driver-ahci/
│   │   ├── net/
│   │   │   ├── efflux-driver-virtio-net/
│   │   │   └── efflux-driver-e1000/
│   │   ├── input/
│   │   │   ├── efflux-driver-ps2/
│   │   │   └── efflux-driver-virtio-input/
│   │   ├── gpu/
│   │   │   ├── efflux-driver-virtio-gpu/
│   │   │   └── efflux-driver-framebuffer/
│   │   ├── usb/
│   │   │   └── efflux-driver-xhci/
│   │   └── timer/
│   │       ├── efflux-driver-apic-timer/
│   │       ├── efflux-driver-hpet/
│   │       └── efflux-driver-arm-timer/
│   │
│   ├── net/                        # Network stack
│   │   ├── efflux-net-traits/      # Network traits
│   │   ├── efflux-net-stack/       # TCP/IP stack
│   │   └── efflux-net-socket/      # Socket API
│   │
│   ├── ipc/                        # IPC mechanisms
│   │   ├── efflux-ipc-pipe/        # Pipes
│   │   ├── efflux-ipc-socket/      # Unix sockets
│   │   └── efflux-ipc-shm/         # Shared memory
│   │
│   ├── tty/                        # Terminal
│   │   ├── efflux-tty/             # TTY subsystem
│   │   └── efflux-pty/             # PTY pairs
│   │
│   ├── security/                   # Security subsystem
│   │   ├── efflux-crypto/          # Crypto primitives
│   │   ├── efflux-trust/           # Trust store, certs
│   │   └── efflux-quarantine/      # Quarantine system
│   │
│   ├── ai/                         # AI subsystem
│   │   ├── efflux-embeddings/      # Embedding generation
│   │   └── efflux-search/          # Vector search
│   │
│   └── module/                     # Loadable modules
│       └── efflux-module/          # Module loader
│
├── bootloader/                     # BOOTLOADER CRATES
│   ├── efflux-boot-common/         # Shared boot code
│   ├── efflux-boot-uefi/           # UEFI bootloader
│   ├── efflux-boot-bios/           # Legacy BIOS
│   ├── efflux-boot-opensbi/        # RISC-V OpenSBI
│   ├── efflux-boot-arcs/           # MIPS ARCS (SGI)
│   └── efflux-boot-uboot/          # ARM U-Boot
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
│   ├── efflux-tools/               # EFFLUX-specific tools
│   │   ├── trust/                  # Trust management CLI
│   │   ├── sign/                   # File signing
│   │   └── search/                 # Semantic search CLI
│   └── tests/                      # Test applications
│       ├── hello/                  # Minimal test
│       ├── fork-test/
│       └── ...
│
├── tools/                          # BUILD TOOLS (host)
│   ├── mkfs-efflux/                # Create effluxfs image
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
│   │   ├── x86_64-efflux.json
│   │   ├── aarch64-efflux.json
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
    ├── efflux-arch-{arch}          # Selected architecture
    │   └── efflux-arch-traits
    │
    ├── efflux-mm-*                  # Memory management
    │   └── efflux-mm-traits
    │
    ├── efflux-sched-*               # Scheduler
    │   └── efflux-sched-traits
    │
    ├── efflux-vfs                   # VFS
    │   └── efflux-vfs-traits
    │
    ├── efflux-fs-*                  # Filesystems
    │
    ├── efflux-driver-*              # Drivers
    │   └── efflux-driver-traits
    │
    ├── efflux-syscall               # Syscalls
    │
    └── efflux-core                  # Core utilities
        └── efflux-alloc
```

### Trait Crates

Every subsystem has a `-traits` crate defining interfaces:

| Trait Crate | Defines |
|-------------|---------|
| efflux-arch-traits | `Arch`, `Mmu`, `Tlb`, `InterruptController`, `Timer`, `Context` |
| efflux-mm-traits | `FrameAllocator`, `PageTableOps`, `HeapAllocator` |
| efflux-sched-traits | `Scheduler`, `Thread`, `RunQueue` |
| efflux-vfs-traits | `Filesystem`, `Vnode`, `FileOps` |
| efflux-driver-traits | `Driver`, `BlockDevice`, `NetworkDevice`, `CharDevice` |
| efflux-net-traits | `NetworkStack`, `Socket` |

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
// tools/mkfs-efflux/src/main.rs
use std::fs::File;  // OK - runs on host
```

---

## Feature Flags

Crates use features for optional functionality:

```toml
# crates/mm/efflux-mm-buddy/Cargo.toml
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
efflux-mm-buddy = { path = "../crates/mm/efflux-mm-buddy", features = ["smp"] }
```

---

## Swappable Components

The kernel can swap implementations by changing dependencies:

```toml
# kernel/Cargo.toml

# Option A: Round-robin scheduler
efflux-sched = { package = "efflux-sched-rr", path = "../crates/sched/efflux-sched-rr" }

# Option B: CFS scheduler (swap this in)
# efflux-sched = { package = "efflux-sched-cfs", path = "../crates/sched/efflux-sched-cfs" }
```

Both implement `efflux-sched-traits::Scheduler`.

---

## Architecture Selection

Arch is selected at build time via target:

```toml
# kernel/Cargo.toml

[target.'cfg(target_arch = "x86_64")'.dependencies]
efflux-arch = { package = "efflux-arch-x86_64", path = "../crates/arch/efflux-arch-x86_64" }

[target.'cfg(target_arch = "aarch64")'.dependencies]
efflux-arch = { package = "efflux-arch-aarch64", path = "../crates/arch/efflux-arch-aarch64" }

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

use efflux_arch as arch;
use efflux_mm_buddy as frame_alloc;
use efflux_mm_slab as heap;
use efflux_sched as sched;
use efflux_vfs as vfs;
use efflux_syscall as syscall;

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
name = "efflux-shell"

[dependencies]
efflux-libc = { path = "../../libc" }

[[bin]]
name = "sh"
```

Coreutils are a single crate with multiple binaries:

```toml
# apps/coreutils/Cargo.toml
[package]
name = "efflux-coreutils"

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

*EFFLUX Project Structure*
