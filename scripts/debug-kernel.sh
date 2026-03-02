#!/usr/bin/bash
#
# Autonomous kernel debugging wrapper
# — ColdCipher: One script to rule them all, because remembering
# GDB incantations at 3 AM is not in my job description.
#
# This script provides a simple interface for autonomous debugging:
# - Starts QEMU with GDB server
# - Executes specified debugging task
# - Captures and returns output
#
# Usage:
#   ./scripts/debug-kernel.sh capture    # Capture crash/panic
#   ./scripts/debug-kernel.sh boot       # Boot sanity check
#   ./scripts/debug-kernel.sh exec <cmd> # Execute GDB command
#   ./scripts/debug-kernel.sh repl       # Interactive REPL

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Configuration
TARGET_DIR="${TARGET_DIR:-target}"
KERNEL_TARGET="$TARGET_DIR/x86_64-unknown-oxide/debug/kernel"
OVMF="${OVMF:-$(find /usr/share -name "OVMF_CODE.fd" -o -name "OVMF.fd" 2>/dev/null | head -1 || true)}"
QEMU_LOG="$TARGET_DIR/qemu.log"
SERIAL_LOG="$TARGET_DIR/serial.log"
GDB_PORT="${GDB_PORT:-1234}"
# Use rootfs disk if available, otherwise boot.img
DISK_IMAGE="${DISK_IMAGE:-${TARGET_DIR}/oxide-disk.img}"
if [ ! -f "$DISK_IMAGE" ]; then
    DISK_IMAGE="${TARGET_DIR}/boot.img"
fi

# Ensure target directory exists
mkdir -p "$TARGET_DIR" /tmp/qemu-oxide

# Kill any existing QEMU
pkill -x 'qemu-system-x86_64' 2>/dev/null || true
sleep 0.5

# Start QEMU with GDB server (background)
start_qemu() {
    echo "[*] Starting QEMU with GDB server on port $GDB_PORT..."

    if [ -z "$OVMF" ]; then
        echo "Error: OVMF firmware not found" >&2
        exit 1
    fi

    # Build disk image if needed
    if [ ! -f "$DISK_IMAGE" ]; then
        echo "[*] Building disk image..."
        if [ "$DISK_IMAGE" = "${TARGET_DIR}/oxide-disk.img" ]; then
            make -s create-rootfs
        else
            make -s boot-image
        fi
    fi

    echo "[*] Using disk: $DISK_IMAGE"

    TMPDIR=/tmp/qemu-oxide qemu-system-x86_64 \
        -machine q35 \
        -cpu qemu64 \
        -smp 1 \
        -m 512M \
        -bios "$OVMF" \
        -drive file="$DISK_IMAGE",format=raw,if=none,id=disk \
        -device virtio-blk-pci,drive=disk \
        -device isa-debugcon,iobase=0xe9,chardev=dbg \
        -chardev stdio,id=dbg,signal=off \
        -serial "file:$SERIAL_LOG" \
        -device virtio-gpu-pci \
        -no-reboot -no-shutdown \
        -d int,cpu_reset,guest_errors \
        -D "$QEMU_LOG" \
        -s -S \
        > /dev/null 2>&1 &

    QEMU_PID=$!
    echo "[*] QEMU started (PID $QEMU_PID)"

    # Wait for GDB server to be ready
    sleep 2

    # Verify QEMU is still running
    if ! kill -0 $QEMU_PID 2>/dev/null; then
        echo "Error: QEMU failed to start" >&2
        cat "$QEMU_LOG" >&2
        exit 1
    fi

    echo "$QEMU_PID"
}

# Stop QEMU
stop_qemu() {
    local pid=$1
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
        echo "[*] Stopping QEMU (PID $pid)..."
        kill "$pid" 2>/dev/null || true
        sleep 0.5
        kill -9 "$pid" 2>/dev/null || true
    fi
}

# Cleanup on exit
cleanup() {
    if [ -n "${QEMU_PID:-}" ]; then
        stop_qemu "$QEMU_PID"
    fi
}
trap cleanup EXIT INT TERM

# Main command dispatch
case "${1:-help}" in
    capture)
        echo "=== Autonomous Crash Capture ==="
        QEMU_PID=$(start_qemu)

        echo "[*] Running crash capture..."
        timeout 60 gdb -q -batch \
            -x scripts/gdb-capture-crash.gdb \
            "$KERNEL_TARGET" \
            2>&1 | tee "$TARGET_DIR/crash-capture.log"

        echo "[+] Crash capture complete: $TARGET_DIR/crash-capture.log"
        ;;

    boot)
        echo "=== Boot Sanity Check ==="
        QEMU_PID=$(start_qemu)

        echo "[*] Running boot check..."
        timeout 30 gdb -q -batch \
            -x scripts/gdb-check-boot.gdb \
            "$KERNEL_TARGET" \
            2>&1 | tee "$TARGET_DIR/boot-check.log"

        echo "[+] Boot check complete: $TARGET_DIR/boot-check.log"
        ;;

    exec)
        if [ -z "${2:-}" ]; then
            echo "Error: Must specify GDB command" >&2
            echo "Usage: $0 exec <gdb-command>" >&2
            exit 1
        fi

        QEMU_PID=$(start_qemu)

        echo "[*] Executing: $2"
        ./scripts/gdb-autonomous.py --exec "$2"
        ;;

    repl)
        echo "=== Autonomous GDB REPL ==="
        QEMU_PID=$(start_qemu)

        echo "[*] Starting REPL..."
        ./scripts/gdb-autonomous.py --repl
        ;;

    script)
        if [ -z "${2:-}" ]; then
            echo "Error: Must specify GDB script file" >&2
            echo "Usage: $0 script <script-path>" >&2
            exit 1
        fi

        QEMU_PID=$(start_qemu)

        echo "[*] Executing script: $2"
        ./scripts/gdb-autonomous.py --script "$2"
        ;;

    help|*)
        cat <<EOF
Autonomous Kernel Debugging Wrapper

Usage: $0 <command> [args]

Commands:
  capture          Capture crash/panic (runs until crash, dumps state)
  boot             Boot sanity check (verify kernel starts)
  exec <cmd>       Execute single GDB command
  repl             Interactive autonomous REPL
  script <file>    Execute GDB script file
  help             Show this message

Examples:
  $0 capture                    # Capture any crash/panic
  $0 boot                       # Check if kernel boots
  $0 exec "bt"                  # Get backtrace
  $0 exec "info registers"      # Dump registers
  $0 repl                       # Interactive debugging
  $0 script my-debug.gdb        # Run custom script

Environment Variables:
  GDB_PORT         GDB server port (default: 1234)
  TARGET_DIR       Build target directory (default: target)
  OVMF             OVMF firmware path (auto-detected)

Files:
  $TARGET_DIR/crash-capture.log  Crash capture output
  $TARGET_DIR/boot-check.log     Boot check output
  $TARGET_DIR/qemu.log           QEMU debug log
  $TARGET_DIR/serial.log         Serial port output

— ColdCipher: Debug smart, not hard.
EOF
        ;;
esac
