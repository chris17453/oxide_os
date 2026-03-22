#!/bin/esh
# — CrashBloom: function definition and call test suite.
# Tests both `function name { ... }` and `name() { ... }` forms.
# If this breaks, your scripts are dead in the water.

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

# Test 1: basic function definition and call
greet() {
    echo "hello"
}
RESULT=$(greet)
assert_eq "$RESULT" "hello"

# Test 2: function with positional params
add_prefix() {
    echo "prefix-$1"
}
RESULT=$(add_prefix world)
assert_eq "$RESULT" "prefix-world"

# Test 3: function keyword form
function multiply {
    echo $(( $1 * $2 ))
}
RESULT=$(multiply 6 7)
assert_eq "$RESULT" "42"

# Test 4: return from function
check_even() {
    if [ $(( $1 % 2 )) -eq 0 ]; then
        return 0
    fi
    return 1
}
check_even 4
assert_eq "$?" "0"
check_even 7
assert_eq "$?" "1"

# Test 5: nested function calls
outer() {
    inner() {
        echo "deep"
    }
    inner
}
RESULT=$(outer)
assert_eq "$RESULT" "deep"

# Test 6: function overriding
myfunc() { echo "v1"; }
RESULT=$(myfunc)
assert_eq "$RESULT" "v1"
myfunc() { echo "v2"; }
RESULT=$(myfunc)
assert_eq "$RESULT" "v2"

echo "=== Function Tests ==="
echo "Passed: $PASS  Failed: $FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "ALL PASSED"
    exit 0
else
    exit 1
fi
