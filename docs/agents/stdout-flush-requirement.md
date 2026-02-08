# STDOUT Flush Requirement for Interactive Prompts

**Rule**: Always call `fflush_stdout()` after printing a prompt and before calling `read()` or any blocking input operation.

## Context

OXIDE OS terminal output is buffered for performance. When a program prints a prompt (like "login: ") and immediately blocks on `read()`, the prompt remains in the buffer and is never displayed to the user. The user sees a blinking cursor but no prompt.

## Symptoms

- User sees a blinking cursor but no visible prompt
- Terminal appears to be waiting for input but user doesn't know what to enter
- Described as "screen clears and a blinking cursor. NO LOGIN"

## Root Cause

Programs like login print interactive prompts using `prints()`, which writes to stdout. The output is buffered in userspace. When the program immediately calls `read()` to wait for user input, it blocks before the buffer is flushed. The kernel scheduler switches away, and the prompt never reaches the terminal.

## Solution

Call `fflush_stdout()` immediately after printing any interactive prompt:

```rust
// WRONG - prompt not visible
prints("login: ");
let input = read_line(&mut buf, true);

// CORRECT - flush before blocking
prints("login: ");
fflush_stdout();  // — SoftGlyph: flush prompt before blocking on read
let input = read_line(&mut buf, true);
```

## Affected Programs

- login: username and password prompts
- Any interactive CLI tool that prints a prompt and waits for input
- Shell command-line prompts (if using buffered I/O)

## Technical Details

The libc implementation provides:
- `prints(s: &str)` - writes to stdout buffer
- `fflush_stdout()` - forces write syscall to flush buffer to kernel
- `read(fd, buf)` - blocks waiting for input

Without `fflush_stdout()`, the sequence is:
1. `prints("login: ")` → adds to buffer
2. `read(0, buf)` → blocks immediately, buffer not flushed
3. Scheduler switches to another task
4. User sees nothing, waits forever

With `fflush_stdout()`:
1. `prints("login: ")` → adds to buffer
2. `fflush_stdout()` → syscall writes buffer to terminal
3. `read(0, buf)` → blocks, but user can see prompt

## Detection

If a program appears to hang with a blinking cursor but no visible output:
1. Check if it prints a prompt before calling `read()` or similar blocking I/O
2. Add `fflush_stdout()` between the `prints()` and blocking call
3. Test again

— SoftGlyph: buffering breaks interactivity; flush before blocking
