# OXIDE Cargo Workspace

This document shows the Cargo workspace configuration for OXIDE.

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
    "crates/core/core",
    "crates/core/alloc",
    "crates/core/log",

    # Architecture crates
    "crates/arch/arch-traits",
    "crates/arch/arch-x86_64",
    "crates/arch/arch-i686",
    "crates/arch/arch-aarch64",
    "crates/arch/arch-arm",
    "crates/arch/arch-mips64",
    "crates/arch/arch-mips32",
    "crates/arch/arch-riscv64",
    "crates/arch/arch-riscv32",

    # Memory management
    "crates/mm/mm-traits",
    "crates/mm/mm-buddy",
    "crates/mm/mm-slab",
    "crates/mm/mm-vmm",
    "crates/mm/mm-heap",

    # Scheduler
    "crates/sched/sched-traits",
    "crates/sched/sched-rr",

    # Process
    "crates/process/process",
    "crates/process/elf",
    "crates/process/signal",

    # Syscall
    "crates/syscall/syscall-traits",
    "crates/syscall/syscall",

    # VFS
    "crates/vfs/vfs-traits",
    "crates/vfs/vfs",

    # Filesystems
    "crates/fs/fs-tmpfs",
    "crates/fs/fs-devfs",
    "crates/fs/fs-procfs",
    "crates/fs/fs-initramfs",
    "crates/fs/fs-fat32",
    "crates/fs/fs-oxidefs",

    # Drivers
    "crates/drivers/driver-traits",
    "crates/drivers/serial/driver-uart-8250",
    "crates/drivers/serial/driver-uart-pl011",
    "crates/drivers/block/driver-virtio-blk",
    "crates/drivers/net/driver-virtio-net",

    # TTY
    "crates/tty/tty",
    "crates/tty/pty",

    # IPC
    "crates/ipc/ipc-pipe",

    # Network
    "crates/net/net-traits",
    "crates/net/net-stack",

    # Bootloader
    "bootloader/boot-common",
    "bootloader/boot-uefi",

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
repository = "https://github.com/user/oxide"

[workspace.dependencies]
# Shared no_std dependencies
bitflags = "2.4"
spin = "0.9"
log = { version = "0.4", default-features = false }

# Internal crates (version = workspace)
core = { path = "crates/core/core" }
alloc = { path = "crates/core/alloc" }
log = { path = "crates/core/log" }
arch-traits = { path = "crates/arch/arch-traits" }
mm-traits = { path = "crates/mm/mm-traits" }
sched-traits = { path = "crates/sched/sched-traits" }
vfs-traits = { path = "crates/vfs/vfs-traits" }
driver-traits = { path = "crates/drivers/driver-traits" }

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
name = "kernel"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
# Core
core.workspace = true
alloc.workspace = true
log.workspace = true

# Memory
mm-traits.workspace = true
mm-buddy = { path = "../crates/mm/mm-buddy" }
mm-slab = { path = "../crates/mm/mm-slab" }
mm-vmm = { path = "../crates/mm/mm-vmm" }
mm-heap = { path = "../crates/mm/mm-heap" }

# Scheduler
sched-traits.workspace = true
sched-rr = { path = "../crates/sched/sched-rr" }

# Process
process = { path = "../crates/process/process" }
elf = { path = "../crates/process/elf" }
signal = { path = "../crates/process/signal" }

# Syscall
syscall = { path = "../crates/syscall/syscall" }

# VFS
vfs = { path = "../crates/vfs/vfs" }
fs-tmpfs = { path = "../crates/fs/fs-tmpfs" }
fs-devfs = { path = "../crates/fs/fs-devfs" }
fs-initramfs = { path = "../crates/fs/fs-initramfs" }

# TTY
tty = { path = "../crates/tty/tty" }

# Architecture-specific (conditional)
[target.'cfg(target_arch = "x86_64")'.dependencies]
arch = { package = "arch-x86_64", path = "../crates/arch/arch-x86_64" }
driver-serial = { package = "driver-uart-8250", path = "../crates/drivers/serial/driver-uart-8250" }

