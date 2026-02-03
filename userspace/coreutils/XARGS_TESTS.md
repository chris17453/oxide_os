# XARGS Test Plan - OXIDE OS

```
╔═══════════════════════════════════════════════════════════════╗
║          XARGS v2.0 - COMPREHENSIVE TEST SUITE               ║
║                                                                ║
║  "Testing the parallel execution pipelines in the chrome     ║
║   depths of the data flow..."                                ║
║                                  -- CrashBloom & StaticRiot  ║
╚═══════════════════════════════════════════════════════════════╝
```

## Feature Matrix

| Feature | Flag | Status | Priority |
|---------|------|--------|----------|
| Basic execution | none | ✅ DONE | P0 |
| Null-separated input | `-0, --null` | ✅ DONE | P0 |
| Custom delimiter | `-d, --delimiter=DELIM` | ✅ DONE | P1 |
| Replace string | `-I, --replace` | ✅ DONE | P0 |
| Insert mode | `-i, --replace-i` | ✅ DONE | P1 |
| Max arguments | `-n, --max-args` | ✅ DONE | P0 |
| Max lines | `-L, --max-lines` | ✅ DONE | P1 |
| Max chars | `-s, --max-chars` | ✅ DONE | P1 |
| Parallel execution | `-P, --max-procs` | ✅ DONE | P0 |
| Exit on error | `-x, --exit` | ✅ DONE | P1 |
| No run if empty | `-r, --no-run-if-empty` | ✅ DONE | P1 |
| Verbose mode | `-t, --verbose` | ✅ DONE | P1 |
| Interactive mode | `-p, --interactive` | ✅ DONE | P2 |
| Open TTY | `-o, --open-tty` | ✅ DONE | P2 |
| EOF string | `-e, --eof` | ✅ DONE | P2 |
| Arg file | `-a, --arg-file` | ✅ DONE | P1 |
| Show limits | `--show-limits` | ✅ DONE | P2 |
| Statistics | `--verbose-stats` | ✅ DONE | P2 |

## Test Categories

### 1. Basic Functionality Tests

#### Test 1.1: Default echo command
```bash
echo "one two three" | xargs
# Expected: one two three
```

#### Test 1.2: Simple command execution
```bash
echo "file1 file2 file3" | xargs ls
# Expected: executes ls file1 file2 file3
```

#### Test 1.3: Empty input with default behavior
```bash
echo -n "" | xargs echo test
# Expected: test (runs once with no arguments)
```

#### Test 1.4: Empty input with -r flag
```bash
echo -n "" | xargs -r echo test
# Expected: (no output, doesn't run)
```

### 2. Delimiter and Input Parsing Tests

#### Test 2.1: Null-separated input
```bash
printf "one\0two\0three\0" | xargs -0 echo
# Expected: one two three
```

#### Test 2.2: Custom delimiter (colon)
```bash
echo "one:two:three" | xargs -d: echo
# Expected: one two three
```

#### Test 2.3: Newline handling
```bash
printf "one\ntwo\nthree\n" | xargs echo
# Expected: one two three
```

#### Test 2.4: Mixed whitespace
```bash
echo "  one  	two   three  " | xargs echo
# Expected: one two three
```

### 3. Argument Batching Tests

#### Test 3.1: Max arguments (-n)
```bash
echo "1 2 3 4 5 6" | xargs -n2 echo
# Expected:
# 1 2
# 3 4
# 5 6
```

#### Test 3.2: Max lines (-L)
```bash
printf "line1 a\nline2 b\nline3 c\n" | xargs -L1 echo
# Expected:
# line1 a
# line2 b
# line3 c
```

#### Test 3.3: Max chars limit (-s)
```bash
echo "a b c d e f g h" | xargs -s20 echo
# Expected: Multiple invocations to stay under 20 chars
```

### 4. Replace String Tests

#### Test 4.1: Basic replace (-I)
```bash
echo "file1 file2 file3" | xargs -I{} echo "Processing: {}"
# Expected:
# Processing: file1
# Processing: file2
# Processing: file3
```

#### Test 4.2: Custom replace string
```bash
echo "1 2 3" | xargs -IFILE echo "Number: FILE"
# Expected:
# Number: 1
# Number: 2
# Number: 3
```

#### Test 4.3: Insert mode (-i is same as -I{})
```bash
echo "a b c" | xargs -i echo "Item: {}"
# Expected:
# Item: a
# Item: b
# Item: c
```

#### Test 4.4: Replace in middle of command
```bash
echo "log.txt config.txt" | xargs -I{} sh -c 'cat {} | wc -l'
# Expected: Line counts for each file
```

### 5. Parallel Execution Tests

