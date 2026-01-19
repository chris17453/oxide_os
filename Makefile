# EFFLUX OS Makefile
#
# Build and test the EFFLUX operating system

SHELL := /usr/bin/bash

.PHONY: all build kernel bootloader userspace initramfs clean run test check fmt clippy

# Configuration
ARCH ?= x86_64
PROFILE ?= debug
QEMU_TIMEOUT ?= 15

# Paths
TARGET_DIR := target
KERNEL_TARGET := $(TARGET_DIR)/$(ARCH)-unknown-none/$(PROFILE)/efflux-kernel
BOOTLOADER_TARGET := $(TARGET_DIR)/$(ARCH)-unknown-uefi/$(PROFILE)/efflux-boot-uefi.efi
BOOT_DIR := $(TARGET_DIR)/boot
INITRAMFS := $(TARGET_DIR)/initramfs.cpio
OVMF := $(shell for p in /usr/share/OVMF/OVMF_CODE.fd /usr/share/edk2-ovmf/x64/OVMF_CODE.fd /usr/share/edk2/ovmf/OVMF_CODE.fd /usr/share/qemu/OVMF.fd; do [ -f "$$p" ] && echo "$$p" && break; done)

# Userspace configuration
USERSPACE_TARGET := userspace/x86_64-efflux-user.json
USERSPACE_OUT := $(TARGET_DIR)/x86_64-efflux-user/$(PROFILE)
USERSPACE_OUT_RELEASE := $(TARGET_DIR)/x86_64-efflux-user/release
CARGO_USER_FLAGS := -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem

# Userspace packages to build
USERSPACE_PACKAGES := init esh login coreutils

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
kernel:
	cargo build --package efflux-kernel

# Build bootloader
bootloader:
	cargo build --package efflux-boot-uefi --target $(ARCH)-unknown-uefi

# Build release
release:
	cargo build --package efflux-kernel --release
	cargo build --package efflux-boot-uefi --target $(ARCH)-unknown-uefi --release

# Build all userspace programs
userspace:
	@echo "Building userspace programs..."
	@for pkg in $(USERSPACE_PACKAGES); do \
		echo "  Building $$pkg..."; \
		cargo build --package $$pkg --target $(USERSPACE_TARGET) $(CARGO_USER_FLAGS) || exit 1; \
	done
	@echo "Userspace programs built."

# Build optimized userspace (smaller binaries)
userspace-release:
	@echo "Building userspace programs (release)..."
	@for pkg in $(USERSPACE_PACKAGES); do \
		echo "  Building $$pkg (release)..."; \
		cargo build --package $$pkg --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS) || exit 1; \
	done
	@echo "Stripping binaries..."
	@for prog in init esh login $(COREUTILS_BINS); do \
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
	@mkdir -p $(TARGET_DIR)/initramfs/home
	@mkdir -p $(TARGET_DIR)/initramfs/root
	@# Copy init to /sbin
	@cp "$(USERSPACE_OUT_RELEASE)/init" "$(TARGET_DIR)/initramfs/sbin/init"
	@ln -sf /sbin/init "$(TARGET_DIR)/initramfs/init"
	@# Copy shell
	@cp "$(USERSPACE_OUT_RELEASE)/esh" "$(TARGET_DIR)/initramfs/bin/esh"
	@ln -sf /bin/esh "$(TARGET_DIR)/initramfs/bin/sh"
	@# Copy login
	@cp "$(USERSPACE_OUT_RELEASE)/login" "$(TARGET_DIR)/initramfs/bin/login"
	@# Copy coreutils
	@for prog in $(COREUTILS_BINS); do \
		if [ -f "$(USERSPACE_OUT_RELEASE)/$$prog" ]; then \
			cp "$(USERSPACE_OUT_RELEASE)/$$prog" "$(TARGET_DIR)/initramfs/bin/"; \
		fi; \
	done
	@# Create symlinks for common aliases
	@ln -sf /bin/true "$(TARGET_DIR)/initramfs/bin/:" 2>/dev/null || true
	@ln -sf /bin/ls "$(TARGET_DIR)/initramfs/bin/dir" 2>/dev/null || true
	@# Create etc files
	@echo "root:x:0:0:root:/root:/bin/esh" > $(TARGET_DIR)/initramfs/etc/passwd
	@echo "root:x:0:" > $(TARGET_DIR)/initramfs/etc/group
	@echo "PATH=/bin:/sbin" > $(TARGET_DIR)/initramfs/etc/profile
	@echo "export PATH" >> $(TARGET_DIR)/initramfs/etc/profile
	@echo "EFFLUX" > $(TARGET_DIR)/initramfs/etc/hostname
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
	@echo "PATH=/bin:/sbin" > $(TARGET_DIR)/initramfs/etc/profile
	@echo "export PATH" >> $(TARGET_DIR)/initramfs/etc/profile
	@cd $(TARGET_DIR)/initramfs && find . | cpio -o -H newc > ../initramfs.cpio 2>/dev/null
	@echo "Initramfs created (debug): $(INITRAMFS)"
	@ls -la $(INITRAMFS)

# List all binaries that will be included in initramfs
list-bins:
	@echo "Userspace binaries:"
	@echo "  System: init login"
	@echo "  Shell: esh (sh)"
	@echo "  Coreutils: $(COREUTILS_BINS)"

