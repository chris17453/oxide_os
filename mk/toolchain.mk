# — Hexline: Toolchain and package manager builds.
# Cross-compilers, sysroot wrangling, and the dark art of getting C code to link against our kernel.

.PHONY: toolchain install-toolchain test-toolchain clean-toolchain external-libs pkgmgr-binaries pkgmgr-sysroot-deps pkgmgr-ncurses pkgmgr-readline pkgmgr-vim pkgmgr-python pkgmgr-rebuild-vim pkgmgr-rebuild-python clean-pkgmgr zlib openssl xz zstd tls-test thread-test

# — PulseForge: Stable staging directory for package manager outputs.
# oxdnf builds go here so the rootfs pipeline has a deterministic path.
PKGMGR_STAGING := pkgmgr/staging

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
	@echo "  Hello example built"
	@cd toolchain/examples/echo && $(MAKE) clean && $(MAKE)
	@echo "  Echo example built"
	@cd toolchain/examples/calculator && $(MAKE) clean && $(MAKE)
	@echo "  Calculator example built"
	@echo ""
	@echo "All toolchain tests passed!"

# Clean toolchain
clean-toolchain:
	@rm -rf toolchain/sysroot/lib/*.a
	@cd toolchain/examples/hello && $(MAKE) clean || true
	@cd toolchain/examples/echo && $(MAKE) clean || true
	@cd toolchain/examples/calculator && $(MAKE) clean || true

# External libraries (zlib, openssl, xz, zstd)
external-libs: toolchain zlib openssl xz zstd

zlib: toolchain
	@echo "Building zlib..."
	@./scripts/build-zlib.sh

openssl: toolchain zlib
	@echo "Building OpenSSL..."
	@./scripts/build-openssl.sh

xz: toolchain
	@echo "Building XZ Utils..."
	@./scripts/build-xz.sh

zstd: toolchain
	@echo "Building Zstandard..."
	@./scripts/build-zstd.sh

# TLS test program
tls-test: toolchain
	@echo "Building TLS test program..."
	@toolchain/bin/oxide-cc -o $(USERSPACE_OUT_RELEASE)/tls-test userspace/tests/tls-test.c
	@echo "TLS test built: $(USERSPACE_OUT_RELEASE)/tls-test"

thread-test: toolchain
	@echo "Building thread test program..."
	@toolchain/bin/oxide-cc -o $(USERSPACE_OUT_RELEASE)/thread-test userspace/tests/thread-test.c
	@echo "Thread test built: $(USERSPACE_OUT_RELEASE)/thread-test"

# — Hexline: Package manager builds via oxdnf.
# Fetches Fedora SRPMs, cross-compiles with overrides, stages binaries for rootfs.
# Dependencies (ncurses, readline) are built as sysroot libraries.
# Applications (vim, python) are staged as userspace binaries.
#
# Flow: oxdnf buildsrpm <pkg> → pkgmgr/cache/builds/ → extract to staging/
#
# To add a new package:
#   1. Create pkgmgr/specs/overrides/<pkg>.override
#   2. Add a target below following the pattern
#   3. Add the binary name to the install loop in rootfs.mk line ~126

# — Hexline: Build sysroot deps first, then applications that link against them.
pkgmgr-binaries: toolchain pkgmgr-sysroot-deps pkgmgr-vim pkgmgr-python
	@echo "Package manager binaries staged."

# — Hexline: Sysroot dependencies — ncurses and readline are libraries, not binaries.
# They install to toolchain/sysroot/ so vim/python can link against them.
pkgmgr-sysroot-deps: pkgmgr-ncurses pkgmgr-readline

pkgmgr-ncurses: toolchain
	@if [ -f "toolchain/sysroot/lib/libncursesw.a" ]; then \
		echo "  ncurses already in sysroot, skipping..."; \
	else \
		echo "  Building ncurses via oxdnf..."; \
		python3 pkgmgr/bin/oxdnf buildsrpm ncurses 2>&1 | tail -5; \
	fi

pkgmgr-readline: toolchain pkgmgr-ncurses
	@if [ -f "toolchain/sysroot/lib/libreadline.a" ]; then \
		echo "  readline already in sysroot, skipping..."; \
	else \
		echo "  Building readline via oxdnf..."; \
		python3 pkgmgr/bin/oxdnf buildsrpm readline 2>&1 | tail -5; \
	fi

# — Hexline: Application binaries — built from Fedora SRPMs, staged for rootfs inclusion.
pkgmgr-vim: toolchain pkgmgr-sysroot-deps
	@mkdir -p $(PKGMGR_STAGING)/bin $(PKGMGR_STAGING)/share
	@if [ -f "$(PKGMGR_STAGING)/bin/vim" ]; then \
		echo "  vim already staged, skipping..."; \
	else \
		echo "  Building vim via oxdnf..."; \
		python3 pkgmgr/bin/oxdnf buildsrpm vim 2>&1 | tail -5; \
		VIM_BUILD=$$(ls -td pkgmgr/cache/builds/build-*/build/vim*/src/vim 2>/dev/null | head -1); \
		if [ -n "$$VIM_BUILD" ] && [ -f "$$VIM_BUILD" ]; then \
			cp "$$VIM_BUILD" $(PKGMGR_STAGING)/bin/vim; \
			echo "  vim staged: $(PKGMGR_STAGING)/bin/vim"; \
		else \
			echo "  ERROR: vim binary not found after build"; \
			exit 1; \
		fi; \
		VIM_RT=$$(ls -td pkgmgr/cache/builds/build-*/build/vim*/runtime 2>/dev/null | head -1); \
		if [ -n "$$VIM_RT" ] && [ -d "$$VIM_RT" ]; then \
			mkdir -p $(PKGMGR_STAGING)/share/vim/vim92; \
			cp -r $$VIM_RT/syntax $(PKGMGR_STAGING)/share/vim/vim92/; \
			cp -r $$VIM_RT/colors $(PKGMGR_STAGING)/share/vim/vim92/; \
			cp -r $$VIM_RT/indent $(PKGMGR_STAGING)/share/vim/vim92/; \
			cp -r $$VIM_RT/ftplugin $(PKGMGR_STAGING)/share/vim/vim92/; \
			cp $$VIM_RT/filetype.vim $(PKGMGR_STAGING)/share/vim/vim92/ 2>/dev/null || true; \
			cp $$VIM_RT/defaults.vim $(PKGMGR_STAGING)/share/vim/vim92/ 2>/dev/null || true; \
			echo "  vim runtime staged"; \
		fi; \
	fi

