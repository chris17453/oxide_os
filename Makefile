# EFFLUX OS Makefile
#
# Build and test the EFFLUX operating system

.PHONY: all build kernel bootloader clean run test check fmt clippy

# Configuration
ARCH ?= x86_64
PROFILE ?= debug
QEMU_TIMEOUT ?= 15

# Paths
TARGET_DIR := target
KERNEL_TARGET := $(TARGET_DIR)/$(ARCH)-unknown-none/$(PROFILE)/efflux-kernel
BOOTLOADER_TARGET := $(TARGET_DIR)/$(ARCH)-unknown-uefi/$(PROFILE)/efflux-boot-uefi.efi
BOOT_DIR := $(TARGET_DIR)/boot
OVMF := $(shell for p in /usr/share/OVMF/OVMF_CODE.fd /usr/share/edk2-ovmf/x64/OVMF_CODE.fd /usr/share/edk2/ovmf/OVMF_CODE.fd /usr/share/qemu/OVMF.fd; do [ -f "$$p" ] && echo "$$p" && break; done)

# Default target
all: build

# Build everything
build: kernel bootloader

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
	@echo "  all        - Build kernel and bootloader (default)"
	@echo "  kernel     - Build kernel only"
	@echo "  bootloader - Build UEFI bootloader only"
	@echo "  release    - Build in release mode"
	@echo "  run        - Run in QEMU (interactive)"
	@echo "  run-headless - Run in QEMU without display"
	@echo "  test       - Automated boot test"
	@echo "  check      - Quick syntax/type check"
	@echo "  fmt        - Format code"
	@echo "  fmt-check  - Check formatting"
	@echo "  clippy     - Run clippy linter"
	@echo "  clean      - Remove build artifacts"
	@echo ""
	@echo "Variables:"
	@echo "  ARCH         - Target architecture (default: x86_64)"
	@echo "  PROFILE      - Build profile (default: debug)"
	@echo "  QEMU_TIMEOUT - Test timeout in seconds (default: 15)"
