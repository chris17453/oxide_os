# OXIDE OS Makefile
#
# Build and test the OXIDE operating system

SHELL := /usr/bin/bash

.PHONY: all build kernel bootloader userspace initramfs clean run run-no-net run-headless run-headless-no-net run-kvm run-kvm-vnc run-kvm-serial test check fmt clippy boot-image toolchain install-toolchain test-toolchain clean-toolchain

# Configuration
ARCH ?= x86_64
PROFILE ?= debug
QEMU_TIMEOUT ?= 15

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

# Paths
TARGET_DIR := target
KERNEL_TARGET := $(TARGET_DIR)/$(ARCH)-unknown-none/$(PROFILE)/kernel
BOOTLOADER_TARGET := $(TARGET_DIR)/$(ARCH)-unknown-uefi/$(PROFILE)/boot-uefi.efi
BOOT_DIR := $(TARGET_DIR)/boot
INITRAMFS := $(TARGET_DIR)/initramfs.cpio
OVMF := $(shell for p in /usr/share/OVMF/OVMF_CODE.fd /usr/share/edk2-ovmf/x64/OVMF_CODE.fd /usr/share/edk2/ovmf/OVMF_CODE.fd /usr/share/qemu/OVMF.fd; do [ -f "$$p" ] && echo "$$p" && break; done)

# Userspace configuration
USERSPACE_TARGET := userspace/x86_64-user.json
USERSPACE_OUT := $(TARGET_DIR)/x86_64-user/$(PROFILE)
USERSPACE_OUT_RELEASE := $(TARGET_DIR)/x86_64-user/release
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
	cargo build --package kernel

# Build bootloader
bootloader:
	cargo build --package boot-uefi --target $(ARCH)-unknown-uefi

# Build release
release:
	cargo build --package kernel --release
	cargo build --package boot-uefi --target $(ARCH)-unknown-uefi --release

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
	@echo "  Building gwbasic (release)..."
	@cargo build --package oxide-gwbasic --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS) --features oxide || exit 1
	@echo "Stripping binaries..."
	@for prog in init esh login gwbasic $(COREUTILS_BINS); do \
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
	@# Copy gwbasic
	@cp "$(USERSPACE_OUT_RELEASE)/gwbasic" "$(TARGET_DIR)/initramfs/bin/gwbasic"
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
	@echo "PATH=/initramfs/bin:/initramfs/sbin:/bin:/sbin" > $(TARGET_DIR)/initramfs/etc/profile
	@echo "export PATH" >> $(TARGET_DIR)/initramfs/etc/profile
	@echo "OXIDE" > $(TARGET_DIR)/initramfs/etc/hostname
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
	@echo "PATH=/initramfs/bin:/initramfs/sbin:/bin:/sbin" > $(TARGET_DIR)/initramfs/etc/profile
	@echo "export PATH" >> $(TARGET_DIR)/initramfs/etc/profile
	@cd $(TARGET_DIR)/initramfs && find . | cpio -o -H newc > ../initramfs.cpio 2>/dev/null
	@echo "Initramfs created (debug): $(INITRAMFS)"
	@ls -la $(INITRAMFS)

# List all binaries that will be included in initramfs
list-bins:
	@echo "Userspace binaries:"
	@echo "  System: init login"
	@echo "  Shell: esh (sh)"
	@echo "  Apps: gwbasic"
	@echo "  Coreutils: $(COREUTILS_BINS)"

# Create boot directory structure with kernel, bootloader, and initramfs
boot-dir: kernel bootloader initramfs
	@mkdir -p $(BOOT_DIR)/EFI/BOOT
	@mkdir -p $(BOOT_DIR)/EFI/OXIDE
	@cp $(BOOTLOADER_TARGET) $(BOOT_DIR)/EFI/BOOT/BOOTX64.EFI
	@cp $(KERNEL_TARGET) $(BOOT_DIR)/EFI/OXIDE/kernel.elf
	@cp $(TARGET_DIR)/initramfs.cpio $(BOOT_DIR)/EFI/OXIDE/initramfs.cpio
	@echo "Boot directory created at $(BOOT_DIR)"
	@echo "  - Bootloader: EFI/BOOT/BOOTX64.EFI"
	@echo "  - Kernel: EFI/OXIDE/kernel.elf"
	@echo "  - Initramfs: EFI/OXIDE/initramfs.cpio"

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

