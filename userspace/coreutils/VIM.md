# Vim - Vi Improved Text Editor for OXIDE OS

## Overview

A minimal modal text editor implementation for OXIDE OS with support for the most essential vim commands and features.

## Features

### Modes

- **Normal Mode**: Navigate and execute commands (default mode)
- **Insert Mode**: Insert and edit text
- **Command Mode**: Execute file operations and editor commands
- **Search Mode**: Search for patterns in the text

### Normal Mode Commands

#### Movement
- `h` - Move left
- `j` - Move down
- `k` - Move up
- `l` - Move right
- `w` - Move forward one word
- `b` - Move backward one word
- `0` - Move to beginning of line
- `$` - Move to end of line
- `gg` - Go to first line
- `G` - Go to last line

#### Editing
- `x` - Delete character under cursor
- `dd` - Delete (cut) current line
- `yy` - Yank (copy) current line
- `p` - Paste after cursor

#### Entering Insert Mode
- `i` - Insert before cursor
- `a` - Append after cursor
- `o` - Open new line below and insert
- `O` - Open new line above and insert
- `A` - Append at end of line

#### Search
- `/pattern` - Search forward for pattern
- `?pattern` - Search backward for pattern
- `n` - Repeat last search in same direction
- `N` - Repeat last search in opposite direction

#### Command Mode
- `:` - Enter command mode

### Insert Mode

In insert mode, you can type text normally. Special keys:
- `ESC` - Return to normal mode
- `Backspace` - Delete character before cursor
- `Enter` - Start new line

### Command Mode

- `:w` - Write (save) file
- `:q` - Quit (fails if unsaved changes)
- `:wq` - Write and quit
- `:q!` - Quit without saving
- `ESC` - Cancel and return to normal mode

## Usage

### Opening a File

```bash
vim filename.txt
```

### Creating a New File

```bash
vim newfile.txt
```

If the file doesn't exist, it will be created when you save.

### Basic Workflow

1. Start vim: `vim myfile.txt`
2. Press `i` to enter insert mode
3. Type your text
4. Press `ESC` to return to normal mode
5. Type `:wq` and press Enter to save and quit

## Limitations

- Maximum 10,000 lines per file
- Maximum 2,048 characters per line
- No undo/redo functionality
- No visual mode
- No line wrapping (long lines may not display fully)
- No syntax highlighting
- No multiple buffers/windows
- No macros or registers (except yank buffer)
- Search is case-sensitive only
- Screen size is fixed to 24 lines (no automatic terminal size detection)

## Implementation Notes

The vim implementation uses:
- Line-based buffer storage for efficient editing
- Modal architecture following vim's design principles
- ANSI escape sequences for terminal control
- Direct console I/O via `/dev/console`
- File I/O using OXIDE OS syscalls (open, read, write, close)

## Examples

### Edit a configuration file

```bash
vim /etc/config.conf
```

### Create and edit a new file

```bash
vim notes.txt
# Press 'i' to start typing
# Type your notes
# Press ESC, then :wq to save and exit
```

### Search and replace workflow

```bash
vim document.txt
# In normal mode:
# /searchterm    (search for "searchterm")
# n              (go to next match)
# i              (enter insert mode)
# [edit text]
# ESC            (back to normal mode)
# :w             (save changes)
```

## Tips

1. **Save often**: Use `:w` frequently to save your work
2. **Cancel operations**: Use `ESC` to cancel command or search mode
3. **Navigation**: Use `gg` and `G` to quickly jump to file start/end
4. **Word movement**: Use `w` and `b` for faster navigation
5. **Line editing**: Use `dd` to delete entire lines quickly

## Troubleshooting

### Vim won't quit
- Make sure you're in normal mode (press ESC)
- If you have unsaved changes, use `:wq` to save and quit or `:q!` to quit without saving

### Can't see cursor
- The cursor position is shown in the status line
- Press ESC to return to normal mode and verify position

### Text not saving
- Check that you have write permissions for the file
- Verify the filename in the status line is correct
- The status line will show "[+]" if there are unsaved changes

## Cyberpunk Comments in Code

The vim implementation includes cyberpunk-style developer comments from various personas:

- **BlackLatch**: Paranoid memory bounds checking and security
- **GraveShift**: Core systems architecture and modal editing
- **WireSaint**: Storage systems and file I/O
- **NeonRoot**: System integration and state management
- **NeonVale**: UI rendering and terminal control
- **TorqueJax**: Terminal device setup

These comments provide context about design decisions and safety considerations in the implementation.
