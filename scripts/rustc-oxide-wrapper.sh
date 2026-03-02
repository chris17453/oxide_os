#!/bin/bash
# — PulseForge: rustc wrapper that redirects sysroot for -Zbuild-std
# Intercepts `--print sysroot` so cargo finds our patched std source
# instead of the vanilla nightly toolchain source.

OXIDE_SYSROOT="${OXIDE_SYSROOT_PATH:-$(dirname "$0")/../target/oxide-sysroot}"

# Check if this is a `--print sysroot` query
for arg in "$@"; do
    if [ "$arg" = "sysroot" ]; then
        # Cargo is asking where the sysroot is — give it our custom one
        echo "$OXIDE_SYSROOT"
        exit 0
    fi
done

# For all other invocations, pass through to real rustc
exec "$@"