# Run in QEMU (interactive, with networking)
run: boot-dir
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		echo "Install: sudo apt install ovmf (Debian/Ubuntu)"; \
		echo "         sudo dnf install edk2-ovmf (Fedora/RHEL)"; \
		exit 1; \
	fi
	@if ! command -v $(QEMU) >/dev/null 2>&1; then \
		echo "Error: QEMU command '$(QEMU)' not found"; \
		echo "Install: sudo apt install qemu-system-x86 (Debian/Ubuntu)"; \
		echo "         sudo dnf install qemu-system-x86 (Fedora/RHEL)"; \
		exit 1; \
	fi
	@mkdir -p /tmp/qemu-oxide
	TMPDIR=/tmp/qemu-oxide $(QEMU) \
		-machine q35 \
		-m 256M \
		-bios "$(OVMF)" \
		-drive format=raw,file=fat:rw:$(BOOT_DIR),if=none,id=disk \
		-device ide-hd,drive=disk \
		-device virtio-net-pci,netdev=net0 \
		-netdev user,id=net0,hostfwd=tcp::2222-:22 \
		-serial stdio \
		-no-reboot

# Run in QEMU without networking (for minimal testing)
run-no-net: boot-dir
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		echo "Install: sudo apt install ovmf (Debian/Ubuntu)"; \
		echo "         sudo dnf install edk2-ovmf (Fedora/RHEL)"; \
		exit 1; \
	fi
	@mkdir -p /tmp/qemu-oxide
	TMPDIR=/tmp/qemu-oxide $(QEMU) \
		-machine q35 \
		-m 256M \
		-bios "$(OVMF)" \
		-drive format=raw,file=fat:rw:$(BOOT_DIR),if=none,id=disk \
		-device ide-hd,drive=disk \
		-serial stdio \
		-no-reboot

# Run headless (for testing, with networking)
run-headless: boot-dir
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		exit 1; \
	fi
	@mkdir -p /tmp/qemu-oxide
	TMPDIR=/tmp/qemu-oxide $(QEMU) \
		-machine q35 \
		-m 256M \
		-bios "$(OVMF)" \
		-drive format=raw,file=fat:rw:$(BOOT_DIR),if=none,id=disk \
		-device ide-hd,drive=disk \
		-device virtio-net-pci,netdev=net0 \
		-netdev user,id=net0,hostfwd=tcp::2222-:22 \
		-serial stdio \
		-display none \
		-no-reboot

# Run headless without networking
run-headless-no-net: boot-dir
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		exit 1; \
	fi
	@mkdir -p /tmp/qemu-oxide
	TMPDIR=/tmp/qemu-oxide $(QEMU) \
		-machine q35 \
		-m 256M \
		-bios "$(OVMF)" \
		-drive format=raw,file=fat:rw:$(BOOT_DIR),if=none,id=disk \
		-device ide-hd,drive=disk \
		-serial stdio \
		-display none \
		-no-reboot

# Run with qemu-kvm using disk image (RHEL 10 compatible)
run-kvm: boot-image
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
	@# Create a writable copy of OVMF_VARS.fd for this session
	@mkdir -p $(TARGET_DIR)
	@cp /usr/share/edk2/ovmf/OVMF_VARS.fd $(TARGET_DIR)/OVMF_VARS.fd 2>/dev/null || true
	@mkdir -p /tmp/qemu-oxide
	@echo "Starting QEMU..."
	@TMPDIR=/tmp/qemu-oxide /usr/libexec/qemu-kvm \
		-machine q35,accel=kvm:tcg \
		-cpu max,+invtsc \
		-smp 2 \
		-m 256M \
		-drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/ovmf/OVMF_CODE.fd \
		-drive if=pflash,format=raw,file=$(TARGET_DIR)/OVMF_VARS.fd \
		-drive file=$(TARGET_DIR)/boot.img,format=raw,if=none,id=bootdisk \
		-device ide-hd,drive=bootdisk,bus=ide.0 \
		-vga std \
		-vnc :0 \
		-chardev stdio,id=char0,mux=on \
		-serial chardev:char0 \
		-no-reboot & \
	QEMU_PID=$$!; \
	echo "QEMU started (PID: $$QEMU_PID)"; \
	sleep 2; \
	if command -v vncviewer >/dev/null 2>&1; then \
		echo "Launching VNC viewer window..."; \
		vncviewer localhost:5900 2>/dev/null; \
	elif flatpak list --app 2>/dev/null | grep -q tigervnc; then \
		echo "Launching VNC viewer window (Flatpak)..."; \
		flatpak run org.tigervnc.vncviewer localhost:5900 2>/dev/null; \
	else \
		echo "VNC viewer not found - connect manually to localhost:5900"; \
		echo "Install: sudo dnf install tigervnc"; \
		wait $$QEMU_PID; \
	fi; \
	echo "VNC viewer closed, stopping QEMU..."; \
	kill $$QEMU_PID 2>/dev/null || true; \
	wait $$QEMU_PID 2>/dev/null || true

