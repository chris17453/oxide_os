#!/usr/bin/env bash
#
# Launch OXIDE OS in QEMU (x86_64)
#
# — NeonRoot

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Check if disk image exists
if [ ! -f "$REPO_ROOT/oxide.img" ]; then
    echo "Error: oxide.img not found"
    echo "Build the OS first with: make build"
    exit 1
fi

# UEFI firmware path (update for your system)
OVMF_CODE="/usr/share/edk2/ovmf/OVMF_CODE.fd"
if [ ! -f "$OVMF_CODE" ]; then
    echo "Warning: UEFI firmware not found at $OVMF_CODE"
    echo "Install with: sudo apt install ovmf"
    echo "Falling back to legacy BIOS mode"
    OVMF_CODE=""
fi

# QEMU options
MEMORY="${MEMORY:-512M}"
CPU="${CPU:-max}"
SMP="${SMP:-2}"

echo "Starting OXIDE OS (x86_64) in QEMU..."
echo "  Memory: $MEMORY"
echo "  CPUs: $SMP"
echo "  Disk: $REPO_ROOT/oxide.img"

if [ -n "$OVMF_CODE" ]; then
    echo "  Boot: UEFI"
    exec qemu-system-x86_64 \
        -bios "$OVMF_CODE" \
        -drive format=raw,file="$REPO_ROOT/oxide.img" \
        -m "$MEMORY" \
        -smp "$SMP" \
        -cpu "$CPU" \
        -serial stdio \
        -nographic \
        "$@"
else
    echo "  Boot: Legacy BIOS"
    exec qemu-system-x86_64 \
        -drive format=raw,file="$REPO_ROOT/oxide.img" \
        -m "$MEMORY" \
        -smp "$SMP" \
        -cpu "$CPU" \
        -serial stdio \
        -nographic \
        "$@"
fi
