# — NeonRoot: QEMU launch orchestration.
# Auto-detects Fedora vs RHEL, kills stale instances, and prays the OVMF gods are merciful.

.PHONY: detect-qemu-mode kill-qemu run run-fedora-debug run-rhel-debug run-disk run-fedora run-rhel run-kvm run-debug-input-gui run-debug-mouse run-debug-lock run-debug-fork run-debug-sched run-debug-all debug-server debug-capture debug-boot-check debug-exec debug-repl

# Auto-detect QEMU mode (Fedora vs RHEL)
detect-qemu-mode:
	@if command -v qemu-system-x86_64 >/dev/null 2>&1; then \
		echo "fedora"; \
	elif [ -f /usr/libexec/qemu-kvm ]; then \
		echo "rhel"; \
	else \
		echo "unknown"; \
	fi

# Kill any stale QEMU processes from previous runs
# — NeonRoot: QEMU sometimes outlives the VNC viewer or make process.
# Without this, the next `make run` fails on port/resource conflicts.
kill-qemu:
	@STALE=$$(pgrep -x 'qemu-system-x86|qemu-system-x86_64|qemu-kvm' 2>/dev/null); \
	if [ -n "$$STALE" ]; then \
		echo "Killing stale QEMU processes: $$STALE"; \
		kill $$STALE 2>/dev/null || true; \
		sleep 1; \
		STILL=$$(pgrep -x 'qemu-system-x86|qemu-system-x86_64|qemu-kvm' 2>/dev/null); \
		if [ -n "$$STILL" ]; then \
			echo "Force-killing stubborn QEMU processes: $$STILL"; \
			kill -9 $$STILL 2>/dev/null || true; \
		fi; \
	fi

# Debug-first run (ext4 rootfs with init/getty/login, GDB-ready QEMU)
run: kill-qemu create-rootfs
	@MODE=$$($(MAKE) -s detect-qemu-mode); \
	if [ "$$MODE" = "fedora" ]; then \
		echo "Detected Fedora mode (debug QEMU run)"; \
		$(MAKE) run-fedora-debug; \
	elif [ "$$MODE" = "rhel" ]; then \
		echo "Detected RHEL mode (debug QEMU run)"; \
		$(MAKE) run-rhel-debug; \
	else \
		echo "Error: No compatible QEMU found"; \
		echo "Install: sudo dnf install qemu-system-x86 (Fedora) or qemu-kvm (RHEL)"; \
		exit 1; \
	fi

# Legacy disk-image workflow (requires sudo for losetup/mkfs)
run-disk: kill-qemu clean-rootfs
	@MODE=$$($(MAKE) -s detect-qemu-mode); \
	KERNEL_FEATURES="$(RUN_KERNEL_FEATURES)" $(MAKE) create-rootfs; \
	if [ "$$MODE" = "fedora" ]; then \
		echo "Detected Fedora mode (qemu-system-x86_64)"; \
		KERNEL_FEATURES="$(RUN_KERNEL_FEATURES)" $(MAKE) run-fedora; \
	elif [ "$$MODE" = "rhel" ]; then \
		echo "Detected RHEL mode (qemu-kvm)"; \
		KERNEL_FEATURES="$(RUN_KERNEL_FEATURES)" $(MAKE) run-rhel; \
	else \
		echo "Error: No compatible QEMU found"; \
		echo "Install: sudo dnf install qemu-system-x86 (Fedora) or qemu-kvm (RHEL)"; \
		exit 1; \
	fi

