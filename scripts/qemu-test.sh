#!/bin/bash
# QEMU test harness for OXIDE OS
# Runs QEMU and captures serial output for debugging

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BOOT_DIR="$PROJECT_DIR/target/boot"
OUTPUT_FILE="${1:-/tmp/oxide-serial.log}"
TIMEOUT="${2:-30}"

# Find OVMF
OVMF=""
for p in /usr/share/OVMF/OVMF_CODE.fd /usr/share/edk2-ovmf/x64/OVMF_CODE.fd /usr/share/edk2/ovmf/OVMF_CODE.fd /usr/share/qemu/OVMF.fd; do
    if [ -f "$p" ]; then
        OVMF="$p"
        break
    fi
done

if [ -z "$OVMF" ]; then
    echo "Error: OVMF firmware not found"
    exit 1
fi

# Find QEMU
QEMU=""
if command -v qemu-system-x86_64 >/dev/null 2>&1; then
    QEMU="qemu-system-x86_64"
elif [ -f /usr/libexec/qemu-kvm ]; then
    QEMU="/usr/libexec/qemu-kvm"
else
    echo "Error: No QEMU found"
    exit 1
fi

# Ensure boot directory exists
if [ ! -d "$BOOT_DIR" ]; then
    echo "Boot directory not found. Run 'make boot-quick' first."
    exit 1
fi

echo "Running OXIDE OS test (timeout: ${TIMEOUT}s)..."
echo "Output file: $OUTPUT_FILE"
echo "QEMU: $QEMU"
echo "OVMF: $OVMF"
echo ""

# Clear output file
> "$OUTPUT_FILE"

# Create input FIFO for sending commands
INPUT_FIFO="/tmp/oxide-input.fifo"
rm -f "$INPUT_FIFO"
mkfifo "$INPUT_FIFO"

mkdir -p /tmp/qemu-oxide

# Run QEMU with serial to file, input from FIFO
# Use -chardev to set up bidirectional serial
TMPDIR=/tmp/qemu-oxide timeout "$TIMEOUT" "$QEMU" \
    -machine q35 \
    -cpu qemu64,+smap,+smep \
    -m 256M \
    -bios "$OVMF" \
    -drive format=raw,file=fat:rw:"$BOOT_DIR",if=none,id=disk \
    -device ide-hd,drive=disk \
    -chardev pipe,id=serial0,path="${INPUT_FIFO%.*}" \
    -serial chardev:serial0 \
    -display none \
    -no-reboot \
    2>/dev/null &

QEMU_PID=$!

# Give QEMU time to start
sleep 2

# Function to send command and wait for output
send_cmd() {
    echo "$1" > "$INPUT_FIFO"
    sleep 1
}

# Wait for boot and send test commands
sleep 5

# Send some test commands
echo "ls" > "$INPUT_FIFO" 2>/dev/null || true
sleep 2
echo "ls -la" > "$INPUT_FIFO" 2>/dev/null || true
sleep 2
echo "uname -a" > "$INPUT_FIFO" 2>/dev/null || true
sleep 2

# Wait for QEMU to finish or timeout
wait $QEMU_PID 2>/dev/null || true

# Clean up
rm -f "$INPUT_FIFO"

echo ""
echo "=== Serial Output ==="
cat "$OUTPUT_FILE" 2>/dev/null || echo "(no output captured)"
echo "=== End Output ==="
