# External Dependency Versions

| Library | Version | Source | Purpose |
|---------|---------|--------|---------|
| musl | 1.2.5 | https://musl.libc.org | Regex library extraction for libc |
| zlib | 1.3.1 | https://zlib.net | Compression (used by cpython, ssh) |
| CPython | 3.x | https://python.org | Python interpreter |
| Vim | 9.x | https://vim.org | Text editor |

## Build Scripts

| Library | Build Script |
|---------|-------------|
| musl | `scripts/build-all-libs.sh` |
| zlib | `scripts/build-zlib.sh` |
| CPython | `scripts/build-cpython.sh` |
| Vim | `scripts/build-vim.sh` |
