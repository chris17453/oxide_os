# XARGS Usage Examples - OXIDE OS

```
╔═══════════════════════════════════════════════════════════════╗
║              XARGS - PRACTICAL USAGE EXAMPLES                 ║
║                                                                ║
║  "Command-line wizardry in the chrome depths..."             ║
║                                          -- NightDoc          ║
╚═══════════════════════════════════════════════════════════════╝
```

## Quick Start

### Basic Usage
```bash
# Echo arguments from stdin (default behavior)
echo "file1 file2 file3" | xargs

# Execute a command with arguments from stdin
echo "test.txt log.txt" | xargs cat

# List all .rs files
find . -name "*.rs" | xargs ls -l
```

## Common Patterns

### 1. File Processing

```bash
# Find and delete all .tmp files
find . -name "*.tmp" | xargs rm

# Safe deletion with null separator (handles spaces in filenames)
find . -name "*.tmp" -print0 | xargs -0 rm

# Copy multiple files to a directory
ls *.txt | xargs -I{} cp {} /backup/

# Rename files with a pattern
ls *.txt | xargs -I{} mv {} {}.backup
```

### 2. Text Processing

```bash
# Search multiple files for a pattern
find . -name "*.log" | xargs grep "ERROR"

# Count lines in multiple files
ls *.txt | xargs wc -l

# Replace text in multiple files
find . -name "*.conf" -print0 | xargs -0 sed -i 's/old/new/g'
```

### 3. Parallel Processing

```bash
# Process files in parallel (4 at a time)
find /data -name "*.dat" -print0 | xargs -0 -P4 -n1 process_file

# Compress files in parallel
ls *.txt | xargs -P8 -I{} gzip {}

# Download multiple URLs in parallel
cat urls.txt | xargs -P10 -n1 wget

# Run tests in parallel
find tests -name "test_*.rs" | xargs -P4 -I{} cargo test --bin {}
```

### 4. Batch Operations

```bash
# Process 5 arguments at a time
seq 100 | xargs -n5 echo "Batch:"

# Process one line at a time
cat file.txt | xargs -L1 echo "Line:"

# Limit command line length
find /usr/bin -type f | xargs -s1000 file
```

### 5. Interactive Operations

```bash
# Prompt before deleting each file
find . -name "*.bak" | xargs -p rm

# Show commands before executing
echo "file1 file2" | xargs -t cat

# Show execution statistics
seq 100 | xargs -P4 -n10 --verbose-stats echo
```

## Advanced Examples

### Replace Mode

```bash
# Custom placeholder
echo "apple banana cherry" | xargs -IFRUIT echo "I like FRUIT"

# Multiple occurrences
echo "file.txt" | xargs -I{} sh -c 'cat {} && echo "---" && wc -l {}'

# In commands
ls *.log | xargs -I{} sh -c 'echo "=== {} ===" && tail -n5 {}'
```

### Complex Pipelines

```bash
# Find large files and archive them
find /data -size +100M -print0 | \
  xargs -0 -P4 -I{} sh -c 'tar czf {}.tar.gz {} && rm {}'

# Process and validate
find . -name "*.json" -print0 | \
  xargs -0 -P8 -I{} sh -c 'jsonlint {} || echo "Invalid: {}"'

# Multi-stage processing
cat list.txt | \
  xargs -n1 download_file | \
  xargs -P4 -n1 process_file | \
  xargs -n10 generate_report
```

### Error Handling

```bash
# Exit on first error
seq 10 | xargs -n1 -x validate_item

# Continue on errors (default)
seq 10 | xargs -n1 process_item

# Redirect errors
seq 10 | xargs -n1 risky_command 2>errors.log
```

### Input Sources

```bash
# From file instead of stdin
xargs -a filelist.txt cat

# Stop at EOF marker
printf "one\ntwo\nEND\nthree\n" | xargs -eEND echo
# Output: one two

# Mixed with arguments
xargs -a filelist.txt -I{} echo "File: {}"
```

## Real-World Scenarios

### 1. Log Analysis
```bash
# Find errors in all log files in parallel
find /var/log -name "*.log" -print0 | \
  xargs -0 -P4 grep -i "error" | \
  sort | uniq -c | sort -rn
```

### 2. Code Search
```bash
# Find all TODO comments in source files
find . -name "*.rs" -print0 | \
  xargs -0 grep -n "TODO" | \
  xargs -L1 echo "Found:"
```

### 3. Backup Creation
```bash
# Parallel backup of important files
find /home -name "*.conf" -print0 | \
  xargs -0 -P8 -I{} sh -c 'cp {} /backup/{}.$(date +%Y%m%d)'
```

### 4. Performance Testing
```bash
# Load test with parallel requests
seq 1000 | xargs -P20 -n1 -I{} curl -s "http://api/test?id={}"
```

### 5. Batch Renaming
```bash
# Rename files with sequential numbers
ls *.jpg | sort | xargs -n1 -I{} sh -c 'i=$((i+1)); mv {} photo_$i.jpg'
```

## Tips and Tricks

### 1. Debugging Commands
```bash
# Use -t to see what will be executed
find . -name "*.tmp" | xargs -t rm

# Dry run with echo
find . -name "*.tmp" | xargs -n1 echo rm
```

### 2. Safe File Handling
```bash
# Always use -0 with find -print0 for safety
find . -name "file with spaces.txt" -print0 | xargs -0 cat

# Quote arguments properly
echo "file with spaces.txt" | xargs -I{} sh -c 'cat "{}"'
```

### 3. Optimizing Performance
```bash
# Batch small operations for efficiency
seq 10000 | xargs -n100 echo > /dev/null

# Parallel processing for I/O bound tasks
ls *.gz | xargs -P$(nproc) gunzip

# But keep serial for CPU-bound tasks on single core
ls *.txt | xargs -n1 intensive_processing
```

### 4. Combining with Other Tools
```bash
# With grep
grep -l "pattern" *.txt | xargs sed -i 's/old/new/g'

# With awk
ls -l | awk '{print $9}' | xargs -I{} echo "File: {}"

# With curl
cat urls.txt | xargs -P10 -I{} curl -O {}
```

## Common Pitfalls

### ❌ Don't Do This
```bash
# Without -0, spaces in filenames break
find . -name "*.txt" | xargs rm  # BREAKS on "my file.txt"

# Too many parallel processes can overwhelm system
seq 10000 | xargs -P1000 echo  # BAD: Too many processes
```

### ✅ Do This Instead
```bash
# Use null separator for safety
find . -name "*.txt" -print0 | xargs -0 rm

# Reasonable parallelism
seq 10000 | xargs -P8 -n100 echo  # GOOD: Reasonable batch size
```

## Cheat Sheet

| Task | Command |
|------|---------|
| Basic usage | `echo "a b c" \| xargs command` |
| Null separator | `find . -print0 \| xargs -0 command` |
| Replace mode | `... \| xargs -I{} command {}` |
| Parallel (4 jobs) | `... \| xargs -P4 command` |
| Batch of 10 | `... \| xargs -n10 command` |
| Interactive | `... \| xargs -p command` |
| Verbose | `... \| xargs -t command` |
| From file | `xargs -a file.txt command` |
| Show limits | `xargs --show-limits` |
| Statistics | `... \| xargs --verbose-stats command` |

## Getting Help

```bash
# Show help message
xargs -h
xargs --help

# Show system limits
xargs --show-limits

# Test command construction
echo "arg1 arg2" | xargs -t echo  # Shows: echo arg1 arg2
```

---

*Signed by: NightDoc - "Documentation is the bridge between confusion and clarity"*
