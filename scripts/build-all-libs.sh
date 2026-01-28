#!/bin/bash
# Build all external libraries for OXIDE OS
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "========================================="
echo "Building All Libraries for OXIDE OS"
echo "========================================="
echo ""

# Build in dependency order
echo "Step 1/4: Building zlib..."
"$SCRIPT_DIR/build-zlib.sh"
echo ""

echo "Step 2/4: Building XZ Utils..."
"$SCRIPT_DIR/build-xz.sh"
echo ""

echo "Step 3/4: Building Zstandard..."
"$SCRIPT_DIR/build-zstd.sh"
echo ""

echo "Step 4/4: Building OpenSSL..."
"$SCRIPT_DIR/build-openssl.sh"
echo ""

echo "========================================="
echo "All Libraries Built Successfully!"
echo "========================================="
echo ""
echo "Installed libraries in toolchain/sysroot/lib:"
ls -lh toolchain/sysroot/lib/*.a 2>/dev/null || true
