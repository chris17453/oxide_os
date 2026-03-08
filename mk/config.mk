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
# Shorthand: make run DEBUG=all
# Options: all, input, mouse, sched, fork, lock, syscall-perf, tty-read
# Combine: make run DEBUG=sched,fork
# Old syntax still works: make run KERNEL_FEATURES=debug-all
# ========================================
DEBUG ?=
RUN_KERNEL_FEATURES ?=
# Kernel command line options (passed to boot.cfg)
# Usage: make run KERNEL_CMDLINE="console=ttyS0"
#        make run KERNEL_CMDLINE="console=tty2 quiet nosmp"
# — GraveShift: Linux-compatible console= redirection — just like GRUB
KERNEL_CMDLINE ?=
# ========================================

# — SableWire: Map DEBUG= shorthand to KERNEL_FEATURES=debug-<value>.
# "DEBUG=all" becomes "debug-all". "DEBUG=sched,fork" becomes "debug-sched,debug-fork".
# Existing KERNEL_FEATURES takes precedence if set explicitly.
KERNEL_FEATURES ?=
ifdef DEBUG
ifneq ($(DEBUG),)
override KERNEL_FEATURES := $(shell echo "$(DEBUG)" | sed 's/,/,debug-/g; s/^/debug-/')
endif
endif

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
# — SableWire: KERNEL_TARGET and BOOTLOADER_TARGET use = (recursive) not := (immediate).
# With :=, $(PROFILE) is locked to 'debug' at parse time, ignoring target-specific
# overrides like 'run-release: PROFILE = release'. Recursive = re-evaluates at use time.
TARGET_DIR := target
KERNEL_TARGET = $(TARGET_DIR)/$(ARCH)-unknown-oxide/$(PROFILE)/kernel
BOOTLOADER_TARGET = $(TARGET_DIR)/$(ARCH)-unknown-uefi/$(PROFILE)/boot-uefi.efi
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

# OXIDE version (extracted from workspace Cargo.toml or git tag)
# — PatchBay: single source of truth for the version string
OXIDE_VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*= *"\([^"]*\)".*/\1/')

# Build number — NT-style auto-incrementing integer
# — PatchBay: use recursive expansion so targets that run after increment-build
# see the freshly bumped value in the same make invocation.
OXIDE_BUILD = $(shell cat build/build-number 2>/dev/null || echo 0)
OXIDE_FULL_VERSION = $(OXIDE_VERSION).$(OXIDE_BUILD)

# Serial log capture — piped through tee so you still see it in the terminal
# — SableWire: the flight recorder that outlives the crash
SERIAL_LOG := $(TARGET_DIR)/serial.log
BUILD_ARCHIVE_DIR = BUILD/$(OXIDE_BUILD)

# Disk image configuration for root filesystem
ROOTFS_IMAGE := $(TARGET_DIR)/oxide-disk.img
ROOTFS_SIZE := 896
BOOT_SIZE := 128
ROOT_SIZE := 384
HOME_SIZE := 64
BOOT_START := 1
ROOT_START := 129
HOME_START := 513

# — PatchBay: kernel archive — accumulates builds so the boot manager can offer rollback
KERNEL_ARCHIVE := build/kernels
MAX_KERNEL_ARCHIVE := 2

# Toolchain install prefix
INSTALL_PREFIX ?= /usr/local/oxide