QEMU_DEBUG_LOG ?= $(TARGET_DIR)/qemu.log
DEBUG_SERIAL ?= file:$(TARGET_DIR)/serial.log
GDB_PAUSE ?= 1
DEBUG_QEMU_ARGS = \
	-machine q35 \
	-cpu qemu64,+smap,+smep \
	-smp 1 \
	-m 512M \
	-drive file=$(ROOTFS_IMAGE),format=raw,if=none,id=disk \
	-device virtio-blk-pci,drive=disk \
	-device isa-debugcon,iobase=0xe9,chardev=dbg \
	-chardev stdio,id=dbg,signal=off \
	-serial $(DEBUG_SERIAL) \
	-device virtio-gpu-pci \
	-device virtio-keyboard-pci \
	-device virtio-tablet-pci \
	-no-reboot -no-shutdown \
	-d int,cpu_reset,guest_errors \
	-D $(QEMU_DEBUG_LOG) \
	$(if $(filter 1,$(GDB_PAUSE)),-s -S,)

run-fedora-debug:
	@set -e; \
	echo "Running OXIDE OS with qemu-system-x86_64 (debug mode)..."; \
	if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; exit 1; \
	fi; \
	mkdir -p /tmp/qemu-oxide $(TARGET_DIR); \
	echo "Serial log: $(TARGET_DIR)/serial.log"; \
	echo "QEMU log: $(QEMU_DEBUG_LOG)"; \
	TMPDIR=/tmp/qemu-oxide qemu-system-x86_64 \
		-bios "$(OVMF)" \
		$(DEBUG_QEMU_ARGS) \
		& \
	QEMU_PID=$$!; \
	trap 'kill $$QEMU_PID 2>/dev/null || true' INT TERM; \
	echo "QEMU started (PID $$QEMU_PID)."; \
	sleep 1; \
	if [ "$(GDB_AUTO)" = "1" ]; then \
		echo "Launching $(GDB) with $(GDB_CMDS)"; \
		$(GDB) -q $(KERNEL_TARGET) $(GDB_CMDS); \
	else \
		echo "Attach manually with: $(GDB) -q $(KERNEL_TARGET) $(GDB_CMDS)"; \
		wait $$QEMU_PID; \
	fi; \
	kill $$QEMU_PID 2>/dev/null || true; \
	wait $$QEMU_PID 2>/dev/null || true


run-rhel-debug:
	@set -e; \
	echo "Running OXIDE OS with qemu-kvm (debug mode)..."; \
	if [ ! -f /usr/share/edk2/ovmf/OVMF_CODE.fd ]; then \
		echo "Error: OVMF firmware not found"; \
		echo "Install: sudo dnf install edk2-ovmf"; \
		exit 1; \
	fi; \
	if [ ! -f /usr/libexec/qemu-kvm ]; then \
		echo "Error: /usr/libexec/qemu-kvm not found"; \
		echo "Install: sudo dnf install qemu-kvm"; \
		exit 1; \
	fi; \
	mkdir -p $(TARGET_DIR) /tmp/qemu-oxide; \
	echo "Serial log: $(TARGET_DIR)/serial.log"; \
	echo "QEMU log: $(QEMU_DEBUG_LOG)"; \
	TMPDIR=/tmp/qemu-oxide /usr/libexec/qemu-kvm \
		-drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/ovmf/OVMF_CODE.fd \
		$(DEBUG_QEMU_ARGS) \
		& \
	QEMU_PID=$$!; \
	trap 'kill $$QEMU_PID 2>/dev/null || true' INT TERM; \
	echo "QEMU started (PID $$QEMU_PID)."; \
	sleep 1; \
	if [ "$(GDB_AUTO)" = "1" ]; then \
		echo "Launching $(GDB) with $(GDB_CMDS)"; \
		$(GDB) -q $(KERNEL_TARGET) $(GDB_CMDS); \
	else \
		echo "Attach manually with: $(GDB) -q $(KERNEL_TARGET) $(GDB_CMDS)"; \
		wait $$QEMU_PID; \
	fi; \
	kill $$QEMU_PID 2>/dev/null || true; \
	wait $$QEMU_PID 2>/dev/null || true


