# — CrashBloom: Test, lint, and quality gates.
# If it compiles but doesn't pass here, it doesn't ship. Period.

.PHONY: test check fmt fmt-check clippy clean

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
