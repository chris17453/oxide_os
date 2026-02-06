# external/ — Third-Party Source Drops

This directory contains third-party source code that gets cross-compiled for
the OXIDE target. These are **not** OXIDE-native programs — they're ported
software from the broader ecosystem.

## How It Works

1. Source trees are downloaded or checked out here
2. Build scripts in `scripts/build-*.sh` handle cross-compilation
3. Built binaries are installed into the rootfs by the Makefile
4. The cross-compiler toolchain in `toolchain/` provides the `oxide-cc` wrapper

## Contents

See `VERSIONS.md` for the full version manifest.

## Adding a New Dependency

1. Download / extract source into `external/<name>/`
2. Create `scripts/build-<name>.sh` for cross-compilation
3. Add a Makefile target that calls the build script
4. Update `VERSIONS.md` with version and source URL
5. Add any tarballs to `.gitignore` — don't commit binary blobs
