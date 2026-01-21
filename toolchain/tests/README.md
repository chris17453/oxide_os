# Toolchain Test Suite

Test the OXIDE cross-compiler toolchain to ensure all components work correctly.

## Running All Tests

```bash
make test-toolchain
```

## Individual Test Categories

### 1. Basic Compilation Test

```bash
cd examples/hello
make clean
make
```

**Expected output:**
- Binary `hello` created
- No compilation errors

### 2. Linking Test

```bash
cd examples/echo
make clean
make
```

**Expected output:**
- Binary `echo` created with argument handling

### 3. Math Library Test

```bash
cd examples/calculator
make clean
make
```

**Expected output:**
- Binary `calculator` created
- Links with math library (`-lm`)

## Manual Tests

### Test 1: Compiler Invocation

```bash
oxide-cc --version
```

**Expected:** Shows clang version and OXIDE wrapper info

### Test 2: Preprocessor

```bash
echo '#include <stdio.h>' | oxide-cpp -
```

**Expected:** Preprocessed output without errors

### Test 3: Assembly

Create `test.s`:
```asm
.global _start
_start:
    movl $60, %eax
    xorl %edi, %edi
    syscall
```

```bash
oxide-as -o test.o test.s
```

**Expected:** Object file `test.o` created

### Test 4: Linking

```bash
oxide-ld -o test test.o
```

**Expected:** Executable `test` created

### Test 5: Archive Creation

```bash
oxide-cc -c test1.c
oxide-cc -c test2.c
oxide-ar rcs libtest.a test1.o test2.o
```

**Expected:** Archive `libtest.a` created

### Test 6: Header Discovery

```bash
cat > test.c << 'EOF'
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
int main() { return 0; }
EOF

oxide-cc -c test.c
```

**Expected:** Compiles without "file not found" errors

## Verification Checklist

- [ ] `oxide-cc` is in PATH
- [ ] `oxide-cc --version` works
- [ ] `oxide-ld` is accessible
- [ ] `oxide-as` is accessible
- [ ] `oxide-ar` is accessible
- [ ] Headers in `toolchain/sysroot/include/` exist
- [ ] `liboxide_libc.a` in `toolchain/sysroot/lib/` exists
- [ ] Hello world example builds
- [ ] Echo example builds
- [ ] Calculator example builds
- [ ] Math library linking works (`-lm`)
- [ ] Multiple source files can be compiled and linked
- [ ] Static libraries can be created and linked

## Known Limitations

Current test suite limitations (to be expanded):

- No tests for C++ (not fully supported yet)
- No tests for dynamic linking (not implemented yet)
- No tests for threading (pthread not fully implemented)
- No runtime tests (requires running on OXIDE)

## Adding New Tests

To add a new test:

1. Create directory in `toolchain/examples/`
2. Add source files and Makefile
3. Update `Makefile` test-toolchain target
4. Document expected behavior

## Reporting Issues

If tests fail:

1. Check prerequisites (clang, lld installed)
2. Rebuild toolchain: `make clean-toolchain toolchain`
3. Run verbose: `OXIDE_CC_VERBOSE=1 make test-toolchain`
4. Report with full error output and system info
