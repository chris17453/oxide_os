# — ThreadRogue: Userspace build orchestration.
# Every binary that runs above ring 0 gets forged here. Mess up the RUSTFLAGS and enjoy your triple fault.

.PHONY: userspace userspace-release userspace-pkg

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
	@for prog in init esh login getty gwbasic curses-demo htop tls-test thread-test ssh sshd rdpd service networkd resolvd journald journalctl soundd evtest argtest doom python $(COREUTILS_BINS); do \
		if [ -f "$(USERSPACE_OUT_RELEASE)/$$prog" ]; then \
			strip "$(USERSPACE_OUT_RELEASE)/$$prog" 2>/dev/null || true; \
		fi; \
	done
	@echo "Userspace programs built (release)."

# Build a single userspace package (usage: make userspace-pkg PKG=coreutils)
userspace-pkg:
	@if [ -z "$(PKG)" ]; then echo "Usage: make userspace-pkg PKG=<package>"; exit 1; fi
	cargo build --package $(PKG) --target $(USERSPACE_TARGET) $(CARGO_USER_FLAGS)

# — IronGhost: Build a single std-enabled userspace package
# Uses -Zbuild-std to compile Rust's std with OXIDE PAL support.
# Requires: scripts/setup-std-source.sh has been run first.
# Usage: make userspace-std-pkg PKG=hello-std
.PHONY: userspace-std-pkg setup-std-source

setup-std-source:
	@if [ ! -d "$(OXIDE_SYSROOT)" ]; then \
		echo "Setting up std source (first time)..."; \
		./scripts/setup-std-source.sh; \
	fi

userspace-std-pkg: setup-std-source
	@if [ -z "$(PKG)" ]; then echo "Usage: make userspace-std-pkg PKG=<package>"; exit 1; fi
	@echo "Building $(PKG) with Rust std..."
	__CARGO_TESTS_ONLY_SRC_ROOT=$(CURDIR)/rust-std/library \
	RUSTC_BOOTSTRAP=1 \
	RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static \
		-C link-arg=-Tuserspace/userspace.ld -C link-arg=-e_start" \
	cargo +nightly build --package $(PKG) \
		--target $(USERSPACE_STD_TARGET_JSON) \
		-Zbuild-std=std,panic_abort \
		-Zbuild-std-features=compiler-builtins-mem \
		$(CARGO_USER_FLAGS)

# Build hello-std (std userspace test program)
.PHONY: userspace-std
userspace-std:
	@echo "Building std userspace binaries..."
	__CARGO_TESTS_ONLY_SRC_ROOT=$(CURDIR)/rust-std/library \
	RUSTC_BOOTSTRAP=1 \
	RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static \
		-C link-arg=-Tuserspace/userspace.ld -C link-arg=-e_start" \
	cargo build --package hello-std \
		--target $(USERSPACE_STD_TARGET_JSON) \
		-Zbuild-std=std,panic_abort \
		-Zbuild-std-features=compiler-builtins-mem \
		$(CARGO_USER_FLAGS)
