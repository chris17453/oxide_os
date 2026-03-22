# — Hexline: Toolchain and package manager builds.
# Cross-compilers, sysroot wrangling, and the dark art of getting C code to link against our kernel.

.PHONY: toolchain install-toolchain test-toolchain clean-toolchain external-libs pkgmgr-binaries pkgmgr-sysroot-deps pkgmgr-ncurses pkgmgr-readline pkgmgr-vim pkgmgr-python pkgmgr-rebuild-vim pkgmgr-rebuild-python pkgmgr-make pkgmgr-binutils pkgmgr-gcc clean-pkgmgr zlib openssl xz zstd tls-test thread-test

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
	@# — PulseForge: build CRT object files for OXIDE target.
	@# crt0.o = entry point (_start → main → exit)
	@# crti.o/crtn.o = .init/.fini section prologues/epilogues (for constructors/destructors)
	@# crtbegin.o/crtend.o = .ctors/.dtors section markers (GCC collect2 needs these)
	@for crt in crt0 crti crtn crtbegin crtend; do \
		if [ -f "toolchain/crt/$$crt.S" ]; then \
			clang --target=x86_64-oxide-elf -c -o "toolchain/sysroot/lib/$$crt.o" "toolchain/crt/$$crt.S"; \
		fi; \
	done
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
	@# — IronGhost: Build shared .so from every PIC static archive in the sysroot.
	@# Extract .o files, relink as ET_DYN. Same objects, different packaging.
	@# Build libc.so FIRST (no deps), then all others link against it.
	@echo "  Building shared libraries from static archives..."
	@# Step 1: build libc.so first (it has no dependencies)
	@# — IronGhost: compiler-builtins provides WEAK HIDDEN versions of memcpy/memset/
	@# strlen/memmove/memcmp. These shadow our GLOBAL DEFAULT versions from c_exports.rs
	@# and can't be exported from .so (HIDDEN visibility). We remove the conflicting
	@# compiler-builtins .o files so our strong definitions win.
	@if [ -f "toolchain/sysroot/lib/liboxide_libc.a" ]; then \
		echo "    liboxide_libc.a -> libc.so"; \
		TMPDIR=$$(mktemp -d) && cd $$TMPDIR && \
		llvm-ar x "$(CURDIR)/toolchain/sysroot/lib/liboxide_libc.a" && \
		for o in compiler_builtins-*.o; do \
			[ -f "$$o" ] || continue; \
			if llvm-nm "$$o" 2>/dev/null | grep -qE "^[0-9a-f]+ W (memcpy|memset|memmove|strlen|memcmp|bcmp)$$"; then \
				rm "$$o"; \
			fi; \
		done && \
		ld.lld --shared --export-dynamic -o "$(CURDIR)/toolchain/sysroot/lib/libc.so" *.o \
			--no-undefined-version -soname libc.so 2>/dev/null && \
		cd "$(CURDIR)" && rm -rf $$TMPDIR && \
		strip toolchain/sysroot/lib/libc.so 2>/dev/null || true; \
	fi
	@# Step 2: build all other .so files, linking against libc.so
	@# — IronGhost: --whole-archive so all symbols are exported.
	@for archive in toolchain/sysroot/lib/lib*.a; do \
		[ -f "$$archive" ] || continue; \
		BASENAME=$$(basename "$$archive" .a); \
		SONAME=$${BASENAME#lib}; \
		[ "$$SONAME" = "oxide_libc" ] && continue; \
		SOFILE="toolchain/sysroot/lib/lib$${SONAME}.so"; \
		[ -L "$$archive" ] && continue; \
		echo "    $$BASENAME.a -> lib$${SONAME}.so"; \
		ld.lld --shared -o "$(CURDIR)/$$SOFILE" \
			--whole-archive "$(CURDIR)/$$archive" --no-whole-archive \
			--no-undefined-version -soname "lib$${SONAME}.so" \
			-L"$(CURDIR)/toolchain/sysroot/lib" -lc 2>/dev/null && \
		strip "$$SOFILE" 2>/dev/null || true; \
	done
	@echo "  Shared libraries built:"
	@ls -lh toolchain/sysroot/lib/*.so 2>/dev/null | awk '{print "    " $$NF " (" $$5 ")"}'
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

# — CrashBloom: mmap-write-test — unit tests for mmap demand paging
mmap-write-test: toolchain
	@echo "Building mmap write test..."
	@toolchain/bin/oxide-cc -o $(USERSPACE_OUT_RELEASE)/mmap-write-test userspace/tests/mmap-write-test.c
	@echo "mmap write test built: $(USERSPACE_OUT_RELEASE)/mmap-write-test"

# — ThreadRogue: ipc-suite — message queues + semaphores test suite
ipc-suite: toolchain
	@echo "Building IPC test suite..."
	@toolchain/bin/oxide-cc -o $(USERSPACE_OUT_RELEASE)/ipc-suite userspace/tests/ipc-suite.c
	@echo "IPC test suite built: $(USERSPACE_OUT_RELEASE)/ipc-suite"

# — ThreadRogue: shm-test — System V shared memory test
shm-test: toolchain
	@echo "Building shared memory test..."
	@toolchain/bin/oxide-cc -o $(USERSPACE_OUT_RELEASE)/shm-test userspace/tests/shm-test.c
	@echo "Shared memory test built: $(USERSPACE_OUT_RELEASE)/shm-test"

# — ThreadRogue: shm-fork-test — cross-process shared memory
shm-fork-test: toolchain
	@echo "Building cross-process SHM test..."
	@toolchain/bin/oxide-cc -o $(USERSPACE_OUT_RELEASE)/shm-fork-test userspace/tests/shm-fork-test.c
	@echo "Cross-process SHM test built: $(USERSPACE_OUT_RELEASE)/shm-fork-test"

# — CrashBloom: dynlink-suite — comprehensive dynamic linking test suite
dynlink-suite: toolchain
	@echo "Building dynamic linking test suite..."
	@toolchain/bin/oxide-cc -dynamic -o $(USERSPACE_OUT_RELEASE)/dynlink-suite userspace/tests/dynlink-suite.c -lncursesw -lreadline
	@echo "Dynamic linking test suite built: $(USERSPACE_OUT_RELEASE)/dynlink-suite"

# — CrashBloom: dynlink-ncurses-test — multi-library dynamic linking (libc + ncurses)
dynlink-ncurses-test: toolchain
	@echo "Building ncurses dynamic linking test..."
	@toolchain/bin/oxide-cc -dynamic -o $(USERSPACE_OUT_RELEASE)/dynlink-ncurses-test userspace/tests/dynlink-ncurses-test.c -lncursesw
	@echo "Ncurses dynamic test built: $(USERSPACE_OUT_RELEASE)/dynlink-ncurses-test"

# — CrashBloom: dynlink-test — C program dynamically linked against libc.so
dynlink-test: toolchain
	@echo "Building dynamic linking C test..."
	@toolchain/bin/oxide-cc -dynamic -o $(USERSPACE_OUT_RELEASE)/dynlink-test userspace/tests/dynlink-test.c
	@echo "Dynamic C test built: $(USERSPACE_OUT_RELEASE)/dynlink-test"

# — CrashBloom: dynamic linking test — binary with PT_INTERP pointing to ld-oxide.so.1
dyntest:
	@echo "Building dynamic linking test..."
	@RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static -C link-arg=-Tuserspace/userspace-dynamic.ld -C link-arg=-e_start" cargo build --package dyntest --target $(USERSPACE_TARGET) --release $(CARGO_USER_FLAGS)
	@echo "Dynamic test built: $(USERSPACE_OUT_RELEASE)/dyntest"

# — Hexline: Sysroot staleness gate. If libc source changed, rebuild the sysroot.
# This is the firewall between "libc got new syscall numbers" and
# "vim is still calling the old ones." Two days of debugging, one target to prevent it.
SYSROOT_LIBC := toolchain/sysroot/lib/liboxide_libc.a

.PHONY: sysroot-check pkgmgr-check

sysroot-check:
	@if [ -f "$(SYSROOT_LIBC)" ]; then \
		STALE=$$(find userspace/libs/libc/src -name "*.rs" -newer "$(SYSROOT_LIBC)" 2>/dev/null | head -1); \
		if [ -n "$$STALE" ]; then \
			echo "  libc source changed — rebuilding sysroot..."; \
			rm -f "$(SYSROOT_LIBC)"; \
			$(MAKE) toolchain; \
		fi; \
	fi

# — Hexline: Staged binary staleness gate. If sysroot is newer than any staged
# C package, nuke the stale binaries so pkgmgr-vim/pkgmgr-python rebuild them.
# The sentinel that would have saved us two days on the vim incident.
pkgmgr-check: sysroot-check
	@if [ -f "$(SYSROOT_LIBC)" ] && [ -d "$(PKGMGR_STAGING)/bin" ]; then \
		NEED_REBUILD=0; \
		for bin in $(PKGMGR_STAGING)/bin/*; do \
			if [ -f "$$bin" ] && [ "$(SYSROOT_LIBC)" -nt "$$bin" ]; then \
				echo "  $$(basename $$bin) is older than sysroot — will rebuild"; \
				NEED_REBUILD=1; \
			fi; \
		done; \
		if [ "$$NEED_REBUILD" = "1" ]; then \
			echo "  Cleaning stale staged binaries..."; \
			rm -rf $(PKGMGR_STAGING)/bin $(PKGMGR_STAGING)/lib $(PKGMGR_STAGING)/share; \
		fi; \
	fi

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
			llvm-strip $(PKGMGR_STAGING)/bin/vim 2>/dev/null || strip $(PKGMGR_STAGING)/bin/vim 2>/dev/null || true; \
			echo "  vim staged (stripped): $(PKGMGR_STAGING)/bin/vim"; \
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
			cp -r $$VIM_RT/autoload $(PKGMGR_STAGING)/share/vim/vim92/; \
			cp $$VIM_RT/filetype.vim $(PKGMGR_STAGING)/share/vim/vim92/ 2>/dev/null || true; \
			cp $$VIM_RT/defaults.vim $(PKGMGR_STAGING)/share/vim/vim92/ 2>/dev/null || true; \
			echo "  vim runtime staged"; \
		fi; \
	fi

# — IronGhost: Don't stage a diet stdlib; python needs lib-dynload/_pyrepl or it
# throws an exec_prefix tantrum and faceplants in the REPL.
pkgmgr-python: toolchain pkgmgr-sysroot-deps
	@mkdir -p $(PKGMGR_STAGING)/bin $(PKGMGR_STAGING)/lib
	@if [ -f "$(PKGMGR_STAGING)/bin/python" ] && [ -d "$(PKGMGR_STAGING)/lib/python3.13/lib-dynload" ] && [ -d "$(PKGMGR_STAGING)/lib/python3.13/_pyrepl" ] && [ ! pkgmgr/specs/overrides/python3.13.override -nt "$(PKGMGR_STAGING)/bin/python" ]; then \
		echo "  python already staged with platform libs, skipping..."; \
	else \
		PY_INSTALL_ROOT=$$(ls -td pkgmgr/cache/builds/build-*/install/usr 2>/dev/null | head -1); \
		NEED_BUILD=0; \
		if [ -z "$$PY_INSTALL_ROOT" ] || [ ! -f "$$PY_INSTALL_ROOT/bin/python3.13" ] || [ ! -d "$$PY_INSTALL_ROOT/lib/python3.13" ]; then \
			NEED_BUILD=1; \
		fi; \
		if [ "$$NEED_BUILD" = "0" ] && [ -f pkgmgr/specs/overrides/python3.13.override ] && [ pkgmgr/specs/overrides/python3.13.override -nt "$$PY_INSTALL_ROOT/bin/python3.13" ]; then \
			NEED_BUILD=1; \
		fi; \
		if [ "$$NEED_BUILD" = "1" ]; then \
			echo "  Building Python 3.13 via oxdnf..."; \
			python3 pkgmgr/bin/oxdnf buildsrpm python3.13 2>&1 | tail -5; \
			PY_INSTALL_ROOT=$$(ls -td pkgmgr/cache/builds/build-*/install/usr 2>/dev/null | head -1); \
		else \
			echo "  Reusing cached Python build: $$PY_INSTALL_ROOT"; \
		fi; \
		PY_BIN="$$PY_INSTALL_ROOT/bin/python3.13"; \
		if [ -n "$$PY_BIN" ] && [ -f "$$PY_BIN" ]; then \
			cp "$$PY_BIN" $(PKGMGR_STAGING)/bin/python; \
			llvm-strip $(PKGMGR_STAGING)/bin/python 2>/dev/null || strip $(PKGMGR_STAGING)/bin/python 2>/dev/null || true; \
			echo "  python staged (stripped): $(PKGMGR_STAGING)/bin/python"; \
		else \
			echo "  ERROR: python binary not found after build"; \
			exit 1; \
		fi; \
		PY_LIB="$$PY_INSTALL_ROOT/lib/python3.13"; \
		if [ -n "$$PY_LIB" ] && [ -d "$$PY_LIB" ]; then \
			rm -rf $(PKGMGR_STAGING)/lib/python3.13; \
			mkdir -p $(PKGMGR_STAGING)/lib/python3.13; \
			cp -a $$PY_LIB/. $(PKGMGR_STAGING)/lib/python3.13/; \
			find $(PKGMGR_STAGING)/lib/python3.13 -type d -name "__pycache__" -prune -exec rm -rf {} +; \
			if [ ! -d "$(PKGMGR_STAGING)/lib/python3.13/lib-dynload" ] || [ ! -d "$(PKGMGR_STAGING)/lib/python3.13/_pyrepl" ]; then \
				echo "  ERROR: python stdlib staging incomplete (missing lib-dynload or _pyrepl)"; \
				exit 1; \
			fi; \
			echo "  python stdlib staged (full runtime + platform libs)"; \
		else \
			echo "  ERROR: python stdlib not found after build"; \
			exit 1; \
		fi; \
	fi

