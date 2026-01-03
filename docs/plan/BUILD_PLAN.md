# EFFLUX Build Plan

**Status:** Draft
**Goal:** Source code → Bootable image for any architecture

---

## Overview

Building EFFLUX produces:
1. Kernel binary (ELF)
2. Bootloader (arch-specific)
3. Initramfs (apps, config)
4. Boot image (ISO, disk image, etc.)

---

## Build Targets

| Architecture | Rust Target | QEMU Machine | Image Type |
|--------------|-------------|--------------|------------|
| x86_64 | x86_64-efflux | q35 | ISO, GPT disk |
| i686 | i686-efflux | pc | ISO, GPT disk |
| aarch64 | aarch64-efflux | virt | GPT disk |
| arm | arm-efflux | virt | GPT disk |
| mips64 | mips64-efflux | malta | Raw binary |
| mips32 | mips32-efflux | malta | Raw binary |
| riscv64 | riscv64-efflux | virt | GPT disk |
| riscv32 | riscv32-efflux | virt | GPT disk |

---

## Build Stages

```
┌─────────────────────────────────────────────────────────┐
│                    1. TOOLCHAIN                         │
│  Install Rust nightly + targets + build tools           │
└─────────────────────────────────────────────────────────┘
                          │
                          v
┌─────────────────────────────────────────────────────────┐
│                    2. KERNEL                            │
│  Build kernel binary for target architecture            │
└─────────────────────────────────────────────────────────┘
                          │
                          v
┌─────────────────────────────────────────────────────────┐
│                   3. BOOTLOADER                         │
│  Build bootloader for target platform                   │
└─────────────────────────────────────────────────────────┘
                          │
                          v
┌─────────────────────────────────────────────────────────┐
│                    4. USERLAND                          │
│  Build libc + apps for target                           │
└─────────────────────────────────────────────────────────┘
                          │
                          v
┌─────────────────────────────────────────────────────────┐
│                   5. INITRAMFS                          │
│  Pack apps + config into cpio archive                   │
└─────────────────────────────────────────────────────────┘
                          │
                          v
┌─────────────────────────────────────────────────────────┐
│                   6. BOOT IMAGE                         │
│  Create bootable disk/ISO image                         │
└─────────────────────────────────────────────────────────┘
                          │
                          v
┌─────────────────────────────────────────────────────────┐
│                     7. TEST                             │
│  Boot in QEMU, run tests                                │
└─────────────────────────────────────────────────────────┘
```

---

## 1. Toolchain Setup

### Requirements

- Rust nightly
- Target toolchains for cross-compilation
- QEMU for testing
- Host tools (xorriso, mtools, etc.)

### rust-toolchain.toml

```toml
[toolchain]
channel = "nightly"
components = ["rust-src", "llvm-tools-preview"]
targets = [
    "x86_64-unknown-none",
    "i686-unknown-none",
    "aarch64-unknown-none",
    "armv7a-none-eabi",
    "mips64-unknown-none",
    "mips-unknown-none",
    "riscv64gc-unknown-none-elf",
    "riscv32imac-unknown-none-elf",
]
```

### Host Dependencies

```bash
# Debian/Ubuntu
apt install qemu-system-x86 qemu-system-arm qemu-system-misc \
    xorriso mtools gdisk dosfstools cpio

# Fedora
dnf install qemu-system-x86 qemu-system-arm qemu-system-riscv \
    xorriso mtools gdisk dosfstools cpio
```

---

## 2. Kernel Build

### Custom Target Specs

Each architecture needs a target spec in `build/targets/`:

```json
// build/targets/x86_64-efflux.json
{
    "llvm-target": "x86_64-unknown-none",
    "data-layout": "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128",
    "arch": "x86_64",
    "target-endian": "little",
    "target-pointer-width": "64",
    "target-c-int-width": "32",
    "os": "none",
    "executables": true,
    "linker-flavor": "ld.lld",
    "linker": "rust-lld",
    "panic-strategy": "abort",
    "disable-redzone": true,
    "features": "-mmx,-sse,+soft-float"
}
```

### Build Command

```bash
# Build kernel for x86_64
cargo build \
    --manifest-path kernel/Cargo.toml \
    --target build/targets/x86_64-efflux.json \
    --release

# Output: target/x86_64-efflux/release/efflux-kernel
```

### Linker Script

Each architecture has a linker script:

```ld
/* build/targets/x86_64-linker.ld */
ENTRY(_start)

SECTIONS {
    . = 0xFFFFFFFF80100000;  /* Kernel virtual base */

    .text : {
        *(.text._start)
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .data : {
        *(.data .data.*)
    }

    .bss : {
        *(.bss .bss.*)
    }
}
```

---

## 3. Bootloader Build

### UEFI Bootloader (x86_64, aarch64)

