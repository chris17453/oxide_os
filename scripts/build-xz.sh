#!/bin/bash
# Build XZ Utils (liblzma) for OXIDE OS
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
XZ_SRC="$PROJECT_ROOT/external/xz"
SYSROOT="$PROJECT_ROOT/toolchain/sysroot"
TOOLCHAIN="$PROJECT_ROOT/toolchain"

echo "=== Building XZ Utils for OXIDE OS ==="

# Check if XZ source exists
if [ ! -d "$XZ_SRC" ]; then
    echo "Downloading XZ Utils 5.4.5..."
    mkdir -p "$PROJECT_ROOT/external"
    cd "$PROJECT_ROOT/external"
    git clone --depth 1 --branch v5.4.5 https://github.com/tukaani-project/xz.git
fi

cd "$XZ_SRC"

# Set up cross-compilation environment
export PATH="$TOOLCHAIN/bin:$PATH"
export CC="oxide-cc"
export AR="llvm-ar"
export RANLIB="llvm-ranlib"
export CFLAGS="-O2 -fno-pic -march=x86-64 -I$SYSROOT/include"
export LDFLAGS="-L$SYSROOT/lib"

# Run autogen if needed
if [ ! -f configure ]; then
    echo "Running autogen.sh..."
    ./autogen.sh --no-po4a
fi

# Configure XZ Utils for static library build
./configure \
    --host=x86_64-unknown-linux-gnu \
    --prefix=$SYSROOT \
    --enable-static \
    --disable-shared \
    --disable-doc \
    --disable-nls \
    --disable-xz \
    --disable-xzdec \
    --disable-lzmadec \
    --disable-lzmainfo \
    --disable-scripts

# Build XZ Utils
make clean || true
make -j$(nproc)

# Install to sysroot
make install

echo ""
echo "=== XZ Utils Build Complete ==="
echo "Library: $SYSROOT/lib/liblzma.a"
echo "Headers: $SYSROOT/include/lzma.h, $SYSROOT/include/lzma/"
