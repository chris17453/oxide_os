# OXIDE OS Makefile
#
# Build and test the OXIDE operating system

SHELL := /usr/bin/bash

.PHONY: all build build-full kernel bootloader userspace userspace-release userspace-pkg initramfs initramfs-debug initramfs-minimal boot-dir boot-quick boot-image create-rootfs release clean run run-fedora run-rhel run-kvm detect-qemu-mode test check fmt fmt-check clippy list-bins show-config help toolchain install-toolchain test-toolchain clean-toolchain external-libs zlib openssl xz zstd cpython tls-test thread-test vim claude

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
# Prefer qemu-system-x86_64 as it supports all features (including fat: protocol)
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
KERNEL_TARGET := $(TARGET_DIR)/$(ARCH)-unknown-none/$(PROFILE)/kernel
BOOTLOADER_TARGET := $(TARGET_DIR)/$(ARCH)-unknown-uefi/$(PROFILE)/boot-uefi.efi
BOOT_DIR := $(TARGET_DIR)/boot
INITRAMFS := $(TARGET_DIR)/initramfs.cpio
OVMF := $(shell for p in /usr/share/OVMF/OVMF_CODE.fd /usr/share/edk2-ovmf/x64/OVMF_CODE.fd /usr/share/edk2/ovmf/OVMF_CODE.fd /usr/share/qemu/OVMF.fd; do [ -f "$$p" ] && echo "$$p" && break; done)

# Userspace configuration - use standard target with Fedora's pre-built std
USERSPACE_TARGET := x86_64-unknown-none
USERSPACE_OUT := $(TARGET_DIR)/$(USERSPACE_TARGET)/$(PROFILE)
USERSPACE_OUT_RELEASE := $(TARGET_DIR)/$(USERSPACE_TARGET)/release
CARGO_USER_FLAGS :=

# Userspace packages to build
USERSPACE_PACKAGES := init esh getty login coreutils ssh sshd rdpd service networkd journald journalctl soundd evtest argtest htop doom

# Coreutils binaries (auto-detected from Cargo.toml [[bin]] entries)
# Extract binary names from [[bin]] sections in coreutils/Cargo.toml
COREUTILS_BINS := $(shell grep -A1 '^\[\[bin\]\]' userspace/coreutils/Cargo.toml | grep '^name' | sed 's/.*= *"\([^"]*\)".*/\1/' | tr '\n' ' ')

# Default target
all: build

# Build everything
build: kernel bootloader

# Build with userspace
build-full: kernel bootloader userspace initramfs

# Build kernel
# Pass KERNEL_FEATURES to enable debug output, e.g.: make run KERNEL_FEATURES=debug-all
kernel:
	@echo "Building kernel..."
ifeq ($(PROFILE),release)
ifneq ($(KERNEL_FEATURES),)
	@cargo build --package kernel --release --features $(KERNEL_FEATURES)
else
	@cargo build --package kernel --release
endif
else
ifneq ($(KERNEL_FEATURES),)
	@cargo build --package kernel --features $(KERNEL_FEATURES)
else
	@cargo build --package kernel
endif
endif

# Build bootloader
bootloader:
	@echo "Building bootloader..."
ifeq ($(PROFILE),release)
	@cargo build --package boot-uefi --target $(ARCH)-unknown-uefi --release -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem
else
	@cargo build --package boot-uefi --target $(ARCH)-unknown-uefi -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem
endif

# Build release
release:
	cargo build --package kernel --release
	cargo build --package boot-uefi --target $(ARCH)-unknown-uefi --release

# Build all userspace programs
userspace:
	@echo "Building userspace programs..."
	@for pkg in $(USERSPACE_PACKAGES); do \
		echo "  Building $$pkg..."; \
		RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static -C link-arg=-Tuserspace/userspace.ld -C link-arg=-e_start" cargo build --package $$pkg --target $(USERSPACE_TARGET) $(CARGO_USER_FLAGS) || exit 1; \
	done
	@echo "Userspace programs built."