#### Test 5.1: Sequential execution (default)
```bash
seq 5 | xargs -n1 sh -c 'echo "Start $1"; sleep 1; echo "End $1"'
# Expected: Sequential execution (total ~5 seconds)
```

#### Test 5.2: Parallel execution with -P2
```bash
seq 4 | xargs -P2 -n1 sh -c 'echo "Start $1"; sleep 1; echo "End $1"'
# Expected: Parallel execution, 2 at a time (total ~2 seconds)
```

#### Test 5.3: Max parallel processes -P4
```bash
seq 8 | xargs -P4 -n1 sh -c 'echo "Worker $1"; sleep 1'
# Expected: 4 concurrent workers at maximum
```

#### Test 5.4: Unlimited parallel with -P0
```bash
seq 10 | xargs -P0 -n1 echo
# Expected: All executions in parallel (up to MAX_PROCS limit)
```

### 6. Error Handling Tests

#### Test 6.1: Command not found
```bash
echo "test" | xargs nonexistent_command
# Expected: Exit code 127
```

#### Test 6.2: Exit on error (-x)
```bash
seq 5 | xargs -n1 -x sh -c 'test $1 -eq 3 && exit 1 || echo $1'
# Expected: Prints 1, 2, then exits with error
```

#### Test 6.3: Continue on error (default)
```bash
seq 5 | xargs -n1 sh -c 'test $1 -eq 3 && exit 1 || echo $1'
# Expected: Prints 1, 2, 4, 5 (skips 3 but continues)
```

#### Test 6.4: Command line too long with -x
```bash
seq 1000 | xargs -s100 -x echo
# Expected: Exit 124 if line exceeds limit
```

### 7. Interactive Mode Tests

#### Test 7.1: Interactive confirmation (-p)
```bash
echo "file1 file2" | xargs -p rm
# Expected: Prompts for confirmation before each execution
# Input: y<enter> or n<enter>
```

#### Test 7.2: Verbose mode (-t)
```bash
echo "1 2 3" | xargs -t -n1 echo
# Expected: Prints commands before executing them
# echo 1
# 1
# echo 2
# 2
# echo 3
# 3
```

### 8. File Input Tests

#### Test 8.1: Read from file (-a)
```bash
# Create test file
echo -e "arg1\narg2\narg3" > /tmp/args.txt
xargs -a /tmp/args.txt echo
# Expected: arg1 arg2 arg3
```

#### Test 8.2: EOF string handling
```bash
printf "one\ntwo\nEOF\nthree\nfour\n" | xargs -eEOF echo
# Expected: one two (stops at EOF)
```

### 9. Statistics and Limits Tests

#### Test 9.1: Show system limits
```bash
xargs --show-limits
# Expected: Display of MAX_ARGS, MAX_ARG_LEN, MAX_CMD_LEN, etc.
```

#### Test 9.2: Execution statistics
```bash
seq 10 | xargs -P2 -n2 --verbose-stats echo
# Expected: Statistics report at end showing:
# - Commands executed
# - Commands failed
# - Max parallel
# - Arguments processed
# - Bytes read
```

### 10. Edge Cases and Stress Tests

#### Test 10.1: Very long argument
```bash
python3 -c 'print("x" * 4000)' | xargs echo
# Expected: Handles long arguments up to MAX_ARG_LEN
```

#### Test 10.2: Many arguments
```bash
seq 200 | xargs -n50 echo
# Expected: Batches into groups of 50
```

#### Test 10.3: Binary input with -0
```bash
find /bin -type f -print0 | xargs -0 file
# Expected: Correctly handles filenames with spaces/special chars
```

#### Test 10.4: Empty lines in input
```bash
printf "one\n\ntwo\n\nthree\n" | xargs echo
# Expected: one two three (empty lines ignored in whitespace mode)
```

#### Test 10.5: Multiple delimiters mixed
```bash
echo "a b	c d" | xargs echo  # space, tab mixed
# Expected: a b c d
```

### 11. Compatibility Tests (GNU xargs comparison)

#### Test 11.1: GNU xargs -I behavior
```bash
# Both should behave identically
echo "test" | gnu-xargs -I{} echo ">{}<"
echo "test" | xargs -I{} echo ">{}<"
# Expected: >test<
```

#### Test 11.2: GNU xargs -P behavior
```bash
# Test parallel execution matches
seq 4 | gnu-xargs -P2 -n1 echo
seq 4 | xargs -P2 -n1 echo
# Expected: Same output (possibly different order)
```

### 12. Integration Tests

#### Test 12.1: Pipeline with find
```bash
find /tmp -name "*.txt" -print0 | xargs -0 wc -l
# Expected: Line counts for all .txt files
```

#### Test 12.2: Pipeline with grep
```bash
find . -name "*.rs" | xargs grep -l "unsafe"
# Expected: List of .rs files containing "unsafe"
```

