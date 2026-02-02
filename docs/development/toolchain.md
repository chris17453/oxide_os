# OXIDE Cross-Compiler Toolchain

The `toolchain/` directory provides a complete cross-compilation environment
for building C/C++ programs that run on OXIDE OS.

## Quick Start

```bash
make toolchain                          # Build the toolchain
export PATH=$PWD/toolchain/bin:$PATH    # Add to PATH
oxide-cc -o hello hello.c              # Compile a C program
```

## Tools Provided

| Tool | Purpose |
|------|---------|
| `oxide-cc` | C compiler (wraps host gcc with OXIDE sysroot) |
| `oxide-c++` | C++ compiler |
| `oxide-cpp` | C preprocessor |
| `oxide-as` | Assembler |
| `oxide-ld` | Linker |
| `oxide-ar` | Archiver |
| `oxide-pkg-config` | pkg-config for OXIDE libraries |

## Sysroot

The toolchain sysroot (`toolchain/sysroot/`) contains:

- `lib/liboxide_libc.a` — static libc
- `lib/libpthread.a` — POSIX threads
- `include/` — C headers

## CMake Integration

A CMake toolchain file is provided at `toolchain/cmake/oxide-toolchain.cmake`:

```bash
cmake -DCMAKE_TOOLCHAIN_FILE=$PWD/toolchain/cmake/oxide-toolchain.cmake ..
```

## Documentation

- `toolchain/README.md` — full documentation
- `toolchain/QUICKSTART.md` — getting started guide
- `toolchain/INTEGRATION.md` — CMake/build system integration
- `toolchain/SUMMARY.md` — feature summary
