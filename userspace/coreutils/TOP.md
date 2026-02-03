# TOP Utility - Process Monitor

## Overview

The `top` utility provides real-time system monitoring with an interactive interface showing:
- System uptime, load averages, and task counts
- CPU usage statistics (user, system, idle)
- Memory and swap usage
- Per-process CPU and memory usage
- Interactive sorting and filtering capabilities

## Features

### Display Modes
- **Interactive Mode**: Full-screen ncurses interface with real-time updates
- **Batch Mode**: Non-interactive output suitable for logging and scripting

### Process Information
- PID (Process ID)
- USER (Process owner)
- PR (Priority)
- NI (Nice value)
- VIRT (Virtual memory size)
- RES (Resident memory size)
- %CPU (CPU usage percentage)
- %MEM (Memory usage percentage)
- TIME+ (Cumulative CPU time)
- COMMAND (Process name)

### Sorting Options
- By PID (`N` key or `-o PID` flag)
- By CPU usage (`P` key or `-o CPU` flag) - Default
- By memory usage (`M` key or `-o MEM` flag)
- By time (`T` key or `-o TIME` flag)
- By command name (`-o COMMAND` flag)
- Reverse sort order (`R` key)

## Command-Line Flags

### Display Options
- `-b, --batch` - Batch mode (non-interactive)
- `-c, --command-line` - Show full command line
- `-d, --delay=SECS` - Delay between updates (default: 3.0)
- `-n, --iterations=N` - Number of iterations before exiting
- `-1, --single-cpu` - Show individual CPU stats

### Filtering Options
- `-i, --idle` - Don't show idle processes
- `-p, --pid=PID` - Monitor only this PID
- `-u, --user=USER` - Monitor only this user (UID)

### Sorting Options
- `-o, --sort-override=FLD` - Sort by field (PID, CPU, MEM, TIME, COMMAND)

### Threading Options
- `-H, --threads` - Show individual threads

### Security Options
- `-s, --secure-mode` - Secure mode (disable some commands)

### Other Options
- `-h, --help` - Show help
- `-v, --version` - Show version
- `-S, --cumulative` - Cumulative time mode

## Interactive Commands

### Navigation & Display
- `Space` - Force update/refresh display
- `q` - Quit

### Sorting
- `M` - Sort by memory usage
- `P` - Sort by CPU usage (default)
- `T` - Sort by time
- `N` - Sort by PID
- `R` - Reverse sort order

### Filtering
- `i` - Toggle showing idle processes
- `u` - Filter by user (prompts for UID)

### Process Control
- `k` - Kill a process (prompts for PID and signal)
- `r` - Renice a process (prompts for PID and nice value)

### Help
- `h` or `?` - Show help screen
- `d` or `s` - Set update delay
- `n` or `#` - Set number of lines to display

## Examples

### Basic Usage
```bash
# Run with default settings (interactive, 3 second updates)
top

# Batch mode with 5 second updates, 10 iterations
top -b -d 5 -n 10

# Monitor only PID 1234
top -p 1234

# Sort by memory usage
top -o MEM

# Show only running processes (no idle)
top -i

# Monitor specific user (UID 1000)
top -u 1000
```

### Batch Mode for Logging
```bash
# Take 12 snapshots, 5 seconds apart, save to file
top -b -d 5 -n 12 > system_monitor.log

# Continuous logging (redirect to file)
top -b -d 1 >> system_monitor_continuous.log &
```

### Interactive Shortcuts
```
# While running interactively:
P - Sort by CPU (default)
M - Sort by memory
T - Sort by time
N - Sort by PID
R - Reverse sort
i - Toggle idle processes
Space - Force refresh
q - Quit
```

## Implementation Details

### Data Sources
The top utility reads system information from:
- `/proc/uptime` - System uptime
- `/proc/meminfo` - Memory statistics (total, free, buffers, cached, swap)
- `/proc/loadavg` - Load averages (1, 5, 15 minutes)
- `/proc/stat` - CPU statistics
- `/proc/[pid]/stat` - Per-process statistics
- `/proc/[pid]/status` - Per-process status information

### CPU Percentage Calculation
CPU percentage is calculated based on the difference in CPU time between samples:
```
cpu_percent = (delta_time * 100) / (elapsed_time * CLK_TCK)
```
Where:
- `delta_time` = Current (user_time + system_time) - Previous (user_time + system_time)
- `elapsed_time` = Time between samples in seconds
- `CLK_TCK` = 100 (standard Linux clock ticks per second)

### Memory Percentage Calculation
Memory percentage is based on resident set size (RSS):
```
mem_percent = (rss * page_size * 100) / total_memory
```
Where:
- `rss` = Resident set size in pages
- `page_size` = 4096 bytes (4KB pages)
- `total_memory` = Total system memory in bytes

### Ncurses Interface
The interactive mode uses the ncurses library for:
- Full-screen terminal control
- Efficient screen updates (only changed content)
- Keyboard input handling
- Color support (when available)
- Window resizing

## Technical Architecture

### Data Structures
- `ProcessInfo` - Per-process metrics and metadata
- `SystemStats` - System-wide statistics
- `TopConfig` - Configuration and runtime state

### Update Cycle
1. Read system statistics from /proc
2. Read process information for each PID in /proc
3. Calculate CPU/memory percentages (delta from previous sample)
4. Sort processes according to current sort field
5. Update display (ncurses or batch output)
6. Sleep for configured delay
7. Repeat

### Memory Efficiency
- Uses fixed-size buffers for file reading
- Reuses process list between updates
- Minimal allocations per update cycle
- Stack-based data structures where possible

## Limitations

### Current Limitations
- USER field shows "root" for all processes (needs UID to username mapping)
- Thread display not yet implemented (`-H` flag accepted but ignored)
- Process control (kill, renice) not yet implemented
- No support for custom fields or column configuration
- No mouse support
- Terminal must support minimum dimensions

### Future Enhancements
- Implement UID to username mapping
- Add thread display support
- Implement interactive kill/renice
- Add custom field selection
- Add mouse support for clicking column headers
- Add search/filter capabilities
- Add process tree view
- Add per-CPU statistics view

## Comparison with Linux top

### Implemented Features
- ✓ Real-time process monitoring
- ✓ Interactive ncurses interface
- ✓ Batch mode output
- ✓ All major command-line flags
- ✓ CPU and memory statistics
- ✓ Load averages
- ✓ Process sorting (multiple fields)
- ✓ Idle process filtering
- ✓ PID filtering
- ✓ Help screen

### Not Yet Implemented
- ✗ User/group name resolution
- ✗ Thread display
- ✗ Interactive process control (kill/renice)
- ✗ Custom field selection
- ✗ Mouse support
- ✗ Process tree view
- ✗ Per-CPU display
- ✗ Color customization
- ✗ Configuration file support

## Related Utilities
- `ps` - Snapshot of current processes
- `htop` - Enhanced interactive process viewer (external)
- `free` - Memory usage display
- `uptime` - System uptime and load
- `vmstat` - Virtual memory statistics

## Author
Created for OXIDE OS as part of the coreutils package.

## License
Same as OXIDE OS (see LICENSE file in repository root)
