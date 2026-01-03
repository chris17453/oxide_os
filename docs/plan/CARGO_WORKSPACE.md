# EFFLUX Cargo Workspace

This document shows the Cargo workspace configuration for EFFLUX.

---

## Root Workspace

```toml
# /Cargo.toml

[workspace]
resolver = "2"

members = [
    # Kernel binary
    "kernel",

    # Core crates
    "crates/core/efflux-core",
    "crates/core/efflux-alloc",
    "crates/core/efflux-log",

    # Architecture crates
    "crates/arch/efflux-arch-traits",
    "crates/arch/efflux-arch-x86_64",
    "crates/arch/efflux-arch-i686",
    "crates/arch/efflux-arch-aarch64",
    "crates/arch/efflux-arch-arm",
    "crates/arch/efflux-arch-mips64",
    "crates/arch/efflux-arch-mips32",
    "crates/arch/efflux-arch-riscv64",
    "crates/arch/efflux-arch-riscv32",

    # Memory management
    "crates/mm/efflux-mm-traits",
    "crates/mm/efflux-mm-buddy",
    "crates/mm/efflux-mm-slab",
    "crates/mm/efflux-mm-vmm",
    "crates/mm/efflux-mm-heap",

    # Scheduler
    "crates/sched/efflux-sched-traits",
    "crates/sched/efflux-sched-rr",

    # Process
    "crates/process/efflux-process",
    "crates/process/efflux-elf",
    "crates/process/efflux-signal",

    # Syscall
    "crates/syscall/efflux-syscall-traits",
    "crates/syscall/efflux-syscall",

    # VFS
    "crates/vfs/efflux-vfs-traits",
    "crates/vfs/efflux-vfs",

    # Filesystems
    "crates/fs/efflux-fs-tmpfs",
    "crates/fs/efflux-fs-devfs",
    "crates/fs/efflux-fs-procfs",
    "crates/fs/efflux-fs-initramfs",
    "crates/fs/efflux-fs-fat32",
    "crates/fs/efflux-fs-effluxfs",

    # Drivers
    "crates/drivers/efflux-driver-traits",
    "crates/drivers/serial/efflux-driver-uart-8250",
    "crates/drivers/serial/efflux-driver-uart-pl011",
    "crates/drivers/block/efflux-driver-virtio-blk",
    "crates/drivers/net/efflux-driver-virtio-net",

    # TTY
    "crates/tty/efflux-tty",
    "crates/tty/efflux-pty",

    # IPC
    "crates/ipc/efflux-ipc-pipe",

    # Network
    "crates/net/efflux-net-traits",
    "crates/net/efflux-net-stack",

    # Bootloader
    "bootloader/efflux-boot-common",
    "bootloader/efflux-boot-uefi",

    # Libc
    "libc",

    # Apps
    "apps/init",
    "apps/shell",
    "apps/coreutils",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"
repository = "https://github.com/user/efflux"

[workspace.dependencies]
# Shared no_std dependencies
bitflags = "2.4"
spin = "0.9"
log = { version = "0.4", default-features = false }

# Internal crates (version = workspace)
efflux-core = { path = "crates/core/efflux-core" }
efflux-alloc = { path = "crates/core/efflux-alloc" }
efflux-log = { path = "crates/core/efflux-log" }
efflux-arch-traits = { path = "crates/arch/efflux-arch-traits" }
efflux-mm-traits = { path = "crates/mm/efflux-mm-traits" }
efflux-sched-traits = { path = "crates/sched/efflux-sched-traits" }
efflux-vfs-traits = { path = "crates/vfs/efflux-vfs-traits" }
efflux-driver-traits = { path = "crates/drivers/efflux-driver-traits" }

[profile.release]
opt-level = "z"      # Optimize for size
lto = true
panic = "abort"
```

---

## Kernel Crate

