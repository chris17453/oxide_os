# OXIDE OS Makefile
#
# — GraveShift: The master orchestrator. One file to rule them all,
# ten includes to actually do the work. Touch the include order at your peril.
#
# Build and run the OXIDE operating system

# ========================================
# Configuration & Variables
# ========================================
include mk/config.mk

# ========================================
# Default target
# ========================================
all: build

# Build kernel + bootloader
build: kernel bootloader

# Build with userspace
build-full: kernel bootloader userspace initramfs

# ========================================
# Include all build subsystems
# ========================================
include mk/kernel.mk
include mk/userspace.mk
include mk/initramfs.mk
include mk/rootfs.mk
include mk/qemu.mk
include mk/toolchain.mk
include mk/test.mk
include mk/pkgmgr.mk
include mk/help.mk
