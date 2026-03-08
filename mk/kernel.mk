# — GraveShift: Kernel and bootloader build targets.
# The foundation everything else sits on. Break this and nothing boots.

.PHONY: kernel bootloader release

# — SableWire: Custom target requires building core/alloc from source.
# Target JSON is parameterized by $(ARCH) — add a new arch, add a new JSON. Done.
KERNEL_TARGET_JSON := targets/$(ARCH)-unknown-oxide.json
KERNEL_BUILD_STD := -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem

# — SableWire: arch feature is always injected — Makefile is the single source of truth
# for which architecture we're building. KERNEL_FEATURES adds debug flags on top.
ARCH_FEATURE := arch-$(ARCH)
ifneq ($(KERNEL_FEATURES),)
ALL_KERNEL_FEATURES := $(ARCH_FEATURE),$(KERNEL_FEATURES)
else
ALL_KERNEL_FEATURES := $(ARCH_FEATURE)
endif

# Build kernel
# Pass KERNEL_FEATURES to enable debug output, e.g.: make run DEBUG=all
# — PatchBay: OXIDE_BUILD_NUMBER env var injects the NT-style build counter at compile time
# — SableWire: Shell conditional, not ifeq. ifeq is evaluated at parse time and ignores
# target-specific variables (run-release: PROFILE = release). Shell runs at recipe time.
kernel:
	@echo "Building kernel ($(ARCH), $(PROFILE))..."
	@RELEASE_FLAG=""; \
	if [ "$(PROFILE)" = "release" ]; then RELEASE_FLAG="--release"; fi; \
	OXIDE_BUILD_NUMBER=$$(cat build/build-number 2>/dev/null || echo 0) \
	OXIDE_VERSION_STRING=$(OXIDE_VERSION) \
	cargo build --package kernel --target $(KERNEL_TARGET_JSON) $(KERNEL_BUILD_STD) \
		$$RELEASE_FLAG --features $(ALL_KERNEL_FEATURES)

# Build bootloader
# — SableWire: Same shell-conditional fix as kernel. See above.
bootloader:
	@echo "Building bootloader ($(PROFILE))..."
	@RELEASE_FLAG=""; \
	if [ "$(PROFILE)" = "release" ]; then RELEASE_FLAG="--release"; fi; \
	OXIDE_BUILD_NUMBER=$$(cat build/build-number 2>/dev/null || echo 0) \
	OXIDE_VERSION_STRING=$(OXIDE_VERSION) \
	cargo build --package boot-uefi --target $(ARCH)-unknown-uefi \
		$$RELEASE_FLAG -Zbuild-std=core -Zbuild-std-features=compiler-builtins-mem

# Build release
release:
	cargo build --package kernel --target $(KERNEL_TARGET_JSON) $(KERNEL_BUILD_STD) --release --features $(ARCH_FEATURE)
	cargo build --package boot-uefi --target $(ARCH)-unknown-uefi --release