```toml
# /kernel/Cargo.toml

[package]
name = "efflux-kernel"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
# Core
efflux-core.workspace = true
efflux-alloc.workspace = true
efflux-log.workspace = true

# Memory
efflux-mm-traits.workspace = true
efflux-mm-buddy = { path = "../crates/mm/efflux-mm-buddy" }
efflux-mm-slab = { path = "../crates/mm/efflux-mm-slab" }
efflux-mm-vmm = { path = "../crates/mm/efflux-mm-vmm" }
efflux-mm-heap = { path = "../crates/mm/efflux-mm-heap" }

# Scheduler
efflux-sched-traits.workspace = true
efflux-sched-rr = { path = "../crates/sched/efflux-sched-rr" }

# Process
efflux-process = { path = "../crates/process/efflux-process" }
efflux-elf = { path = "../crates/process/efflux-elf" }
efflux-signal = { path = "../crates/process/efflux-signal" }

# Syscall
efflux-syscall = { path = "../crates/syscall/efflux-syscall" }

# VFS
efflux-vfs = { path = "../crates/vfs/efflux-vfs" }
efflux-fs-tmpfs = { path = "../crates/fs/efflux-fs-tmpfs" }
efflux-fs-devfs = { path = "../crates/fs/efflux-fs-devfs" }
efflux-fs-initramfs = { path = "../crates/fs/efflux-fs-initramfs" }

# TTY
efflux-tty = { path = "../crates/tty/efflux-tty" }

# Architecture-specific (conditional)
[target.'cfg(target_arch = "x86_64")'.dependencies]
efflux-arch = { package = "efflux-arch-x86_64", path = "../crates/arch/efflux-arch-x86_64" }
efflux-driver-serial = { package = "efflux-driver-uart-8250", path = "../crates/drivers/serial/efflux-driver-uart-8250" }

[target.'cfg(target_arch = "aarch64")'.dependencies]
efflux-arch = { package = "efflux-arch-aarch64", path = "../crates/arch/efflux-arch-aarch64" }
efflux-driver-serial = { package = "efflux-driver-uart-pl011", path = "../crates/drivers/serial/efflux-driver-uart-pl011" }

[target.'cfg(target_arch = "riscv64")'.dependencies]
efflux-arch = { package = "efflux-arch-riscv64", path = "../crates/arch/efflux-arch-riscv64" }

# ... other architectures

[features]
default = []
smp = ["efflux-mm-buddy/smp", "efflux-sched-rr/smp"]
stats = ["efflux-mm-buddy/stats"]
```

---

## Trait Crate Example

```toml
# /crates/arch/efflux-arch-traits/Cargo.toml

[package]
name = "efflux-arch-traits"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
efflux-core.workspace = true

[lib]
# No std!
```

```rust
// /crates/arch/efflux-arch-traits/src/lib.rs

#![no_std]

use efflux_core::PhysAddr;

/// Architecture trait - all arch crates implement this
pub trait Arch: Send + Sync {
    fn name() -> &'static str;
    fn page_size() -> usize;
    fn kernel_base() -> usize;
}

/// MMU trait
pub trait Mmu: Send + Sync {
    type PageTable;

    fn new_page_table() -> Self::PageTable;
    fn activate(pt: &Self::PageTable);
    fn map(pt: &mut Self::PageTable, va: usize, pa: PhysAddr, flags: MapFlags) -> Result<(), MmuError>;
    fn unmap(pt: &mut Self::PageTable, va: usize) -> Result<(), MmuError>;
}

/// TLB trait
pub trait Tlb {
    fn invalidate_page(va: usize);
    fn invalidate_all();
    fn shootdown(va: usize);
}

/// Interrupt controller trait
pub trait InterruptController {
    fn init();
    fn enable_irq(irq: u32);
    fn disable_irq(irq: u32);
    fn ack_irq(irq: u32);
}

/// Timer trait
pub trait Timer {
    fn init(freq_hz: u32);
    fn current_ticks() -> u64;
    fn set_oneshot(ticks: u64);
    fn ack();
}

/// Context switch trait
pub trait Context {
    type Registers;

    fn new_kernel(entry: fn(), stack: usize) -> Self::Registers;
    fn new_user(entry: usize, stack: usize, arg: usize) -> Self::Registers;
    fn switch(old: &mut Self::Registers, new: &Self::Registers);
}

// ... flags, errors, etc.
```

---

