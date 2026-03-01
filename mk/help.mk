# — NightDoc: Help, config display, and miscellaneous targets.
# The parts of the build system that tell you what the other parts do.

.PHONY: help show-config claude

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
	@echo "Run Targets:"
	@echo "  go                - Boot existing disk image (no rebuild)"
	@echo "  run               - Build and run (auto-detects Fedora/RHEL)"
	@echo "  run-fedora        - Build and run with qemu-system-x86_64"
	@echo "  run-rhel          - Build and run with qemu-kvm (RHEL 10)"
	@echo "  run-kvm           - Alias for run-rhel"
	@echo "  kill-qemu         - Kill stale QEMU processes (auto-called by run)"
	@echo "  clean-rootfs      - Remove generated disk images/initramfs so run rebuilds fresh"
	@echo ""
	@echo "Debug Targets:"
	@echo "  run-debug-mouse   - Run with mouse/input debug output"
	@echo "  run-debug-lock    - Run with lock contention warnings"
	@echo "  run-debug-fork    - Run with fork/exec debug output"
	@echo "  run-debug-sched   - Run with scheduler debug output"
	@echo "  run-debug-all     - Run with ALL debug output"
	@echo "  Or: make run KERNEL_FEATURES=debug-fork,debug-mouse"
	@echo ""
	@echo "Build Targets:"
	@echo "  all               - Build kernel and bootloader (default)"
	@echo "  build-full        - Build kernel, bootloader, userspace, and initramfs"
	@echo "  kernel            - Build kernel only"
	@echo "  bootloader        - Build UEFI bootloader only"
	@echo "  userspace         - Build all userspace programs (debug)"
	@echo "  userspace-release - Build all userspace programs (release)"
	@echo "  userspace-pkg     - Build single package (PKG=name)"
	@echo "  initramfs         - Create bootstrap initramfs (init + esh + coreutils only)"
	@echo "  boot-image        - Create bootable disk image"
	@echo "  create-rootfs     - Create 512MB disk with ESP + ext4 root + ext4 home"
	@echo "  release           - Build kernel/bootloader in release mode"
	@echo ""
	@echo "Toolchain:"
	@echo "  toolchain         - Build OXIDE cross-compiler toolchain"
	@echo "  test-toolchain    - Test toolchain with examples"
	@echo "  install-toolchain - Install toolchain (PREFIX=/usr/local/oxide)"
	@echo ""
	@echo "Package Manager:"
	@echo "  pkgmgr-sync       - Sync Fedora repository metadata"
	@echo "  pkgmgr-search     - Search for packages (PKG=name)"
	@echo "  pkgmgr-build      - Build package from SRPM (PKG=name)"
	@echo "  pkgmgr-install    - Install built package (PKG=name)"
	@echo "  pkgmgr-help       - Show package manager help"
	@echo ""
	@echo "Other Targets:"
	@echo "  test              - Automated boot test"
	@echo "  check             - Quick syntax/type check"
	@echo "  fmt               - Format code"
	@echo "  clippy            - Run clippy linter"
	@echo "  clean             - Remove build artifacts"
	@echo "  clean-rootfs      - Remove generated rootfs artifacts"
	@echo "  show-config       - Show detected configuration"
	@echo ""
	@echo "Examples:"
	@echo "  make run           - Build and run (auto-detects Fedora/RHEL)"
	@echo "  make test          - Run automated boot test"
	@echo ""
	@echo "All run targets create a disk image with:"
	@echo "  - /boot  (FAT32/ESP): bootloader, kernel, initramfs"
	@echo "  - /      (ext4):      OS binaries and config"
	@echo "  - /home  (ext4):      user data"
	@echo "  - /tmp   (tmpfs):     in-memory temp files"
	@echo ""
	@echo "Requirements:"
	@echo "  Fedora: sudo dnf install qemu-system-x86 edk2-ovmf parted"
	@echo "  RHEL:   sudo dnf install qemu-kvm edk2-ovmf parted tigervnc"

# — WireSaint: Sandboxed Claude Code launch.
claude:
	bwrap \
	--ro-bind /usr /usr \
	--ro-bind /etc /etc \
	--ro-bind /lib /lib \
	--ro-bind /lib64 /lib64 \
	--ro-bind /run /run \
	--bind $(HOME) $(HOME) \
	--bind $(HOME)/.claude.json $(HOME)/.claude.json \
	--bind $(HOME)/.claude $(HOME)/.claude \
	--bind "$(CURDIR)" "$(CURDIR)" \
	--bind /tmp /tmp \
	--dev /dev \
	--proc /proc \
	--chdir "$(CURDIR)" \
	--die-with-parent \
	-- /usr/bin/node /usr/local/lib/node_modules/@anthropic-ai/claude-code/cli.js --dangerously-skip-permissions
