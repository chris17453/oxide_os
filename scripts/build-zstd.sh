#!/bin/bash
# Build Zstandard for OXIDE OS
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
ZSTD_SRC="$PROJECT_ROOT/external/zstd"
SYSROOT="$PROJECT_ROOT/toolchain/sysroot"
TOOLCHAIN="$PROJECT_ROOT/toolchain"

echo "=== Building Zstandard for OXIDE OS ==="

# Check if Zstandard source exists
if [ ! -d "$ZSTD_SRC" ]; then
    echo "Downloading Zstandard 1.5.5..."
    mkdir -p "$PROJECT_ROOT/external"
    cd "$PROJECT_ROOT/external"
    git clone --depth 1 --branch v1.5.5 https://github.com/facebook/zstd.git
fi

cd "$ZSTD_SRC"

# Set up cross-compilation environment
export PATH="$TOOLCHAIN/bin:$PATH"
export CC="oxide-cc"
export AR="llvm-ar"
export RANLIB="llvm-ranlib"
export CFLAGS="-O2 -fno-pic -march=x86-64 -I$SYSROOT/include"
export LDFLAGS="-L$SYSROOT/lib"

# Build Zstandard library only (not CLI tools)
cd lib
make clean || true
make -j$(nproc) libzstd.a

# Install to sysroot
make PREFIX=$SYSROOT install-static install-includes install-pc

echo ""
echo "=== Zstandard Build Complete ==="
echo "Library: $SYSROOT/lib/libzstd.a"
echo "Headers: $SYSROOT/include/zstd.h, $SYSROOT/include/zstd_errors.h"
