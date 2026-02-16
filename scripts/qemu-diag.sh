#!/bin/bash
# — GraveShift: Query QEMU's PCI + display state to find where the VGA BAR actually is.
# Run this AFTER QEMU starts: ./scripts/qemu-diag.sh
SOCK="${1:-target/qemu-monitor.sock}"
if [ ! -S "$SOCK" ]; then
    echo "No monitor socket at $SOCK — is QEMU running?"
    exit 1
fi
echo "=== PCI Devices ==="
echo "info pci" | socat - UNIX-CONNECT:"$SOCK" 2>/dev/null | head -80
echo ""
echo "=== Display Info ==="
echo "info display" | socat - UNIX-CONNECT:"$SOCK" 2>/dev/null
echo ""
echo "=== VGA Device Memory Regions ==="
echo "info mtree -f" | socat - UNIX-CONNECT:"$SOCK" 2>/dev/null | grep -A5 -i "vga\|VGA\|framebuffer\|bochs\|std"
