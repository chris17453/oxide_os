# OXIDE Package Manager (oxdnf) - System Summary

## Overview

The OXIDE Package Manager (`oxdnf`) is a complete DNF-like package management system for OXIDE OS that enables downloading Fedora source RPMs, cross-compiling them for OXIDE using the OXIDE toolchain, and maintaining a local repository of compiled packages.

## What Has Been Implemented

### Core Components

1. **Fedora Repository Integration** (`lib/fedora.py`)
   - Repository metadata fetching and parsing
   - Package searching across multiple Fedora repositories
   - SRPM downloading from Fedora mirrors
   - Support for Fedora 39, Fedora 40, and EPEL 9 source repositories

2. **RPM Handling** (`lib/rpm.py`)
   - SRPM extraction using rpm2cpio
   - RPM spec file parsing
   - Source and patch file management
   - Metadata extraction from RPM packages

3. **Build Orchestration** (`lib/builder.py`)
   - Automated SRPM build process
   - Cross-compilation environment setup
   - Support for multiple build systems:
     - Autotools (./configure, make, make install)
     - CMake
     - Meson
     - Python setuptools
     - Plain Makefiles
   - Build logging and error handling
   - Package creation in .opkg format

4. **Local Repository Management** (`lib/repository.py`)
   - Local package repository maintenance
   - Package installation and removal
   - Installed packages database
   - Package metadata management

5. **Dependency Resolution** (`lib/resolver.py`)
   - Dependency graph building
   - Topological sorting for build order
   - Circular dependency detection
   - Version constraint parsing

6. **Main CLI Tool** (`bin/oxdnf`)
   - DNF-compatible command-line interface
   - Commands: search, info, buildsrpm, install, remove, list, clean
   - Repository synchronization
   - Package information display

7. **Helper Tools**
   - `srpm-fetch`: Download Fedora SRPMs
   - `srpm-build`: Build SRPMs for OXIDE
   - `repo-sync`: Sync repository metadata

### Configuration System

1. **Main Configuration** (`config/oxdnf.conf`)
   - Cache and build directory settings
   - Toolchain and sysroot paths
   - Build options (parallel jobs, compiler flags)
   - Network settings

2. **Repository Configuration** (`config/repos.d/fedora.repo`)
   - Multiple Fedora repository definitions
   - Configurable priorities
   - Enable/disable repositories

3. **Build Configuration** (`config/build.conf`)
   - Build system detection and configuration
   - Autotools, CMake, Meson, Make settings
   - Package compression options
   - Patch management

### Package Overrides

Pre-configured build overrides for common packages:
- `bash.override`: Bash-specific build settings for OXIDE
- `vim.override`: Vim build customizations
- `coreutils.override`: GNU coreutils modifications

### Documentation

Comprehensive documentation:
- `README.md`: Complete system overview and reference
- `QUICKSTART.md`: Step-by-step getting started guide
- `INTEGRATION_EXAMPLES.md`: Real-world integration examples

### Build System Integration

Makefile targets for easy access:
- `make pkgmgr-sync`: Sync repository metadata
- `make pkgmgr-search PKG=<name>`: Search for packages
- `make pkgmgr-build PKG=<name>`: Build a package
- `make pkgmgr-install PKG=<name>`: Install a package
- `make pkgmgr-help`: Show help

## Directory Structure

```
pkgmgr/
├── README.md                 # Complete system documentation
├── QUICKSTART.md             # Getting started guide
├── INTEGRATION_EXAMPLES.md   # Integration examples
├── .gitignore               # Git ignore rules
├── bin/                     # Executable tools
│   ├── oxdnf               # Main package manager CLI
│   ├── srpm-fetch          # SRPM downloader
│   ├── srpm-build          # SRPM builder
│   └── repo-sync           # Repository sync tool
├── lib/                     # Python modules
│   ├── rpm.py              # RPM parsing and extraction
│   ├── fedora.py           # Fedora repository interaction
│   ├── builder.py          # Build orchestration
│   ├── resolver.py         # Dependency resolution
│   └── repository.py       # Local repository management
├── config/                  # Configuration files
│   ├── oxdnf.conf          # Main configuration
│   ├── build.conf          # Build system configuration
│   └── repos.d/            # Repository definitions
│       └── fedora.repo     # Fedora repositories
├── specs/                   # Build specifications
│   └── overrides/          # Package-specific overrides
│       ├── bash.override
│       ├── vim.override
│       └── coreutils.override
├── cache/                   # Build cache (gitignored)
│   ├── srpms/              # Downloaded SRPMs
│   ├── extracted/          # Extracted SRPM contents
│   └── builds/             # Build directories
└── repo/                    # Local package repository
    ├── packages/           # Compiled .opkg packages
    ├── metadata/           # Repository metadata
    └── sources/            # Downloaded SRPMs
```

## Usage Examples

### Basic Usage

```bash
# Sync repositories
make pkgmgr-sync

# Search for a package
make pkgmgr-search PKG=bash

# Build a package
make pkgmgr-build PKG=bash

# Install a package
make pkgmgr-install PKG=bash

# List packages
python3 pkgmgr/bin/oxdnf list installed
python3 pkgmgr/bin/oxdnf list available
```

