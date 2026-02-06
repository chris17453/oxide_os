# OXIDE Package Manager (oxdnf)

## Overview

The OXIDE Package Manager (`oxdnf`) is a DNF-like package management system for OXIDE OS. It downloads Fedora source RPMs, cross-compiles them for OXIDE OS using the OXIDE toolchain, and maintains a local repository of compiled packages.

## Architecture

```
pkgmgr/
├── bin/               # Executable tools
│   ├── oxdnf          # Main CLI tool (DNF-like interface)
│   ├── srpm-fetch     # Download Fedora SRPMs
│   ├── srpm-build     # Cross-compile SRPMs for OXIDE
│   └── repo-sync      # Sync and maintain local repo
├── lib/               # Shared libraries/modules
│   ├── rpm.py         # RPM parsing and extraction
│   ├── fedora.py      # Fedora repo interaction
│   ├── builder.py     # SRPM build orchestration
│   ├── resolver.py    # Dependency resolution
│   └── repository.py  # Local repo management
├── repo/              # Local package repository
│   ├── packages/      # Compiled .opkg packages
│   ├── metadata/      # Repository metadata
│   └── sources/       # Downloaded SRPMs
├── cache/             # Build cache and temporary files
│   ├── srpms/         # Downloaded SRPM cache
│   ├── extracted/     # Extracted SRPM contents
│   └── builds/        # Build directories
├── config/            # Configuration files
│   ├── oxdnf.conf     # Main configuration
│   ├── repos.d/       # Repository definitions
│   └── build.conf     # Build system configuration
└── specs/             # Package build specifications
    └── overrides/     # OXIDE-specific build overrides

```

## Features

### Core Capabilities
- **SRPM Download**: Fetch source RPMs from Fedora repositories
- **Cross-Compilation**: Build packages using OXIDE toolchain
- **Dependency Resolution**: Automatic dependency management
- **Local Repository**: Maintain local package cache
- **DNF-Compatible Interface**: Familiar command-line interface

### Command Reference

#### Package Management
```bash
# Search for packages
oxdnf search <package-name>

# Show package information
oxdnf info <package-name>

# Install a package
oxdnf install <package-name>

# Remove a package
oxdnf remove <package-name>

# Update packages
oxdnf update [package-name]

# List installed packages
oxdnf list installed

# List available packages
oxdnf list available
```

#### Repository Management
```bash
# Sync repository metadata
oxdnf repo-sync

# List configured repositories
oxdnf repolist

# Clean cache
oxdnf clean all
```

#### Build Operations
```bash
# Build from SRPM
oxdnf build <srpm-file>

# Build from Fedora repo
oxdnf buildsrpm <package-name>

# Show build log
oxdnf buildlog <package-name>
```

## Configuration

### Main Config (`config/oxdnf.conf`)
```ini
[main]
# Cache directory
cachedir=/var/cache/oxdnf

# Keep cache after successful install
keepcache=0

# Logging level
debuglevel=2

# Install root
installroot=/

# OXIDE-specific settings
[oxide]
# Toolchain path
toolchain_path=/path/to/oxide_os/toolchain

# Target architecture
target_arch=x86_64-oxide

# Build jobs
make_jobs=4

# Sysroot path
sysroot=/path/to/oxide_os/toolchain/sysroot
```

### Repository Config (`config/repos.d/fedora.repo`)
```ini
[fedora]
name=Fedora $releasever - Source
baseurl=https://download.fedoraproject.org/pub/fedora/linux/releases/$releasever/Everything/source/tree/
enabled=1
gpgcheck=1

[fedora-updates]
name=Fedora $releasever - Updates Source
baseurl=https://download.fedoraproject.org/pub/fedora/linux/updates/$releasever/Everything/source/tree/
enabled=1
gpgcheck=0
```

## Build System

### Cross-Compilation Process

1. **SRPM Download**: Fetch source RPM from Fedora
2. **Extraction**: Extract SRPM contents (spec, sources, patches)
3. **Spec Analysis**: Parse RPM spec file for dependencies
4. **Dependency Check**: Verify all build dependencies are available
5. **Patch Application**: Apply OXIDE-specific patches if needed
6. **Configure**: Run configure with OXIDE toolchain settings
7. **Build**: Compile using `oxide-cc` and OXIDE toolchain
8. **Package**: Create `.opkg` package with metadata
9. **Repository Update**: Add to local repository

### Build Environment

The build system sets up a clean environment for each package:

```bash
export CC="oxide-cc"
export CXX="oxide-c++"
export AR="oxide-ar"
export LD="oxide-ld"
export RANLIB="ranlib"
export CFLAGS="-O2 -I$SYSROOT/include"
export LDFLAGS="-L$SYSROOT/lib"
export PKG_CONFIG_PATH="$SYSROOT/lib/pkgconfig"
export PKG_CONFIG_SYSROOT_DIR="$SYSROOT"
```

