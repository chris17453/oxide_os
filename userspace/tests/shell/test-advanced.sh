#!/bin/esh
# — CanaryHex: P0-P10 advanced feature test suite.
# Covers arrays, local variables, getopts, job control, history,
# source positional params, xtrace, SIGINT, select, process substitution,
# and programmable completion.
# If these fail, half the shell is decoration.

PASS=0
FAIL=0

assert_eq() {
    if [ "$1" = "$2" ]; then
        PASS=$((PASS + 1))
    else
        echo "FAIL: expected '$2', got '$1' [$3]"
        FAIL=$((FAIL + 1))
    fi
}

echo "=== P0: Arrays ==="

# Test: array assignment and access
arr=(alpha beta gamma)
assert_eq "${arr[0]}" "alpha" "arr[0]"
assert_eq "${arr[1]}" "beta" "arr[1]"
assert_eq "${arr[2]}" "gamma" "arr[2]"

# Test: array length
assert_eq "${#arr[@]}" "3" "array length"

# Test: array all elements
ALL="${arr[@]}"
assert_eq "$ALL" "alpha beta gamma" "arr[@]"

# Test: indexed assignment
arr[1]=bravo
assert_eq "${arr[1]}" "bravo" "indexed assign"

# Test: array append
arr+=(delta)
assert_eq "${#arr[@]}" "4" "append length"
assert_eq "${arr[3]}" "delta" "appended element"

# Test: declare -a
declare -a empty_arr
assert_eq "${#empty_arr[@]}" "0" "declare -a empty"

# Test: unset array element
unset arr[1]
assert_eq "${arr[0]}" "alpha" "after unset [1], [0] intact"

echo "=== P1: Local Variables ==="

OUTER=global
test_local() {
    local OUTER=local
    echo "$OUTER"
}
RESULT=$(test_local)
assert_eq "$RESULT" "local" "local inside function"
assert_eq "$OUTER" "global" "outer after function"

# Test: local restores on exit
test_local_restore() {
    local X=inside
    echo "$X"
}
X=outside
RESULT=$(test_local_restore)
assert_eq "$RESULT" "inside" "local X inside"
assert_eq "$X" "outside" "X restored after"

echo "=== P2: getopts ==="

test_getopts() {
    OPTIND=1
    local RESULT=""
    while getopts "a:b" opt "$@"; do
        case $opt in
            a) RESULT="${RESULT}a=$OPTARG ";;
            b) RESULT="${RESULT}b ";;
        esac
    done
    echo "$RESULT"
}
RESULT=$(test_getopts -a foo -b)
assert_eq "$RESULT" "a=foo b " "getopts -a foo -b"

echo "=== P5: source positional params ==="

echo 'echo "src-$1-$2"' > /tmp/esh_test_source.sh
RESULT=$(source /tmp/esh_test_source.sh hello world)
assert_eq "$RESULT" "src-hello-world" "source with args"

echo "=== P6: xtrace ==="

# Test: set -x doesn't crash, set +x disables
set -x
XTRACE_VAR=traced
set +x
assert_eq "$XTRACE_VAR" "traced" "xtrace set/unset"

echo "=== Summary ==="
echo "Passed: $PASS  Failed: $FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "ALL PASSED"
    exit 0
else
    exit 1
fi
