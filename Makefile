
# ========================================
# Package Manager Targets
# ========================================

# Sync Fedora repository metadata
pkgmgr-sync:
	@echo "Syncing package repository metadata..."
	@python3 pkgmgr/bin/repo-sync
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

ifeq ($(RUN_BUILD_USERSPACE),1)
INITRAMFS_PREREQ := userspace-release
INITRAMFS_MINIMAL_PREREQ := userspace-release
else
INITRAMFS_PREREQ :=
INITRAMFS_MINIMAL_PREREQ :=
endif

# Userspace packages to build (Cargo-based)
USERSPACE_ALL_PACKAGES := init esh getty login coreutils ssh sshd rdpd service networkd resolvd journald journalctl soundd evtest argtest htop doom gwbasic curses-demo
USERSPACE_PACKAGES ?= $(USERSPACE_ALL_PACKAGES)
# Non-Cargo extra targets (built via dedicated rules)
USERSPACE_EXTRA_TARGETS_ALL := tls-test thread-test
USERSPACE_EXTRA_TARGETS ?= $(USERSPACE_EXTRA_TARGETS_ALL)

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
		if [ "$$pkg" = "gwbasic" ]; then \
			RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static -C link-arg=-Tuserspace/userspace.ld -C link-arg=-e_start" cargo build --package oxide-gwbasic --target $(USERSPACE_TARGET) $(CARGO_USER_FLAGS) --features oxide || exit 1; \
		else \
			RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static -C link-arg=-Tuserspace/userspace.ld -C link-arg=-e_start" cargo build --package $$pkg --target $(USERSPACE_TARGET) $(CARGO_USER_FLAGS) || exit 1; \
		fi; \
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
	@# Check if linker script has changed - cargo won't detect .ld changes
	@if [ -d "$(USERSPACE_OUT_RELEASE)" ] && [ -f "$(USERSPACE_OUT_RELEASE)/init" ]; then \
		if [ "userspace/userspace.ld" -nt "$(USERSPACE_OUT_RELEASE)/init" ]; then \
			echo "  linker script changed - cleaning userspace binaries to force relink..."; \
			rm -rf $(USERSPACE_OUT_RELEASE)/*; \
		fi; \
	fi
	@for pkg in $(USERSPACE_PACKAGES); do \
		echo "  Building $$pkg (release)..."; \
		if [ "$$pkg" = "gwbasic" ]; then \
			RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static -C link-arg=-Tuserspace/userspace.ld -C link-arg=-e_start" cargo build --package oxide-gwbasic --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS) --features oxide || exit 1; \
		else \
			RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static -C link-arg=-Tuserspace/userspace.ld -C link-arg=-e_start" cargo build --package $$pkg --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS) || exit 1; \
		fi; \
	done
ifneq (,$(filter coreutils,$(USERSPACE_PACKAGES)))
	@echo "  Building testcolors (release)..."
	@RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static -C link-arg=-Tuserspace/userspace.ld -C link-arg=-e_start" cargo build --package coreutils --bin testcolors --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS) || exit 1
endif
	@for target in $(USERSPACE_EXTRA_TARGETS); do \
		echo "  Building $$target ..."; \
		$(MAKE) $$target || exit 1; \
	done
	@echo "Stripping binaries..."
	@for prog in init esh login getty gwbasic curses-demo htop tls-test thread-test ssh sshd rdpd service networkd journald journalctl soundd evtest argtest doom python $(COREUTILS_BINS); do \
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
initramfs: $(INITRAMFS_PREREQ)
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
	@if [ -f "$(USERSPACE_OUT_RELEASE)/init" ]; then cp "$(USERSPACE_OUT_RELEASE)/init" "$(TARGET_DIR)/initramfs/sbin/init"; ln -sf /sbin/init "$(TARGET_DIR)/initramfs/init"; fi
	@# Copy shell
	@if [ -f "$(USERSPACE_OUT_RELEASE)/esh" ]; then cp "$(USERSPACE_OUT_RELEASE)/esh" "$(TARGET_DIR)/initramfs/bin/esh"; ln -sf /bin/esh "$(TARGET_DIR)/initramfs/bin/sh"; fi
	@# Copy login
	@if [ -f "$(USERSPACE_OUT_RELEASE)/login" ]; then cp "$(USERSPACE_OUT_RELEASE)/login" "$(TARGET_DIR)/initramfs/bin/login"; fi
	@# Copy gwbasic
	@if [ -f "$(USERSPACE_OUT_RELEASE)/gwbasic" ]; then cp "$(USERSPACE_OUT_RELEASE)/gwbasic" "$(TARGET_DIR)/initramfs/bin/gwbasic"; fi
	@# Copy curses-demo
	@if [ -f "$(USERSPACE_OUT_RELEASE)/curses-demo" ]; then cp "$(USERSPACE_OUT_RELEASE)/curses-demo" "$(TARGET_DIR)/initramfs/bin/curses-demo"; fi
	@# Copy htop
	@if [ -f "$(USERSPACE_OUT_RELEASE)/htop" ]; then cp "$(USERSPACE_OUT_RELEASE)/htop" "$(TARGET_DIR)/initramfs/bin/htop"; fi
	@# Copy BASIC example programs
	@mkdir -p "$(TARGET_DIR)/initramfs/usr/share/gwbasic"
	@cp userspace/apps/gwbasic/examples/*.bas "$(TARGET_DIR)/initramfs/usr/share/gwbasic/" 2>/dev/null || true
	@# Copy ssh client
	@if [ -f "$(USERSPACE_OUT_RELEASE)/ssh" ]; then cp "$(USERSPACE_OUT_RELEASE)/ssh" "$(TARGET_DIR)/initramfs/bin/ssh"; fi
	@# Copy sshd
	@if [ -f "$(USERSPACE_OUT_RELEASE)/sshd" ]; then cp "$(USERSPACE_OUT_RELEASE)/sshd" "$(TARGET_DIR)/initramfs/bin/sshd"; fi
	@# Copy service manager
	@if [ -f "$(USERSPACE_OUT_RELEASE)/service" ]; then cp "$(USERSPACE_OUT_RELEASE)/service" "$(TARGET_DIR)/initramfs/bin/service"; ln -sf /bin/service "$(TARGET_DIR)/initramfs/bin/servicemgr"; fi
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
	@if [ -f "$(USERSPACE_OUT_RELEASE)/networkd" ]; then cp "$(USERSPACE_OUT_RELEASE)/networkd" "$(TARGET_DIR)/initramfs/bin/networkd"; fi
	@# Copy resolvd
	@if [ -f "$(USERSPACE_OUT_RELEASE)/resolvd" ]; then cp "$(USERSPACE_OUT_RELEASE)/resolvd" "$(TARGET_DIR)/initramfs/bin/resolvd"; fi
	@# Copy rdpd
	@if [ -f "$(USERSPACE_OUT_RELEASE)/rdpd" ]; then cp "$(USERSPACE_OUT_RELEASE)/rdpd" "$(TARGET_DIR)/initramfs/bin/rdpd"; fi
	@# Copy journald and journalctl
	@if [ -f "$(USERSPACE_OUT_RELEASE)/journald" ]; then cp "$(USERSPACE_OUT_RELEASE)/journald" "$(TARGET_DIR)/initramfs/bin/journald"; fi
	@if [ -f "$(USERSPACE_OUT_RELEASE)/journalctl" ]; then cp "$(USERSPACE_OUT_RELEASE)/journalctl" "$(TARGET_DIR)/initramfs/bin/journalctl"; fi
	@# Copy evtest and argtest
	@if [ -f "$(USERSPACE_OUT_RELEASE)/evtest" ]; then cp "$(USERSPACE_OUT_RELEASE)/evtest" "$(TARGET_DIR)/initramfs/bin/evtest"; fi
	@if [ -f "$(USERSPACE_OUT_RELEASE)/argtest" ]; then cp "$(USERSPACE_OUT_RELEASE)/argtest" "$(TARGET_DIR)/initramfs/bin/argtest"; fi
	@# Copy getty
	@if [ -f "$(USERSPACE_OUT_RELEASE)/getty" ]; then cp "$(USERSPACE_OUT_RELEASE)/getty" "$(TARGET_DIR)/initramfs/bin/getty"; fi
	@# Copy soundd
	@if [ -f "$(USERSPACE_OUT_RELEASE)/soundd" ]; then cp "$(USERSPACE_OUT_RELEASE)/soundd" "$(TARGET_DIR)/initramfs/bin/soundd"; fi
	@# Copy doom
	@if [ -f "$(USERSPACE_OUT_RELEASE)/doom" ]; then cp "$(USERSPACE_OUT_RELEASE)/doom" "$(TARGET_DIR)/initramfs/bin/doom"; fi
	@# Copy python
	@if [ -f "$(USERSPACE_OUT_RELEASE)/python" ]; then cp "$(USERSPACE_OUT_RELEASE)/python" "$(TARGET_DIR)/initramfs/bin/python"; fi
	@# Copy vim
	@if [ -f "$(USERSPACE_OUT_RELEASE)/vim" ]; then cp "$(USERSPACE_OUT_RELEASE)/vim" "$(TARGET_DIR)/initramfs/bin/vim"; fi
	@# Copy signal-test (optional test utility)
	@if [ -f "$(USERSPACE_OUT_RELEASE)/signal-test" ]; then cp "$(USERSPACE_OUT_RELEASE)/signal-test" "$(TARGET_DIR)/initramfs/bin/signal-test"; fi
	@# Create services.d directory with service definitions
	@mkdir -p $(TARGET_DIR)/initramfs/etc/services.d
	@echo "PATH=/bin/journald" > $(TARGET_DIR)/initramfs/etc/services.d/journald
	@echo "ENABLED=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/journald
	@echo "RESTART=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/journald
	@echo "PATH=/bin/networkd" > $(TARGET_DIR)/initramfs/etc/services.d/networkd
	@echo "ENABLED=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/networkd
	@echo "RESTART=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/networkd
	@echo "PATH=/bin/resolvd" > $(TARGET_DIR)/initramfs/etc/services.d/resolvd
	@echo "ENABLED=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/resolvd
	@echo "RESTART=yes" >> $(TARGET_DIR)/initramfs/etc/services.d/resolvd
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
	@# Create default hosts file with common entries
	@echo "# /etc/hosts - static hostname-to-IP mappings" > $(TARGET_DIR)/initramfs/etc/hosts
	@echo "# Managed by resolvd and hostctl" >> $(TARGET_DIR)/initramfs/etc/hosts
	@echo "" >> $(TARGET_DIR)/initramfs/etc/hosts
	@echo "# Loopback addresses" >> $(TARGET_DIR)/initramfs/etc/hosts
	@echo "127.0.0.1       localhost localhost.localdomain" >> $(TARGET_DIR)/initramfs/etc/hosts
	@echo "::1             localhost localhost.localdomain ip6-localhost ip6-loopback" >> $(TARGET_DIR)/initramfs/etc/hosts
	@echo "" >> $(TARGET_DIR)/initramfs/etc/hosts
	@echo "# IPv6 special addresses" >> $(TARGET_DIR)/initramfs/etc/hosts
	@echo "fe00::0         ip6-localnet" >> $(TARGET_DIR)/initramfs/etc/hosts
	@echo "ff00::0         ip6-mcastprefix" >> $(TARGET_DIR)/initramfs/etc/hosts
	@echo "ff02::1         ip6-allnodes" >> $(TARGET_DIR)/initramfs/etc/hosts
	@echo "ff02::2         ip6-allrouters" >> $(TARGET_DIR)/initramfs/etc/hosts
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
create-rootfs: kernel bootloader external-binaries initramfs-minimal
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
	[ -f "$(USERSPACE_OUT_RELEASE)/init" ] && sudo cp "$(USERSPACE_OUT_RELEASE)/init" $(TARGET_DIR)/mnt/root/sbin/init || true && \
	[ -f "$(USERSPACE_OUT_RELEASE)/init" ] && sudo ln -sf /sbin/init $(TARGET_DIR)/mnt/root/init || true && \
	[ -f "$(USERSPACE_OUT_RELEASE)/esh" ] && sudo cp "$(USERSPACE_OUT_RELEASE)/esh" $(TARGET_DIR)/mnt/root/bin/esh || true && \
	[ -f "$(USERSPACE_OUT_RELEASE)/esh" ] && sudo ln -sf /bin/esh $(TARGET_DIR)/mnt/root/bin/sh || true && \
	[ -f "$(USERSPACE_OUT_RELEASE)/getty" ] && sudo cp "$(USERSPACE_OUT_RELEASE)/getty" $(TARGET_DIR)/mnt/root/bin/getty || true && \
	[ -f "$(USERSPACE_OUT_RELEASE)/login" ] && sudo cp "$(USERSPACE_OUT_RELEASE)/login" $(TARGET_DIR)/mnt/root/bin/login || true && \
	for prog in gwbasic curses-demo tls-test thread-test ssh sshd rdpd service networkd journald journalctl evtest argtest vim python $(COREUTILS_BINS) testcolors; do \
		[ -f "$(USERSPACE_OUT_RELEASE)/$$prog" ] && sudo cp "$(USERSPACE_OUT_RELEASE)/$$prog" $(TARGET_DIR)/mnt/root/usr/bin/ || true; \
	done && \
	sudo cp userspace/apps/gwbasic/examples/*.bas $(TARGET_DIR)/mnt/root/usr/share/gwbasic/ 2>/dev/null || true; \
	[ -f "$(USERSPACE_OUT_RELEASE)/tls-test" ] && echo "TLS test installed" || true; \
	[ -f "$(USERSPACE_OUT_RELEASE)/vim" ] && echo "vim installed" || true; \
	[ -f "$(USERSPACE_OUT_RELEASE)/python" ] && echo "Python installed" || true; \
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

# Search for packages
pkgmgr-search:
	@if [ -z "$(PKG)" ]; then \
		echo "Usage: make pkgmgr-search PKG=<package-name>"; \
		exit 1; \
	fi
	@python3 pkgmgr/bin/oxdnf search $(PKG)

# Build package from SRPM
pkgmgr-build:
	@if [ -z "$(PKG)" ]; then \
		echo "Usage: make pkgmgr-build PKG=<package-name>"; \
		exit 1; \
	fi
	@echo "Building package: $(PKG)"
	@python3 pkgmgr/bin/oxdnf buildsrpm $(PKG)

# Install package
pkgmgr-install:
	@if [ -z "$(PKG)" ]; then \
		echo "Usage: make pkgmgr-install PKG=<package-name>"; \
		exit 1; \
	fi
	@echo "Installing package: $(PKG)"
	@python3 pkgmgr/bin/oxdnf install $(PKG)

# Show package manager help
pkgmgr-help:
	@echo "OXIDE Package Manager (oxdnf) - Make Targets"
	@echo ""
	@echo "Available targets:"
	@echo "  make pkgmgr-sync              - Sync Fedora repository metadata"
	@echo "  make pkgmgr-search PKG=bash   - Search for packages"
	@echo "  make pkgmgr-build PKG=bash    - Build package from Fedora SRPM"
	@echo "  make pkgmgr-install PKG=bash  - Install a built package"
	@echo ""
	@echo "Direct oxdnf usage:"
	@echo "  python3 pkgmgr/bin/oxdnf --help"
	@echo ""
	@echo "Examples:"
	@echo "  make pkgmgr-sync"
	@echo "  make pkgmgr-search PKG=vim"
	@echo "  make pkgmgr-build PKG=vim"
	@echo "  make pkgmgr-install PKG=vim"
	@echo ""
	@echo "See pkgmgr/README.md for full documentation"