# Internal target: Run with qemu-system-x86_64 (Fedora)
run-fedora:
	@echo "Running OXIDE OS with qemu-system-x86_64..."
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		echo "Install: sudo dnf install edk2-ovmf"; \
		exit 1; \
	fi
	@echo "Forwarding SSH to localhost:$(SSH_HOST_PORT)"
	@mkdir -p /tmp/qemu-oxide
	TMPDIR=/tmp/qemu-oxide qemu-system-x86_64 \
		-machine q35 \
		-cpu qemu64,+smap,+smep \
		-smp 4 \
		-m 256M \
		-bios "$(OVMF)" \
		-drive file=$(ROOTFS_IMAGE),format=raw,if=none,id=disk \
		-device virtio-blk-pci,drive=disk \
		-device virtio-net-pci,netdev=net0 \
		-netdev user,id=net0,hostfwd=tcp::$(SSH_HOST_PORT)-:22 \
		-device virtio-gpu-pci \
		-device virtio-keyboard-pci \
		-device virtio-tablet-pci \
		-audiodev none,id=snd0 \
		-device intel-hda \
		-device hda-duplex,audiodev=snd0 \
		-serial stdio \
		-no-reboot

# Internal target: Run with qemu-kvm (RHEL)
run-rhel:
	@echo "Running OXIDE OS with qemu-kvm..."
	@if [ ! -f /usr/share/edk2/ovmf/OVMF_CODE.fd ]; then \
		echo "Error: OVMF firmware not found"; \
		echo "Install: sudo dnf install edk2-ovmf"; \
		exit 1; \
	fi
	@if [ ! -f /usr/libexec/qemu-kvm ]; then \
		echo "Error: /usr/libexec/qemu-kvm not found"; \
		echo "Install: sudo dnf install qemu-kvm"; \
		exit 1; \
	fi
	@mkdir -p $(TARGET_DIR)
	@cp /usr/share/edk2/ovmf/OVMF_VARS.fd $(TARGET_DIR)/OVMF_VARS.fd 2>/dev/null || true
	@mkdir -p /tmp/qemu-oxide
	@echo "Starting QEMU (VNC on :5900, serial on stdio)..."
	@echo "Forwarding SSH to localhost:$(SSH_HOST_PORT)"
	@TMPDIR=/tmp/qemu-oxide /usr/libexec/qemu-kvm \
		-machine q35,accel=kvm:tcg \
		-cpu max,+invtsc \
		-smp 4 \
		-m 256M \
		-drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/ovmf/OVMF_CODE.fd \
		-drive if=pflash,format=raw,file=$(TARGET_DIR)/OVMF_VARS.fd \
		-drive file=$(ROOTFS_IMAGE),format=raw,if=none,id=bootdisk \
		-device virtio-blk-pci,drive=bootdisk \
		-device virtio-net-pci,netdev=net0 \
		-netdev user,id=net0,hostfwd=tcp::$(SSH_HOST_PORT)-:22 \
		-device virtio-gpu-pci \
		-device virtio-keyboard-pci \
		-device virtio-tablet-pci \
		-audiodev none,id=snd0 \
		-device intel-hda \
		-device hda-duplex,audiodev=snd0 \
		-vga std \
		-vnc :0 \
		-serial stdio \
		-no-reboot & \
	QEMU_PID=$$!; \
	trap 'kill $$QEMU_PID 2>/dev/null; exit' INT TERM; \
	echo "QEMU started (PID: $$QEMU_PID)"; \
	sleep 2; \
	# Launch VNC viewer with 2x scaling (640x480 -> 1280x960 for easier viewing)
	if command -v vncviewer >/dev/null 2>&1; then \
		echo "Launching VNC viewer (scaled 2x)..."; \
		vncviewer -Scaling=2x localhost:5900 2>/dev/null || vncviewer -scale 2 localhost:5900 2>/dev/null || vncviewer localhost:5900 2>/dev/null; \
	elif flatpak list --app 2>/dev/null | grep -q tigervnc; then \
		echo "Launching VNC viewer (Flatpak, scaled 2x)..."; \
		flatpak run org.tigervnc.vncviewer -Scaling=2x localhost:5900 2>/dev/null || flatpak run org.tigervnc.vncviewer localhost:5900 2>/dev/null; \
	else \
		echo "VNC viewer not found - connect manually to localhost:5900"; \
		echo "Install: sudo dnf install tigervnc"; \
		wait $$QEMU_PID; \
	fi; \
	echo "Stopping QEMU..."; \
	kill $$QEMU_PID 2>/dev/null || true; \
	wait $$QEMU_PID 2>/dev/null || true

