# OXIDE Package Manager - Integration Examples

This document provides complete examples of integrating packages built with oxdnf into OXIDE OS.

## Example 1: Building and Installing Bash

### Step 1: Build Bash

```bash
cd /path/to/oxide_os

# First time: sync repository metadata
make pkgmgr-sync

# Build bash from Fedora source RPM
make pkgmgr-build PKG=bash
```

This will:
1. Download bash-5.x source RPM from Fedora
2. Extract source, patches, and spec file
3. Configure with OXIDE toolchain
4. Compile using oxide-cc
5. Create bash-5.x.opkg package
6. Add to local repository

Expected time: 5-10 minutes

### Step 2: Install to Staging

```bash
# Create staging directory
mkdir -p /tmp/oxide-bash-staging

# Install bash to staging
python3 pkgmgr/bin/oxdnf install bash --installroot=/tmp/oxide-bash-staging

# Check what was installed
find /tmp/oxide-bash-staging -type f
```

### Step 3: Integrate with OXIDE

```bash
# Copy bash binary to userspace
cp /tmp/oxide-bash-staging/usr/bin/bash userspace/shell/

# Rebuild initramfs with bash
make initramfs

# Test in QEMU
make run
```

### Step 4: Test in OXIDE

Once booted in QEMU:
```bash
# Login as root
# At the prompt:
/bin/bash --version
echo "Hello from bash!" | /bin/bash
```

## Example 2: Building and Using Vim

### Complete Workflow

```bash
# 1. Sync repos
make pkgmgr-sync

# 2. Build vim
make pkgmgr-build PKG=vim
# Takes about 10-15 minutes

# 3. Install to staging
mkdir -p /tmp/oxide-vim
python3 pkgmgr/bin/oxdnf install vim --installroot=/tmp/oxide-vim

# 4. Copy to userspace
cp /tmp/oxide-vim/usr/bin/vim userspace/apps/vim/
mkdir -p userspace/apps/vim/runtime
cp -r /tmp/oxide-vim/usr/share/vim/* userspace/apps/vim/runtime/

# 5. Rebuild and test
make initramfs
make run
```

In QEMU:
```bash
# Test vim
/bin/vim --version
echo "test" > /tmp/test.txt
/bin/vim /tmp/test.txt
```

## Example 3: Building Development Tools Chain

Build a complete development environment:

```bash
# Build make
make pkgmgr-build PKG=make

# Build binutils (assembler, linker, etc.)
make pkgmgr-build PKG=binutils

# Build gcc (C compiler)
make pkgmgr-build PKG=gcc
# Note: This takes 30-60 minutes

# Install to common staging area
mkdir -p /tmp/oxide-devtools
python3 pkgmgr/bin/oxdnf install make --installroot=/tmp/oxide-devtools
python3 pkgmgr/bin/oxdnf install binutils --installroot=/tmp/oxide-devtools
python3 pkgmgr/bin/oxdnf install gcc --installroot=/tmp/oxide-devtools

# Copy to userspace
cp /tmp/oxide-devtools/usr/bin/* userspace/devtools/
```

## Example 4: Building Network Utilities

```bash
# Build curl
make pkgmgr-build PKG=curl

# Build wget
make pkgmgr-build PKG=wget

# Build openssh
make pkgmgr-build PKG=openssh

# Install all
mkdir -p /tmp/oxide-net
for pkg in curl wget openssh; do
    python3 pkgmgr/bin/oxdnf install $pkg --installroot=/tmp/oxide-net
done

# Copy to userspace
cp /tmp/oxide-net/usr/bin/curl userspace/network/
cp /tmp/oxide-net/usr/bin/wget userspace/network/
cp /tmp/oxide-net/usr/bin/ssh userspace/network/
```

## Example 5: Building Libraries for Development

### Build ncurses library

```bash
# Build ncurses
make pkgmgr-build PKG=ncurses

# Install to sysroot (for linking other packages)
mkdir -p /tmp/oxide-libs
python3 pkgmgr/bin/oxdnf install ncurses --installroot=/tmp/oxide-libs

# Copy to toolchain sysroot
cp /tmp/oxide-libs/usr/lib/*.a toolchain/sysroot/lib/
cp -r /tmp/oxide-libs/usr/include/* toolchain/sysroot/include/

# Now other packages can link against ncurses
make pkgmgr-build PKG=vim  # vim needs ncurses
```

## Example 6: Dependency Chain Build

Building packages with dependencies:

```bash
# Example: Building htop (requires ncurses)

# 1. Build dependency first
make pkgmgr-build PKG=ncurses

# 2. Install ncurses to sysroot
mkdir -p /tmp/oxide-libs
python3 pkgmgr/bin/oxdnf install ncurses --installroot=/tmp/oxide-libs
cp /tmp/oxide-libs/usr/lib/*.a toolchain/sysroot/lib/
cp -r /tmp/oxide-libs/usr/include/* toolchain/sysroot/include/

# 3. Now build htop
make pkgmgr-build PKG=htop

# 4. Install htop
mkdir -p /tmp/oxide-htop
python3 pkgmgr/bin/oxdnf install htop --installroot=/tmp/oxide-htop

# 5. Copy to userspace
cp /tmp/oxide-htop/usr/bin/htop userspace/apps/
```

## Example 7: Batch Building

Build multiple packages in one go:

```bash
# Create a list of packages
cat > /tmp/packages.txt <<EOF
bash
vim
make
sed
awk
grep
coreutils
EOF

# Build all packages
while read pkg; do
    echo "Building $pkg..."
    make pkgmgr-build PKG=$pkg || echo "Failed: $pkg"
done < /tmp/packages.txt

# Install all to common staging
mkdir -p /tmp/oxide-staging
while read pkg; do
    python3 pkgmgr/bin/oxdnf install $pkg --installroot=/tmp/oxide-staging 2>/dev/null || true
done < /tmp/packages.txt
```

