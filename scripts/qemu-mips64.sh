#!/usr/bin/env bash
#
# Launch OXIDE OS in QEMU (MIPS64)
#
# Note: QEMU Malta board doesn't have ARCS firmware.
# This boots the kernel directly for testing.
# For real ARCS boot, use SGI hardware.
#
# — GraveShift

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Check if kernel exists
if [ ! -f "$REPO_ROOT/target/mips64-unknown-linux-gnu/debug/kernel" ]; then
    echo "Error: MIPS64 kernel not found"
    echo "Build with: cargo build -p kernel --target mips64-unknown-linux-gnu"
    exit 1
fi

# QEMU options
MEMORY="${MEMORY:-256M}"
CPU="${CPU:-MIPS64R2-generic}"

echo "Starting OXIDE OS (MIPS64) in QEMU..."
echo "  Memory: $MEMORY"
echo "  CPU Model: $CPU"
echo "  Kernel: Direct load (no ARCS)"
echo ""
echo "⚠️  Note: QEMU Malta doesn't support ARCS boot protocol."
echo "    For ARCS testing, use real SGI hardware (Indy, Octane, etc.)"
echo ""

exec qemu-system-mips64 \
    -M malta \
    -cpu "$CPU" \
    -m "$MEMORY" \
    -kernel "$REPO_ROOT/target/mips64-unknown-linux-gnu/debug/kernel" \
    -serial stdio \
    -nographic \
    "$@"
