# — SableWire: The config that holds this whole circus together.
# Touch these variables wrong and watch the entire build unravel like cheap solder.

SHELL := /usr/bin/bash

# Configuration
ARCH ?= x86_64
PROFILE ?= debug
QEMU_TIMEOUT ?= 15
LINKER ?= ld.lld

# ========================================
# DEBUG FEATURES (toggle here)
# ========================================
# Enable debug output for 'make run'
# Options: debug-input, debug-mouse, debug-sched, debug-fork, debug-lock, debug-syscall-perf, debug-tty-read, debug-all
# Combine multiple: debug-input,debug-mouse
# Disable all: leave empty or comment out
# debug-syscall-perf: Logs syscalls taking >100K CPU cycles (~33us @ 3GHz)
# debug-tty-read: Logs TTY read queue status
# ========================================
RUN_KERNEL_FEATURES ?=
# ========================================

# Internal: Don't modify these unless using specific debug targets
KERNEL_FEATURES ?=

# QEMU command auto-detection (can be overridden with QEMU=command)
# — NeonRoot: Prefer qemu-system-x86_64 — it supports all features (including fat: protocol)
# Note: qemu-kvm on RHEL doesn't support fat: protocol needed for boot testing
# On RHEL 10: sudo dnf install qemu-system-x86
QEMU ?= $(shell \
	if command -v qemu-system-x86_64 >/dev/null 2>&1; then \
		echo "qemu-system-x86_64"; \
	else \
		echo "qemu-system-x86_64"; \
	fi)

# Randomized host port for SSH forwarding (override via SSH_HOST_PORT=2223, etc.)
SSH_HOST_PORT ?= $(shell python3 -c 'import socket; s = socket.socket(); s.bind(("", 0)); print(s.getsockname()[1])')

# Paths
TARGET_DIR := target
KERNEL_TARGET := $(TARGET_DIR)/$(ARCH)-unknown-oxide/$(PROFILE)/kernel
BOOTLOADER_TARGET := $(TARGET_DIR)/$(ARCH)-unknown-uefi/$(PROFILE)/boot-uefi.efi
BOOT_DIR := $(TARGET_DIR)/boot
INITRAMFS := $(TARGET_DIR)/initramfs.cpio
OVMF := $(shell for p in /usr/share/OVMF/OVMF_CODE.fd /usr/share/edk2-ovmf/x64/OVMF_CODE.fd /usr/share/edk2/ovmf/OVMF_CODE.fd /usr/share/qemu/OVMF.fd; do [ -f "$$p" ] && echo "$$p" && break; done)
CONFIG_DIR := build
RUN_CONFIG := $(CONFIG_DIR)/run.local.mk
USERSPACE_CONFIG := $(CONFIG_DIR)/userspace-packages.mk
-include $(RUN_CONFIG)
-include $(USERSPACE_CONFIG)

# Userspace configuration - use standard target with Fedora's pre-built std
USERSPACE_TARGET := x86_64-unknown-none
USERSPACE_OUT := $(TARGET_DIR)/$(USERSPACE_TARGET)/$(PROFILE)
USERSPACE_OUT_RELEASE := $(TARGET_DIR)/$(USERSPACE_TARGET)/release
CARGO_USER_FLAGS :=
RUN_BUILD_USERSPACE ?= 1

# — IronGhost: std-enabled userspace configuration (Rust std via -Zbuild-std)
USERSPACE_STD_TARGET_JSON := targets/x86_64-unknown-oxide-user.json
USERSPACE_STD_OUT := $(TARGET_DIR)/x86_64-unknown-oxide-user/$(PROFILE)
USERSPACE_STD_OUT_RELEASE := $(TARGET_DIR)/x86_64-unknown-oxide-user/release
OXIDE_SYSROOT := $(TARGET_DIR)/oxide-sysroot

ifeq ($(RUN_BUILD_USERSPACE),1)
INITRAMFS_PREREQ := userspace-release
else
INITRAMFS_PREREQ :=
endif

# Userspace packages to build (Cargo-based)
USERSPACE_ALL_PACKAGES := init esh getty login coreutils ssh sshd rdpd service networkd resolvd journald journalctl soundd evtest argtest htop doom gwbasic curses-demo
USERSPACE_PACKAGES ?= $(USERSPACE_ALL_PACKAGES)
# Non-Cargo extra targets (built via dedicated rules)
USERSPACE_EXTRA_TARGETS_ALL := tls-test thread-test
USERSPACE_EXTRA_TARGETS ?= $(USERSPACE_EXTRA_TARGETS_ALL)

# Coreutils binaries (auto-detected from Cargo.toml [[bin]] entries)
# — PulseForge: Grep-based detection because hardcoding bin lists is a recipe for drift.
COREUTILS_BINS := $(shell grep -A1 '^\[\[bin\]\]' userspace/coreutils/Cargo.toml | grep '^name' | sed 's/.*= *"\([^"]*\)".*/\1/' | tr '\n' ' ')

# Disk image configuration for root filesystem
ROOTFS_IMAGE := $(TARGET_DIR)/oxide-disk.img
ROOTFS_SIZE := 800
BOOT_SIZE := 64
ROOT_SIZE := 384
HOME_SIZE := 64
BOOT_START := 1
ROOT_START := 65
HOME_START := 449

# Toolchain install prefix
INSTALL_PREFIX ?= /usr/local/oxide
