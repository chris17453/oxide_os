# OXIDE Cross-Compiler Toolchain - Quick Start Guide

## Overview

This guide will help you get started with the OXIDE cross-compiler toolchain for building native applications that run on OXIDE OS.

## Prerequisites

### Required Tools

Install these on your Linux development machine:

```bash
# Ubuntu/Debian
sudo apt install clang lld make cmake

# Fedora/RHEL
sudo dnf install clang lld make cmake

# Arch Linux
sudo pacman -S clang lld make cmake
```

### Rust Toolchain

The OXIDE toolchain components (assembler, linker, archiver) are built with Rust:

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# The project uses nightly Rust (configured in rust-toolchain.toml)
```

## Building the Toolchain

```bash
# From the OXIDE OS repository root
make toolchain
```

This will:
1. Build the OXIDE assembler (`as`)
2. Build the OXIDE linker (`ld`)
3. Build the OXIDE archiver (`ar`)
4. Build OXIDE libc
5. Install libraries to the sysroot

## Setting Up Your Environment

### Temporary (Current Shell Session)

```bash
export PATH=$(pwd)/toolchain/bin:$PATH
```

### Permanent (Add to ~/.bashrc or ~/.zshrc)

```bash
export OXIDE_TOOLCHAIN=/path/to/oxide_os/toolchain
export PATH=$OXIDE_TOOLCHAIN/bin:$PATH
```

## Verifying the Installation

```bash
# Check compiler is accessible
oxide-cc --version

# Test with hello world
cd toolchain/examples/hello
make
```

## Compiling Your First Program

### Simple C Program

Create `hello.c`:

```c
#include <stdio.h>

int main() {
    printf("Hello from OXIDE!\n");
    return 0;
}
```

Compile:

```bash
oxide-cc -o hello hello.c
```

### With Optimization

```bash
oxide-cc -O2 -o hello hello.c
```

### With Debug Symbols

```bash
oxide-cc -g -o hello hello.c
```

## Linking with Libraries

### Math Library

```c
#include <stdio.h>
#include <math.h>

int main() {
    double result = sqrt(16.0);
    printf("sqrt(16) = %.2f\n", result);
    return 0;
}
```

```bash
oxide-cc -o math_example math_example.c -lm
```

## Creating Static Libraries

```bash
# Compile object files
oxide-cc -c utils.c
oxide-cc -c helpers.c

# Create archive
oxide-ar rcs libmylib.a utils.o helpers.o

# Link with your library
oxide-cc -o myapp main.c -L. -lmylib
```

## Using with Build Systems

### Make

```makefile
CC = oxide-cc
CFLAGS = -O2 -Wall
LDFLAGS = -lm

myapp: main.o utils.o
	$(CC) $(CFLAGS) -o myapp main.o utils.o $(LDFLAGS)

%.o: %.c
	$(CC) $(CFLAGS) -c $<

clean:
	rm -f *.o myapp
```

### CMake

```cmake
cmake_minimum_required(VERSION 3.15)

# Set toolchain file before project()
set(CMAKE_TOOLCHAIN_FILE ${CMAKE_SOURCE_DIR}/../../cmake/oxide-toolchain.cmake)

project(MyApp C)

add_executable(myapp main.c utils.c)
```

Build:

```bash
mkdir build
cd build
cmake ..
make
```

## Running on OXIDE

### Method 1: Add to Initramfs

1. Copy your binary to the OXIDE source tree:
   ```bash
   cp myapp /path/to/oxide_os/target/initramfs/bin/
   ```

2. Rebuild initramfs:
   ```bash
   cd /path/to/oxide_os
   make initramfs
   ```

3. Run OXIDE:
   ```bash
   make run
   ```

### Method 2: Copy to Running System

If OXIDE filesystem is mounted or accessible, copy the binary there.

## Common Compiler Flags

| Flag | Description |
|------|-------------|
| `-O0` | No optimization (fastest compile) |
| `-O1` | Basic optimization |
| `-O2` | Recommended optimization |
| `-O3` | Aggressive optimization |
| `-Os` | Optimize for size |
| `-Oz` | Optimize aggressively for size |
| `-g` | Include debug information |
| `-Wall` | Enable common warnings |
| `-Werror` | Treat warnings as errors |
| `-std=c11` | Use C11 standard |
| `-march=x86-64` | Target architecture |
| `-I<dir>` | Add include directory |
| `-L<dir>` | Add library directory |
| `-l<name>` | Link with library |

## Troubleshooting

### Problem: `clang: command not found`

**Solution:** Install LLVM/Clang:
```bash
# Ubuntu/Debian
sudo apt install clang lld

# Fedora/RHEL
sudo dnf install clang lld
```

### Problem: `error: 'stdio.h' file not found`

**Solution:** Check that headers are in the sysroot:
```bash
ls toolchain/sysroot/include/
```

If missing, rebuild toolchain:
```bash
make toolchain
```

### Problem: `undefined reference to 'printf'`

**Solution:** Ensure libc is built and in sysroot:
```bash
# Check if libc exists
ls toolchain/sysroot/lib/liboxide_libc.a

# If missing, rebuild:
make toolchain
```

### Problem: Custom headers not found

**Solution:** Add include path:
```bash
oxide-cc -I/path/to/headers -o myapp myapp.c
```

## Examples

See `toolchain/examples/` for complete working examples:

- `hello/` - Simple hello world
- `echo/` - Command-line arguments
- `calculator/` - Math operations

## Advanced Topics

### Cross-Compiling from Different Architectures

The toolchain currently targets x86_64. To cross-compile from ARM or other architectures, ensure you have the x86_64 LLVM backend installed.

### Static vs Dynamic Linking

Currently, only static linking is supported. Dynamic linking will be added in a future release.

### Custom Linker Scripts

To use a custom linker script:

```bash
oxide-cc -T custom.ld -o myapp myapp.c
```

## Getting Help

- Read `toolchain/README.md` for detailed documentation
- Check examples in `toolchain/examples/`
- Review OXIDE OS documentation in `docs/`
- Report issues on GitHub

## Next Steps

- Try the examples in `toolchain/examples/`
- Read `docs/ABI_SPEC.md` for ABI details
- Explore OXIDE libc in `userspace/libc/`
- Build your own applications!

## License

MIT - See LICENSE file in repository root