pkgmgr-python: toolchain pkgmgr-sysroot-deps
	@mkdir -p $(PKGMGR_STAGING)/bin $(PKGMGR_STAGING)/lib
	@if [ -f "$(PKGMGR_STAGING)/bin/python" ]; then \
		echo "  python already staged, skipping..."; \
	else \
		echo "  Building Python 3.13 via oxdnf..."; \
		python3 pkgmgr/bin/oxdnf buildsrpm python3.13 2>&1 | tail -5; \
		PY_BIN=$$(ls -td pkgmgr/cache/builds/build-*/install/usr/bin/python3.13 2>/dev/null | head -1); \
		if [ -n "$$PY_BIN" ] && [ -f "$$PY_BIN" ]; then \
			cp "$$PY_BIN" $(PKGMGR_STAGING)/bin/python; \
			echo "  python staged: $(PKGMGR_STAGING)/bin/python"; \
		else \
			echo "  ERROR: python binary not found after build"; \
			exit 1; \
		fi; \
		PY_LIB=$$(ls -td pkgmgr/cache/builds/build-*/install/usr/lib/python3.13 2>/dev/null | head -1); \
		if [ -n "$$PY_LIB" ] && [ -d "$$PY_LIB" ]; then \
			mkdir -p $(PKGMGR_STAGING)/lib/python3.13; \
			cp -r $$PY_LIB/*.py $(PKGMGR_STAGING)/lib/python3.13/ 2>/dev/null || true; \
			for subdir in encodings collections importlib json email http; do \
				if [ -d "$$PY_LIB/$$subdir" ]; then \
					cp -r $$PY_LIB/$$subdir $(PKGMGR_STAGING)/lib/python3.13/; \
				fi; \
			done; \
			echo "  python stdlib staged"; \
		fi; \
	fi

# — Hexline: Force rebuild of a specific package (usage: make pkgmgr-rebuild-vim)
pkgmgr-rebuild-vim:
	@rm -f $(PKGMGR_STAGING)/bin/vim
	@rm -rf $(PKGMGR_STAGING)/share/vim
	@$(MAKE) pkgmgr-vim

pkgmgr-rebuild-python:
	@rm -f $(PKGMGR_STAGING)/bin/python
	@rm -rf $(PKGMGR_STAGING)/lib/python3.13
	@$(MAKE) pkgmgr-python

# Clean package manager staging
clean-pkgmgr:
	@echo "Cleaning package manager staging..."
	@rm -rf $(PKGMGR_STAGING)
	@echo "  (build cache in pkgmgr/cache/builds/ preserved — run 'rm -rf pkgmgr/cache/builds' to nuke)"
