#!/usr/bin/bash
#
# Quick smoke test for autonomous debugging infrastructure
# — ColdCipher: Verifying our debug tooling works before the kernel breaks at 3 AM.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "=== Autonomous Debugging Infrastructure Smoke Test ==="
echo

# Check prerequisites
echo "[1/6] Checking prerequisites..."

if ! command -v gdb >/dev/null 2>&1; then
    echo "  ✗ GDB not found"
    exit 1
fi
echo "  ✓ GDB found: $(gdb --version | head -1)"

if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "  ✗ QEMU not found"
    exit 1
fi
echo "  ✓ QEMU found"

if ! command -v python3 >/dev/null 2>&1; then
    echo "  ✗ Python3 not found"
    exit 1
fi
echo "  ✓ Python3 found"

# Check scripts exist
echo
echo "[2/6] Checking scripts..."

SCRIPTS=(
    "scripts/gdb-autonomous.py"
    "scripts/debug-kernel.sh"
    "scripts/gdb-init-kernel.gdb"
    "scripts/gdb-capture-crash.gdb"
    "scripts/gdb-check-boot.gdb"
)

for script in "${SCRIPTS[@]}"; do
    if [ ! -f "$script" ]; then
        echo "  ✗ Missing: $script"
        exit 1
    fi
    echo "  ✓ Found: $script"
done

# Check Python script syntax
echo
echo "[3/6] Validating Python script..."
if python3 -m py_compile scripts/gdb-autonomous.py 2>/dev/null; then
    echo "  ✓ Python script syntax valid"
else
    echo "  ✗ Python script has syntax errors"
    exit 1
fi

# Check help works
echo
echo "[4/6] Testing help output..."
if ./scripts/gdb-autonomous.py --help >/dev/null 2>&1; then
    echo "  ✓ Python controller help works"
else
    echo "  ✗ Python controller help failed"
    exit 1
fi

if ./scripts/debug-kernel.sh help >/dev/null 2>&1; then
    echo "  ✓ Shell wrapper help works"
else
    echo "  ✗ Shell wrapper help failed"
    exit 1
fi

# Check Makefile targets exist
echo
echo "[5/6] Checking Makefile targets..."

TARGETS=(
    "debug-server"
    "debug-capture"
    "debug-boot-check"
    "debug-exec"
    "debug-repl"
)

for target in "${TARGETS[@]}"; do
    if make -n "$target" >/dev/null 2>&1; then
        echo "  ✓ Target exists: $target"
    else
        echo "  ✗ Target missing: $target"
        exit 1
    fi
done

# Check kernel binary exists
echo
echo "[6/6] Checking kernel binary..."

if [ -f "target/x86_64-unknown-none/debug/kernel" ]; then
    echo "  ✓ Kernel binary found"
else
    echo "  ⚠ Kernel binary not found (run 'make build' first)"
    echo "    This is OK for testing the scripts, but you'll need it to actually debug"
fi

echo
echo "=== All Checks Passed ==="
echo
echo "Autonomous debugging infrastructure is ready!"
echo
echo "Quick start:"
echo "  1. Build kernel:  make build"
echo "  2. Run test:      ./scripts/debug-kernel.sh boot"
echo "  3. Capture crash: ./scripts/debug-kernel.sh capture"
echo
echo "See docs/AUTONOMOUS-DEBUGGING.md for full documentation."