### Build Overrides

For packages that need OXIDE-specific modifications, create override specs in `specs/overrides/`:

```bash
specs/overrides/
├── bash.override        # Bash-specific build settings
├── coreutils.override   # GNU coreutils modifications
└── vim.override         # Vim build customizations
```

## Package Format

### OXIDE Package (.opkg)

OXIDE packages are compressed archives containing:

```
package-name-version.opkg
├── metadata.json      # Package metadata
├── files.tar.xz       # Compiled files
├── install.sh         # Post-install script (optional)
└── dependencies.txt   # Runtime dependencies
```

### Metadata Format (`metadata.json`)
```json
{
  "name": "bash",
  "version": "5.2.15",
  "release": "1.oxide",
  "arch": "x86_64",
  "summary": "GNU Bourne Again shell",
  "description": "Bash is the shell...",
  "license": "GPLv3+",
  "url": "https://www.gnu.org/software/bash",
  "source": "bash-5.2.15-1.fc39.src.rpm",
  "builddate": "2024-01-15T10:30:00Z",
  "dependencies": [
    "libc",
    "ncurses >= 6.2"
  ],
  "provides": [
    "bash",
    "/bin/sh"
  ],
  "files": [
    "/bin/bash",
    "/usr/share/man/man1/bash.1.gz"
  ]
}
```

## Integration with OXIDE OS

### Installation Paths
- Binaries: `/bin`, `/usr/bin`
- Libraries: `/lib`, `/usr/lib`
- Headers: `/usr/include`
- Documentation: `/usr/share/doc`
- Man pages: `/usr/share/man`

### Toolchain Integration
oxdnf uses the OXIDE cross-compilation toolchain located at `toolchain/bin/`:
- `oxide-cc` for C compilation
- `oxide-c++` for C++ compilation
- `oxide-ld` for linking
- `oxide-ar` for archiving

### Initramfs Integration
Packages can be included in the initramfs:
```bash
# Build package
oxdnf buildsrpm coreutils

# Install to staging directory
oxdnf install --installroot=/tmp/initramfs-staging coreutils

# Rebuild initramfs
make initramfs
```

## Development

### Adding New Repositories

Create a new repo file in `config/repos.d/`:

```bash
cat > config/repos.d/myrepo.repo <<EOF
[myrepo]
name=My Custom Repository
baseurl=https://my-repo.example.com/srpms/
enabled=1
gpgcheck=0
EOF
```

### Creating Build Overrides

For packages that need custom build options:

```bash
cat > specs/overrides/mypackage.override <<EOF
# Override configure flags
CONFIGURE_FLAGS="--enable-oxide --disable-feature-x"

# Additional CFLAGS
EXTRA_CFLAGS="-DOXIDE_OS"

# Pre-build script
pre_build() {
    # Custom pre-build steps
    patch -p1 < oxide-specific.patch
}
EOF
```

### Testing Package Builds

```bash
# Test build without installing
oxdnf buildsrpm --test mypackage

# Build with verbose output
oxdnf buildsrpm -v mypackage

# Build with specific configure options
oxdnf buildsrpm --configure-opts="--enable-debug" mypackage
```

## Troubleshooting

### Common Issues

**Build fails with missing headers:**
```bash
# Check sysroot
ls toolchain/sysroot/include/

# Install missing dependency
oxdnf install <dependency>-devel
```

**Configure script fails to detect toolchain:**
```bash
# Verify toolchain is in PATH
export PATH=/path/to/oxide_os/toolchain/bin:$PATH

# Check CC is set
echo $CC  # Should show 'oxide-cc'
```

**Package has unmet dependencies:**
```bash
# Check dependencies
oxdnf info <package>

# Build dependencies first
oxdnf buildsrpm <dependency1> <dependency2>
```

## Limitations

Current limitations (to be addressed):
- Only x86_64 architecture supported
- Limited RPM spec macro expansion
- No GPG signature verification yet
- No delta RPM support
- Single-threaded builds (parallel builds coming)

## Future Enhancements

Planned features:
- Parallel package builds
- Binary package caching
- Cross-architecture builds (aarch64, riscv64)
- Integration with OXIDE OS installer
- Web-based repository browser
- Automatic security updates
- Package signing and verification

## Contributing

When adding package management features:

1. Update relevant tools in `bin/`
2. Add tests to validate functionality
3. Update this README
4. Document any new configuration options
5. Test with multiple packages

## References

- [RPM Packaging Guide](https://rpm-packaging-guide.github.io/)
- [Fedora Source RPMs](https://src.fedoraproject.org/)
- [DNF Documentation](https://dnf.readthedocs.io/)
- [OXIDE Toolchain](../toolchain/README.md)
- [OXIDE Build System](../AGENTS.md)

## License

MIT License - See LICENSE file in repository root

---
*"Because every OS needs a package manager, even if it's built from scratch."* — NeonRoot