## Arch Implementation Example

```toml
# /crates/arch/efflux-arch-x86_64/Cargo.toml

[package]
name = "efflux-arch-x86_64"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
efflux-core.workspace = true
efflux-arch-traits.workspace = true
bitflags.workspace = true

[features]
default = []
```

```rust
// /crates/arch/efflux-arch-x86_64/src/lib.rs

#![no_std]
#![feature(asm_const)]

use efflux_arch_traits::{Arch, Mmu, Tlb, InterruptController, Timer, Context};

pub struct X86_64;

impl Arch for X86_64 {
    fn name() -> &'static str { "x86_64" }
    fn page_size() -> usize { 4096 }
    fn kernel_base() -> usize { 0xFFFF_8000_0000_0000 }
}

pub mod boot;
pub mod mm;
pub mod interrupt;
pub mod timer;
pub mod context;
pub mod syscall;
```

---

## Driver Trait Example

```toml
# /crates/drivers/efflux-driver-traits/Cargo.toml

[package]
name = "efflux-driver-traits"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
efflux-core.workspace = true
```

```rust
// /crates/drivers/efflux-driver-traits/src/lib.rs

#![no_std]

/// Serial port trait
pub trait Serial: Send + Sync {
    fn init(&mut self);
    fn putc(&mut self, c: u8);
    fn getc(&mut self) -> Option<u8>;
    fn puts(&mut self, s: &str) {
        for c in s.bytes() {
            self.putc(c);
        }
    }
}

/// Block device trait
pub trait BlockDevice: Send + Sync {
    fn block_size(&self) -> usize;
    fn block_count(&self) -> u64;
    fn read_blocks(&self, start: u64, buf: &mut [u8]) -> Result<(), BlockError>;
    fn write_blocks(&self, start: u64, buf: &[u8]) -> Result<(), BlockError>;
    fn flush(&self) -> Result<(), BlockError>;
}

/// Network device trait
pub trait NetworkDevice: Send + Sync {
    fn mac_address(&self) -> [u8; 6];
    fn mtu(&self) -> usize;
    fn send(&self, packet: &[u8]) -> Result<(), NetError>;
    fn recv(&self, buf: &mut [u8]) -> Result<usize, NetError>;
}

// ... errors, etc.
```

---

## App Example

```toml
# /apps/init/Cargo.toml

[package]
name = "efflux-init"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
efflux-libc = { path = "../../libc" }

[[bin]]
name = "init"
path = "src/main.rs"
```

```rust
// /apps/init/src/main.rs

#![no_std]
#![no_main]

extern crate efflux_libc;

use efflux_libc::{mount, fork, exec, wait, open, dup2};
use efflux_libc::{O_RDWR, STDIN_FILENO, STDOUT_FILENO, STDERR_FILENO};

#[no_mangle]
pub extern "C" fn main() -> i32 {
    // Mount filesystems
    mount("devfs", "/dev", "devfs", 0, core::ptr::null());
    mount("procfs", "/proc", "procfs", 0, core::ptr::null());
    mount("tmpfs", "/tmp", "tmpfs", 0, core::ptr::null());

    // Open console
    let console = open("/dev/console\0".as_ptr() as *const i8, O_RDWR);
    dup2(console, STDIN_FILENO);
    dup2(console, STDOUT_FILENO);
    dup2(console, STDERR_FILENO);

    // Spawn shell
    loop {
        let pid = fork();
        if pid == 0 {
            exec("/bin/sh\0".as_ptr() as *const i8, core::ptr::null());
        } else {
            wait(core::ptr::null_mut());
        }
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
```

---

## Libc

```toml
# /libc/Cargo.toml

[package]
name = "efflux-libc"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
crate-type = ["staticlib", "rlib"]
```

```rust
// /libc/src/lib.rs

#![no_std]
#![allow(non_camel_case_types)]

pub mod errno;
pub mod fcntl;
pub mod stdio;
pub mod stdlib;
pub mod string;
pub mod unistd;
pub mod sys;
pub mod signal;

pub use errno::*;
pub use fcntl::*;
pub use stdio::*;
pub use stdlib::*;
pub use string::*;
pub use unistd::*;
pub use sys::*;
pub use signal::*;

// Re-export common types
pub type c_char = i8;
pub type c_int = i32;
pub type c_long = i64;
pub type c_void = core::ffi::c_void;
pub type size_t = usize;
pub type ssize_t = isize;
pub type pid_t = i32;
pub type uid_t = u32;
pub type gid_t = u32;
```