[target.'cfg(target_arch = "aarch64")'.dependencies]
arch = { package = "arch-aarch64", path = "../crates/arch/arch-aarch64" }
driver-serial = { package = "driver-uart-pl011", path = "../crates/drivers/serial/driver-uart-pl011" }

[target.'cfg(target_arch = "riscv64")'.dependencies]
arch = { package = "arch-riscv64", path = "../crates/arch/arch-riscv64" }

# ... other architectures

[features]
default = []
smp = ["mm-buddy/smp", "sched-rr/smp"]
stats = ["mm-buddy/stats"]
```

---

## Trait Crate Example

```toml
# /crates/arch/arch-traits/Cargo.toml

[package]
name = "arch-traits"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
core.workspace = true

[lib]
# No std!
```

```rust
// /crates/arch/arch-traits/src/lib.rs

#![no_std]

use core::PhysAddr;

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
# /crates/arch/arch-x86_64/Cargo.toml

[package]
name = "arch-x86_64"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
core.workspace = true
arch-traits.workspace = true
bitflags.workspace = true

[features]
default = []
```

```rust
// /crates/arch/arch-x86_64/src/lib.rs

#![no_std]
#![feature(asm_const)]

use arch_traits::{Arch, Mmu, Tlb, InterruptController, Timer, Context};

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
# /crates/drivers/driver-traits/Cargo.toml

[package]
name = "driver-traits"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
core.workspace = true
```

```rust
// /crates/drivers/driver-traits/src/lib.rs

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
name = "init"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
libc = { path = "../../libc" }

[[bin]]
name = "init"
path = "src/main.rs"
```

```rust
// /apps/init/src/main.rs

#![no_std]
#![no_main]

extern crate libc;

use libc::{mount, fork, exec, wait, open, dup2};
use libc::{O_RDWR, STDIN_FILENO, STDOUT_FILENO, STDERR_FILENO};

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
name = "libc"
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
target = "build/targets/x86_64-oxide.json"

[target.x86_64-oxide]
linker = "rust-lld"
rustflags = [
    "-C", "link-arg=-Tbuild/targets/x86_64-linker.ld",
    "-C", "code-model=kernel",
]

[target.aarch64-oxide]
linker = "rust-lld"
rustflags = [
    "-C", "link-arg=-Tbuild/targets/aarch64-linker.ld",
]

[target.riscv64-oxide]
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
mkdir -p crates/core/{core,alloc,log}/src
mkdir -p crates/arch/arch-traits/src
mkdir -p crates/arch/arch-{x86_64,i686,aarch64,arm,mips64,mips32,riscv64,riscv32}/src
mkdir -p crates/mm/{mm-traits,mm-buddy,mm-slab,mm-vmm,mm-heap}/src
mkdir -p crates/sched/{sched-traits,sched-rr}/src
mkdir -p crates/process/{process,elf,signal}/src
mkdir -p crates/syscall/{syscall-traits,syscall}/src
mkdir -p crates/vfs/{vfs-traits,vfs}/src
mkdir -p crates/fs/{fs-tmpfs,fs-devfs,fs-procfs,fs-initramfs,fs-fat32,fs-oxidefs}/src
mkdir -p crates/drivers/driver-traits/src
mkdir -p crates/drivers/serial/{driver-uart-8250,driver-uart-pl011}/src
mkdir -p crates/drivers/block/driver-virtio-blk/src
mkdir -p crates/drivers/net/driver-virtio-net/src
mkdir -p crates/tty/{tty,pty}/src
mkdir -p crates/ipc/ipc-pipe/src
mkdir -p crates/net/{net-traits,net-stack}/src
mkdir -p bootloader/{boot-common,boot-uefi}/src
mkdir -p libc/src
mkdir -p apps/{init,shell,coreutils}/src
mkdir -p tools/{mkfs-oxide,mkimage,qemu-runner}/src
mkdir -p policies
mkdir -p build/{scripts,targets,images}

echo "Directory structure created!"
```

---

*OXIDE Cargo Workspace Configuration*
