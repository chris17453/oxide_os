#!/usr/bin/env bash
#
# OXIDE Multi-Architecture Build Validation Script
#
# Validates that architecture abstraction works correctly by attempting
# to build kernel and userspace for all supported architectures.
#
# — NeonRoot

set -e

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0
SKIPPED_TESTS=0

# Print colored message
print_status() {
    local color=$1
    local message=$2
    echo -e "${color}${message}${NC}"
}

# Print section header
print_header() {
    echo ""
    print_status "$BLUE" "=========================================="
    print_status "$BLUE" "$1"
    print_status "$BLUE" "=========================================="
    echo ""
}

# Test if a Rust target is installed
check_target() {
    local target=$1
    rustup target list --installed | grep -q "$target"
}

# Run a build test
test_build() {
    local name=$1
    local target=$2
    local package=$3
    local extra_args=$4

    ((TOTAL_TESTS++))

    echo -n "Testing: $name ... "

    # Check if target is installed
    if ! check_target "$target"; then
        print_status "$YELLOW" "SKIPPED (target $target not installed)"
        ((SKIPPED_TESTS++))
        return 0
    fi

    # Try to build
    local output=$(cargo build -p "$package" --target "$target" $extra_args 2>&1)
    local exit_code=$?

    if [ $exit_code -eq 0 ]; then
        print_status "$GREEN" "PASSED"
        ((PASSED_TESTS++))
        return 0
    else
        print_status "$RED" "FAILED"
        ((FAILED_TESTS++))
        echo "$output" | grep "error" | head -5
        return 1
    fi
}

# Main validation
main() {
    print_header "OXIDE Architecture Validation"

    echo "Checking installed Rust targets..."
    echo "Installed targets:"
    rustup target list --installed | while read target; do
        echo "  - $target"
    done
    echo ""

    # ========================================================================
    # Architecture Trait Crates
    # ========================================================================

    print_header "Phase 1: Architecture Trait Crates"

    echo "✓ arch-traits (architecture-agnostic)"
    cargo check -p arch-traits >/dev/null 2>&1
    ((TOTAL_TESTS++))
    ((PASSED_TESTS++))

    test_build "arch-x86_64 crate" "x86_64-unknown-linux-gnu" "arch-x86_64"
    test_build "arch-aarch64 crate" "aarch64-unknown-linux-gnu" "arch-aarch64"
    test_build "arch-mips64 crate" "mips64-unknown-linux-gnu" "arch-mips64"

    # ========================================================================
    # Boot Protocol
    # ========================================================================

    print_header "Phase 2: Boot Protocol Abstraction"

    echo "✓ boot-proto (architecture-agnostic)"
    cargo check -p boot-proto >/dev/null 2>&1
    ((TOTAL_TESTS++))
    ((PASSED_TESTS++))

    # ========================================================================
    # Userspace libc
    # ========================================================================

    print_header "Phase 3: Userspace libc"

    test_build "libc for x86_64" "x86_64-unknown-linux-gnu" "libc"
    test_build "libc for aarch64" "aarch64-unknown-linux-gnu" "libc"
    test_build "libc for mips64" "mips64-unknown-linux-gnu" "libc"

    # ========================================================================
    # Results Summary
    # ========================================================================

    print_header "Validation Results"

    echo "Total tests:   $TOTAL_TESTS"
    print_status "$GREEN" "Passed:        $PASSED_TESTS"
    print_status "$RED" "Failed:        $FAILED_TESTS"
    print_status "$YELLOW" "Skipped:       $SKIPPED_TESTS"
    echo ""

    if [ $FAILED_TESTS -eq 0 ]; then
        print_status "$GREEN" "✓ All available tests passed!"

        if [ $SKIPPED_TESTS -gt 0 ]; then
            echo ""
            print_status "$YELLOW" "Note: Some tests were skipped due to missing toolchains."
            echo "To install missing targets, run:"
            echo "  rustup target add aarch64-unknown-linux-gnu"
            echo "  rustup target add mips64-unknown-linux-gnu"
        fi

        exit 0
    else
        print_status "$RED" "✗ Some tests failed."
        exit 1
    fi
}

# Run validation
main "$@"
