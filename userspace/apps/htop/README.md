# OXIDE htop - System Process Monitor

A terminal-based system monitor for OXIDE OS inspired by the classic htop utility. Provides real-time monitoring of system processes, CPU usage, and memory consumption.

## Features

- **Real-time Process Monitoring**: View all running processes with their stats
- **System Information Display**: CPU, memory, and task statistics at a glance
- **Process Details**: PID, PPID, state, memory usage, and command name for each process
- **Interactive Controls**: Navigate through the process list with keyboard shortcuts
- **Color-Coded Display**: Cyberpunk-themed color scheme for easy reading
- **Live Updates**: Automatically refreshes every second

## Building

From the repository root:

```bash
make userspace-pkg PKG=htop
```

Or build all userspace packages:

```bash
make userspace
```

Or build the full system:

```bash
make build-full
```

## Running

The monitor can be run on OXIDE OS directly or in the QEMU environment:

```bash
make build-full
make run
# Inside the OS, run:
htop
```

## Controls

- **q**: Quit the monitor
- **Up Arrow / k**: Scroll up in the process list
- **Down Arrow / j**: Scroll down in the process list
- **F5 / r**: Refresh display (happens automatically every second)

## Display Layout

The htop display consists of several sections:

1. **Title Bar** (Cyan): Shows "OXIDE htop - System Monitor"
2. **Memory Info** (Green): Displays memory usage (used/total in MB)
3. **Task Count** (Yellow): Shows total tasks and running tasks
4. **Separator Line** (Blue): Visual divider
5. **Column Headers** (Magenta): Labels for PID, PPID, State, Memory, Command
6. **Process List** (White): Scrollable list of all processes
7. **Status Bar** (Cyan): Quick reference for keyboard shortcuts

## Implementation Details

- Uses the OXIDE OS ncurses library for terminal control
- Reads process information from the `/proc` filesystem
  - `/proc/meminfo` for system memory stats
  - `/proc/[pid]/status` for per-process information
- No heap allocations - uses stack-based fixed-size buffers for safety
- Supports up to 256 processes displayed at once
- Updates at 1-second intervals
- Color scheme follows OXIDE OS cyberpunk aesthetic

## Technical Notes

### /proc Filesystem Dependencies

This monitor relies on the OXIDE OS procfs implementation to provide:
- `/proc/meminfo`: System-wide memory statistics
- `/proc/[pid]/status`: Per-process status information including:
  - Name: Process command name
  - State: Process state (R=Running, S=Sleeping, Z=Zombie, etc.)
  - PPid: Parent process ID
  - VmRSS: Resident memory size in KB
  - Threads: Number of threads

### Performance Considerations

- The monitor scans PIDs from 0-1000 to discover processes
- Each refresh cycle reads multiple files from `/proc`
- Non-blocking input allows the display to update while waiting for keypress
- 1-second refresh rate balances responsiveness with system load

## Cyberpunk Code Comments

Following OXIDE OS conventions, the code includes personality-driven comments from various personas:
- **IronGhost**: Application platform & process management
- **ThreadRogue**: Runtime & process model engineering
- **NeonRoot**: System integration & cross-subsystem work
- **WireSaint**: Storage systems & filesystem interaction
- **GraveShift**: Kernel systems & timing primitives
- **NeonVale**: UI systems & windowing
- **Hexline**: Compiler & toolchain work
- **ColdCipher**: Cryptography & color schemes

-- IronGhost: Process monitor - giving users visibility into the machine's soul
