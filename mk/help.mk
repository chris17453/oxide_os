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
	@echo "Primary Targets:"
	@echo "  make run              - Build everything + boot QEMU"
	@echo "  make go               - Boot existing image (no rebuild)"
	@echo "  make build            - Build everything (no rootfs/QEMU)"
	@echo "  make test             - Headless boot test"
	@echo "  make test-kernel      - Integration test suite (oxide-test)"
	@echo ""
	@echo "Profiles & Debug:"
	@echo "  make run DEBUG=all              - Full debug output"
	@echo "  make run DEBUG=sched,fork       - Selective debug"
	@echo "  make run PROFILE=release        - Optimized kernel"
	@echo "  make run-release                - Alias for PROFILE=release"
	@echo "  make run-debug-all              - Alias for DEBUG=all"
	@echo "  make run-debug-sched            - Alias for DEBUG=sched"
	@echo "  make run-debug-fork             - Alias for DEBUG=fork"
	@echo "  make run-debug-mouse            - Alias for DEBUG=mouse"
	@echo "  make run-debug-lock             - Alias for DEBUG=lock"
	@echo ""
	@echo "  DEBUG options: all, input, mouse, sched, fork, lock, syscall-perf, tty-read"
	@echo "  Old syntax works too: make run KERNEL_FEATURES=debug-all"
	@echo ""
	@echo "Dependency Chain (automatic):"
	@echo "  libc source changes  -->  sysroot rebuild  -->  vim/python rebuild"
	@echo "  kernel source changes -->  kernel rebuild"
	@echo "  any change           -->  rootfs rebuilt on 'make run'"
	@echo ""
	@echo "Build Targets:"
	@echo "  kernel            - Build kernel only"
	@echo "  bootloader        - Build UEFI bootloader only"
	@echo "  userspace-release - Build all userspace programs (release)"
	@echo "  userspace-pkg     - Build single package (PKG=name)"
	@echo "  initramfs         - Create bootstrap initramfs"
	@echo "  create-rootfs     - Create disk image with ESP + ext4 root + ext4 home"
	@echo ""
	@echo "Toolchain & Packages:"
	@echo "  toolchain         - Build OXIDE cross-compiler toolchain"
	@echo "  sysroot-check     - Rebuild sysroot if libc changed"
	@echo "  pkgmgr-check      - Rebuild staged C packages if sysroot changed"
	@echo "  pkgmgr-rebuild-vim    - Force rebuild vim"
	@echo "  pkgmgr-rebuild-python - Force rebuild python"
	@echo "  pkgmgr-help       - Show package manager help"
	@echo ""
	@echo "Cleanup:"
	@echo "  clean             - Remove all build artifacts"
	@echo "  clean-rootfs      - Remove disk image (forces rebuild on next run)"
	@echo "  clean-pkgmgr      - Remove staged C packages (forces rebuild)"
	@echo ""
	@echo "Other:"
	@echo "  check             - Quick syntax/type check"
	@echo "  fmt               - Format code"
	@echo "  clippy            - Run clippy linter"
	@echo "  attach            - GDB attach to running QEMU"
	@echo "  show-config       - Show detected configuration"
	@echo ""
	@echo "Workflow:"
	@echo "  make run                  Daily development"
	@echo "  make run DEBUG=all        Debug session"
	@echo "  make go                   Quick reboot (no rebuild)"
	@echo "  make kernel && make go    Kernel-only iteration"
	@echo "  make clean-rootfs && make run   Force fresh rootfs"
	@echo ""
	@echo "Disk Layout:"
	@echo "  1. /boot  (FAT32/ESP)  - bootloader, kernel, initramfs"
	@echo "  2. /      (ext4)       - OS binaries and config"
	@echo "  3. /home  (ext4)       - user data"
	@echo "  4. /tmp   (tmpfs)      - in-memory"
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
