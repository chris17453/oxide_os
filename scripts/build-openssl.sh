#!/bin/bash
# Build OpenSSL for OXIDE OS
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
OPENSSL_SRC="$PROJECT_ROOT/external/openssl"
SYSROOT="$PROJECT_ROOT/toolchain/sysroot"
TOOLCHAIN="$PROJECT_ROOT/toolchain"

echo "=== Building OpenSSL for OXIDE OS ==="

# Check if OpenSSL source exists
if [ ! -d "$OPENSSL_SRC" ]; then
    echo "Downloading OpenSSL 3.0.13..."
    mkdir -p "$PROJECT_ROOT/external"
    cd "$PROJECT_ROOT/external"
    git clone --depth 1 --branch openssl-3.0.13 https://github.com/openssl/openssl.git
fi

cd "$OPENSSL_SRC"

# Set up cross-compilation environment
export PATH="$TOOLCHAIN/bin:$PATH"
export CC="oxide-cc"
export AR="llvm-ar"
export RANLIB="llvm-ranlib"
export CFLAGS="-O2 -fno-pic -march=x86-64 -I$SYSROOT/include"
export LDFLAGS="-L$SYSROOT/lib"

# Configure OpenSSL for OXIDE
# Use linux-x86_64 target with custom settings
./Configure linux-x86_64 \
    --prefix=$SYSROOT \
    --openssldir=$SYSROOT/etc/ssl \
    no-shared \
    no-dso \
    no-engine \
    no-hw \
    no-async \
    no-tests \
    -static \
    -fno-pic

# Build OpenSSL
make clean || true
make -j$(nproc)

# Install to sysroot
make install_sw install_ssldirs

echo ""
echo "=== OpenSSL Build Complete ==="
echo "Libraries: $SYSROOT/lib/libssl.a, $SYSROOT/lib/libcrypto.a"
echo "Headers: $SYSROOT/include/openssl/"