# Alias for backward compatibility
run-kvm: run-rhel

# Debug run with graphical display for PS/2 keyboard testing
run-debug-input-gui:
	@echo "Running OXIDE OS with graphical display for PS/2 keyboard testing..."
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		exit 1; \
	fi
	@KERNEL_FEATURES="debug-input" $(MAKE) create-rootfs
	@mkdir -p /tmp/qemu-oxide
	@echo "Serial output: /tmp/oxide-serial.log"
	@echo "Use the QEMU graphical window to type and test keyboard events"
	TMPDIR=/tmp/qemu-oxide qemu-system-x86_64 \
		-machine q35 \
		-cpu qemu64,+smap,+smep \
		-m 256M \
		-bios "$(OVMF)" \
		-drive file=$(ROOTFS_IMAGE),format=raw,if=none,id=disk \
		-device virtio-blk-pci,drive=disk \
		-device virtio-net-pci,netdev=net0 \
		-netdev user,id=net0,hostfwd=tcp::$(SSH_HOST_PORT)-:22 \
		-device virtio-keyboard-pci \
		-device virtio-tablet-pci \
		-serial file:/tmp/oxide-serial.log \
		-display gtk,zoom-to-fit=on,grab-on-hover=on \
		-no-reboot & \
	echo "QEMU started. Use: tail -f /tmp/oxide-serial.log to see debug output"

# Debug convenience targets — shorthand for KERNEL_FEATURES=...
run-debug-mouse: KERNEL_FEATURES = debug-mouse
run-debug-mouse: run

run-debug-lock: KERNEL_FEATURES = debug-lock
run-debug-lock: run

run-debug-fork: KERNEL_FEATURES = debug-fork
run-debug-fork: run

run-debug-sched: KERNEL_FEATURES = debug-sched
run-debug-sched: run

run-debug-all: KERNEL_FEATURES = debug-all
run-debug-all: run

# ========================================
# Autonomous GDB Debugging Targets
# — ColdCipher: Because debugging at 3 AM requires programmatic control, not point-and-click.
# ========================================

# Start QEMU with GDB server (no auto-launch GDB)
# Use this when you want to connect GDB manually or programmatically
debug-server: kill-qemu boot-image
	@echo "Starting QEMU with GDB server on port 1234 (paused at start)..."
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; exit 1; \
	fi; \
	mkdir -p /tmp/qemu-oxide $(TARGET_DIR); \
	echo "Serial log: $(TARGET_DIR)/serial.log"; \
	echo "QEMU log: $(QEMU_DEBUG_LOG)"; \
	echo "Connect with: gdb $(KERNEL_TARGET) -ex 'target remote :1234'"; \
	echo "Or use: ./scripts/gdb-autonomous.py --repl"; \
	TMPDIR=/tmp/qemu-oxide qemu-system-x86_64 \
		-bios "$(OVMF)" \
		$(DEBUG_QEMU_ARGS)

