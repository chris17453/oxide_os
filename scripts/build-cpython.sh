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

# Ensure the sysroot exposes the headers CPython expects so that
# configure doesn't cache "missing" results and silently disable
# modules we rely on.
REQUIRED_HEADERS=(
    "$SYSROOT/include/langinfo.h"
    "$SYSROOT/include/netdb.h"
    "$SYSROOT/include/netinet/in.h"
    "$SYSROOT/include/sys/socket.h"
    "$SYSROOT/include/sys/resource.h"
    "$SYSROOT/include/sys/eventfd.h"
    "$SYSROOT/include/sys/epoll.h"
    "$SYSROOT/include/sys/poll.h"
    "$SYSROOT/include/sys/select.h"
    "$SYSROOT/include/pty.h"
    "$SYSROOT/include/spawn.h"
    "$SYSROOT/include/utmp.h"
    "$SYSROOT/include/readline/readline.h"
)

MISSING_HEADERS=()
for header in "${REQUIRED_HEADERS[@]}"; do
    if [ ! -f "$header" ]; then
        MISSING_HEADERS+=("${header#$SYSROOT/}")
    fi
done

if [ ${#MISSING_HEADERS[@]} -ne 0 ]; then
    echo "ERROR: Required sysroot headers are missing:"
    for rel in "${MISSING_HEADERS[@]}"; do
        echo "  - $rel"
    done
    echo "Rebuild the toolchain (make toolchain) or add the headers above before continuing."
    exit 1
fi

if [ ! -f "$SYSROOT/lib/liboxide_libc.a" ]; then
    echo "ERROR: libc archive not found in $SYSROOT/lib"
    echo "Run: make toolchain"
    exit 1
fi

# — SableWire: CPython's Makefile insists on -lm and -ldl like a stubborn
# configure script that never learned our libc has everything baked in.
# Symlink 'em to shut it up.
echo "Creating library symlinks for CPython compatibility..."
ln -sf liboxide_libc.a "$SYSROOT/lib/libm.a"
ln -sf liboxide_libc.a "$SYSROOT/lib/libdl.a"

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

# Remove stale cache files so configure re-probes the toolchain after
# we add new headers or update wrapper behaviour.
if [ -f "config.cache" ]; then
    echo "Removing stale config.cache"
    rm -f config.cache
fi

# Copy config site file
cp "$OXIDE_ROOT/external/cpython-oxide-config.site" "$CPYTHON_BUILD/config.site"

# Set up cross-compilation environment
export CC="oxide-cc"
export CXX="oxide-c++"
export AR="oxide-ar"
export RANLIB="llvm-ranlib"
export LD="oxide-ld"
export CFLAGS="-O2 -fno-pic -march=x86-64 -mtune=generic -mno-avx -mno-avx2 -I$SYSROOT/include -DHAVE_LANGINFO_H=1 -DHAVE_ZLIB_H=1"
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
# — GraveShift: Limit parallelism to 4 jobs — full $(nproc) on 8+ core boxes
# can OOM the linker when it's juggling a 40MB static archive. Ask me how I know.
make -j4 SHELL=/usr/bin/bash

echo ""
echo "=== Step 4: Strip binary ==="
# — TorqueJax: Shed the debug weight. 25MB → 6MB. Still a chonker but at least
# it'll fit in the initramfs without making QEMU weep.
strip "$CPYTHON_BUILD/python" 2>/dev/null || true

echo ""
echo "=== CPython Build Complete ==="
echo "Binary: $CPYTHON_BUILD/python"
echo "Size: $(du -h "$CPYTHON_BUILD/python" 2>/dev/null | cut -f1)"
