# OXIDE QEMU MCP Server

MCP server for controlling QEMU to test and debug the OXIDE operating system.

## Features

- **Auto-detect environment**: Automatically detects Fedora vs RHEL and uses appropriate QEMU configuration
- **Build before run**: Automatically builds the kernel before starting QEMU (ensures no stale code)
- **Serial output capture**: Captures all serial output for inspection
- **QEMU monitor access**: Send commands to QEMU monitor (screenshots, keystrokes, etc.)
- **Synchronized with Makefile**: Uses the same QEMU arguments as `make run`

## Architecture

The MCP server launches QEMU directly (not via Makefile) because it needs to:
- Track the QEMU process (PID, status)
- Add a monitor socket for interactive commands
- Capture serial output to a file

However, it uses **the same QEMU arguments** as the Makefile to ensure consistency.
The build step calls `make create-rootfs` to use the Makefile's build logic.

## Modes

### Fedora Mode
- Uses `qemu-system-x86_64`
- Single OVMF firmware file
- TCG acceleration

### RHEL Mode
- Uses `/usr/libexec/qemu-kvm`
- Split OVMF firmware files (CODE + VARS)
- KVM acceleration

## Available Tools

| Tool | Description |
|------|-------------|
| `qemu_build` | Build kernel/bootloader (specify target) |
| `qemu_start` | Build and start QEMU (auto-detects mode) |
| `qemu_stop` | Stop running QEMU instance |
| `qemu_status` | Check if QEMU is running, get environment info |
| `qemu_serial` | Read serial output from VM |
| `qemu_screenshot` | Take screenshot of VM display |
| `qemu_sendkeys` | Send keystrokes (QEMU format) |
| `qemu_sendtext` | Send text (auto-converts to keys) |
| `qemu_command` | Send raw QEMU monitor command |

## Usage

After restarting Claude Code, the tools are available. Example workflow:

1. `qemu_start` - Builds and starts the VM
2. `qemu_serial` - Check boot output
3. `qemu_sendtext` - Type commands
4. `qemu_screenshot` - See current display
5. `qemu_stop` - Shut down VM

## Manual Testing

```bash
# Test the server directly
cd tools/qemu-mcp
node index.js
```

## Configuration

The server is configured in `.mcp.json` at the project root.
