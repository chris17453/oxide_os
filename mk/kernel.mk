# — GraveShift: Kernel and bootloader build targets.
# The foundation everything else sits on. Break this and nothing boots.

.PHONY: kernel bootloader release

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