---

## .cargo/config.toml

```toml
# /.cargo/config.toml

[build]
# Default target for `cargo build`
# Override with --target flag
target = "build/targets/x86_64-efflux.json"

[target.x86_64-efflux]
linker = "rust-lld"
rustflags = [
    "-C", "link-arg=-Tbuild/targets/x86_64-linker.ld",
    "-C", "code-model=kernel",
]

[target.aarch64-efflux]
linker = "rust-lld"
rustflags = [
    "-C", "link-arg=-Tbuild/targets/aarch64-linker.ld",
]

[target.riscv64-efflux]
linker = "rust-lld"
rustflags = [
    "-C", "link-arg=-Tbuild/targets/riscv64-linker.ld",
]

# UEFI targets
[target.x86_64-unknown-uefi]
runner = "tools/uefi-run"

[target.aarch64-unknown-uefi]
runner = "tools/uefi-run"

[unstable]
build-std = ["core", "alloc", "compiler_builtins"]
build-std-features = ["compiler-builtins-mem"]
```

---

## rust-toolchain.toml

```toml
# /rust-toolchain.toml

[toolchain]
channel = "nightly-2025-01-01"
components = [
    "rust-src",
    "llvm-tools-preview",
    "rustfmt",
    "clippy",
]
targets = [
    "x86_64-unknown-none",
    "x86_64-unknown-uefi",
    "aarch64-unknown-none",
    "aarch64-unknown-uefi",
    "riscv64gc-unknown-none-elf",
    "riscv32imac-unknown-none-elf",
]
```

---

## Directory Creation Script

```bash
#!/bin/bash
# tools/scripts/init-structure.sh

# Create directory structure
mkdir -p kernel/src
mkdir -p crates/core/{efflux-core,efflux-alloc,efflux-log}/src
mkdir -p crates/arch/efflux-arch-traits/src
mkdir -p crates/arch/efflux-arch-{x86_64,i686,aarch64,arm,mips64,mips32,riscv64,riscv32}/src
mkdir -p crates/mm/{efflux-mm-traits,efflux-mm-buddy,efflux-mm-slab,efflux-mm-vmm,efflux-mm-heap}/src
mkdir -p crates/sched/{efflux-sched-traits,efflux-sched-rr}/src
mkdir -p crates/process/{efflux-process,efflux-elf,efflux-signal}/src
mkdir -p crates/syscall/{efflux-syscall-traits,efflux-syscall}/src
mkdir -p crates/vfs/{efflux-vfs-traits,efflux-vfs}/src
mkdir -p crates/fs/{efflux-fs-tmpfs,efflux-fs-devfs,efflux-fs-procfs,efflux-fs-initramfs,efflux-fs-fat32,efflux-fs-effluxfs}/src
mkdir -p crates/drivers/efflux-driver-traits/src
mkdir -p crates/drivers/serial/{efflux-driver-uart-8250,efflux-driver-uart-pl011}/src
mkdir -p crates/drivers/block/efflux-driver-virtio-blk/src
mkdir -p crates/drivers/net/efflux-driver-virtio-net/src
mkdir -p crates/tty/{efflux-tty,efflux-pty}/src
mkdir -p crates/ipc/efflux-ipc-pipe/src
mkdir -p crates/net/{efflux-net-traits,efflux-net-stack}/src
mkdir -p bootloader/{efflux-boot-common,efflux-boot-uefi}/src
mkdir -p libc/src
mkdir -p apps/{init,shell,coreutils}/src
mkdir -p tools/{mkfs-efflux,mkimage,qemu-runner}/src
mkdir -p policies
mkdir -p build/{scripts,targets,images}

echo "Directory structure created!"
```

---

*EFFLUX Cargo Workspace Configuration*
