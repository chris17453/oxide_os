# — GraveShift: Kernel and bootloader build targets.
# The foundation everything else sits on. Break this and nothing boots.

.PHONY: kernel bootloader release

# — SableWire: Custom target requires building core/alloc from source.
# Same dance the bootloader does for x86_64-unknown-uefi. — SableWire
KERNEL_TARGET_JSON := targets/x86_64-unknown-oxide.json
KERNEL_BUILD_STD := -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem

# Build kernel
# Pass KERNEL_FEATURES to enable debug output, e.g.: make run KERNEL_FEATURES=debug-all
# — PatchBay: OXIDE_BUILD_NUMBER env var injects the NT-style build counter at compile time
kernel:
	@echo "Building kernel..."
ifeq ($(PROFILE),release)
ifneq ($(KERNEL_FEATURES),)
	@OXIDE_BUILD_NUMBER=$(OXIDE_BUILD) OXIDE_VERSION_STRING=$(OXIDE_VERSION) cargo build --package kernel --target $(KERNEL_TARGET_JSON) $(KERNEL_BUILD_STD) --release --features $(KERNEL_FEATURES)
else
	@OXIDE_BUILD_NUMBER=$(OXIDE_BUILD) OXIDE_VERSION_STRING=$(OXIDE_VERSION) cargo build --package kernel --target $(KERNEL_TARGET_JSON) $(KERNEL_BUILD_STD) --release
endif
else
ifneq ($(KERNEL_FEATURES),)
	@OXIDE_BUILD_NUMBER=$(OXIDE_BUILD) OXIDE_VERSION_STRING=$(OXIDE_VERSION) cargo build --package kernel --target $(KERNEL_TARGET_JSON) $(KERNEL_BUILD_STD) --features $(KERNEL_FEATURES)
else
	@OXIDE_BUILD_NUMBER=$(OXIDE_BUILD) OXIDE_VERSION_STRING=$(OXIDE_VERSION) cargo build --package kernel --target $(KERNEL_TARGET_JSON) $(KERNEL_BUILD_STD)
endif
endif

# Build bootloader
bootloader:
	@echo "Building bootloader..."
ifeq ($(PROFILE),release)
	@OXIDE_BUILD_NUMBER=$(OXIDE_BUILD) OXIDE_VERSION_STRING=$(OXIDE_VERSION) cargo build --package boot-uefi --target $(ARCH)-unknown-uefi --release -Zbuild-std=core -Zbuild-std-features=compiler-builtins-mem
else
	@OXIDE_BUILD_NUMBER=$(OXIDE_BUILD) OXIDE_VERSION_STRING=$(OXIDE_VERSION) cargo build --package boot-uefi --target $(ARCH)-unknown-uefi -Zbuild-std=core -Zbuild-std-features=compiler-builtins-mem
endif

# Build release
release:
	cargo build --package kernel --target $(KERNEL_TARGET_JSON) $(KERNEL_BUILD_STD) --release
	cargo build --package boot-uefi --target $(ARCH)-unknown-uefi --release
