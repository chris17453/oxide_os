#!/bin/esh
# — VeilAudit: permission enforcement test suite.
# Verifies that scripts without executable bit are rejected,
# and that group/other permission checks work correctly.

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

# Test 1: script with +x runs successfully
echo '#!/bin/esh
echo "exec-ok"' > /tmp/perm-test-exec.sh
chmod 755 /tmp/perm-test-exec.sh
RESULT=$(/tmp/perm-test-exec.sh)
assert_eq "$RESULT" "exec-ok"

# Test 2: script without +x fails with permission denied (exit 126)
echo '#!/bin/esh
echo "should-not-run"' > /tmp/perm-test-noexec.sh
chmod 644 /tmp/perm-test-noexec.sh
/tmp/perm-test-noexec.sh 2>/dev/null
assert_eq "$?" "126"

# Test 3: nonexistent script fails with not found (exit 127)
/tmp/perm-test-nonexistent-xyzzy.sh 2>/dev/null
assert_eq "$?" "127"

# Test 4: directory is not executable (exit 126)
/tmp 2>/dev/null
assert_eq "$?" "126"

# Cleanup
rm -f /tmp/perm-test-exec.sh /tmp/perm-test-noexec.sh

echo "=== Permission Tests ==="
echo "Passed: $PASS  Failed: $FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "ALL PASSED"
    exit 0
else
    exit 1
fi
