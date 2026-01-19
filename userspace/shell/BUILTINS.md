# EFFLUX Shell (esh) - Builtin Commands

This document tracks the implementation status of shell builtin commands.

## Implementation Status

### Currently Implemented

| Builtin | Status | Notes |
|---------|--------|-------|
| `.` | Done | Source/execute commands from file |
| `:` | Done | Null command (always succeeds) |
| `[` | Done | Test expression (same as `test`) |
| `alias` | Done | Define/list command aliases |
| `bg` | Stub | Job control not yet implemented |
| `builtin` | Done | Execute builtin directly |
| `cd` | Done | Change directory |
| `command` | Done | Run command bypassing aliases |
| `declare` | Done | Declare variables (simplified) |
| `echo` | Done | Print arguments |
| `eval` | Done | Evaluate arguments as shell command |
| `exec` | Done | Replace shell with command |
| `exit` | Done | Exit shell with optional code |
| `export` | Done | List/set environment variables |
| `false` | Done | Return false (exit 1) |
| `fg` | Stub | Job control not yet implemented |
| `getopts` | Stub | Not yet implemented |
| `help` | Done | Show help |
| `history` | Done | Display command history |
| `jobs` | Stub | Job control not yet implemented |
| `kill` | Done | Send signal to process |
| `let` | Done | Evaluate arithmetic expression |
| `local` | Done | Create local variable (simplified) |
| `printf` | Done | Formatted output |
| `pwd` | Done | Print working directory |
| `read` | Done | Read line from stdin into variable |
| `readonly` | Done | Mark variable as read-only (simplified) |
| `set` | Done | Set positional parameters |
| `shift` | Done | Shift positional parameters |
| `source` | Done | Execute commands from file |
| `test` | Done | Evaluate conditional expression |
| `true` | Done | Return true (exit 0) |
| `type` | Done | Show how command would be interpreted |
| `umask` | Done | Set/display file creation mask |
| `unalias` | Done | Remove aliases |
| `unset` | Done | Unset environment variables |
| `wait` | Done | Wait for background jobs |

### To Implement

#### Control Flow (requires loop/function support)
| Builtin | Priority | Description |
|---------|----------|-------------|
| `break` | High | Exit from loop |
| `continue` | High | Continue to next loop iteration |
| `return` | High | Return from function/sourced script |

#### Job Control (requires job table)
| Builtin | Priority | Description |
|---------|----------|-------------|
| `disown` | Low | Remove job from job table |
| `suspend` | Low | Suspend shell execution |

#### Directory Stack
| Builtin | Priority | Description |
|---------|----------|-------------|
| `dirs` | Low | Display directory stack |
| `popd` | Low | Pop directory from stack |
| `pushd` | Low | Push directory to stack |

#### Completion
| Builtin | Priority | Description |
|---------|----------|-------------|
| `compgen` | Low | Generate completions |
| `complete` | Low | Set completion rules |
| `compopt` | Low | Modify completion options |

#### Configuration
| Builtin | Priority | Description |
|---------|----------|-------------|
| `bind` | Low | Bind key sequences |
| `enable` | Low | Enable/disable builtins |
| `hash` | Low | Remember command locations |
| `shopt` | Medium | Set shell options |

#### History
| Builtin | Priority | Description |
|---------|----------|-------------|
| `fc` | Low | Fix/edit command history |

#### Miscellaneous
| Builtin | Priority | Description |
|---------|----------|-------------|
| `caller` | Low | Return call stack context |
| `logout` | Low | Exit login shell |
| `mapfile` | Low | Read lines into array |
| `readarray` | Low | Read lines into array (same as `mapfile`) |
| `times` | Low | Print shell timing statistics |
| `trap` | Medium | Set signal handlers |
| `typeset` | Low | Declare variable (same as `declare`) |
| `ulimit` | Low | Set resource limits |

## Test Expression Support

The `test` / `[` builtin supports:

### Unary Operators
- `-n STRING` - String is non-empty
- `-z STRING` - String is empty
- `-e FILE` - File exists
- `-a FILE` - File exists (same as -e)
- `-f FILE` - Regular file exists
- `-d FILE` - Directory exists
- `-r FILE` - File is readable
- `-w FILE` - File is writable
- `-x FILE` - File is executable
- `-s FILE` - File has size > 0
- `! EXPR` - Negate expression

### Binary Operators
- `STRING = STRING` - Strings are equal
- `STRING == STRING` - Strings are equal
- `STRING != STRING` - Strings are not equal
- `INT -eq INT` - Integers are equal
- `INT -ne INT` - Integers are not equal
- `INT -lt INT` - Less than
- `INT -le INT` - Less than or equal
- `INT -gt INT` - Greater than
- `INT -ge INT` - Greater than or equal

## Printf Format Support

The `printf` builtin supports:
- `%s` - String
- `%d` / `%i` - Decimal integer
- `%x` - Hexadecimal
- `%c` - Character
- `%%` - Literal percent
- `%n` - Newline (extension)
- `\n` - Newline escape
- `\t` - Tab escape
- `\r` - Carriage return escape
- `\\` - Literal backslash

## Let Arithmetic Support

The `let` builtin supports:
- Addition: `+`
- Subtraction: `-`
- Multiplication: `*`
- Division: `/`
- Modulo: `%`
- Assignment: `VAR=expr`

## Dependencies

Some builtins require other features:
- `bg`, `fg`, `jobs`, `disown` - Requires job control infrastructure
- `fc` - Requires history editing
- `complete`, `compgen`, `compopt` - Requires programmable completion framework
- `bind` - Requires readline-like input handling
- `trap` - Requires signal handling infrastructure
- `break`, `continue`, `return` - Requires loop/function support