### Direct CLI Usage

```bash
# Search
python3 pkgmgr/bin/oxdnf search vim

# Get package info
python3 pkgmgr/bin/oxdnf info bash

# Build from SRPM
python3 pkgmgr/bin/oxdnf buildsrpm bash

# Install with custom root
python3 pkgmgr/bin/oxdnf install bash --installroot=/tmp/staging

# Remove package
python3 pkgmgr/bin/oxdnf remove bash

# Clean cache
python3 pkgmgr/bin/oxdnf clean all
```

## Key Features

### Cross-Compilation Support

- Automatically configures OXIDE toolchain (oxide-cc, oxide-ld, etc.)
- Sets correct CFLAGS, LDFLAGS, PKG_CONFIG paths
- Handles static linking requirements
- Supports sysroot-based builds

### Build System Detection

Automatically detects and handles:
- Autotools-based projects (./configure)
- CMake projects
- Meson projects
- Python packages (setup.py)
- Plain Makefiles

### Package Format

Custom .opkg package format containing:
- metadata.json: Package metadata (name, version, dependencies, etc.)
- files.tar.xz: Compiled files and binaries
- Optional install scripts
- Dependency information

### Repository Management

- Local package repository with metadata
- Installed packages database
- Package search and information
- Dependency tracking

## Integration with OXIDE OS

### Toolchain Integration

Uses the OXIDE cross-compilation toolchain:
- `toolchain/bin/oxide-cc`: C compiler
- `toolchain/bin/oxide-c++`: C++ compiler
- `toolchain/bin/oxide-ld`: Linker
- `toolchain/bin/oxide-ar`: Archiver
- `toolchain/sysroot`: System root with headers and libraries

### Initramfs Integration

Packages can be integrated into the OXIDE initramfs:
1. Build package with oxdnf
2. Install to staging directory
3. Copy binaries to userspace/
4. Rebuild initramfs with `make initramfs`
5. Test with `make run`

## Supported Repositories

### Fedora 39 (Enabled by Default)
- Source RPMs from Fedora 39 base repository
- Updates from Fedora 39 updates repository

### Fedora 40 (Disabled by Default)
- Can be enabled in config/repos.d/fedora.repo

### EPEL 9 (Disabled by Default)
- Enterprise Linux packages
- Can be enabled for additional packages

## Limitations and Future Work

### Current Limitations

1. **Architecture**: Only x86_64 currently supported
2. **RPM Features**: Limited macro expansion in spec files
3. **Security**: No GPG signature verification yet
4. **Parallelism**: Single-threaded builds
5. **Dynamic Linking**: Limited support (focus on static linking)

### Planned Enhancements

1. **Multi-architecture**: aarch64, riscv64 support
2. **Parallel Builds**: Support for building multiple packages concurrently
3. **Binary Caching**: Cache compiled packages for faster rebuilds
4. **GPG Support**: Package signature verification
5. **Delta RPMs**: Efficient package updates
6. **Web UI**: Repository browser and package search interface
7. **Auto-updates**: Automatic security updates
8. **Dependency Graph Visualization**: Visual dependency tree
9. **Build Farm**: Distributed build system

## Testing

### Basic Functionality Test

```bash
# Test repository sync
make pkgmgr-sync

# Test search
python3 pkgmgr/bin/oxdnf search bash

# Test package info
python3 pkgmgr/bin/oxdnf info bash

# Test list
python3 pkgmgr/bin/oxdnf repolist
```

### Build Test (requires time and network)

```bash
# Build a small package (5-10 minutes)
make pkgmgr-build PKG=bash

# Check if package was created
python3 pkgmgr/bin/oxdnf list available | grep bash

# Install to test directory
mkdir -p /tmp/test-install
python3 pkgmgr/bin/oxdnf install bash --installroot=/tmp/test-install

# Verify installation
find /tmp/test-install -name bash
```

## Security Considerations

1. **Source Verification**: Currently downloads from Fedora without GPG verification (planned)
2. **Build Isolation**: Builds run with standard user permissions
3. **Package Origin**: All packages traced back to Fedora SRPMs
4. **Local Repository**: Packages stored locally for inspection

## Performance

- **Metadata Sync**: ~30-60 seconds (one-time, cached)
- **Small Package Build**: 5-10 minutes (bash, grep, sed)
- **Medium Package Build**: 10-20 minutes (vim, curl)
- **Large Package Build**: 30-60+ minutes (gcc, llvm)

## Dependencies

### Host System Requirements

- Python 3.6+
- rpm2cpio (for SRPM extraction)
- cpio (for archive extraction)
- tar, gzip, bzip2, xz (for compression)
- OXIDE toolchain (automatically configured)

### Optional

- ccache (for faster rebuilds)
- internet connection (for downloading SRPMs)

## Conclusion

The OXIDE Package Manager provides a complete solution for:
- Downloading source packages from Fedora
- Cross-compiling them for OXIDE OS
- Managing a local package repository
- Installing and tracking packages
- Integrating packages into OXIDE OS

It bridges the gap between the Fedora ecosystem and OXIDE OS, enabling rapid expansion of available software while maintaining full control over the build process.

---
*"Package management done right... one SRPM at a time."* — NeonRoot & PatchBay