# Create boot directory structure with kernel and bootloader
boot-dir: kernel bootloader
	@mkdir -p $(BOOT_DIR)/EFI/BOOT
	@mkdir -p $(BOOT_DIR)/EFI/EFFLUX
	@cp $(BOOTLOADER_TARGET) $(BOOT_DIR)/EFI/BOOT/BOOTX64.EFI
	@cp $(KERNEL_TARGET) $(BOOT_DIR)/EFI/EFFLUX/kernel.elf
	@echo "Boot directory created at $(BOOT_DIR)"
	@echo "  - Bootloader: EFI/BOOT/BOOTX64.EFI"
	@echo "  - Kernel: EFI/EFFLUX/kernel.elf"

# Run in QEMU (interactive)
run: boot-dir
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		echo "Install: sudo apt install ovmf (Debian/Ubuntu)"; \
		echo "         sudo dnf install edk2-ovmf (Fedora)"; \
		exit 1; \
	fi
	qemu-system-x86_64 \
		-machine q35 \
		-m 256M \
		-bios "$(OVMF)" \
		-drive format=raw,file=fat:rw:$(BOOT_DIR) \
		-serial stdio \
		-no-reboot

# Run in QEMU with networking (interactive)
run-net: boot-dir
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		echo "Install: sudo apt install ovmf (Debian/Ubuntu)"; \
		echo "         sudo dnf install edk2-ovmf (Fedora)"; \
		exit 1; \
	fi
	qemu-system-x86_64 \
		-machine q35 \
		-m 256M \
		-bios "$(OVMF)" \
		-drive format=raw,file=fat:rw:$(BOOT_DIR) \
		-device virtio-net-pci,netdev=net0 \
		-netdev user,id=net0,hostfwd=tcp::2222-:22 \
		-serial stdio \
		-no-reboot

# Run headless (for testing)
run-headless: boot-dir
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		exit 1; \
	fi
	qemu-system-x86_64 \
		-machine q35 \
		-m 256M \
		-bios "$(OVMF)" \
		-drive format=raw,file=fat:rw:$(BOOT_DIR) \
		-serial stdio \
		-display none \
		-no-reboot

# Run headless with networking
run-headless-net: boot-dir
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		exit 1; \
	fi
	qemu-system-x86_64 \
		-machine q35 \
		-m 256M \
		-bios "$(OVMF)" \
		-drive format=raw,file=fat:rw:$(BOOT_DIR) \
		-device virtio-net-pci,netdev=net0 \
		-netdev user,id=net0,hostfwd=tcp::2222-:22 \
		-serial stdio \
		-display none \
		-no-reboot

# Automated test: boot and check for expected output
test: boot-dir
	@echo "Running automated boot test..."
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		exit 1; \
	fi
	@timeout $(QEMU_TIMEOUT) qemu-system-x86_64 \
		-machine q35 \
		-m 256M \
		-bios "$(OVMF)" \
		-drive format=raw,file=fat:rw:$(BOOT_DIR) \
		-serial file:$(TARGET_DIR)/serial.log \
		-display none \
		-no-reboot \
		2>/dev/null || true
	@echo "--- Serial Output ---"
	@cat $(TARGET_DIR)/serial.log 2>/dev/null || echo "(no output)"
	@echo "--- End Output ---"
	@if grep -q "EFFLUX" $(TARGET_DIR)/serial.log 2>/dev/null; then \
		echo ""; \
		echo "TEST PASSED: Boot message found"; \
		exit 0; \
	else \
		echo ""; \
		echo "TEST FAILED: Expected 'EFFLUX' in output"; \
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

# Show help
help:
	@echo "EFFLUX OS Build System"
	@echo ""
	@echo "Targets:"
	@echo "  all            - Build kernel and bootloader (default)"
	@echo "  build-full     - Build kernel, bootloader, userspace, and initramfs"
	@echo "  kernel         - Build kernel only"
	@echo "  bootloader     - Build UEFI bootloader only"
	@echo "  userspace      - Build all userspace programs (debug)"
	@echo "  userspace-release - Build all userspace programs (release)"
	@echo "  userspace-pkg  - Build single package (PKG=name)"
	@echo "  initramfs      - Create initramfs (release)"
	@echo "  initramfs-debug - Create initramfs (debug)"
	@echo "  list-bins      - List all userspace binaries"
	@echo "  release        - Build kernel/bootloader in release mode"
	@echo "  run            - Run in QEMU (interactive)"
	@echo "  run-net        - Run in QEMU with networking"
	@echo "  run-headless   - Run in QEMU without display"
	@echo "  run-headless-net - Run headless with networking"
	@echo "  test           - Automated boot test"
	@echo "  check          - Quick syntax/type check"
	@echo "  fmt            - Format code"
	@echo "  fmt-check      - Check formatting"
	@echo "  clippy         - Run clippy linter"
	@echo "  clean          - Remove build artifacts"
	@echo ""
	@echo "Variables:"
	@echo "  ARCH           - Target architecture (default: x86_64)"
	@echo "  PROFILE        - Build profile (default: debug)"
	@echo "  QEMU_TIMEOUT   - Test timeout in seconds (default: 15)"
	@echo ""
	@echo "Examples:"
	@echo "  make build-full          - Build everything"
	@echo "  make userspace-pkg PKG=coreutils - Build only coreutils"
	@echo "  make initramfs && make run - Build and run with userspace"
