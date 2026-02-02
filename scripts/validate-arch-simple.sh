#!/usr/bin/env bash
#
# OXIDE Multi-Architecture Build Validation (Simplified)
#
# — NeonRoot

set -e

echo "=========================================="
echo "  OXIDE Architecture Validation"
echo "=========================================="
echo ""

# Check installed targets
echo "Installed Rust targets:"
rustup target list --installed
echo ""

# Test x86_64 architecture crates
echo "[1/7] Building arch-x86_64..."
cargo build -p arch-x86_64 --target x86_64-unknown-linux-gnu --quiet && echo "  ✓ PASSED" || echo "  ✗ FAILED"

echo "[2/7] Building arch-aarch64..."
if rustup target list --installed | grep -q "aarch64-unknown-linux-gnu"; then
    cargo build -p arch-aarch64 --target aarch64-unknown-linux-gnu --quiet && echo "  ✓ PASSED" || echo "  ✗ FAILED"
else
    echo "  ⊘ SKIPPED (target not installed)"
fi

echo "[3/7] Building arch-mips64..."
if rustup target list --installed | grep -q "mips64-unknown-linux-gnu"; then
    cargo build -p arch-mips64 --target mips64-unknown-linux-gnu --quiet && echo "  ✓ PASSED" || echo "  ✗ FAILED"
else
    echo "  ⊘ SKIPPED (target not installed)"
fi

echo "[4/7] Building boot-proto..."
cargo build -p boot-proto --quiet && echo "  ✓ PASSED" || echo "  ✗ FAILED"

echo "[5/7] Building libc for x86_64..."
cargo build -p libc --target x86_64-unknown-linux-gnu --quiet && echo "  ✓ PASSED" || echo "  ✗ FAILED"

echo "[6/7] Building libc for aarch64..."
if rustup target list --installed | grep -q "aarch64-unknown-linux-gnu"; then
    cargo build -p libc --target aarch64-unknown-linux-gnu --quiet && echo "  ✓ PASSED" || echo "  ✗ FAILED"
else
    echo "  ⊘ SKIPPED (target not installed)"
fi

echo "[7/7] Building libc for mips64..."
if rustup target list --installed | grep -q "mips64-unknown-linux-gnu"; then
    cargo build -p libc --target mips64-unknown-linux-gnu --quiet && echo "  ✓ PASSED" || echo "  ✗ FAILED"
else
    echo "  ⊘ SKIPPED (target not installed)"
fi

echo ""
echo "=========================================="
echo "  Validation Complete"
echo "=========================================="