## Example 8: Custom Package Override

Create custom build settings for a package:

```bash
# Create override for package 'mypackage'
cat > pkgmgr/specs/overrides/mypackage.override <<'EOF'
# Custom build flags
CONFIGURE_FLAGS="
  --enable-oxide-support
  --disable-fancy-features
  --with-oxide-api
"

# Extra CFLAGS
EXTRA_CFLAGS="-DOXIDE_OS -O3"

# Pre-build hook
pre_build() {
    echo "Applying OXIDE patches..."
    patch -p1 < /path/to/oxide-specific.patch
    
    # Fix for OXIDE
    sed -i 's/OLD_CODE/NEW_CODE/' src/main.c
}

# Post-build hook
post_build() {
    echo "Package built successfully"
    # Copy additional files
    cp extra-files/* install/usr/share/
}
EOF

# Build with override
make pkgmgr-build PKG=mypackage
```

## Example 9: Creating a Minimal System

Build a minimal bootable system:

```bash
# Essential packages
PACKAGES="bash coreutils util-linux grep sed gawk"

# Build all
for pkg in $PACKAGES; do
    make pkgmgr-build PKG=$pkg
done

# Install to minimal root
mkdir -p /tmp/oxide-minimal
for pkg in $PACKAGES; do
    python3 pkgmgr/bin/oxdnf install $pkg --installroot=/tmp/oxide-minimal
done

# Copy to userspace
cp -r /tmp/oxide-minimal/usr/bin/* userspace/coreutils/
cp -r /tmp/oxide-minimal/bin/* userspace/shell/

# Rebuild initramfs
make initramfs

# Test
make run
```

## Example 10: Package Info and Management

```bash
# Get package information
python3 pkgmgr/bin/oxdnf info bash

# List all available packages
python3 pkgmgr/bin/oxdnf list available

# List installed packages
python3 pkgmgr/bin/oxdnf list installed

# Search for packages
python3 pkgmgr/bin/oxdnf search editor

# Remove a package
python3 pkgmgr/bin/oxdnf remove bash

# Clean cache to save space
python3 pkgmgr/bin/oxdnf clean all
```

## Automation Scripts

### Build Script for Complete System

```bash
#!/bin/bash
# build-full-system.sh

set -e

PACKAGES=(
    "coreutils"
    "bash"
    "vim"
    "make"
    "grep"
    "sed"
    "gawk"
    "tar"
    "gzip"
    "bzip2"
    "xz"
)

STAGING="/tmp/oxide-full-system"

# Sync repos
make pkgmgr-sync

# Build all packages
for pkg in "${PACKAGES[@]}"; do
    echo "=== Building $pkg ==="
    make pkgmgr-build PKG=$pkg || {
        echo "WARNING: Failed to build $pkg"
        continue
    }
done

# Install all
mkdir -p $STAGING
for pkg in "${PACKAGES[@]}"; do
    echo "=== Installing $pkg ==="
    python3 pkgmgr/bin/oxdnf install $pkg --installroot=$STAGING 2>/dev/null || true
done

# Copy to userspace
echo "=== Copying to userspace ==="
cp -r $STAGING/usr/bin/* userspace/coreutils/ 2>/dev/null || true
cp -r $STAGING/bin/* userspace/shell/ 2>/dev/null || true

# Rebuild
echo "=== Rebuilding initramfs ==="
make initramfs

echo "=== Complete! ==="
echo "Run: make run"
```

### Check Build Status Script

```bash
#!/bin/bash
# check-build-status.sh

PACKAGES=("bash" "vim" "make" "coreutils")

echo "Package Build Status"
echo "===================="

for pkg in "${PACKAGES[@]}"; do
    if python3 pkgmgr/bin/oxdnf info $pkg 2>/dev/null | grep -q "Package:"; then
        status="✓ Built"
    else
        status="✗ Not built"
    fi
    
    printf "%-20s %s\n" "$pkg" "$status"
done
```

## Tips and Best Practices

1. **Always sync first**: Run `make pkgmgr-sync` before building packages
2. **Build dependencies first**: Check package requirements and build dependencies before the main package
3. **Use staging directories**: Install to temporary directories before copying to userspace
4. **Clean cache regularly**: Use `python3 pkgmgr/bin/oxdnf clean all` to free disk space
5. **Check build logs**: Build logs are in `pkgmgr/cache/builds/build-*/build.log`
6. **Test in isolation**: Test new packages in QEMU before integrating
7. **Create overrides**: Use package overrides for OXIDE-specific patches
8. **Document customizations**: Keep notes on any custom build flags or patches

## Troubleshooting

### Build Fails

```bash
# Check build log
cat pkgmgr/cache/builds/build-*/build.log

# Check for missing dependencies
python3 pkgmgr/bin/oxdnf info <failed-package>

# Clean and retry
python3 pkgmgr/bin/oxdnf clean all
make pkgmgr-build PKG=<package>
```

### Package Not Found

```bash
# Refresh metadata
make pkgmgr-sync

# Search with partial name
python3 pkgmgr/bin/oxdnf search <partial-name>
```

### Installation Fails

```bash
# Check if package is built
python3 pkgmgr/bin/oxdnf list available | grep <package>

# Rebuild package
make pkgmgr-build PKG=<package>
```

---
*"Integration examples... because building is one thing, making it work is another."* — StackTrace
