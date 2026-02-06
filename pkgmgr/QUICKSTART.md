# OXIDE Package Manager - Quick Start Guide

This guide will help you get started with oxdnf, the OXIDE package management system.

## Prerequisites

Before using oxdnf, ensure you have:
1. OXIDE toolchain built (`make toolchain`)
2. Python 3.6 or later
3. Standard build tools (tar, gzip, rpm2cpio, cpio)

## Installation

No installation needed! oxdnf is part of the OXIDE OS repository.

## Basic Workflow

### 1. Sync Repository Metadata

First, sync the Fedora repository metadata:

```bash
make pkgmgr-sync
# or directly:
python3 pkgmgr/bin/oxdnf repo-sync
```

This downloads package lists from configured Fedora repositories. It only needs to be run once, or when you want to refresh the package list.

### 2. Search for Packages

Search for a package you want to build:

```bash
make pkgmgr-search PKG=bash
# or directly:
python3 pkgmgr/bin/oxdnf search bash
```

Example output:
```
Searching for: bash
Available from Fedora:
Name                           Version        Repo                
----------------------------------------------------------------------
bash                           5.2.15         fedora-source       
bash-completion                2.11           fedora-source       
```

### 3. Build a Package

Build a package from source RPM:

```bash
make pkgmgr-build PKG=bash
# or directly:
python3 pkgmgr/bin/oxdnf buildsrpm bash
```

This will:
1. Download the source RPM from Fedora
2. Extract source and patches
3. Configure with OXIDE toolchain
4. Build using oxide-cc
5. Create .opkg package
6. Add to local repository

Build time varies by package (1-30 minutes typically).

### 4. Install a Package

Install a built package:

```bash
make pkgmgr-install PKG=bash
# or directly:
python3 pkgmgr/bin/oxdnf install bash
```

For initramfs integration:
```bash
# Install to staging directory
python3 pkgmgr/bin/oxdnf install bash --installroot=/tmp/staging

# Copy binaries to userspace
cp /tmp/staging/bin/bash userspace/shell/

# Rebuild initramfs
make initramfs
```

## Example: Building and Installing Vim

Complete workflow for building and using Vim:

```bash
# 1. Sync repos (first time only)
make pkgmgr-sync

# 2. Search for vim
make pkgmgr-search PKG=vim

# 3. Build vim from source RPM
make pkgmgr-build PKG=vim
# This takes about 5-10 minutes

# 4. Install vim
make pkgmgr-install PKG=vim

# 5. Verify
python3 pkgmgr/bin/oxdnf list installed | grep vim
```

## Managing Packages

### List Installed Packages

```bash
python3 pkgmgr/bin/oxdnf list installed
```

### List Available Packages

```bash
python3 pkgmgr/bin/oxdnf list available
```

### Get Package Information

```bash
python3 pkgmgr/bin/oxdnf info bash
```

### Remove a Package

```bash
python3 pkgmgr/bin/oxdnf remove bash
```

### Clean Cache

```bash
python3 pkgmgr/bin/oxdnf clean all
```

## Common Packages to Build

Here are some commonly useful packages:

**Development Tools:**
```bash
make pkgmgr-build PKG=make
make pkgmgr-build PKG=gcc
make pkgmgr-build PKG=binutils
```

**Text Editors:**
```bash
make pkgmgr-build PKG=vim
make pkgmgr-build PKG=nano
make pkgmgr-build PKG=ed
```

**Shell Utilities:**
```bash
make pkgmgr-build PKG=bash
make pkgmgr-build PKG=coreutils
make pkgmgr-build PKG=util-linux
```

**Compression:**
```bash
make pkgmgr-build PKG=gzip
make pkgmgr-build PKG=bzip2
make pkgmgr-build PKG=xz
```

**Network Tools:**
```bash
make pkgmgr-build PKG=curl
make pkgmgr-build PKG=wget
make pkgmgr-build PKG=openssh
```

## Troubleshooting

### Build Fails with Missing Dependencies

If a build fails due to missing dependencies:

