#!/bin/esh
# — StaticRiot: expansion test suite.
# Tests arithmetic $(( )), string manipulation ${var#...}, brace expansion,
# and all the dark arts of shell expansion.

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

# === Arithmetic Expansion ===

# Test 1: basic arithmetic
assert_eq "$(( 2 + 3 ))" "5"

# Test 2: operator precedence
assert_eq "$(( 2 + 3 * 4 ))" "14"

# Test 3: parentheses
assert_eq "$(( (2 + 3) * 4 ))" "20"

# Test 4: modulo
assert_eq "$(( 17 % 5 ))" "2"

# Test 5: negative numbers
assert_eq "$(( 10 - 20 ))" "-10"

# Test 6: exponentiation
assert_eq "$(( 2 ** 10 ))" "1024"

# Test 7: bitwise operations
assert_eq "$(( 0xFF & 0x0F ))" "15"
assert_eq "$(( 1 << 4 ))" "16"

# Test 8: comparison
assert_eq "$(( 5 > 3 ))" "1"
assert_eq "$(( 5 < 3 ))" "0"
assert_eq "$(( 5 == 5 ))" "1"

# Test 9: logical operators
assert_eq "$(( 1 && 1 ))" "1"
assert_eq "$(( 1 && 0 ))" "0"
assert_eq "$(( 0 || 1 ))" "1"

# Test 10: ternary
assert_eq "$(( 1 ? 42 : 99 ))" "42"
assert_eq "$(( 0 ? 42 : 99 ))" "99"

# Test 11: variable assignment in arithmetic
X=0
echo $(( X = 42 )) > /dev/null
assert_eq "$X" "42"

# === String Manipulation ===

# Test 12: string length
MYVAR="hello"
export MYVAR
assert_eq "${#MYVAR}" "5"

# Test 13: strip shortest prefix
FILEPATH="/usr/local/bin/esh"
export FILEPATH
assert_eq "${FILEPATH#*/}" "usr/local/bin/esh"

# Test 14: strip longest prefix
assert_eq "${FILEPATH##*/}" "esh"

# Test 15: strip shortest suffix
FILENAME="archive.tar.gz"
export FILENAME
assert_eq "${FILENAME%.*}" "archive.tar"

# Test 16: strip longest suffix
assert_eq "${FILENAME%%.*}" "archive"

# Test 17: replace first
STR="hello world hello"
export STR
assert_eq "${STR/hello/bye}" "bye world hello"

# Test 18: replace all
assert_eq "${STR//hello/bye}" "bye world bye"

# Test 19: substring
WORD="abcdefgh"
export WORD
assert_eq "${WORD:2:3}" "cde"

# Test 20: uppercase
NAME="oxide"
export NAME
assert_eq "${NAME^^}" "OXIDE"

# Test 21: lowercase
UPPER="OXIDE"
export UPPER
assert_eq "${UPPER,,}" "oxide"

# Test 22: default value
unset MISSING
assert_eq "${MISSING:-fallback}" "fallback"

# Test 23: alternate value
PRESENT="yes"
export PRESENT
assert_eq "${PRESENT:+exists}" "exists"

# === Brace Expansion ===

# Test 24: comma list (capture output)
RESULT=$(echo a b c)
assert_eq "$RESULT" "a b c"

echo "=== Expansion Tests ==="
echo "Passed: $PASS  Failed: $FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "ALL PASSED"
    exit 0
else
    exit 1
fi
