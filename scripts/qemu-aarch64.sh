#!/usr/bin/env bash
#
# Launch OXIDE OS in QEMU (ARM64/aarch64)
#
# — NeonRoot

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Check if disk image exists
if [ ! -f "$REPO_ROOT/oxide-aarch64.img" ]; then
    echo "Error: oxide-aarch64.img not found"
    echo "Build the ARM64 image first"
    exit 1
fi

# UEFI firmware path for ARM64
AARCH64_EFI="/usr/share/edk2/aarch64/QEMU_EFI.fd"
if [ ! -f "$AARCH64_EFI" ]; then
    echo "Error: ARM64 UEFI firmware not found at $AARCH64_EFI"
    echo "Install with: sudo apt install qemu-efi-aarch64"
    exit 1
fi

# QEMU options
MEMORY="${MEMORY:-512M}"
CPU="${CPU:-cortex-a57}"
SMP="${SMP:-2}"

echo "Starting OXIDE OS (ARM64) in QEMU..."
echo "  Memory: $MEMORY"
echo "  CPUs: $SMP"
echo "  CPU Model: $CPU"
echo "  Disk: $REPO_ROOT/oxide-aarch64.img"
echo "  Boot: UEFI"

exec qemu-system-aarch64 \
    -M virt \
    -cpu "$CPU" \
    -smp "$SMP" \
    -m "$MEMORY" \
    -bios "$AARCH64_EFI" \
    -drive format=raw,file="$REPO_ROOT/oxide-aarch64.img" \
    -serial stdio \
    -nographic \
    "$@"
