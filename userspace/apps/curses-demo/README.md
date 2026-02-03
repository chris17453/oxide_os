# Curses VGA Demo for Oxide OS

A terminal-based demonstration showcasing the ncurses library capabilities on Oxide OS.

## Features

- **VGA-Style Box Drawing**: Classic terminal box characters using Unicode box-drawing symbols
- **Color Palette**: Full 8-color ANSI palette demonstration
- **Animated Objects**: Bouncing diamond shapes with collision detection
- **Text Effects**: Blinking, reverse video, and underlined text
- **Terminal Agnostic**: Works on any terminal that supports colors and box-drawing characters

## Building

From the repository root:

```bash
make userspace-pkg PKG=curses-demo
```

Or build all userspace packages:

```bash
make userspace
```

## Running

The demo can be run on Oxide OS directly or in the QEMU environment:

```bash
make build-full
make run
# Inside the OS, run:
curses-demo
```

The demo will run for approximately 10 seconds (200 frames at 50ms per frame), showcasing:
- A title banner in cyan
- A color palette box showing all 7 colors with bold styling
- An animation area with bouncing colored diamond objects
- A text effects demonstration area showing blinking, reverse, and underlined text
- A status bar at the bottom

## Implementation Details

- Uses the Oxide OS ncurses library for terminal control
- Implements simple physics for bouncing ball animation
- No heap allocations - everything is stack-based for safety
- Cyberpunk-themed comments following Oxide OS conventions

-- NeonVale: Terminal graphics demo - showcasing the power of text mode
