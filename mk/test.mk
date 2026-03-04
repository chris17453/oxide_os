# — CrashBloom: Test, lint, and quality gates.
# If it compiles but doesn't pass here, it doesn't ship. Period.

.PHONY: test test-kernel check fmt fmt-check clippy clean

# Automated test: boot and check for expected output
test: create-rootfs
	@echo "Running automated boot test..."
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		exit 1; \
	fi
	@mkdir -p /tmp/qemu-oxide
	@TMPDIR=/tmp/qemu-oxide timeout $(QEMU_TIMEOUT) $(QEMU) \
		-machine q35 \
		-cpu qemu64,+smap,+smep \
		-m 256M \
		-bios "$(OVMF)" \
		-drive file=$(ROOTFS_IMAGE),format=raw,if=none,id=disk \
		-device ide-hd,drive=disk \
		-serial file:$(TARGET_DIR)/serial.log \
		-display none \
		-no-reboot \
		2>/dev/null || true
	@echo "--- Serial Output ---"
	@cat $(TARGET_DIR)/serial.log 2>/dev/null || echo "(no output)"
	@echo "--- End Output ---"
	@if grep -q "OXIDE" $(TARGET_DIR)/serial.log 2>/dev/null; then \
		echo ""; \
		echo "TEST PASSED: Boot message found"; \
		exit 0; \
	else \
		echo ""; \
		echo "TEST FAILED: Expected 'OXIDE' in output"; \
		exit 1; \
	fi

# — CrashBloom: Kernel integration test suite.
# Boots QEMU headless, runs oxide-test binary, captures serial output,
# parses results. If the kernel can't survive 45 tests, it doesn't ship.
#
# The test binary writes [OXIDE-TEST] prefixed output to /dev/serial (COM1).
# QEMU captures serial to a file. We parse the file for pass/fail counts.
#
# To run oxide-test manually on a running system: /usr/bin/oxide-test
# The test binary also writes to stdout (terminal) for live monitoring.
KERNEL_TEST_TIMEOUT ?= 120

test-kernel: create-rootfs
	@echo ""
	@echo "=== OXIDE Kernel Integration Tests ==="
	@echo ""
	@if [ -z "$(OVMF)" ]; then \
		echo "Error: OVMF firmware not found"; \
		exit 1; \
	fi
	@echo "Booting QEMU (headless, timeout=$(KERNEL_TEST_TIMEOUT)s)..."
	@echo "  Serial log: $(TARGET_DIR)/test-serial.log"
	@echo ""
	@mkdir -p /tmp/qemu-oxide
	@rm -f $(TARGET_DIR)/test-serial.log $(TARGET_DIR)/test-debug.log
	@TMPDIR=/tmp/qemu-oxide timeout $(KERNEL_TEST_TIMEOUT) $(QEMU) \
		-machine q35 \
		-cpu qemu64,+smap,+smep \
		-smp 4 \
		-m 512M \
		-bios "$(OVMF)" \
		-drive file=$(ROOTFS_IMAGE),format=raw,if=none,id=disk \
		-device virtio-blk-pci,drive=disk \
		-serial file:$(TARGET_DIR)/test-serial.log \
		-display none \
		-no-reboot \
		-d cpu_reset \
		-D $(TARGET_DIR)/test-debug.log \
		2>/dev/null || true
	@echo ""
	@# — CrashBloom: Parse results from serial output
	@if [ ! -f "$(TARGET_DIR)/test-serial.log" ]; then \
		echo "FATAL: No serial output captured"; \
		exit 1; \
	fi
	@if grep -q "TEST SUITE COMPLETE" $(TARGET_DIR)/test-serial.log 2>/dev/null; then \
		echo "--- Test Results ---"; \
		grep "\[OXIDE-TEST\]" $(TARGET_DIR)/test-serial.log; \
		echo ""; \
		echo "--- Summary ---"; \
		PASSED=$$(grep -c "\[PASS\]" $(TARGET_DIR)/test-serial.log 2>/dev/null || echo 0); \
		FAILED=$$(grep -c "\[FAIL\]" $(TARGET_DIR)/test-serial.log 2>/dev/null || echo 0); \
		SKIPPED=$$(grep -c "\[SKIP\]" $(TARGET_DIR)/test-serial.log 2>/dev/null || echo 0); \
		echo "Passed: $$PASSED  Failed: $$FAILED  Skipped: $$SKIPPED"; \
		echo ""; \
		if [ "$$FAILED" = "0" ]; then \
			echo "=== ALL TESTS PASSED ==="; \
			exit 0; \
		else \
			echo "=== TESTS FAILED ==="; \
			grep "\[FAIL\]" $(TARGET_DIR)/test-serial.log; \
			exit 1; \
		fi; \
	else \
		echo "=== TEST CRASHED (no completion marker) ==="; \
		echo ""; \
		echo "Last 40 lines of serial output:"; \
		tail -40 $(TARGET_DIR)/test-serial.log 2>/dev/null || echo "(empty)"; \
		echo ""; \
		RESETS=$$(grep -c "CPU Reset" $(TARGET_DIR)/test-debug.log 2>/dev/null || echo 0); \
		echo "CPU resets detected: $$RESETS"; \
		echo ""; \
		echo "Look for the last [RUN ] line to find which test killed it."; \
		grep "\[RUN \]" $(TARGET_DIR)/test-serial.log 2>/dev/null | tail -1 || echo "(no RUN markers found)"; \
		exit 1; \
	fi

# Quick syntax and type check
check:
	cargo check --all-targets

# Format code
fmt:
	cargo fmt --all

# Format check
fmt-check:
	cargo fmt --all -- --check

# Clippy lint
clippy:
	cargo clippy --all-targets -- -D warnings

# Clean build artifacts
clean:
	cargo clean
	rm -rf $(BOOT_DIR)
