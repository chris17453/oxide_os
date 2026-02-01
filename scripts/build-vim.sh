#!/usr/bin/env bash
# Build script for vim on OXIDE OS
# Follows pattern from build-cpython.sh

set -e

OXIDE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VIM_SRC="$OXIDE_ROOT/external/vim"
TOOLCHAIN="$OXIDE_ROOT/toolchain"
SYSROOT="$TOOLCHAIN/sysroot"

echo "Building vim for OXIDE OS..."

# Check prerequisites
if [ ! -d "$VIM_SRC" ]; then
    echo "ERROR: vim source not found at $VIM_SRC"
    echo "Run: git clone --depth 1 --branch v9.1.0 https://github.com/vim/vim.git external/vim"
    exit 1
fi

if [ ! -f "$TOOLCHAIN/bin/oxide-cc" ]; then
    echo "ERROR: OXIDE toolchain not found"
    echo "Run: make toolchain"
    exit 1
fi

# Add toolchain to PATH
export PATH="$TOOLCHAIN/bin:$PATH"

# Configure vim
cd "$VIM_SRC/src"

echo "Configuring vim..."
# Force cross-compilation mode to prevent running test binaries
CONFIG_SITE="$OXIDE_ROOT/external/vim-oxide-config.cache" \
ac_cv_c_bigendian=no \
ac_cv_sizeof_int=4 \
ac_cv_sizeof_long=8 \
ac_cv_sizeof_time_t=8 \
ac_cv_sizeof_off_t=8 \
./auto/configure \
    --srcdir=. \
    --cache-file=auto/config.cache \
    --build=x86_64-pc-linux-gnu \
    --host=x86_64-unknown-oxide-elf \
    --with-features=small \
    --disable-gui \
    --disable-gtktest \
    --disable-xim \
    --disable-netbeans \
    --disable-pythoninterp \
    --disable-python3interp \
    --disable-rubyinterp \
    --disable-luainterp \
    --disable-perlinterp \
    --disable-tclinterp \
    --disable-cscope \
    --disable-gpm \
    --disable-sysmouse \
    --enable-multibyte \
    --with-tlib=oxide_libc \
    CC=oxide-cc \
    AR=llvm-ar \
    RANLIB=llvm-ranlib \
    STRIP=llvm-strip \
    CFLAGS="-O2 -fno-strict-aliasing" \
    LDFLAGS="-static -L$SYSROOT/lib -lregex -loxide_libc"

echo "Building vim..."
make -j$(nproc)

# Install to release directory
echo "Installing vim..."
mkdir -p "$OXIDE_ROOT/target/x86_64-unknown-none/release"
cp vim "$OXIDE_ROOT/target/x86_64-unknown-none/release/vim"
llvm-strip "$OXIDE_ROOT/target/x86_64-unknown-none/release/vim"

echo "vim built successfully!"
ls -lh "$OXIDE_ROOT/target/x86_64-unknown-none/release/vim"
