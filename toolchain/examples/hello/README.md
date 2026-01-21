# Hello World Example

Simple hello world program for OXIDE OS.

## Building

```bash
make
```

## Running on OXIDE

```bash
# Copy to OXIDE filesystem
cp hello /path/to/oxide/initramfs/bin/

# Or add to initramfs before building
# Then rebuild OXIDE: make build-full run
```

## Source

See `hello.c` for the source code. This demonstrates:
- Basic C program structure
- Using printf for output
- Returning an exit code
