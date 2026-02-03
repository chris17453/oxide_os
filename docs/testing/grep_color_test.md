# grep Color Highlighting Test Plan

## Overview
This document describes how to test the new color highlighting feature in the grep utility.

## Feature Description
The grep utility now supports the `--color` and `--colour` options to highlight matching text in red.
- Matching text is highlighted with ANSI red color (`\x1b[31m`)
- Color codes reset to default after each match (`\x1b[0m`)
- Works with all existing grep options (-i, -n, -v, etc.)

## Test Cases

### Test 1: Basic Color Highlighting
```bash
# Create a test file
echo -e "Hello world\nThis is a test\nHello again" > /tmp/test.txt

# Run grep with color
grep --color Hello /tmp/test.txt
```
**Expected:** The word "Hello" should appear in red in both matching lines.

### Test 2: Case-Insensitive with Color
```bash
grep --color -i hello /tmp/test.txt
```
**Expected:** Both "Hello" and "hello" should be highlighted in red.

### Test 3: Multiple Matches in One Line
```bash
echo "test test test" > /tmp/multi.txt
grep --color test /tmp/multi.txt
```
**Expected:** All three occurrences of "test" should be highlighted in red.

### Test 4: Color with Line Numbers
```bash
grep --color -n Hello /tmp/test.txt
```
**Expected:** Line numbers shown, with "Hello" highlighted in red.

### Test 5: Alternative Spelling (--colour)
```bash
grep --colour Hello /tmp/test.txt
```
**Expected:** Same behavior as --color (British spelling support).

### Test 6: Without Color Flag
```bash
grep Hello /tmp/test.txt
```
**Expected:** Normal output without color codes (backward compatibility).

### Test 7: Color with Context Lines
```bash
grep --color -C 1 test /tmp/test.txt
```
**Expected:** Matching line has "test" in red, context lines are normal.

## Implementation Details

### Files Modified
- `userspace/coreutils/src/bin/grep.rs` - Main grep implementation

### Key Changes
1. Added ANSI color constants:
   - `COLOR_MATCH`: `\x1b[31m` (red)
   - `COLOR_RESET`: `\x1b[0m` (reset)

2. Added `color` field to `GrepConfig` struct

3. Implemented `print_line_with_color()` function:
   - Scans line for pattern matches
   - Wraps each match with color codes
   - Handles case-insensitive matching
   - Supports multiple matches per line

4. Updated option parsing to handle `--color` and `--colour`

5. Modified line output to use color function when enabled

## Verification in OXIDE OS

After building the OS (`make build-full`), boot the system and run:

```bash
# Create test files
echo "Pattern matching test" > /tmp/test1.txt
echo "Another pattern here" > /tmp/test2.txt

# Test basic color
grep --color pattern /tmp/test1.txt

# Test with multiple files
grep --color pattern /tmp/*.txt

# Test with line numbers
grep --color -n pattern /tmp/test1.txt
```

## Expected Behavior
- Red highlighting should be visible on ANSI-compatible terminals
- No color codes should appear when --color is not specified
- All existing grep options should continue to work with --color
