#!/usr/bin/env bash
# Build CPython for OXIDE OS
set -e

OXIDE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CPYTHON_SRC="$OXIDE_ROOT/external/cpython"
CPYTHON_BUILD_NATIVE="$OXIDE_ROOT/external/cpython-build-native"
CPYTHON_BUILD="$OXIDE_ROOT/external/cpython-build"
TOOLCHAIN="$OXIDE_ROOT/toolchain"
SYSROOT="$TOOLCHAIN/sysroot"

echo "=== Building CPython for OXIDE OS ==="
echo "OXIDE_ROOT: $OXIDE_ROOT"
echo "CPYTHON_SRC: $CPYTHON_SRC"
echo "Toolchain: $TOOLCHAIN"
echo ""

# Check prerequisites
if [ ! -d "$CPYTHON_SRC" ]; then
    echo "ERROR: CPython source not found at $CPYTHON_SRC"
    echo "Run: cd external && git clone --depth 1 --branch v3.13.1 https://github.com/python/cpython.git"
    exit 1
fi

if [ ! -f "$TOOLCHAIN/bin/oxide-cc" ]; then
    echo "ERROR: OXIDE toolchain not found. Building it..."
    cd "$OXIDE_ROOT"
    make toolchain
fi

# Add toolchain to PATH
export PATH="$TOOLCHAIN/bin:$PATH"

# Verify toolchain works
if ! oxide-cc --version >/dev/null 2>&1; then
    echo "ERROR: oxide-cc not working"
    exit 1
fi

echo "=== Step 1: Build native Python (for host tools) ==="
if [ ! -f "$CPYTHON_BUILD_NATIVE/python" ]; then
    mkdir -p "$CPYTHON_BUILD_NATIVE"
    cd "$CPYTHON_BUILD_NATIVE"

    echo "Configuring native Python build..."
    CONFIG_SHELL=/usr/bin/bash /usr/bin/bash "$CPYTHON_SRC/configure" \
        --prefix="$CPYTHON_BUILD_NATIVE/install" \
        --disable-shared \
        --without-ensurepip

    echo "Building native Python..."
    make -j$(nproc) SHELL=/usr/bin/bash
    make install SHELL=/usr/bin/bash

    echo "Native Python built: $CPYTHON_BUILD_NATIVE/python"
else
    echo "Native Python already exists, skipping..."
fi

echo ""
echo "=== Step 2: Configure CPython for OXIDE cross-compilation ==="
mkdir -p "$CPYTHON_BUILD"
cd "$CPYTHON_BUILD"

# Copy config site file
cp "$OXIDE_ROOT/external/cpython-oxide-config.site" "$CPYTHON_BUILD/config.site"

# Set up cross-compilation environment
export CC="oxide-cc"
export CXX="oxide-c++"
export AR="oxide-ar"
export RANLIB="llvm-ranlib"
export LD="oxide-ld"
export CFLAGS="-O2 -fPIC -I$SYSROOT/include"
export LDFLAGS="-L$SYSROOT/lib"
export CONFIG_SITE="$CPYTHON_BUILD/config.site"

# Configure for cross-compilation
echo "Running configure for OXIDE target..."
CONFIG_SHELL=/usr/bin/bash /usr/bin/bash "$CPYTHON_SRC/configure" \
    --host=x86_64-unknown-linux-gnu \
    --build=x86_64-linux-gnu \
    --prefix=/usr \
    --disable-ipv6 \
    --disable-shared \
    --without-ensurepip \
    --without-pymalloc \
    --with-build-python="$CPYTHON_BUILD_NATIVE/python" \
    --with-freeze-module="$CPYTHON_BUILD_NATIVE/Programs/_freeze_module" \
    ac_cv_file__dev_ptmx=no \
    ac_cv_file__dev_ptc=no

# Copy Setup.local after configure creates the Modules directory
echo "Copying Modules/Setup.local..."
cp "$OXIDE_ROOT/external/cpython-Setup.local" "$CPYTHON_BUILD/Modules/Setup.local"

echo ""
echo "=== Step 3: Build CPython for OXIDE ==="
make -j$(nproc) SHELL=/usr/bin/bash

echo ""
echo "=== CPython Build Complete ==="
echo "Binary: $CPYTHON_BUILD/python"
echo "Size: $(du -h "$CPYTHON_BUILD/python" 2>/dev/null | cut -f1)"
echo ""
echo "To install to initramfs, run:"
echo "  cp $CPYTHON_BUILD/python target/x86_64-unknown-none/release/python"
echo "  make initramfs"