# Run with qemu-kvm and auto-launch VNC viewer
run-kvm-vnc: boot-image
	@if ! command -v vncviewer >/dev/null 2>&1 && ! (flatpak list --app 2>/dev/null | grep -q tigervnc); then \
		echo "Error: vncviewer not found"; \
		echo "Install: sudo dnf install tigervnc"; \
		echo "Or Flatpak: flatpak install flathub org.tigervnc.vncviewer"; \
		exit 1; \
	fi
	@# Create a writable copy of OVMF_VARS.fd for this session
	@mkdir -p $(TARGET_DIR)
	@cp /usr/share/edk2/ovmf/OVMF_VARS.fd $(TARGET_DIR)/OVMF_VARS.fd 2>/dev/null || true
	@mkdir -p /tmp/qemu-oxide
	@echo "Starting QEMU in background..."
	@TMPDIR=/tmp/qemu-oxide /usr/libexec/qemu-kvm \
		-machine pc \
		-cpu max \
		-m 256M \
		-drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/ovmf/OVMF_CODE.fd \
		-drive if=pflash,format=raw,file=$(TARGET_DIR)/OVMF_VARS.fd \
		-drive format=raw,file=$(TARGET_DIR)/boot.img,if=ide \
		-vnc :0 \
		-no-reboot & \
	QEMU_PID=$$!; \
	echo "QEMU started with PID $$QEMU_PID"; \
	sleep 1; \
	if command -v vncviewer >/dev/null 2>&1; then \
		echo "Launching VNC viewer (native)..."; \
		vncviewer localhost:5900; \
	elif flatpak list --app 2>/dev/null | grep -q tigervnc; then \
		echo "Launching VNC viewer (Flatpak)..."; \
		flatpak run org.tigervnc.vncviewer localhost:5900; \
	fi; \
	kill $$QEMU_PID 2>/dev/null || true

# Run with qemu-kvm using serial console only (no graphics/VNC)
run-kvm-serial: boot-image
	@# Create a writable copy of OVMF_VARS.fd for this session
	@mkdir -p $(TARGET_DIR)
	@cp /usr/share/edk2/ovmf/OVMF_VARS.fd $(TARGET_DIR)/OVMF_VARS.fd 2>/dev/null || true
	@mkdir -p /tmp/qemu-oxide
	TMPDIR=/tmp/qemu-oxide /usr/libexec/qemu-kvm \
		-machine pc \
		-cpu max \
		-m 256M \
		-drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/ovmf/OVMF_CODE.fd \
		-drive if=pflash,format=raw,file=$(TARGET_DIR)/OVMF_VARS.fd \
		-drive format=raw,file=$(TARGET_DIR)/boot.img,if=ide \
		-nographic \
		-no-reboot

