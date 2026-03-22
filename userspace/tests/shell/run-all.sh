#!/bin/esh
# — CanaryHex: master shell test runner.
# Runs all test suites and reports aggregate results.
# Exit 0 only if every suite passes.

echo "======================================="
echo "  OXIDE Shell (esh) Test Suite"
echo "======================================="
echo ""

TOTAL_PASS=0
TOTAL_FAIL=0

run_suite() {
    echo "--- Running: $1 ---"
    if [ -x "$1" ]; then
        "$1"
        if [ "$?" -ne 0 ]; then
            TOTAL_FAIL=$((TOTAL_FAIL + 1))
        else
            TOTAL_PASS=$((TOTAL_PASS + 1))
        fi
    else
        echo "SKIP: $1 not executable"
        TOTAL_FAIL=$((TOTAL_FAIL + 1))
    fi
    echo ""
}

SCRIPT_DIR="/usr/share/tests/shell"

run_suite "$SCRIPT_DIR/test-functions.sh"
run_suite "$SCRIPT_DIR/test-control-flow.sh"
run_suite "$SCRIPT_DIR/test-expansion.sh"
run_suite "$SCRIPT_DIR/test-options.sh"
run_suite "$SCRIPT_DIR/test-permissions.sh"
run_suite "$SCRIPT_DIR/test-advanced.sh"

echo "======================================="
echo "  Suites Passed: $TOTAL_PASS"
echo "  Suites Failed: $TOTAL_FAIL"
echo "======================================="

if [ "$TOTAL_FAIL" -eq 0 ]; then
    echo "ALL SUITES PASSED"
    exit 0
else
    echo "SOME SUITES FAILED"
    exit 1
fi
