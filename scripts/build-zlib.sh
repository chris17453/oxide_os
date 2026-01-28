#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
ZLIB_SRC="$PROJECT_ROOT/external/zlib-1.3.1"
SYSROOT="$PROJECT_ROOT/toolchain/sysroot"

echo "Building zlib for OXIDE OS..."

cd "$ZLIB_SRC"

# Set up cross-compilation environment
export CC="$PROJECT_ROOT/toolchain/bin/oxide-cc"
export AR="llvm-ar"
export RANLIB="llvm-ranlib"
export CFLAGS="-O2 -fPIC -I$SYSROOT/include"

# Configure zlib for static library build
./configure --prefix="$SYSROOT" --static

# Build zlib
make clean || true
make -j$(nproc)

# Install to sysroot
make install

echo "zlib built and installed to $SYSROOT"
echo "Library: $SYSROOT/lib/libz.a"
