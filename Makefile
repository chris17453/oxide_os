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
.PHONY: all build build-full increment-build

all: build

# Build everything needed for a bootable system (auto-increments build number)
# — PatchBay: If it builds, it boots. No half-baked images, no stale binaries.
build: increment-build kernel bootloader userspace-release initramfs userspace-std

# Increment build number — one tick per build, no exceptions
# — PatchBay: the counter that never lies
increment-build:
	@BUILD=$$(cat build/build-number 2>/dev/null || echo 0) && \
	BUILD=$$((BUILD + 1)) && \
	echo $$BUILD > build/build-number && \
	echo "Build $(OXIDE_VERSION).$$BUILD"

# Build with userspace
build-full: increment-build kernel bootloader userspace initramfs

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