```bash
cargo build \
    --manifest-path bootloader/efflux-boot-uefi/Cargo.toml \
    --target x86_64-unknown-uefi \
    --release

# Output: target/x86_64-unknown-uefi/release/efflux-boot.efi
```

### BIOS Bootloader (i686)

Requires assembly + Rust:

```bash
# Build stage 1 (boot sector)
nasm -f bin bootloader/efflux-boot-bios/stage1.asm -o stage1.bin

# Build stage 2 (Rust)
cargo build \
    --manifest-path bootloader/efflux-boot-bios/Cargo.toml \
    --target i686-unknown-none \
    --release
```

### OpenSBI Payload (RISC-V)

```bash
# Build kernel as OpenSBI payload
cargo build \
    --manifest-path kernel/Cargo.toml \
    --target riscv64gc-unknown-none-elf \
    --release

# Package with OpenSBI
# Uses fw_payload.bin from opensbi build
```

---

## 4. Userland Build

### Libc

```bash
cargo build \
    --manifest-path libc/Cargo.toml \
    --target build/targets/x86_64-efflux.json \
    --release
```

### Apps

```bash
# Build all apps
cargo build \
    --manifest-path apps/Cargo.toml \
    --target build/targets/x86_64-efflux.json \
    --release

# Outputs:
# - target/x86_64-efflux/release/init
# - target/x86_64-efflux/release/sh
# - target/x86_64-efflux/release/ls
# - etc.
```

---

## 5. Initramfs Creation

### Directory Layout

```
initramfs/
├── bin/
│   ├── init
│   ├── sh
│   ├── ls
│   ├── cat
│   └── ...
├── dev/          # Empty, devfs mounted here
├── proc/         # Empty, procfs mounted here
├── tmp/          # Empty, tmpfs mounted here
├── etc/
│   ├── passwd
│   ├── group
│   └── init.rc   # Init config
└── lib/
    └── libc.so   # If dynamic linking
```

### Create CPIO Archive

```bash
# tools/scripts/mkinitramfs.sh

#!/bin/bash
ARCH=$1
OUT=$2

INITRAMFS_DIR=$(mktemp -d)

# Copy binaries
mkdir -p $INITRAMFS_DIR/{bin,dev,proc,tmp,etc,lib}

for bin in init sh ls cat cp mv rm mkdir rmdir pwd echo; do
    cp target/$ARCH-efflux/release/$bin $INITRAMFS_DIR/bin/
done

# Create etc files
echo "root:x:0:0:root:/:/bin/sh" > $INITRAMFS_DIR/etc/passwd
echo "root:x:0:" > $INITRAMFS_DIR/etc/group

# Create cpio archive
cd $INITRAMFS_DIR
find . | cpio -o -H newc | gzip > $OUT

rm -rf $INITRAMFS_DIR
```

---

## 6. Boot Image Creation

### x86_64/i686 UEFI ISO

```bash
# tools/scripts/mkiso-uefi.sh

#!/bin/bash
KERNEL=$1
INITRAMFS=$2
OUTPUT=$3

ISO_DIR=$(mktemp -d)

# Create EFI structure
mkdir -p $ISO_DIR/EFI/BOOT
mkdir -p $ISO_DIR/efflux

# Copy bootloader
cp bootloader/target/x86_64-unknown-uefi/release/efflux-boot.efi \
   $ISO_DIR/EFI/BOOT/BOOTX64.EFI

# Copy kernel and initramfs
cp $KERNEL $ISO_DIR/efflux/kernel
cp $INITRAMFS $ISO_DIR/efflux/initramfs.cpio.gz

# Create ISO
xorriso -as mkisofs \
    -o $OUTPUT \
    -e EFI/BOOT/BOOTX64.EFI \
    -no-emul-boot \
    $ISO_DIR

rm -rf $ISO_DIR
```

### GPT Disk Image

```bash
# tools/scripts/mkdisk.sh

#!/bin/bash
KERNEL=$1
INITRAMFS=$2
OUTPUT=$3
SIZE_MB=${4:-256}

# Create empty disk
dd if=/dev/zero of=$OUTPUT bs=1M count=$SIZE_MB

# Create GPT
sgdisk -o $OUTPUT
sgdisk -n 1:2048:+100M -t 1:ef00 -c 1:"EFI" $OUTPUT   # EFI partition
sgdisk -n 2:0:0 -t 2:8300 -c 2:"EFFLUX" $OUTPUT       # Root partition

# Format EFI partition
LOOP=$(losetup -f --show -P $OUTPUT)
mkfs.fat -F 32 ${LOOP}p1

# Mount and populate EFI
mount ${LOOP}p1 /mnt
mkdir -p /mnt/EFI/BOOT
cp bootloader/target/*/release/efflux-boot.efi /mnt/EFI/BOOT/BOOTX64.EFI
cp $KERNEL /mnt/kernel
cp $INITRAMFS /mnt/initramfs.cpio.gz
umount /mnt

# Format root partition with effluxfs (when available)
# mkfs.efflux ${LOOP}p2

losetup -d $LOOP
```