# — Hexline: Self-hosting toolchain. GCC + binutils + make = compile C on OXIDE.
# This is the endgame. Build these, install them on the rootfs, and OXIDE
# can compile itself. 45 minutes of CPU time for immortality. — Hexline

pkgmgr-make: toolchain
	@mkdir -p $(PKGMGR_STAGING)/bin
	@if [ -f "$(PKGMGR_STAGING)/bin/make" ]; then \
		echo "  make already staged, skipping..."; \
	else \
		echo "  Building GNU make via oxdnf..."; \
		python3 pkgmgr/bin/oxdnf buildsrpm make 2>&1 | tail -5; \
		if [ -f "pkgmgr/build/make/install/usr/bin/make" ]; then \
			cp pkgmgr/build/make/install/usr/bin/make $(PKGMGR_STAGING)/bin/make; \
			echo "  make staged: $(PKGMGR_STAGING)/bin/make"; \
		elif [ -f "pkgmgr/build/make/install/usr/local/bin/make" ]; then \
			cp pkgmgr/build/make/install/usr/local/bin/make $(PKGMGR_STAGING)/bin/make; \
			echo "  make staged: $(PKGMGR_STAGING)/bin/make"; \
		else \
			echo "  ERROR: make binary not found after build"; \
			exit 1; \
		fi; \
	fi