# Autonomous crash capture - runs until crash, then dumps state
debug-capture: kill-qemu boot-image
	@echo "Running autonomous crash capture..."
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; exit 1; \
	fi; \
	mkdir -p /tmp/qemu-oxide $(TARGET_DIR); \
	echo "Starting QEMU with GDB server..."; \
	TMPDIR=/tmp/qemu-oxide qemu-system-x86_64 \
		-bios "$(OVMF)" \
		$(DEBUG_QEMU_ARGS) \
		& \
	QEMU_PID=$$!; \
	trap 'kill $$QEMU_PID 2>/dev/null || true' INT TERM EXIT; \
	sleep 1; \
	echo "Attaching GDB with crash capture script..."; \
	$(GDB) -q -batch \
		-x scripts/gdb-capture-crash.gdb \
		$(KERNEL_TARGET) \
		> $(TARGET_DIR)/crash-capture.log 2>&1; \
	echo "Crash capture log saved to $(TARGET_DIR)/crash-capture.log"; \
	cat $(TARGET_DIR)/crash-capture.log; \
	kill $$QEMU_PID 2>/dev/null || true; \
	wait $$QEMU_PID 2>/dev/null || true

# Quick boot sanity check
debug-boot-check: kill-qemu boot-image
	@echo "Running boot sanity check..."
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; exit 1; \
	fi; \
	mkdir -p /tmp/qemu-oxide $(TARGET_DIR); \
	TMPDIR=/tmp/qemu-oxide qemu-system-x86_64 \
		-bios "$(OVMF)" \
		$(DEBUG_QEMU_ARGS) \
		& \
	QEMU_PID=$$!; \
	trap 'kill $$QEMU_PID 2>/dev/null || true' INT TERM EXIT; \
	sleep 1; \
	timeout 30 $(GDB) -q -batch \
		-x scripts/gdb-check-boot.gdb \
		$(KERNEL_TARGET) \
		> $(TARGET_DIR)/boot-check.log 2>&1 || true; \
	echo "Boot check results:"; \
	cat $(TARGET_DIR)/boot-check.log; \
	kill $$QEMU_PID 2>/dev/null || true; \
	wait $$QEMU_PID 2>/dev/null || true

# Execute specific GDB commands autonomously
# Usage: make debug-exec CMD="bt"
# Usage: make debug-exec CMD="info registers"
debug-exec: kill-qemu boot-image
	@if [ -z "$(CMD)" ]; then \
		echo "Error: Must specify CMD=<gdb-command>"; \
		echo "Example: make debug-exec CMD='bt'"; \
		exit 1; \
	fi; \
	echo "Executing GDB command: $(CMD)"; \
	if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; exit 1; \
	fi; \
	mkdir -p /tmp/qemu-oxide $(TARGET_DIR); \
	TMPDIR=/tmp/qemu-oxide qemu-system-x86_64 \
		-bios "$(OVMF)" \
		$(DEBUG_QEMU_ARGS) \
		& \
	QEMU_PID=$$!; \
	trap 'kill $$QEMU_PID 2>/dev/null || true' INT TERM EXIT; \
	sleep 1; \
	./scripts/gdb-autonomous.py --exec "$(CMD)"; \
	kill $$QEMU_PID 2>/dev/null || true; \
	wait $$QEMU_PID 2>/dev/null || true

# Interactive autonomous REPL (programmatic control)
debug-repl: kill-qemu boot-image
	@echo "Starting autonomous GDB REPL..."
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; exit 1; \
	fi; \
	mkdir -p /tmp/qemu-oxide $(TARGET_DIR); \
	echo "Launching QEMU with GDB server..."; \
	TMPDIR=/tmp/qemu-oxide qemu-system-x86_64 \
		-bios "$(OVMF)" \
		$(DEBUG_QEMU_ARGS) \
		& \
	QEMU_PID=$$!; \
	trap 'kill $$QEMU_PID 2>/dev/null || true' INT TERM EXIT; \
	sleep 2; \
	echo "Connecting autonomous GDB controller..."; \
	./scripts/gdb-autonomous.py --repl; \
	kill $$QEMU_PID 2>/dev/null || true; \
	wait $$QEMU_PID 2>/dev/null || true
