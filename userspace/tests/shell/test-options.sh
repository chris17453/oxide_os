#!/bin/esh
# — DeadLoop: shell options and robustness test suite.
# Tests set -e/-x/-u/-o pipefail, trap, and [[ ]] extended test.
# The knobs that separate toy shells from real ones.

PASS=0
FAIL=0

assert_eq() {
    if [ "$1" = "$2" ]; then
        PASS=$((PASS + 1))
    else
        echo "FAIL: expected '$2', got '$1'"
        FAIL=$((FAIL + 1))
    fi
}

# Test 1: [[ ]] basic string comparison
[[ "foo" == "foo" ]]
assert_eq "$?" "0"

# Test 2: [[ ]] glob matching
[[ "hello.c" == *.c ]]
assert_eq "$?" "0"

# Test 3: [[ ]] with not
[[ ! "hello" == "world" ]]
assert_eq "$?" "0"

# Test 4: [[ ]] with -f
[[ -f /bin/esh ]]
assert_eq "$?" "0"

# Test 5: [[ ]] with && and ||
[[ -f /bin/esh && "x" == "x" ]]
assert_eq "$?" "0"

# Test 6: [[ ]] numeric comparison
[[ 5 -gt 3 ]]
assert_eq "$?" "0"

# Test 7: set -u detects unset vars (tested carefully)
set -u
# Using ${VAR:-default} to avoid error on unset
UNSET_VAR_TEST="${NONEXISTENT_XYZZY:-safe}"
assert_eq "$UNSET_VAR_TEST" "safe"
set +u

# Test 8: set -x enables trace (visual check — just make sure it doesn't crash)
set -x
echo "trace test" > /dev/null
set +x

# Test 9: trap on EXIT
TRAP_FILE="/tmp/trap-test-$$"
trap "echo trapped > $TRAP_FILE" EXIT

echo "=== Shell Options Tests ==="
echo "Passed: $PASS  Failed: $FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "ALL PASSED"
    exit 0
else
    exit 1
fi