# Automated test: boot and check for expected output
test: boot-dir
	@echo "Running automated boot test..."
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		exit 1; \
	fi
	@mkdir -p /tmp/qemu-oxide
	@TMPDIR=/tmp/qemu-oxide timeout $(QEMU_TIMEOUT) $(QEMU) \
		-machine q35 \
		-m 256M \
		-bios "$(OVMF)" \
		-drive format=raw,file=fat:rw:$(BOOT_DIR),if=none,id=disk \
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
help:
	@echo "OXIDE OS Build System"
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
	@echo "  boot-image     - Create bootable disk image (for qemu-kvm)"
	@echo "  list-bins      - List all userspace binaries"
	@echo "  release        - Build kernel/bootloader in release mode"
	@echo "  run            - Run in QEMU (interactive, with networking)"
	@echo "  run-no-net     - Run in QEMU without networking"
	@echo "  run-headless   - Run in QEMU without display (with networking)"
	@echo "  run-headless-no-net - Run headless without networking"
	@echo "  run-kvm        - Run with qemu-kvm (VNC on :5900, serial on stdio)"
	@echo "  run-kvm-vnc    - Run with qemu-kvm and auto-launch VNC viewer"
	@echo "  run-kvm-serial - Run with qemu-kvm serial console only (no graphics)"
	@echo "  test           - Automated boot test"
	@echo "  check          - Quick syntax/type check"
	@echo "  fmt            - Format code"
	@echo "  fmt-check      - Check formatting"
	@echo "  clippy         - Run clippy linter"
	@echo "  clean          - Remove build artifacts"
	@echo "  show-config    - Show detected configuration (QEMU, OVMF, etc.)"
	@echo ""
	@echo "Cross-Compiler Toolchain:"
	@echo "  toolchain      - Build OXIDE cross-compiler toolchain"
	@echo "  test-toolchain - Test toolchain with examples"
	@echo "  install-toolchain - Install toolchain (PREFIX=/usr/local/oxide)"
	@echo "  clean-toolchain - Clean toolchain artifacts"
	@echo ""
	@echo "Variables:"
	@echo "  ARCH           - Target architecture (default: x86_64)"
	@echo "  PROFILE        - Build profile (default: debug)"
	@echo "  QEMU_TIMEOUT   - Test timeout in seconds (default: 15)"
	@echo "  QEMU           - QEMU command (auto-detected, override if needed)"
	@echo ""
	@echo "Examples:"
	@echo "  make build-full          - Build everything"
	@echo "  make userspace-pkg PKG=coreutils - Build only coreutils"
	@echo "  make run                 - Build and run (includes initramfs + net)"
	@echo "  make show-config         - Show detected QEMU and OVMF paths"
	@echo ""
	@echo "RHEL 10 Note:"
	@echo "  RHEL only ships qemu-kvm (no qemu-system-x86_64 with SDL/GTK)"
	@echo "  Install: sudo dnf install qemu-kvm edk2-ovmf parted mtools"
	@echo "  "
	@echo "  Three ways to run on RHEL 10:"
	@echo "    make run-kvm        - VNC on :5900 + serial (auto-launches VNC viewer)"
	@echo "    make run-kvm-vnc    - Same as run-kvm (explicitly launch VNC viewer)"
	@echo "    make run-kvm-serial - Serial console only, no graphics"
	@echo "  "
	@echo "  Note: No sudo required! mtools manipulates FAT without mounting."



# Build toolchain components
toolchain:
	@echo "Building OXIDE cross-compiler toolchain..."
	@echo "  Building assembler (as)..."
	@cargo build --package oxide-as --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS)
	@echo "  Building linker (ld)..."
	@cargo build --package oxide-ld --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS)
	@echo "  Building archiver (ar)..."
	@cargo build --package oxide-ar --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS)
	@echo "  Building libc..."
	@cargo build --package libc --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS)
	@echo ""
	@echo "Installing toolchain components to sysroot..."
	@mkdir -p toolchain/sysroot/lib
	@# Copy libc.a to sysroot
	@if [ -f "$(USERSPACE_OUT_RELEASE)/liblibc.rlib" ]; then \
		cp "$(USERSPACE_OUT_RELEASE)/liblibc.rlib" "toolchain/sysroot/lib/liboxide_libc.a"; \
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
	--ro-bind /home/nd /home/nd \
	--bind /home/nd/.claude.json /home/nd/.claude.json \
	--bind /home/nd/.claude /home/nd/.claude \
	--bind "$(CURDIR)" "$(CURDIR)" \
	--bind /tmp /tmp \
	--dev /dev \
	--proc /proc \
	--chdir "$(CURDIR)" \
	--die-with-parent \
	-- /usr/bin/node /usr/local/lib/node_modules/@anthropic-ai/claude-code/cli.js --dangerously-skip-permissions