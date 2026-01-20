#!/bin/bash
# Run OXIDE UEFI bootloader in QEMU

set -e

# Build the bootloader
cargo build --package boot-uefi --target x86_64-unknown-uefi

# Create EFI directory structure
BOOT_DIR=$(mktemp -d)
mkdir -p "$BOOT_DIR/EFI/BOOT"

# Copy bootloader
cp target/x86_64-unknown-uefi/debug/boot-uefi.efi "$BOOT_DIR/EFI/BOOT/BOOTX64.EFI"

# Find OVMF firmware
OVMF=""
for path in \
    /usr/share/OVMF/OVMF_CODE.fd \
    /usr/share/edk2-ovmf/x64/OVMF_CODE.fd \
    /usr/share/edk2/ovmf/OVMF_CODE.fd \
    /usr/share/qemu/OVMF.fd
do
    if [ -f "$path" ]; then
        OVMF="$path"
        break
    fi
done

if [ -z "$OVMF" ]; then
    echo "Error: OVMF firmware not found"
    echo "Install OVMF: sudo apt install ovmf (Debian/Ubuntu)"
    echo "              sudo dnf install edk2-ovmf (Fedora)"
    rm -rf "$BOOT_DIR"
    exit 1
fi

echo "Using OVMF: $OVMF"
echo "Boot directory: $BOOT_DIR"
echo ""
echo "Starting QEMU..."
echo ""

# Run QEMU
qemu-system-x86_64 \
    -machine q35 \
    -m 256M \
    -bios "$OVMF" \
    -drive format=raw,file=fat:rw:"$BOOT_DIR" \
    -serial stdio \
    -no-reboot \
    -no-shutdown

# Cleanup
rm -rf "$BOOT_DIR"