1. Check the build log in `pkgmgr/cache/builds/build-*/build.log`
2. Identify missing dependencies
3. Build dependencies first
4. Retry the original build

Example:
```bash
# Build fails saying "ncurses not found"
make pkgmgr-build PKG=ncurses
# Now retry original package
make pkgmgr-build PKG=vim
```

### Package Not Found

If search doesn't find a package:

1. Make sure you've run `make pkgmgr-sync`
2. Check spelling
3. Try searching with partial name: `make pkgmgr-search PKG=lib`

### Build Succeeds but Configure Fails

Some packages need OXIDE-specific configure flags. Create an override file:

```bash
cat > pkgmgr/specs/overrides/mypackage.override <<EOF
# Additional configure flags
CONFIGURE_FLAGS="--disable-nls --enable-static"

# Pre-build hook
pre_build() {
    # Apply OXIDE-specific patch
    patch -p1 < /path/to/oxide.patch
}
EOF
```

### Download is Slow

Edit `pkgmgr/config/repos.d/fedora.repo` to use a closer mirror or enable metalink.

## Advanced Usage

### Building Multiple Packages

```bash
# Build multiple packages in sequence
for pkg in bash vim make; do
    make pkgmgr-build PKG=$pkg
done
```

### Custom Build Options

```bash
# Build with more make jobs
python3 pkgmgr/bin/srpm-build /path/to/package.src.rpm -j 8

# Build to custom location
python3 pkgmgr/bin/srpm-build /path/to/package.src.rpm -o /tmp/output
```

### Direct Tool Usage

For more control, use the tools directly:

```bash
# Fetch SRPM only
python3 pkgmgr/bin/srpm-fetch bash

# Build from local SRPM
python3 pkgmgr/bin/srpm-build pkgmgr/cache/srpms/bash-*.src.rpm

# Sync specific repo
python3 pkgmgr/bin/repo-sync
```

## Configuration

### Adding New Repositories

Create a new repo file:

```bash
cat > pkgmgr/config/repos.d/custom.repo <<EOF
[my-custom-repo]
name=My Custom Source Repository
baseurl=https://my-repo.example.com/srpms/
enabled=1
gpgcheck=0
priority=5
type=rpm-src
EOF
```

Then sync:
```bash
make pkgmgr-sync
```

### Adjusting Build Settings

Edit `pkgmgr/config/build.conf`:

```ini
[oxide]
# Increase parallel jobs
make_jobs=8

# Change compiler flags
cflags=-O3 -march=native -fPIC

# Enable ccache
[build]
use_ccache=1
```

## Next Steps

- Read the full documentation: `pkgmgr/README.md`
- Check configuration: `pkgmgr/config/oxdnf.conf`
- Explore examples: Build common packages and integrate with OXIDE
- Contribute: Add package overrides for OXIDE-specific patches

## Getting Help

```bash
# Show all oxdnf commands
python3 pkgmgr/bin/oxdnf --help

# Show make targets
make pkgmgr-help

# Check repository status
python3 pkgmgr/bin/oxdnf repolist
```

## Example Session

Here's a complete example session:

```bash
# Initialize
cd /path/to/oxide_os
export PATH=$PWD/toolchain/bin:$PATH

# Sync repos (first time)
make pkgmgr-sync

# Build ncurses (dependency for many packages)
make pkgmgr-build PKG=ncurses

# Build bash
make pkgmgr-build PKG=bash

# Install bash to staging
mkdir -p /tmp/oxide-staging
python3 pkgmgr/bin/oxdnf install bash --installroot=/tmp/oxide-staging

# Check what was installed
find /tmp/oxide-staging -type f

# Copy to userspace for initramfs
cp /tmp/oxide-staging/usr/bin/bash userspace/shell/bash

# Rebuild initramfs with new bash
make initramfs

# Test in QEMU
make run
```

That's it! You're now ready to build and manage packages for OXIDE OS.

---
*"Building packages... one dependency at a time, because every cyberpunk OS needs its arsenal."* — PatchBay