# Build optimized userspace (smaller binaries)
userspace-release:
	@echo "Building userspace programs (release)..."
	@# Check if libc has changed - if so, force rebuild of all userspace
	@if [ -d "$(USERSPACE_OUT_RELEASE)" ]; then \
		LIBC_CHANGED=$$(find userspace/libs/libc/src -name "*.rs" -newer "$(USERSPACE_OUT_RELEASE)/init" 2>/dev/null | head -1); \
		if [ -n "$$LIBC_CHANGED" ]; then \
			echo "  libc changed - cleaning userspace binaries to force relink..."; \
			rm -rf $(USERSPACE_OUT_RELEASE)/*; \
		fi; \
	fi
	@for pkg in $(USERSPACE_PACKAGES); do \
		echo "  Building $$pkg (release)..."; \
		RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static -C link-arg=-Tuserspace/userspace.ld -C link-arg=-e_start" cargo build --package $$pkg --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS) || exit 1; \
	done
	@echo "  Building gwbasic (release)..."
	@RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static -C link-arg=-Tuserspace/userspace.ld -C link-arg=-e_start" cargo build --package oxide-gwbasic --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS) --features oxide || exit 1
	@echo "  Building curses-demo (release)..."
	@RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static -C link-arg=-Tuserspace/userspace.ld -C link-arg=-e_start" cargo build --package curses-demo --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS) || exit 1
	@echo "  Building htop (release)..."
	@RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static -C link-arg=-Tuserspace/userspace.ld -C link-arg=-e_start" cargo build --package htop --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS) || exit 1
	@echo "  Building testcolors (release)..."
	@RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static -C link-arg=-Tuserspace/userspace.ld -C link-arg=-e_start" cargo build --package coreutils --bin testcolors --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS) || exit 1
	@echo "  Building TLS test..."
	@$(MAKE) tls-test
	@echo "  Building thread test..."
	@$(MAKE) thread-test
	@echo "Stripping binaries..."
	@for prog in init esh login getty gwbasic curses-demo htop tls-test thread-test ssh sshd rdpd service networkd journald journalctl soundd evtest argtest doom $(COREUTILS_BINS); do \
		if [ -f "$(USERSPACE_OUT_RELEASE)/$$prog" ]; then \
			strip "$(USERSPACE_OUT_RELEASE)/$$prog" 2>/dev/null || true; \
		fi; \
	done
	@echo "Userspace programs built (release)."

# Build a single userspace package (usage: make userspace-pkg PKG=coreutils)
userspace-pkg:
	@if [ -z "$(PKG)" ]; then echo "Usage: make userspace-pkg PKG=<package>"; exit 1; fi
	cargo build --package $(PKG) --target $(USERSPACE_TARGET) $(CARGO_USER_FLAGS)

# Create initramfs CPIO archive (release version for smaller size)
initramfs: userspace-release
	@echo "Creating initramfs (release)..."
	@rm -rf $(TARGET_DIR)/initramfs
	@mkdir -p $(TARGET_DIR)/initramfs/bin
	@mkdir -p $(TARGET_DIR)/initramfs/sbin
	@mkdir -p $(TARGET_DIR)/initramfs/etc
	@mkdir -p $(TARGET_DIR)/initramfs/dev
	@mkdir -p $(TARGET_DIR)/initramfs/proc
	@mkdir -p $(TARGET_DIR)/initramfs/sys
	@mkdir -p $(TARGET_DIR)/initramfs/tmp
	@mkdir -p $(TARGET_DIR)/initramfs/var/log
	@mkdir -p $(TARGET_DIR)/initramfs/var/lib/dhcp
	@mkdir -p $(TARGET_DIR)/initramfs/var/run
	@mkdir -p $(TARGET_DIR)/initramfs/home
	@mkdir -p $(TARGET_DIR)/initramfs/root
	@mkdir -p $(TARGET_DIR)/initramfs/run
	@mkdir -p $(TARGET_DIR)/initramfs/run/network
	@# Copy init to /sbin
	@cp "$(USERSPACE_OUT_RELEASE)/init" "$(TARGET_DIR)/initramfs/sbin/init"
	@ln -sf /sbin/init "$(TARGET_DIR)/initramfs/init"
	@# Copy shell
	@cp "$(USERSPACE_OUT_RELEASE)/esh" "$(TARGET_DIR)/initramfs/bin/esh"
	@ln -sf /bin/esh "$(TARGET_DIR)/initramfs/bin/sh"
	@# Copy login
	@cp "$(USERSPACE_OUT_RELEASE)/login" "$(TARGET_DIR)/initramfs/bin/login"
	@# Copy gwbasic
	@cp "$(USERSPACE_OUT_RELEASE)/gwbasic" "$(TARGET_DIR)/initramfs/bin/gwbasic"
	@# Copy curses-demo
	@cp "$(USERSPACE_OUT_RELEASE)/curses-demo" "$(TARGET_DIR)/initramfs/bin/curses-demo"
	@# Copy htop
	@cp "$(USERSPACE_OUT_RELEASE)/htop" "$(TARGET_DIR)/initramfs/bin/htop"
	@# Copy BASIC example programs
	@mkdir -p "$(TARGET_DIR)/initramfs/usr/share/gwbasic"
	@cp userspace/apps/gwbasic/examples/*.bas "$(TARGET_DIR)/initramfs/usr/share/gwbasic/" 2>/dev/null || true
	@# Copy ssh client
	@cp "$(USERSPACE_OUT_RELEASE)/ssh" "$(TARGET_DIR)/initramfs/bin/ssh"
	@# Copy sshd
	@cp "$(USERSPACE_OUT_RELEASE)/sshd" "$(TARGET_DIR)/initramfs/bin/sshd"
	@# Copy service manager
	@cp "$(USERSPACE_OUT_RELEASE)/service" "$(TARGET_DIR)/initramfs/bin/service"
	@ln -sf /bin/service "$(TARGET_DIR)/initramfs/bin/servicemgr"
	@# Copy coreutils
	@for prog in $(COREUTILS_BINS); do \
		if [ -f "$(USERSPACE_OUT_RELEASE)/$$prog" ]; then \
			cp "$(USERSPACE_OUT_RELEASE)/$$prog" "$(TARGET_DIR)/initramfs/bin/"; \
		fi; \
	done
	@# Copy testcolors
	@if [ -f "$(USERSPACE_OUT_RELEASE)/testcolors" ]; then \
		cp "$(USERSPACE_OUT_RELEASE)/testcolors" "$(TARGET_DIR)/initramfs/bin/"; \
	fi
	@# Create symlinks for common aliases
	@ln -sf /bin/true "$(TARGET_DIR)/initramfs/bin/:" 2>/dev/null || true
	@ln -sf /bin/ls "$(TARGET_DIR)/initramfs/bin/dir" 2>/dev/null || true
	@# Create etc files - passwd format: user:pass:uid:gid:gecos:home:shell
	@echo "root:root:0:0:root:/root:/bin/esh" > $(TARGET_DIR)/initramfs/etc/passwd
	@echo "nobody:x:65534:65534:Nobody:/:/bin/false" >> $(TARGET_DIR)/initramfs/etc/passwd
	@echo "sshd:x:74:74:SSH Daemon:/var/empty/sshd:/bin/false" >> $(TARGET_DIR)/initramfs/etc/passwd
	@echo "rdp:x:75:75:RDP Daemon:/var/empty/rdp:/bin/false" >> $(TARGET_DIR)/initramfs/etc/passwd
	@echo "network:x:101:101:Network Daemon:/var/lib/dhcp:/bin/false" >> $(TARGET_DIR)/initramfs/etc/passwd
	@# Create group file - format: group:pass:gid:members
	@echo "root:x:0:" > $(TARGET_DIR)/initramfs/etc/group
	@echo "nobody:x:65534:" >> $(TARGET_DIR)/initramfs/etc/group
	@echo "sshd:x:74:" >> $(TARGET_DIR)/initramfs/etc/group
	@echo "rdp:x:75:" >> $(TARGET_DIR)/initramfs/etc/group
	@echo "network:x:101:" >> $(TARGET_DIR)/initramfs/etc/group
	@# Create sshd privilege separation directory
	@mkdir -p $(TARGET_DIR)/initramfs/var/empty/sshd
	@# Create rdp privilege separation directory
	@mkdir -p $(TARGET_DIR)/initramfs/var/empty/rdp
	@echo "export PATH=/initramfs/bin:/initramfs/sbin:/bin:/sbin" > $(TARGET_DIR)/initramfs/etc/profile
	@echo "OXIDE" > $(TARGET_DIR)/initramfs/etc/hostname
	@# Copy networkd
	@cp "$(USERSPACE_OUT_RELEASE)/networkd" "$(TARGET_DIR)/initramfs/bin/networkd"
	@# Copy rdpd
	@cp "$(USERSPACE_OUT_RELEASE)/rdpd" "$(TARGET_DIR)/initramfs/bin/rdpd"
	@# Copy journald and journalctl
	@cp "$(USERSPACE_OUT_RELEASE)/journald" "$(TARGET_DIR)/initramfs/bin/journald"
	@cp "$(USERSPACE_OUT_RELEASE)/journalctl" "$(TARGET_DIR)/initramfs/bin/journalctl"
	@# Copy evtest and argtest
	@cp "$(USERSPACE_OUT_RELEASE)/evtest" "$(TARGET_DIR)/initramfs/bin/evtest"
	@cp "$(USERSPACE_OUT_RELEASE)/argtest" "$(TARGET_DIR)/initramfs/bin/argtest"
	@# Copy getty
	@cp "$(USERSPACE_OUT_RELEASE)/getty" "$(TARGET_DIR)/initramfs/bin/getty"
	@# Copy soundd
	@cp "$(USERSPACE_OUT_RELEASE)/soundd" "$(TARGET_DIR)/initramfs/bin/soundd"
	@# Copy doom
	@cp "$(USERSPACE_OUT_RELEASE)/doom" "$(TARGET_DIR)/initramfs/bin/doom"
	@# Copy signal-test (optional test utility)
	@cp "$(USERSPACE_OUT_RELEASE)/signal-test" "$(TARGET_DIR)/initramfs/bin/signal-test" 2>/dev/null || true
	@# Create services.d directory with service definitions
	@mkdir -p $(TARGET_DIR)/initramfs/etc/services.d
	@echo "PATH=/bin/journald" > $(TARGET_DIR)/initramfs/etc/services.d/journald
	@echo "ENABLED=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/journald
	@echo "RESTART=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/journald
	@echo "PATH=/bin/networkd" > $(TARGET_DIR)/initramfs/etc/services.d/networkd
	@echo "ENABLED=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/networkd
	@echo "RESTART=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/networkd
	@echo "PATH=/bin/sshd" > $(TARGET_DIR)/initramfs/etc/services.d/sshd
	@echo "ENABLED=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/sshd
	@echo "RESTART=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/sshd
	@echo "PATH=/bin/rdpd" > $(TARGET_DIR)/initramfs/etc/services.d/rdpd
	@echo "ENABLED=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/rdpd
	@echo "RESTART=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/rdpd
	@# Create RDP configuration file
	@echo "# OXIDE RDP Server Configuration" > $(TARGET_DIR)/initramfs/etc/rdpd.conf
	@echo "# Port to listen on (default: 3389)" >> $(TARGET_DIR)/initramfs/etc/rdpd.conf
	@echo "port=3389" >> $(TARGET_DIR)/initramfs/etc/rdpd.conf
	@echo "# Maximum concurrent connections" >> $(TARGET_DIR)/initramfs/etc/rdpd.conf
	@echo "max_connections=10" >> $(TARGET_DIR)/initramfs/etc/rdpd.conf
	@echo "# Require TLS encryption (yes/no)" >> $(TARGET_DIR)/initramfs/etc/rdpd.conf
	@echo "tls_required=yes" >> $(TARGET_DIR)/initramfs/etc/rdpd.conf
	@# Create network configuration directory
	@mkdir -p $(TARGET_DIR)/initramfs/etc/network
	@echo "# eth0 network configuration" > $(TARGET_DIR)/initramfs/etc/network/eth0.conf
	@echo "mode=dhcp" >> $(TARGET_DIR)/initramfs/etc/network/eth0.conf
	@# Create default resolv.conf
	@echo "# DNS servers" > $(TARGET_DIR)/initramfs/etc/resolv.conf
	@echo "nameserver 8.8.8.8" >> $(TARGET_DIR)/initramfs/etc/resolv.conf
	@echo "nameserver 8.8.4.4" >> $(TARGET_DIR)/initramfs/etc/resolv.conf
	@# Create default hosts file
	@echo "127.0.0.1 localhost" > $(TARGET_DIR)/initramfs/etc/hosts
	@echo "::1 localhost" >> $(TARGET_DIR)/initramfs/etc/hosts
	@# Create fstab - format: device mountpoint fstype options dump pass
	@echo "# /etc/fstab - filesystem mount table" > $(TARGET_DIR)/initramfs/etc/fstab
	@echo "# device    mountpoint    fstype    options    dump pass" >> $(TARGET_DIR)/initramfs/etc/fstab
	@echo "proc        /proc         proc      defaults   0    0" >> $(TARGET_DIR)/initramfs/etc/fstab
	@echo "sysfs       /sys          sysfs     defaults   0    0" >> $(TARGET_DIR)/initramfs/etc/fstab
	@echo "# devpts is mounted automatically by kernel during boot" >> $(TARGET_DIR)/initramfs/etc/fstab
	@echo "tmpfs       /tmp          tmpfs     defaults   0    0" >> $(TARGET_DIR)/initramfs/etc/fstab
	@echo "tmpfs       /run          tmpfs     defaults   0    0" >> $(TARGET_DIR)/initramfs/etc/fstab
	@# Create CPIO archive
	@cd $(TARGET_DIR)/initramfs && find . | cpio -o -H newc > ../initramfs.cpio 2>/dev/null
	@echo "Initramfs created: $(INITRAMFS)"
	@ls -la $(INITRAMFS)

# Create initramfs with debug symbols (larger)
initramfs-debug: userspace
	@echo "Creating initramfs (debug)..."
	@rm -rf $(TARGET_DIR)/initramfs
	@mkdir -p $(TARGET_DIR)/initramfs/bin
	@mkdir -p $(TARGET_DIR)/initramfs/sbin
	@mkdir -p $(TARGET_DIR)/initramfs/etc
	@mkdir -p $(TARGET_DIR)/initramfs/dev
	@mkdir -p $(TARGET_DIR)/initramfs/proc
	@mkdir -p $(TARGET_DIR)/initramfs/sys
	@mkdir -p $(TARGET_DIR)/initramfs/tmp
	@mkdir -p $(TARGET_DIR)/initramfs/var/log
	@mkdir -p $(TARGET_DIR)/initramfs/home
	@mkdir -p $(TARGET_DIR)/initramfs/root
	@cp "$(USERSPACE_OUT)/init" "$(TARGET_DIR)/initramfs/sbin/init"
	@ln -sf /sbin/init "$(TARGET_DIR)/initramfs/init"
	@cp "$(USERSPACE_OUT)/esh" "$(TARGET_DIR)/initramfs/bin/esh"
	@ln -sf /bin/esh "$(TARGET_DIR)/initramfs/bin/sh"
	@cp "$(USERSPACE_OUT)/login" "$(TARGET_DIR)/initramfs/bin/login"
	@for prog in $(COREUTILS_BINS); do \
		if [ -f "$(USERSPACE_OUT)/$$prog" ]; then \
			cp "$(USERSPACE_OUT)/$$prog" "$(TARGET_DIR)/initramfs/bin/"; \
		fi; \
	done
	@ln -sf /bin/true "$(TARGET_DIR)/initramfs/bin/:" 2>/dev/null || true
	@echo "root:x:0:0:root:/root:/bin/esh" > $(TARGET_DIR)/initramfs/etc/passwd
	@echo "root:x:0:" > $(TARGET_DIR)/initramfs/etc/group
	@echo "export PATH=/initramfs/bin:/initramfs/sbin:/bin:/sbin" > $(TARGET_DIR)/initramfs/etc/profile
	@cd $(TARGET_DIR)/initramfs && find . | cpio -o -H newc > ../initramfs.cpio 2>/dev/null
	@echo "Initramfs created (debug): $(INITRAMFS)"
	@ls -la $(INITRAMFS)

# List all binaries that will be included in initramfs
list-bins:
	@echo "Userspace binaries:"
	@echo "  System: init login"
	@echo "  Shell: esh (sh)"
	@echo "  Apps: gwbasic, curses-demo"
	@echo "  Coreutils: $(COREUTILS_BINS)"

# Create boot directory structure with kernel, bootloader, and initramfs
# NOTE: This target rebuilds kernel, bootloader, and initramfs before creating boot dir
boot-dir: kernel bootloader initramfs
	@echo "Creating boot directory..."
	@mkdir -p $(BOOT_DIR)/EFI/BOOT
	@mkdir -p $(BOOT_DIR)/EFI/OXIDE
	@cp $(BOOTLOADER_TARGET) $(BOOT_DIR)/EFI/BOOT/BOOTX64.EFI
	@cp $(KERNEL_TARGET) $(BOOT_DIR)/EFI/OXIDE/kernel.elf
	@cp $(TARGET_DIR)/initramfs.cpio $(BOOT_DIR)/EFI/OXIDE/initramfs.cpio
	@echo "Boot directory created at $(BOOT_DIR)"
	@echo "  - Bootloader: EFI/BOOT/BOOTX64.EFI"
	@echo "  - Kernel: EFI/OXIDE/kernel.elf"
	@echo "  - Initramfs: EFI/OXIDE/initramfs.cpio"

# Quick boot - same as create-rootfs (builds ext4 disk image with initramfs-minimal)
boot-quick: create-rootfs
	@echo "Boot ready (ext4 root filesystem disk image)"

# Create a real disk image (for qemu-kvm compatibility on RHEL)
boot-image: boot-dir
	@echo "Creating boot disk image..."
	@# Create 100MB disk image
	@dd if=/dev/zero of=$(TARGET_DIR)/boot.img bs=1M count=100 status=none 2>&1
	@# Create GPT partition table and ESP partition
	@parted -s $(TARGET_DIR)/boot.img mklabel gpt
	@parted -s $(TARGET_DIR)/boot.img mkpart ESP fat32 1MiB 99MiB
	@parted -s $(TARGET_DIR)/boot.img set 1 esp on
	@# Format partition using mtools (no sudo needed!)
	@# Partition starts at 1MiB = 2048 sectors of 512 bytes
	@mformat -i $(TARGET_DIR)/boot.img@@1M -F -v OXIDE ::
	@# Create directory structure
	@mmd -i $(TARGET_DIR)/boot.img@@1M ::/EFI
	@mmd -i $(TARGET_DIR)/boot.img@@1M ::/EFI/BOOT
	@mmd -i $(TARGET_DIR)/boot.img@@1M ::/EFI/OXIDE
	@# Copy bootloader
	@mcopy -i $(TARGET_DIR)/boot.img@@1M $(BOOT_DIR)/EFI/BOOT/BOOTX64.EFI ::/EFI/BOOT/
	@# Copy kernel and initramfs
	@mcopy -i $(TARGET_DIR)/boot.img@@1M $(BOOT_DIR)/EFI/OXIDE/kernel.elf ::/EFI/OXIDE/
	@mcopy -i $(TARGET_DIR)/boot.img@@1M $(BOOT_DIR)/EFI/OXIDE/initramfs.cpio ::/EFI/OXIDE/
	@echo "Boot disk image created: $(TARGET_DIR)/boot.img (no sudo needed!)"

# Disk image configuration for root filesystem
ROOTFS_IMAGE := $(TARGET_DIR)/oxide-disk.img
ROOTFS_SIZE := 800
BOOT_SIZE := 64
ROOT_SIZE := 384
HOME_SIZE := 64
BOOT_START := 1
ROOT_START := 65
HOME_START := 449

# Create root filesystem disk image with 3 partitions:
# - Partition 1 (ESP/boot): FAT32, mounted at /boot - bootloader, kernel, initramfs
# - Partition 2 (root): ext4, mounted at / - OS files
# - Partition 3 (home): ext4, mounted at /home - user data
# - /tmp is tmpfs (in-memory)
create-rootfs: kernel bootloader initramfs-minimal
	@echo "Creating OXIDE root filesystem disk image..."
	@echo ""
	@# Create empty disk image
	dd if=/dev/zero of=$(ROOTFS_IMAGE) bs=1M count=$(ROOTFS_SIZE) status=none
	@# Create GPT partition table
	parted -s $(ROOTFS_IMAGE) mklabel gpt
	@# Create boot/ESP partition (1MiB - 65MiB)
	parted -s $(ROOTFS_IMAGE) mkpart boot fat32 $(BOOT_START)MiB $(ROOT_START)MiB
	parted -s $(ROOTFS_IMAGE) set 1 esp on
	@# Create root partition (65MiB - 449MiB)
	parted -s $(ROOTFS_IMAGE) mkpart root ext4 $(ROOT_START)MiB $(HOME_START)MiB
	@# Create home partition (449MiB - 100%)
	parted -s $(ROOTFS_IMAGE) mkpart home ext4 $(HOME_START)MiB 100%
	@# Set up loop device, format, and populate all partitions
	@echo "Formatting partitions..."
	@mkdir -p $(TARGET_DIR)/mnt/boot $(TARGET_DIR)/mnt/root $(TARGET_DIR)/mnt/home
	@LOOP_DEV=$$(sudo losetup -fP --show $(ROOTFS_IMAGE)) && \
	echo "Loop device: $$LOOP_DEV" && \
	sudo mkfs.vfat -F 32 -n BOOT $${LOOP_DEV}p1 && \
	sudo mkfs.ext4 -F -q -L ROOT $${LOOP_DEV}p2 && \
	sudo mkfs.ext4 -F -q -L HOME $${LOOP_DEV}p3 && \
	\
	echo "" && \
	echo "Populating /boot (ESP)..." && \
	sudo mount $${LOOP_DEV}p1 $(TARGET_DIR)/mnt/boot && \
	sudo mkdir -p $(TARGET_DIR)/mnt/boot/EFI/BOOT && \
	sudo mkdir -p $(TARGET_DIR)/mnt/boot/EFI/OXIDE && \
	sudo cp $(BOOTLOADER_TARGET) $(TARGET_DIR)/mnt/boot/EFI/BOOT/BOOTX64.EFI && \
	sudo cp $(KERNEL_TARGET) $(TARGET_DIR)/mnt/boot/EFI/OXIDE/kernel.elf && \
	sudo cp $(TARGET_DIR)/initramfs-minimal.cpio $(TARGET_DIR)/mnt/boot/EFI/OXIDE/initramfs.cpio && \
	sudo umount $(TARGET_DIR)/mnt/boot && \
	\
	echo "Populating / (root filesystem)..." && \
	sudo mount $${LOOP_DEV}p2 $(TARGET_DIR)/mnt/root && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/bin && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/sbin && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/usr/bin && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/usr/sbin && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/usr/share/gwbasic && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/etc/services.d && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/etc/network && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/var/log && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/var/lib/dhcp && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/var/run && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/var/empty/sshd && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/var/empty/rdp && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/tmp && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/proc && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/sys && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/dev/pts && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/boot && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/home && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/root && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/run/network && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/mnt && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/initramfs && \
	\
	echo "  Copying binaries..." && \
	sudo cp "$(USERSPACE_OUT_RELEASE)/init" $(TARGET_DIR)/mnt/root/sbin/init && \
	sudo ln -sf /sbin/init $(TARGET_DIR)/mnt/root/init && \
	sudo cp "$(USERSPACE_OUT_RELEASE)/esh" $(TARGET_DIR)/mnt/root/bin/esh && \
	sudo ln -sf /bin/esh $(TARGET_DIR)/mnt/root/bin/sh && \
	sudo cp "$(USERSPACE_OUT_RELEASE)/getty" $(TARGET_DIR)/mnt/root/bin/getty && \
	sudo cp "$(USERSPACE_OUT_RELEASE)/login" $(TARGET_DIR)/mnt/root/bin/login && \
	for prog in gwbasic curses-demo tls-test thread-test ssh sshd rdpd service networkd journald journalctl evtest argtest vim $(COREUTILS_BINS) testcolors; do \
		[ -f "$(USERSPACE_OUT_RELEASE)/$$prog" ] && sudo cp "$(USERSPACE_OUT_RELEASE)/$$prog" $(TARGET_DIR)/mnt/root/usr/bin/ || true; \
	done && \
	sudo cp userspace/apps/gwbasic/examples/*.bas $(TARGET_DIR)/mnt/root/usr/share/gwbasic/ 2>/dev/null || true; \
	[ -f "$(USERSPACE_OUT_RELEASE)/tls-test" ] && echo "TLS test installed" || true; \
	[ -f "$(USERSPACE_OUT_RELEASE)/vim" ] && echo "vim installed" || true; \
	sudo ln -sf /usr/bin/service $(TARGET_DIR)/mnt/root/usr/bin/servicemgr && \
	sudo ln -sf /usr/bin/service $(TARGET_DIR)/mnt/root/bin/servicemgr && \
	\
	echo "  Creating /etc/passwd..." && \
	printf "root:root:0:0:root:/root:/bin/esh\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/passwd > /dev/null && \
	printf "nobody:x:65534:65534:Nobody:/:/bin/false\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/passwd > /dev/null && \
	printf "sshd:x:74:74:SSH Daemon:/var/empty/sshd:/bin/false\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/passwd > /dev/null && \
	printf "rdp:x:75:75:RDP Daemon:/var/empty/rdp:/bin/false\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/passwd > /dev/null && \
	printf "network:x:101:101:Network Daemon:/var/lib/dhcp:/bin/false\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/passwd > /dev/null && \
	\
	echo "  Creating /etc/group..." && \
	printf "root:x:0:\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/group > /dev/null && \
	printf "nobody:x:65534:\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/group > /dev/null && \
	printf "sshd:x:74:\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/group > /dev/null && \
	printf "rdp:x:75:\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/group > /dev/null && \
	printf "network:x:101:\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/group > /dev/null && \
	\
	echo "  Creating /etc/fstab..." && \
	printf "# OXIDE filesystem table\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "# <device>       <mountpoint>  <type>   <options>  <dump> <pass>\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "LABEL=BOOT       /boot         vfat     defaults   0      2\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "LABEL=HOME       /home         ext4     defaults   0      2\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "tmpfs            /tmp          tmpfs    defaults   0      0\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "tmpfs            /run          tmpfs    defaults   0      0\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "proc             /proc         proc     defaults   0      0\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "sysfs            /sys          sysfs    defaults   0      0\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "# devpts is mounted automatically by kernel during boot\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	\
	echo "  Creating other config files..." && \
	printf "export PATH=/bin:/sbin:/usr/bin:/usr/sbin\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/profile > /dev/null && \
	printf "OXIDE\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/hostname > /dev/null && \
	printf "PATH=/usr/bin/journald\nENABLED=yes\nRESTART=yes\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/services.d/journald > /dev/null && \
	printf "PATH=/usr/bin/networkd\nENABLED=yes\nRESTART=yes\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/services.d/networkd > /dev/null && \
	printf "PATH=/usr/bin/sshd\nENABLED=yes\nRESTART=yes\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/services.d/sshd > /dev/null && \
	printf "PATH=/usr/bin/rdpd\nENABLED=yes\nRESTART=yes\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/services.d/rdpd > /dev/null && \
	printf "# OXIDE RDP Server Configuration\n# Port to listen on (default: 3389)\nport=3389\n# Maximum concurrent connections\nmax_connections=10\n# Require TLS encryption (yes/no)\ntls_required=yes\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/rdpd.conf > /dev/null && \
	printf "mode=dhcp\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/network/eth0.conf > /dev/null && \
	printf "nameserver 8.8.8.8\nnameserver 8.8.4.4\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/resolv.conf > /dev/null && \
	printf "127.0.0.1 localhost\n::1 localhost\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/hosts > /dev/null && \
	sudo umount $(TARGET_DIR)/mnt/root && \
	\
	echo "Populating /home..." && \
	sudo mount $${LOOP_DEV}p3 $(TARGET_DIR)/mnt/home && \
	sudo mkdir -p $(TARGET_DIR)/mnt/home/root && \
	sudo umount $(TARGET_DIR)/mnt/home && \
	\
	sudo losetup -d $$LOOP_DEV && \
	rm -rf $(TARGET_DIR)/mnt
	@echo ""
	@echo "==============================================="
	@echo "OXIDE disk image created: $(ROOTFS_IMAGE)"
	@echo "==============================================="
	@echo ""
	@echo "Partition layout:"
	@echo "  1. /boot  (ESP)   $(BOOT_SIZE)MB  FAT32  - bootloader, kernel, initramfs"
	@echo "  2. /      (root)  $(ROOT_SIZE)MB  ext4   - OS files"
	@echo "  3. /home          $(HOME_SIZE)MB  ext4   - user data"
	@echo "  4. /tmp           (tmpfs)         - in-memory"
	@echo ""
	@parted -s $(ROOTFS_IMAGE) print

# Create minimal initramfs for ext4 root boot
# Only contains: init, login, esh, and essential /etc files
# Full utilities live on the ext4 root partition
initramfs-minimal: userspace-release
	@echo "Creating minimal initramfs..."
	@rm -rf $(TARGET_DIR)/initramfs-minimal
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/bin
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/sbin
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/etc
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/dev
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/proc
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/sys
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/tmp
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/var/log
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/var/run
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/boot
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/home
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/root
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/run
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/mnt
	@# Copy only essential binaries
	@cp "$(USERSPACE_OUT_RELEASE)/init" "$(TARGET_DIR)/initramfs-minimal/sbin/init"
	@ln -sf /sbin/init "$(TARGET_DIR)/initramfs-minimal/init"
	@cp "$(USERSPACE_OUT_RELEASE)/esh" "$(TARGET_DIR)/initramfs-minimal/bin/esh"
	@ln -sf /bin/esh "$(TARGET_DIR)/initramfs-minimal/bin/sh"
	@cp "$(USERSPACE_OUT_RELEASE)/login" "$(TARGET_DIR)/initramfs-minimal/bin/login"
	@cp "$(USERSPACE_OUT_RELEASE)/getty" "$(TARGET_DIR)/initramfs-minimal/bin/getty"
	@# Copy coreutils
	@for prog in $(COREUTILS_BINS); do \
		if [ -f "$(USERSPACE_OUT_RELEASE)/$$prog" ]; then \
			cp "$(USERSPACE_OUT_RELEASE)/$$prog" "$(TARGET_DIR)/initramfs-minimal/bin/"; \
		fi; \
	done
	@ln -sf /bin/true "$(TARGET_DIR)/initramfs-minimal/bin/:" 2>/dev/null || true
	@ln -sf /bin/ls "$(TARGET_DIR)/initramfs-minimal/bin/dir" 2>/dev/null || true
	@# Copy service manager
	@cp "$(USERSPACE_OUT_RELEASE)/service" "$(TARGET_DIR)/initramfs-minimal/bin/service"
	@ln -sf /bin/service "$(TARGET_DIR)/initramfs-minimal/bin/servicemgr"
	@# Copy daemons
	@cp "$(USERSPACE_OUT_RELEASE)/networkd" "$(TARGET_DIR)/initramfs-minimal/bin/networkd"
	@cp "$(USERSPACE_OUT_RELEASE)/journald" "$(TARGET_DIR)/initramfs-minimal/bin/journald"
	@cp "$(USERSPACE_OUT_RELEASE)/journalctl" "$(TARGET_DIR)/initramfs-minimal/bin/journalctl"
	@cp "$(USERSPACE_OUT_RELEASE)/sshd" "$(TARGET_DIR)/initramfs-minimal/bin/sshd"
	@cp "$(USERSPACE_OUT_RELEASE)/ssh" "$(TARGET_DIR)/initramfs-minimal/bin/ssh"
	@cp "$(USERSPACE_OUT_RELEASE)/gwbasic" "$(TARGET_DIR)/initramfs-minimal/bin/gwbasic"
	@# Copy curses-demo
	@cp "$(USERSPACE_OUT_RELEASE)/curses-demo" "$(TARGET_DIR)/initramfs-minimal/bin/curses-demo"
	@# Copy BASIC example programs
	@mkdir -p "$(TARGET_DIR)/initramfs-minimal/usr/share/gwbasic"
	@cp userspace/apps/gwbasic/examples/*.bas "$(TARGET_DIR)/initramfs-minimal/usr/share/gwbasic/" 2>/dev/null || true
	@# Copy test utilities
	@cp "$(USERSPACE_OUT_RELEASE)/evtest" "$(TARGET_DIR)/initramfs-minimal/bin/evtest"
	@cp "$(USERSPACE_OUT_RELEASE)/argtest" "$(TARGET_DIR)/initramfs-minimal/bin/argtest"
	@# Create service definitions
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/etc/services.d
	@echo "PATH=/bin/journald" > $(TARGET_DIR)/initramfs-minimal/etc/services.d/journald
	@echo "ENABLED=yes" >> $(TARGET_DIR)/initramfs-minimal/etc/services.d/journald
	@echo "RESTART=yes" >> $(TARGET_DIR)/initramfs-minimal/etc/services.d/journald
	@echo "PATH=/bin/networkd" > $(TARGET_DIR)/initramfs-minimal/etc/services.d/networkd
	@echo "ENABLED=yes" >> $(TARGET_DIR)/initramfs-minimal/etc/services.d/networkd
	@echo "RESTART=yes" >> $(TARGET_DIR)/initramfs-minimal/etc/services.d/networkd
	@echo "PATH=/bin/sshd" > $(TARGET_DIR)/initramfs-minimal/etc/services.d/sshd
	@echo "ENABLED=yes" >> $(TARGET_DIR)/initramfs-minimal/etc/services.d/sshd
	@echo "RESTART=yes" >> $(TARGET_DIR)/initramfs-minimal/etc/services.d/sshd
	@echo "PATH=/bin/rdpd" > $(TARGET_DIR)/initramfs-minimal/etc/services.d/rdpd
	@echo "ENABLED=yes" >> $(TARGET_DIR)/initramfs-minimal/etc/services.d/rdpd
	@echo "RESTART=yes" >> $(TARGET_DIR)/initramfs-minimal/etc/services.d/rdpd
	@# Create RDP configuration file
	@echo "# OXIDE RDP Server Configuration" > $(TARGET_DIR)/initramfs-minimal/etc/rdpd.conf
	@echo "port=3389" >> $(TARGET_DIR)/initramfs-minimal/etc/rdpd.conf
	@echo "max_connections=10" >> $(TARGET_DIR)/initramfs-minimal/etc/rdpd.conf
	@echo "tls_required=yes" >> $(TARGET_DIR)/initramfs-minimal/etc/rdpd.conf
	@# Create minimal passwd/group
	@echo "root:root:0:0:root:/root:/bin/esh" > $(TARGET_DIR)/initramfs-minimal/etc/passwd
	@echo "nobody:x:65534:65534:Nobody:/:/bin/false" >> $(TARGET_DIR)/initramfs-minimal/etc/passwd
	@echo "sshd:x:74:74:sshd:/var/empty/sshd:/bin/false" >> $(TARGET_DIR)/initramfs-minimal/etc/passwd
	@echo "rdp:x:75:75:rdp:/var/empty/rdp:/bin/false" >> $(TARGET_DIR)/initramfs-minimal/etc/passwd
	@echo "root:x:0:" > $(TARGET_DIR)/initramfs-minimal/etc/group
	@echo "nobody:x:65534:" >> $(TARGET_DIR)/initramfs-minimal/etc/group
	@echo "sshd:x:74:" >> $(TARGET_DIR)/initramfs-minimal/etc/group
	@echo "rdp:x:75:" >> $(TARGET_DIR)/initramfs-minimal/etc/group
	@echo "network:x:101:" >> $(TARGET_DIR)/initramfs-minimal/etc/group
	@# Create sshd privilege separation directory
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/var/empty/sshd
	@# Create rdp privilege separation directory
	@mkdir -p $(TARGET_DIR)/initramfs-minimal/var/empty/rdp
	@# Create profile with PATH
	@echo "export PATH=/bin:/sbin:/usr/bin:/usr/sbin" > $(TARGET_DIR)/initramfs-minimal/etc/profile
	@echo "OXIDE" > $(TARGET_DIR)/initramfs-minimal/etc/hostname
	@# Create hosts file
	@echo "127.0.0.1 localhost" > $(TARGET_DIR)/initramfs-minimal/etc/hosts
	@echo "::1 localhost" >> $(TARGET_DIR)/initramfs-minimal/etc/hosts
	@# Create fstab
	@# Note: Kernel already mounts /, /proc, /dev, /dev/pts, /run, /tmp, /var/*
	@echo "# /etc/fstab - filesystem mount table" > $(TARGET_DIR)/initramfs-minimal/etc/fstab
	@echo "# device       mountpoint    fstype    options    dump pass" >> $(TARGET_DIR)/initramfs-minimal/etc/fstab
	@echo "sysfs          /sys          sysfs     defaults   0    0" >> $(TARGET_DIR)/initramfs-minimal/etc/fstab
	@echo "LABEL=BOOT     /boot         vfat      defaults   0    2" >> $(TARGET_DIR)/initramfs-minimal/etc/fstab
	@echo "LABEL=HOME     /home         ext4      defaults   0    2" >> $(TARGET_DIR)/initramfs-minimal/etc/fstab
	@# Create CPIO archive
	@cd $(TARGET_DIR)/initramfs-minimal && find . | cpio -o -H newc > ../initramfs-minimal.cpio 2>/dev/null
	@echo "Minimal initramfs created: $(TARGET_DIR)/initramfs-minimal.cpio"
	@ls -la $(TARGET_DIR)/initramfs-minimal.cpio

# Auto-detect QEMU mode (Fedora vs RHEL)
detect-qemu-mode:
	@if command -v qemu-system-x86_64 >/dev/null 2>&1; then \
		echo "fedora"; \
	elif [ -f /usr/libexec/qemu-kvm ]; then \
		echo "rhel"; \
	else \
		echo "unknown"; \
	fi

# Run OXIDE OS - auto-detects Fedora vs RHEL and uses appropriate QEMU
# Builds everything and creates ext4 root filesystem, then runs with networking
run: clean-rootfs
	@MODE=$$($(MAKE) -s detect-qemu-mode); \
	if [ "$$MODE" = "fedora" ]; then \
		echo "Detected Fedora mode (qemu-system-x86_64)"; \
		KERNEL_FEATURES="$(RUN_KERNEL_FEATURES)" $(MAKE) create-rootfs; \
		KERNEL_FEATURES="$(RUN_KERNEL_FEATURES)" $(MAKE) run-fedora; \
	elif [ "$$MODE" = "rhel" ]; then \
		echo "Detected RHEL mode (qemu-kvm)"; \
		KERNEL_FEATURES="$(RUN_KERNEL_FEATURES)" $(MAKE) create-rootfs; \
		KERNEL_FEATURES="$(RUN_KERNEL_FEATURES)" $(MAKE) run-rhel; \
	else \
		echo "Error: No compatible QEMU found"; \
		echo "Install: sudo dnf install qemu-system-x86 (Fedora) or qemu-kvm (RHEL)"; \
		exit 1; \
	fi

# Internal target: Run with qemu-system-x86_64 (Fedora)
run-fedora:
	@echo "Running OXIDE OS with qemu-system-x86_64..."
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		echo "Install: sudo dnf install edk2-ovmf"; \
		exit 1; \
	fi
	@echo "Forwarding SSH to localhost:$(SSH_HOST_PORT)"
	@mkdir -p /tmp/qemu-oxide
	TMPDIR=/tmp/qemu-oxide qemu-system-x86_64 \
		-machine q35 \
		-cpu qemu64,+smap,+smep \
		-smp 4 \
		-m 256M \
		-bios "$(OVMF)" \
		-drive file=$(ROOTFS_IMAGE),format=raw,if=none,id=disk \
		-device virtio-blk-pci,drive=disk \
		-device virtio-net-pci,netdev=net0 \
		-netdev user,id=net0,hostfwd=tcp::$(SSH_HOST_PORT)-:22 \
		-device virtio-gpu-pci \
		-audiodev none,id=snd0 \
		-device intel-hda \
		-device hda-duplex,audiodev=snd0 \
		-serial stdio \
		-no-reboot

# Internal target: Run with qemu-kvm (RHEL)
run-rhel:
	@echo "Running OXIDE OS with qemu-kvm..."
	@if [ ! -f /usr/share/edk2/ovmf/OVMF_CODE.fd ]; then \
		echo "Error: OVMF firmware not found"; \
		echo "Install: sudo dnf install edk2-ovmf"; \
		exit 1; \
	fi
	@if [ ! -f /usr/libexec/qemu-kvm ]; then \
		echo "Error: /usr/libexec/qemu-kvm not found"; \
		echo "Install: sudo dnf install qemu-kvm"; \
		exit 1; \
	fi
	@mkdir -p $(TARGET_DIR)
	@cp /usr/share/edk2/ovmf/OVMF_VARS.fd $(TARGET_DIR)/OVMF_VARS.fd 2>/dev/null || true
	@mkdir -p /tmp/qemu-oxide
	@echo "Starting QEMU (VNC on :5900, serial on stdio)..."
	@echo "Forwarding SSH to localhost:$(SSH_HOST_PORT)"
	@TMPDIR=/tmp/qemu-oxide /usr/libexec/qemu-kvm \
		-machine q35,accel=kvm:tcg \
		-cpu max,+invtsc \
		-smp 4 \
		-m 256M \
		-drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/ovmf/OVMF_CODE.fd \
		-drive if=pflash,format=raw,file=$(TARGET_DIR)/OVMF_VARS.fd \
		-drive file=$(ROOTFS_IMAGE),format=raw,if=none,id=bootdisk \
		-device virtio-blk-pci,drive=bootdisk \
		-device virtio-net-pci,netdev=net0 \
		-netdev user,id=net0,hostfwd=tcp::$(SSH_HOST_PORT)-:22 \
		-device virtio-gpu-pci \
		-audiodev none,id=snd0 \
		-device intel-hda \
		-device hda-duplex,audiodev=snd0 \
		-vga std \
		-vnc :0 \
		-serial stdio \
		-no-reboot & \
	QEMU_PID=$$!; \
	trap 'kill $$QEMU_PID 2>/dev/null; exit' INT TERM; \
	echo "QEMU started (PID: $$QEMU_PID)"; \
	sleep 2; \
	# Launch VNC viewer with 2x scaling (640x480 → 1280x960 for easier viewing)
	if command -v vncviewer >/dev/null 2>&1; then \
		echo "Launching VNC viewer (scaled 2x)..."; \
		vncviewer -Scaling=2x localhost:5900 2>/dev/null || vncviewer -scale 2 localhost:5900 2>/dev/null || vncviewer localhost:5900 2>/dev/null; \
	elif flatpak list --app 2>/dev/null | grep -q tigervnc; then \
		echo "Launching VNC viewer (Flatpak, scaled 2x)..."; \
		flatpak run org.tigervnc.vncviewer -Scaling=2x localhost:5900 2>/dev/null || flatpak run org.tigervnc.vncviewer localhost:5900 2>/dev/null; \
	else \
		echo "VNC viewer not found - connect manually to localhost:5900"; \
		echo "Install: sudo dnf install tigervnc"; \
		wait $$QEMU_PID; \
	fi; \
	echo "Stopping QEMU..."; \
	kill $$QEMU_PID 2>/dev/null || true; \
	wait $$QEMU_PID 2>/dev/null || true

# Alias for backward compatibility
run-kvm: run-rhel

# Automated test: boot and check for expected output
test: create-rootfs
	@echo "Running automated boot test..."
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		exit 1; \
	fi
	@mkdir -p /tmp/qemu-oxide
	@TMPDIR=/tmp/qemu-oxide timeout $(QEMU_TIMEOUT) $(QEMU) \
		-machine q35 \
		-cpu qemu64,+smap,+smep \
		-m 256M \
		-bios "$(OVMF)" \
		-drive file=$(ROOTFS_IMAGE),format=raw,if=none,id=disk \
		-device ide-hd,drive=disk \
		-serial file:$(TARGET_DIR)/serial.log \
		-display none \
		-no-reboot \
		2>/dev/null || true
	@echo "--- Serial Output ---"
	@cat $(TARGET_DIR)/serial.log 2>/dev/null || echo "(no output)"
	@echo "--- End Output ---"
	@if grep -q "OXIDE" $(TARGET_DIR)/serial.log 2>/dev/null; then \
		echo ""; \
		echo "TEST PASSED: Boot message found"; \
		exit 0; \
	else \
		echo ""; \
		echo "TEST FAILED: Expected 'OXIDE' in output"; \
		exit 1; \
	fi

# Quick syntax and type check
check:
	cargo check --all-targets

# Format code
fmt:
	cargo fmt --all

# Format check
fmt-check:
	cargo fmt --all -- --check

# Clippy lint
clippy:
	cargo clippy --all-targets -- -D warnings

# Clean build artifacts
clean:
	cargo clean
	rm -rf $(BOOT_DIR)

# Show detected configuration
show-config:
	@echo "Configuration:"
	@echo "  ARCH:         $(ARCH)"
	@echo "  PROFILE:      $(PROFILE)"
	@echo "  QEMU:         $(QEMU)"
	@echo "  OVMF:         $(OVMF)"
	@echo "  QEMU_TIMEOUT: $(QEMU_TIMEOUT)"

# Show help
# Debug run with graphical display for PS/2 keyboard testing
run-debug-input-gui:
	@echo "Running OXIDE OS with graphical display for PS/2 keyboard testing..."
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		exit 1; \
	fi
	@KERNEL_FEATURES="debug-input" $(MAKE) create-rootfs
	@mkdir -p /tmp/qemu-oxide
	@echo "Serial output: /tmp/oxide-serial.log"
	@echo "Use the QEMU graphical window to type and test keyboard events"
	TMPDIR=/tmp/qemu-oxide qemu-system-x86_64 \
		-machine q35 \
		-cpu qemu64,+smap,+smep \
		-m 256M \
		-bios "$(OVMF)" \
		-drive file=$(ROOTFS_IMAGE),format=raw,if=none,id=disk \
		-device virtio-blk-pci,drive=disk \
		-device virtio-net-pci,netdev=net0 \
		-netdev user,id=net0,hostfwd=tcp::$(SSH_HOST_PORT)-:22 \
		-serial file:/tmp/oxide-serial.log \
		-display gtk \
		-no-reboot & \
	echo "QEMU started. Use: tail -f /tmp/oxide-serial.log to see debug output"

# Debug convenience targets — shorthand for KERNEL_FEATURES=...
run-debug-mouse: KERNEL_FEATURES = debug-mouse
run-debug-mouse: run

run-debug-lock: KERNEL_FEATURES = debug-lock
run-debug-lock: run

run-debug-fork: KERNEL_FEATURES = debug-fork
run-debug-fork: run

run-debug-sched: KERNEL_FEATURES = debug-sched
run-debug-sched: run

run-debug-all: KERNEL_FEATURES = debug-all
run-debug-all: run

help:
	@echo "OXIDE OS Build System"
	@echo ""
	@echo "Run Targets:"
	@echo "  run               - Build and run (auto-detects Fedora/RHEL)"
	@echo "  run-fedora        - Build and run with qemu-system-x86_64"
	@echo "  run-rhel          - Build and run with qemu-kvm (RHEL 10)"
	@echo "  run-kvm           - Alias for run-rhel"
	@echo "  clean-rootfs      - Remove generated disk images/initramfs so run rebuilds fresh"
	@echo ""
	@echo "Debug Targets:"
	@echo "  run-debug-mouse   - Run with mouse/input debug output"
	@echo "  run-debug-lock    - Run with lock contention warnings"
	@echo "  run-debug-fork    - Run with fork/exec debug output"
	@echo "  run-debug-sched   - Run with scheduler debug output"
	@echo "  run-debug-all     - Run with ALL debug output"
	@echo "  Or: make run KERNEL_FEATURES=debug-fork,debug-mouse"
	@echo ""
	@echo "Build Targets:"
	@echo "  all               - Build kernel and bootloader (default)"
	@echo "  build-full        - Build kernel, bootloader, userspace, and initramfs"
	@echo "  kernel            - Build kernel only"
	@echo "  bootloader        - Build UEFI bootloader only"
	@echo "  userspace         - Build all userspace programs (debug)"
	@echo "  userspace-release - Build all userspace programs (release)"
	@echo "  userspace-pkg     - Build single package (PKG=name)"
	@echo "  initramfs         - Create initramfs (release, all utilities)"
	@echo "  boot-image        - Create bootable disk image"
	@echo "  create-rootfs     - Create 512MB disk with ESP + ext4 root + ext4 home"
	@echo "  release           - Build kernel/bootloader in release mode"
	@echo ""
	@echo "Other Targets:"
	@echo "  test              - Automated boot test"
	@echo "  check             - Quick syntax/type check"
	@echo "  fmt               - Format code"
	@echo "  clippy            - Run clippy linter"
	@echo "  clean             - Remove build artifacts"
	@echo "  clean-rootfs      - Remove generated rootfs artifacts"

# Remove generated disk images/initramfs so run rebuilds fresh
clean-rootfs:
	@echo "Cleaning generated rootfs artifacts..."
	@sudo umount $(TARGET_DIR)/mnt/boot 2>/dev/null || true
	@sudo umount $(TARGET_DIR)/mnt/root 2>/dev/null || true
	@sudo umount $(TARGET_DIR)/mnt/home 2>/dev/null || true
	@sudo umount $(TARGET_DIR)/mnt 2>/dev/null || true
	@sudo losetup -D 2>/dev/null || true
	@rm -rf $(TARGET_DIR)/boot $(TARGET_DIR)/initramfs $(INITRAMFS) $(TARGET_DIR)/initramfs-minimal $(TARGET_DIR)/initramfs-minimal.cpio $(ROOTFS_IMAGE) $(TARGET_DIR)/mnt
	@echo "  show-config    - Show detected configuration"
	@echo ""
	@echo "Toolchain:"
	@echo "  toolchain      - Build OXIDE cross-compiler toolchain"
	@echo "  test-toolchain - Test toolchain with examples"
	@echo "  install-toolchain - Install toolchain (PREFIX=/usr/local/oxide)"
	@echo ""
	@echo "Examples:"
	@echo "  make run       - Build and run (auto-detects Fedora/RHEL)"
	@echo "  make test      - Run automated boot test"
	@echo ""
	@echo "All run targets create a 512MB disk image with:"
	@echo "  - /boot  (FAT32/ESP): bootloader, kernel, initramfs"
	@echo "  - /      (ext4):      OS binaries and config"
	@echo "  - /home  (ext4):      user data"
	@echo "  - /tmp   (tmpfs):     in-memory temp files"
	@echo ""
	@echo "Requirements:"
	@echo "  Fedora: sudo dnf install qemu-system-x86 edk2-ovmf parted"
	@echo "  RHEL:   sudo dnf install qemu-kvm edk2-ovmf parted tigervnc"



# Build toolchain components
# External libraries (zlib, openssl, xz, zstd)
external-libs: toolchain zlib openssl xz zstd

zlib: toolchain
	@echo "Building zlib..."
	@./scripts/build-zlib.sh || (echo "Note: zlib test tools failed, but library may be OK" && \
		cd external/zlib-1.3.1 && \
		ar rcs libz.a adler32.o crc32.o deflate.o infback.o inffast.o inflate.o inftrees.o trees.o zutil.o compress.o uncompr.o gzclose.o gzlib.o gzread.o gzwrite.o 2>/dev/null && \
		mkdir -p $(CURDIR)/toolchain/sysroot/lib && \
		cp libz.a $(CURDIR)/toolchain/sysroot/lib/ && \
		echo "zlib library installed to sysroot")

openssl: toolchain zlib
	@echo "Building OpenSSL..."
	@./scripts/build-openssl.sh

xz: toolchain
	@echo "Building XZ Utils..."
	@./scripts/build-xz.sh

zstd: toolchain
	@echo "Building Zstandard..."
	@./scripts/build-zstd.sh

# CPython cross-compilation
cpython: toolchain zlib
	@echo "Building CPython for OXIDE..."
	@./scripts/build-cpython.sh
	@mkdir -p $(USERSPACE_OUT_RELEASE)
	@cp external/cpython-build/python $(USERSPACE_OUT_RELEASE)/python
	@echo "Python installed to $(USERSPACE_OUT_RELEASE)/python"

# TLS test program
tls-test: toolchain
	@echo "Building TLS test program..."
	@toolchain/bin/oxide-cc -o $(USERSPACE_OUT_RELEASE)/tls-test userspace/tests/tls-test.c
	@echo "TLS test built: $(USERSPACE_OUT_RELEASE)/tls-test"

thread-test: toolchain
	@echo "Building thread test program..."
	@toolchain/bin/oxide-cc -o $(USERSPACE_OUT_RELEASE)/thread-test userspace/tests/thread-test.c
	@echo "Thread test built: $(USERSPACE_OUT_RELEASE)/thread-test"

vim: toolchain
	@echo "Building vim for OXIDE..."
	@./scripts/build-vim.sh
	@mkdir -p $(USERSPACE_OUT_RELEASE)
	@cp external/vim/src/vim $(USERSPACE_OUT_RELEASE)/vim
	@llvm-strip $(USERSPACE_OUT_RELEASE)/vim
	@echo "Vim installed to $(USERSPACE_OUT_RELEASE)/vim"

toolchain:
	@echo "Building OXIDE cross-compiler toolchain..."
	@echo "  Building assembler (as)..."
	@cargo build --package oxide-as --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS)
	@echo "  Building linker (ld)..."
	@cargo build --package oxide-ld --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS)
	@echo "  Building archiver (ar)..."
	@cargo build --package oxide-ar --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS)
	@echo "  Building libc..."
	@RUSTFLAGS="-C relocation-model=pic" cargo build --package libc --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS)
	@echo "  Building pthread..."
	@RUSTFLAGS="-C relocation-model=pic" cargo build --package pthread --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS)
	@echo ""
	@echo "Installing toolchain components to sysroot..."
	@mkdir -p toolchain/sysroot/lib
	@# Copy libc.a to sysroot (staticlib produces native ELF objects, rlib has LLVM bitcode)
	@if [ -f "$(USERSPACE_OUT_RELEASE)/liblibc.a" ]; then \
		cp "$(USERSPACE_OUT_RELEASE)/liblibc.a" "toolchain/sysroot/lib/liboxide_libc.a"; \
	elif [ -f "$(USERSPACE_OUT_RELEASE)/liblibc.rlib" ]; then \
		cp "$(USERSPACE_OUT_RELEASE)/liblibc.rlib" "toolchain/sysroot/lib/liboxide_libc.a"; \
	fi
	@# Copy pthread.a to sysroot
	@if [ -f "$(USERSPACE_OUT_RELEASE)/libpthread.a" ]; then \
		cp "$(USERSPACE_OUT_RELEASE)/libpthread.a" "toolchain/sysroot/lib/libpthread.a"; \
	fi
	@echo ""
	@echo "OXIDE toolchain built successfully!"
	@echo ""
	@echo "To use the toolchain:"
	@echo "  export PATH=$(CURDIR)/toolchain/bin:\$$PATH"
	@echo "  oxide-cc -o hello hello.c"
	@echo ""
	@echo "See toolchain/README.md for documentation."
	@echo "See toolchain/examples/ for examples."

# Install toolchain to system
INSTALL_PREFIX ?= /usr/local/oxide
install-toolchain: toolchain
	@echo "Installing OXIDE toolchain to $(INSTALL_PREFIX)..."
	@install -d $(INSTALL_PREFIX)/bin
	@install -d $(INSTALL_PREFIX)/sysroot
	@install -d $(INSTALL_PREFIX)/cmake
	@install -m 755 toolchain/bin/* $(INSTALL_PREFIX)/bin/
	@cp -r toolchain/sysroot/* $(INSTALL_PREFIX)/sysroot/
	@cp toolchain/cmake/oxide-toolchain.cmake $(INSTALL_PREFIX)/cmake/
	@echo "Toolchain installed to $(INSTALL_PREFIX)"
	@echo "Add $(INSTALL_PREFIX)/bin to your PATH"

# Test toolchain with examples
test-toolchain: toolchain
	@echo "Testing OXIDE toolchain..."
	@cd toolchain/examples/hello && $(MAKE) clean && $(MAKE)
	@echo "  ✓ Hello example built"
	@cd toolchain/examples/echo && $(MAKE) clean && $(MAKE)
	@echo "  ✓ Echo example built"
	@cd toolchain/examples/calculator && $(MAKE) clean && $(MAKE)
	@echo "  ✓ Calculator example built"
	@echo ""
	@echo "All toolchain tests passed!"

# Clean toolchain
clean-toolchain:
	@rm -rf toolchain/sysroot/lib/*.a
	@cd toolchain/examples/hello && $(MAKE) clean || true
	@cd toolchain/examples/echo && $(MAKE) clean || true
	@cd toolchain/examples/calculator && $(MAKE) clean || true

claude:
	bwrap \
	--ro-bind /usr /usr \
	--ro-bind /etc /etc \
	--ro-bind /lib /lib \
	--ro-bind /lib64 /lib64 \
	--ro-bind /run /run \
	--bind $(HOME) $(HOME) \
	--bind $(HOME)/.claude.json $(HOME)/.claude.json \
	--bind $(HOME)/.claude $(HOME)/.claude \
	--bind "$(CURDIR)" "$(CURDIR)" \
	--bind /tmp /tmp \
	--dev /dev \
	--proc /proc \
	--chdir "$(CURDIR)" \
	--die-with-parent \
	-- /usr/bin/node /usr/local/lib/node_modules/@anthropic-ai/claude-code/cli.js --dangerously-skip-permissions
