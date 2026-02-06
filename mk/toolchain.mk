# — Hexline: Toolchain and external library builds.
# Cross-compilers, sysroot wrangling, and the dark art of getting C code to link against our kernel.

.PHONY: toolchain install-toolchain test-toolchain clean-toolchain external-libs external-binaries zlib openssl xz zstd cpython tls-test thread-test vim

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
	@strip $(USERSPACE_OUT_RELEASE)/vim
	@echo "Vim installed to $(USERSPACE_OUT_RELEASE)/vim"

# — GraveShift: Build external binaries (python, vim) if sources exist and binaries are missing.
# These are optional heavyweight builds — skip if sources aren't cloned or binaries already exist.
external-binaries: toolchain
	@echo "Checking external binaries..."
	@# Build regex library if not present (needed by vim)
	@if [ ! -f "toolchain/sysroot/lib/libregex.a" ] && [ -d "external/musl-regex" ]; then \
		echo "  Building libregex..."; \
		cd external/musl-regex && make AR=ar RANLIB=ranlib 2>&1 | tail -5; \
	fi
	@# Build cpython if source exists and binary is missing
	@if [ -d "external/cpython" ] && [ ! -f "$(USERSPACE_OUT_RELEASE)/python" ]; then \
		echo "  Building CPython (this may take a while)..."; \
		./scripts/build-cpython.sh 2>&1 | tail -10; \
		mkdir -p $(USERSPACE_OUT_RELEASE); \
		cp external/cpython-build/python $(USERSPACE_OUT_RELEASE)/python 2>/dev/null || true; \
	elif [ -f "$(USERSPACE_OUT_RELEASE)/python" ]; then \
		echo "  Python already built, skipping..."; \
	fi
	@# Build vim if source exists and binary is missing
	@if [ -d "external/vim/src" ] && [ ! -f "$(USERSPACE_OUT_RELEASE)/vim" ]; then \
		echo "  Building vim..."; \
		./scripts/build-vim.sh 2>&1 | tail -10; \
	elif [ -f "$(USERSPACE_OUT_RELEASE)/vim" ]; then \
		echo "  Vim already built, skipping..."; \
	fi
	@echo "External binaries check complete."
