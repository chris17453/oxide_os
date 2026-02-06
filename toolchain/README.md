# OXIDE Cross-Compiler Toolchain

This directory contains the cross-compiler toolchain for building applications that run on OXIDE OS from a Linux development environment.

## Overview

The OXIDE toolchain provides a complete development environment for creating native OXIDE applications:

- **oxide-cc**: C compiler (LLVM-based)
- **oxide-ld**: Linker driver
- **oxide-as**: Assembler (x86_64 AT&T syntax)
- **oxide-ar**: Static library archiver
- **oxide-cpp**: C preprocessor
- **oxide-pkg-config**: Library discovery tool
- Sysroot with headers and libraries
- CMake toolchain file for cross-compilation

## Quick Start

### Building the Toolchain

```bash
# Build all toolchain components
make toolchain

# Install to system (optional, /usr/local/oxide by default)
sudo make install-toolchain
```

### Using the Toolchain

```bash
# Set up environment
export OXIDE_TOOLCHAIN=$(pwd)/toolchain
export PATH=$OXIDE_TOOLCHAIN/bin:$PATH

# Compile a C program
oxide-cc -o hello hello.c

# Link with libraries
oxide-cc -o app main.c -lm

# Create a static library
oxide-ar rcs libmylib.a obj1.o obj2.o
```

### Example: Hello World

```c
// hello.c
#include <stdio.h>

int main() {
    printf("Hello from OXIDE!\n");
    return 0;
}
```

```bash
oxide-cc -o hello hello.c
# Copy hello to OXIDE initramfs or filesystem
```

## Directory Structure

```
toolchain/
├── README.md              # This file
├── bin/                   # Wrapper scripts and tools
│   ├── oxide-cc           # C compiler wrapper
│   ├── oxide-c++          # C++ compiler wrapper  
│   ├── oxide-ld           # Linker driver
│   ├── oxide-cpp          # C preprocessor
│   ├── oxide-as           # Assembler (uses userspace/as)
│   ├── oxide-ar           # Archiver (uses userspace/ar)
│   └── oxide-pkg-config   # Library discovery
├── sysroot/               # Target system root
│   ├── include/           # System headers
│   │   ├── oxide/         # OXIDE-specific headers
│   │   └── sys/           # POSIX headers
│   ├── lib/               # System libraries
│   │   └── libc.a         # OXIDE libc
│   └── bin/               # Target binaries (for reference)
├── cmake/                 # CMake support files
│   └── oxide-toolchain.cmake
├── specs/                 # Compiler spec files
│   └── x86_64-oxide.specs
└── examples/              # Example applications
    ├── hello/
    ├── echo/
    └── calculator/
```

## Architecture Support

Currently supported:
- **x86_64-oxide** (primary target, custom ELF target)

The target triple `x86_64-oxide` identifies binaries as OXIDE OS native. This is used by:
- Autotools: `--host=x86_64-oxide`
- CMake: via `oxide-toolchain.cmake`
- Meson: via `oxide-cross.txt`

Future architectures (as OXIDE OS develops):
- aarch64-oxide
- riscv64-oxide

## Integration with Build Systems

### CMake

```cmake
# CMakeLists.txt
set(CMAKE_TOOLCHAIN_FILE /path/to/oxide_os/toolchain/cmake/oxide-toolchain.cmake)
project(MyApp C)
add_executable(myapp main.c)
```

### Make

```makefile
CC = oxide-cc
AR = oxide-ar
CFLAGS = -O2 -Wall

myapp: main.o utils.o
	$(CC) -o $@ $^ -lm
```

### Autotools

```bash
./configure --host=x86_64-oxide CC=oxide-cc AR=oxide-ar
make
```

## Compiler Flags

### Optimization Levels
- `-O0`: No optimization (debug)
- `-O1`: Basic optimization
- `-O2`: Recommended optimization
- `-O3`: Aggressive optimization
- `-Oz`: Optimize for size

### Debug Information
- `-g`: Include debug symbols
- `-gline-tables-only`: Minimal debug info

### Warnings
- `-Wall`: Enable common warnings
- `-Wextra`: Enable extra warnings
- `-Werror`: Treat warnings as errors

### Target Options
- `-march=x86-64`: Target architecture
- `-mtune=generic`: CPU tuning

## Library Support

### Standard C Library (libc)

OXIDE provides a POSIX-compatible libc with syscall wrappers:

- File I/O: `open`, `read`, `write`, `close`, `lseek`
- Process: `fork`, `exec`, `wait`, `exit`, `getpid`
- Memory: `malloc`, `free`, `mmap`, `munmap`
- Strings: `strlen`, `strcmp`, `memcpy`, etc.
- Math: `sin`, `cos`, `sqrt`, `pow`, etc.
- Time: `time`, `clock_gettime`, `nanosleep`

### System Libraries

Available:
- `liboxide_libc`: Full C library with POSIX APIs, memory functions, stdio, string ops

Planned:
- `libm`: Math functions (separate from libc)
- `libpthread`: POSIX threads
- `librt`: Real-time extensions

## Limitations

Current limitations (to be addressed):
- C++ standard library not yet available
- Dynamic linking not supported (static binaries only)
- No Fortran/Go/other language support yet

## Troubleshooting

### Common Issues

**Compiler not found:**
```bash
export PATH=/path/to/oxide_os/toolchain/bin:$PATH
```

**Missing headers:**
```bash
# Headers are in sysroot/include
oxide-cc -I/path/to/oxide_os/toolchain/sysroot/include ...
```

**Linker errors:**
```bash
# Make sure libc.a is built
cd /path/to/oxide_os
cargo build --package libc --target userspace/x86_64-user.json --release
```

## Development

### Building Toolchain Components

```bash
# Build C compiler wrapper
cd toolchain/bin
./build-cc.sh

# Build all tools
cd /path/to/oxide_os
make toolchain
```

### Testing

```bash
# Run toolchain tests
make test-toolchain

# Test with examples
cd toolchain/examples/hello
make
```

## Contributing

When adding toolchain features:

1. Update relevant wrapper scripts in `toolchain/bin/`
2. Add tests to `toolchain/tests/`
3. Update this README
4. Add examples if introducing new capabilities

## References

- [OXIDE ABI Specification](../docs/ABI_SPEC.md)
- [Build Plan](../docs/plan/BUILD_PLAN.md)
- [Userspace Development](../userspace/README.md)
- LLVM Documentation: https://llvm.org/docs/
- System V ABI: https://refspecs.linuxbase.org/elf/x86_64-abi-0.99.pdf

## License

MIT License - See LICENSE file in repository root