pkgmgr-binutils: toolchain
	@mkdir -p $(PKGMGR_STAGING)/bin
	@if [ -f "$(PKGMGR_STAGING)/bin/as" ]; then \
		echo "  binutils already staged, skipping..."; \
	else \
		echo "  Building binutils via oxdnf..."; \
		python3 pkgmgr/bin/oxdnf buildsrpm binutils 2>&1 | tail -10; \
		for tool in as ld ar nm ranlib objdump objcopy strip readelf size strings; do \
			SRC=$$(find pkgmgr/build/binutils/install -name "$$tool" -o -name "x86_64-oxide-elf-$$tool" 2>/dev/null | head -1); \
			if [ -n "$$SRC" ]; then \
				cp "$$SRC" "$(PKGMGR_STAGING)/bin/$$tool"; \
			fi; \
		done; \
		echo "  binutils staged to $(PKGMGR_STAGING)/bin/"; \
	fi

pkgmgr-gcc: toolchain pkgmgr-binutils
	@mkdir -p $(PKGMGR_STAGING)/bin $(PKGMGR_STAGING)/lib
	@if [ -f "$(PKGMGR_STAGING)/bin/gcc" ]; then \
		echo "  gcc already staged, skipping..."; \
	else \
		echo "  Building GCC via oxdnf (this takes 30-60 minutes)..."; \
		python3 pkgmgr/bin/oxdnf buildsrpm gcc 2>&1 | tail -10; \
		GCC_BIN=$$(find pkgmgr/build/gcc/install -name 'gcc' -type f 2>/dev/null | head -1); \
		CC1_BIN=$$(find pkgmgr/build/gcc/install -name 'cc1' -type f 2>/dev/null | head -1); \
		LIBGCC=$$(find pkgmgr/build/gcc/install -name 'libgcc.a' -type f 2>/dev/null | head -1); \
		if [ -n "$$GCC_BIN" ]; then \
			cp "$$GCC_BIN" "$(PKGMGR_STAGING)/bin/gcc"; \
			ln -sf gcc "$(PKGMGR_STAGING)/bin/cc"; \
			echo "  gcc staged: $(PKGMGR_STAGING)/bin/gcc"; \
		else \
			echo "  ERROR: gcc binary not found after build"; \
			exit 1; \
		fi; \
		if [ -n "$$CC1_BIN" ]; then \
			mkdir -p "$(PKGMGR_STAGING)/libexec/gcc"; \
			cp "$$CC1_BIN" "$(PKGMGR_STAGING)/libexec/gcc/cc1"; \
			echo "  cc1 staged: $(PKGMGR_STAGING)/libexec/gcc/cc1"; \
		fi; \
		if [ -n "$$LIBGCC" ]; then \
			cp "$$LIBGCC" "$(PKGMGR_STAGING)/lib/libgcc.a"; \
			echo "  libgcc.a staged: $(PKGMGR_STAGING)/lib/libgcc.a"; \
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
