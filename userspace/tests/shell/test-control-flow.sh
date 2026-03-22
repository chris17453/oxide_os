#!/bin/esh
# — FuzzStatic: control flow test suite.
# Tests break, continue, case/esac, pipeline negation, and brace groups.
# If these fail, you can't write a real script.

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

# Test 1: break exits loop
COUNT=0
for i in 1 2 3 4 5; do
    if [ "$i" -eq 3 ]; then
        break
    fi
    COUNT=$((COUNT + 1))
done
assert_eq "$COUNT" "2"

# Test 2: continue skips iteration
RESULT=""
for i in 1 2 3 4 5; do
    if [ "$i" -eq 3 ]; then
        continue
    fi
    RESULT="${RESULT}${i}"
done
assert_eq "$RESULT" "1245"

# Test 3: case/esac basic matching
case "hello" in
    hello) RESULT="matched";;
    *) RESULT="no match";;
esac
assert_eq "$RESULT" "matched"

# Test 4: case with glob pattern
EXT="file.c"
case "$EXT" in
    *.rs) LANG="rust";;
    *.c) LANG="c";;
    *.py) LANG="python";;
    *) LANG="unknown";;
esac
assert_eq "$LANG" "c"

# Test 5: case with pipe patterns
case "yes" in
    y|yes|Y|YES) RESULT="affirmative";;
    n|no|N|NO) RESULT="negative";;
    *) RESULT="unknown";;
esac
assert_eq "$RESULT" "affirmative"

# Test 6: pipeline negation
! false
assert_eq "$?" "0"
! true
assert_eq "$?" "1"

# Test 7: nested loops with break 2
OUTER=0
for i in 1 2 3; do
    for j in a b c; do
        if [ "$j" = "b" ]; then
            break 2
        fi
    done
    OUTER=$((OUTER + 1))
done
assert_eq "$OUTER" "0"

# Test 8: while loop with condition
N=0
while [ "$N" -lt 5 ]; do
    N=$((N + 1))
done
assert_eq "$N" "5"

# Test 9: until loop
N=10
until [ "$N" -le 0 ]; do
    N=$((N - 2))
done
assert_eq "$N" "0"

echo "=== Control Flow Tests ==="
echo "Passed: $PASS  Failed: $FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "ALL PASSED"
    exit 0
else
    exit 1
fi