#### Test 12.3: Complex command construction
```bash
ls *.log | xargs -I{} sh -c 'echo "=== {} ==="; tail -n5 {}'
# Expected: Shows last 5 lines of each log file with header
```

#### Test 12.4: Parallel file processing
```bash
find /data -name "*.dat" -print0 | xargs -0 -P4 -n1 process_file
# Expected: Process 4 files in parallel
```

## Exit Code Testing

| Scenario | Expected Exit Code |
|----------|-------------------|
| All commands succeed | 0 |
| Child exits 1-125 | 123 |
| Command not found | 127 |
| Command cannot run | 126 |
| Child exits 255 | 125 |
| Command line too long (with -x) | 124 |
| Internal error | 255 |

## Performance Benchmarks

### Benchmark 1: Sequential vs Parallel
```bash
# Sequential (baseline)
time seq 100 | xargs -n1 sh -c 'sleep 0.01'

# Parallel -P10
time seq 100 | xargs -P10 -n1 sh -c 'sleep 0.01'

# Expected: ~10x speedup with -P10
```

### Benchmark 2: Large input handling
```bash
# Generate 10,000 arguments
time seq 10000 | xargs -n100 echo > /dev/null

# Expected: Complete in < 1 second
```

### Benchmark 3: Parallel processing throughput
```bash
# Process 1000 items with various -P values
for P in 1 2 4 8 16; do
    echo -n "P=$P: "
    time seq 1000 | xargs -P$P -n1 echo > /dev/null
done

# Expected: Increasing throughput up to CPU core count
```

## Regression Tests

### Prevent Known Issues

1. **Buffer overflow protection**: Verify MAX_ARG_LEN enforcement
2. **Process leak**: Ensure all forked processes are waited for
3. **Signal handling**: Proper cleanup on SIGTERM/SIGINT
4. **Memory safety**: No double-free or use-after-free
5. **Race conditions**: Parallel execution doesn't corrupt shared state

## Manual Testing Checklist

- [ ] Help message displays correctly (`xargs -h`)
- [ ] System limits display correctly (`xargs --show-limits`)
- [ ] Interactive mode prompts work (`xargs -p`)
- [ ] Verbose mode shows commands (`xargs -t`)
- [ ] Statistics display correctly (`xargs --verbose-stats`)
- [ ] Error messages are clear and helpful
- [ ] Parallel execution maintains correct order in logs
- [ ] TTY handling works in interactive mode
- [ ] File input reads correctly from `-a` file
- [ ] EOF string terminates input properly

## Automated Test Script

```bash
#!/bin/bash
# Run comprehensive xargs tests

PASSED=0
FAILED=0

test_case() {
    local name=$1
    local command=$2
    local expected=$3
    
    echo "Testing: $name"
    result=$(eval "$command" 2>&1)
    
    if [ "$result" == "$expected" ]; then
        echo "  ✓ PASSED"
        ((PASSED++))
    else
        echo "  ✗ FAILED"
        echo "    Expected: $expected"
        echo "    Got: $result"
        ((FAILED++))
    fi
}

# Basic tests
test_case "Default echo" "echo 'one two three' | xargs" "one two three"
test_case "Empty with -r" "echo -n '' | xargs -r echo test | wc -l" "0"
test_case "Null separator" "printf 'a\0b\0c\0' | xargs -0" "a b c"

# More test cases...

echo ""
echo "Results: $PASSED passed, $FAILED failed"
exit $FAILED
```

## Documentation Requirements

- [x] Comprehensive help text with all options
- [x] Cyberpunk-themed code comments for maintainability
- [x] Exit code documentation
- [x] Feature comparison with GNU xargs
- [x] Performance characteristics documented
- [x] Known limitations listed

## Known Limitations

1. **Max Limits**: Bounded by compile-time constants (MAX_ARGS=256, MAX_ARG_LEN=4096)
2. **Parallel Limit**: Maximum 64 parallel processes (MAX_PROCS)
3. **Command Line**: 128KB limit (MAX_CMD_LEN)
4. **EOF String**: Maximum 256 bytes (MAX_REPLACE_LEN)
5. **File Paths**: Maximum 4096 bytes (MAX_PATH_LEN)

## Future Enhancements

- [ ] Dynamic memory allocation for unlimited arguments
- [ ] Signal handling for graceful shutdown
- [ ] Process priority control (--priority)
- [ ] CPU affinity for parallel workers
- [ ] Progress bar for long-running operations
- [ ] JSON output mode for integration
- [ ] Logging to file (--log-file)
- [ ] Retry logic for failed commands
- [ ] Timeout per command (--timeout)

---

*Signed by: CrashBloom - "Testing is the proving ground where chrome meets reality"*