---

## 7. QEMU Testing

### Run Scripts

```bash
# build/scripts/run-qemu.sh

#!/bin/bash
ARCH=$1
IMAGE=$2

case $ARCH in
    x86_64)
        qemu-system-x86_64 \
            -machine q35 \
            -m 512M \
            -bios /usr/share/OVMF/OVMF_CODE.fd \
            -drive file=$IMAGE,format=raw \
            -serial stdio \
            -no-reboot
        ;;
    aarch64)
        qemu-system-aarch64 \
            -machine virt \
            -cpu cortex-a72 \
            -m 512M \
            -bios /usr/share/AAVMF/AAVMF_CODE.fd \
            -drive file=$IMAGE,format=raw,if=virtio \
            -serial stdio \
            -no-reboot
        ;;
    riscv64)
        qemu-system-riscv64 \
            -machine virt \
            -m 512M \
            -bios /usr/share/opensbi/generic/fw_jump.elf \
            -kernel kernel \
            -initrd initramfs.cpio.gz \
            -serial stdio \
            -no-reboot
        ;;
    # ... other architectures
esac
```

---

## Policies

Policies define what goes into an image:

### policies/default.toml

```toml
[build]
arch = "x86_64"
profile = "release"

[kernel]
features = ["smp", "stats"]

[apps]
include = [
    "init",
    "shell",
    "coreutils",
]

[image]
type = "gpt-disk"
size_mb = 256
filesystem = "effluxfs"

[initramfs]
include = [
    "bin/*",
    "etc/passwd",
    "etc/group",
]
```

### policies/minimal.toml

```toml
[build]
arch = "x86_64"
profile = "release"

[kernel]
features = []

[apps]
include = [
    "init",
]

[image]
type = "iso"
size_mb = 64
```

### policies/embedded.toml

```toml
[build]
arch = "arm"
profile = "release"

[kernel]
features = ["nommu"]

[apps]
include = [
    "init",
]

[image]
type = "raw"
size_mb = 16
```

---

## Master Build Script

```bash
#!/bin/bash
# build/scripts/build-all.sh

POLICY=${1:-policies/default.toml}

# Parse policy
ARCH=$(toml get $POLICY build.arch)
PROFILE=$(toml get $POLICY build.profile)

echo "Building EFFLUX for $ARCH ($PROFILE)"

# 1. Build kernel
cargo build \
    --manifest-path kernel/Cargo.toml \
    --target build/targets/$ARCH-efflux.json \
    --$PROFILE

# 2. Build bootloader
case $ARCH in
    x86_64|aarch64)
        cargo build \
            --manifest-path bootloader/efflux-boot-uefi/Cargo.toml \
            --target $ARCH-unknown-uefi \
            --$PROFILE
        ;;
    # ... other cases
esac

# 3. Build userland
cargo build \
    --manifest-path apps/Cargo.toml \
    --target build/targets/$ARCH-efflux.json \
    --$PROFILE

# 4. Create initramfs
./build/scripts/mkinitramfs.sh $ARCH build/images/initramfs.cpio.gz

# 5. Create boot image
IMAGE_TYPE=$(toml get $POLICY image.type)
case $IMAGE_TYPE in
    iso)
        ./build/scripts/mkiso-uefi.sh \
            target/$ARCH-efflux/release/efflux-kernel \
            build/images/initramfs.cpio.gz \
            build/images/efflux-$ARCH.iso
        ;;
    gpt-disk)
        ./build/scripts/mkdisk.sh \
            target/$ARCH-efflux/release/efflux-kernel \
            build/images/initramfs.cpio.gz \
            build/images/efflux-$ARCH.img
        ;;
esac

echo "Done: build/images/efflux-$ARCH.*"
```

---

## CI Pipeline

```yaml
# .github/workflows/build.yml
name: Build

on: [push, pull_request]

jobs:
  build:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        arch: [x86_64, aarch64, riscv64]

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable
        with:
          toolchain: nightly
          components: rust-src, llvm-tools-preview

      - name: Install QEMU
        run: sudo apt install qemu-system-${{ matrix.arch }}

      - name: Build
        run: ./build/scripts/build-all.sh policies/ci-${{ matrix.arch }}.toml

      - name: Test
        run: ./build/scripts/run-qemu.sh ${{ matrix.arch }} build/images/efflux-${{ matrix.arch }}.img --test
```

---

## Quick Start

```bash
# Build x86_64 with defaults
./build/scripts/build-all.sh

# Build for specific architecture
./build/scripts/build-all.sh policies/aarch64.toml

# Run in QEMU
./build/scripts/run-qemu.sh x86_64 build/images/efflux-x86_64.img
```

---

*EFFLUX Build Plan*
