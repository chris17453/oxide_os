# — NeonRoot: QEMU launch orchestration.
# Auto-detects Fedora vs RHEL, kills stale instances, and prays the OVMF gods are merciful.

.PHONY: detect-qemu-mode kill-qemu run run-disk run-fedora run-rhel run-kvm run-debug-input-gui run-debug-mouse run-debug-lock run-debug-fork run-debug-sched run-debug-all run-256m run-256m-novgpu attach

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

# — NeonRoot: Primary run target. Builds rootfs, detects platform, launches QEMU with VNC.
# No GDB, no training wheels. Just boot the damn thing.
run: kill-qemu create-rootfs
	@MODE=$$($(MAKE) -s detect-qemu-mode); \
	if [ "$$MODE" = "fedora" ]; then \
		echo "Detected Fedora mode"; \
		$(MAKE) run-fedora; \
	elif [ "$$MODE" = "rhel" ]; then \
		echo "Detected RHEL mode"; \
		$(MAKE) run-rhel; \
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
		-m 512M \
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
		-monitor unix:$(TARGET_DIR)/qemu-monitor.sock,server,nowait \
		-s \
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
		-m 512M \
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
		-s \
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

# — GraveShift: 256M diagnostic targets — hunting the blank screen ghost.
# These isolate variables: memory size, VirtIO GPU presence.
# Serial goes to stdout for easy capture: make run-256m 2>&1 | tee target/serial-256m.log

# 256M with all devices (reproduces blank screen on RHEL)
run-256m: kill-qemu create-rootfs
	@mkdir -p $(TARGET_DIR)
	@cp /usr/share/edk2/ovmf/OVMF_VARS.fd $(TARGET_DIR)/OVMF_VARS.fd 2>/dev/null || true
	@mkdir -p /tmp/qemu-oxide
	@echo "=== 256M TEST: VGA std + VirtIO GPU (should reproduce blank screen) ==="
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
		-monitor unix:$(TARGET_DIR)/qemu-monitor.sock,server,nowait \
		-s \
		-no-reboot & \
	QEMU_PID=$$!; \
	trap 'kill $$QEMU_PID 2>/dev/null; exit' INT TERM; \
	echo "QEMU started (PID: $$QEMU_PID)"; \
	sleep 2; \
	if command -v vncviewer >/dev/null 2>&1; then \
		vncviewer -Scaling=2x localhost:5900 2>/dev/null || vncviewer localhost:5900 2>/dev/null; \
	elif flatpak list --app 2>/dev/null | grep -q tigervnc; then \
		flatpak run org.tigervnc.vncviewer -Scaling=2x localhost:5900 2>/dev/null || flatpak run org.tigervnc.vncviewer localhost:5900 2>/dev/null; \
	else \
		echo "VNC viewer not found - connect manually to localhost:5900"; \
		wait $$QEMU_PID; \
	fi; \
	kill $$QEMU_PID 2>/dev/null || true; \
	wait $$QEMU_PID 2>/dev/null || true

# 256M WITHOUT VirtIO GPU — isolates whether SET_SCANOUT steals the display
run-256m-novgpu: kill-qemu create-rootfs
	@mkdir -p $(TARGET_DIR)
	@cp /usr/share/edk2/ovmf/OVMF_VARS.fd $(TARGET_DIR)/OVMF_VARS.fd 2>/dev/null || true
	@mkdir -p /tmp/qemu-oxide
	@echo "=== 256M TEST: VGA std ONLY (no VirtIO GPU) ==="
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
		-device virtio-keyboard-pci \
		-device virtio-tablet-pci \
		-audiodev none,id=snd0 \
		-device intel-hda \
		-device hda-duplex,audiodev=snd0 \
		-vga std \
		-vnc :0 \
		-serial stdio \
		-monitor unix:$(TARGET_DIR)/qemu-monitor.sock,server,nowait \
		-s \
		-no-reboot & \
	QEMU_PID=$$!; \
	trap 'kill $$QEMU_PID 2>/dev/null; exit' INT TERM; \
	echo "QEMU started (PID: $$QEMU_PID)"; \
	sleep 2; \
	if command -v vncviewer >/dev/null 2>&1; then \
		vncviewer -Scaling=2x localhost:5900 2>/dev/null || vncviewer localhost:5900 2>/dev/null; \
	elif flatpak list --app 2>/dev/null | grep -q tigervnc; then \
		flatpak run org.tigervnc.vncviewer -Scaling=2x localhost:5900 2>/dev/null || flatpak run org.tigervnc.vncviewer localhost:5900 2>/dev/null; \
	else \
		echo "VNC viewer not found - connect manually to localhost:5900"; \
		wait $$QEMU_PID; \
	fi; \
	kill $$QEMU_PID 2>/dev/null || true; \
	wait $$QEMU_PID 2>/dev/null || true

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
		-m 512M \
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

# — GraveShift: Attach GDB to a running QEMU and dump all CPU backtraces.
# Run this from a SECOND terminal when the system hangs.
# QEMU must be running with -s (GDB server on port 1234).
KERNEL_BIN := $(TARGET_DIR)/x86_64-unknown-none/debug/kernel
attach:
	@echo "=== Attaching GDB to QEMU (port 1234) ==="
	@echo "Dumping all CPU backtraces..."
	@gdb -batch -q \
		-ex "file $(KERNEL_BIN)" \
		-ex "target remote :1234" \
		-ex "set pagination off" \
		-ex "thread apply all bt" \
		-ex "echo \n=== REGISTERS (current CPU) ===\n" \
		-ex "info registers" \
		-ex "echo \n=== ALL THREADS ===\n" \
		-ex "info threads" \
		-ex "detach" \
		-ex "quit"

