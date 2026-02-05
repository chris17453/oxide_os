# VTE Parser Reset Rule

## The Bug (discovered 2026-02-05)

`top` and other ncurses applications using nodelay mode would stop accepting keyboard
input after a few keypresses, appearing to "lock up" while still running.

## Root Cause: Stale Parser State

The VTE (Virtual Terminal Emulator) parser in ncurses `wgetch()` maintains state across
calls to decode multi-byte escape sequences (arrow keys, function keys, mouse events, etc.).

When an incomplete escape sequence is received:

1. First byte is read from stdin (e.g., ESC / 0x1B)
2. Fed to VTE parser via `parser.advance(byte)`
3. Parser returns `Action::None` (needs more input)
4. `read_escape_sequence()` polls with 50ms timeout for follow-up bytes
5. **No more bytes arrive** (was a standalone ESC, not part of a sequence)
6. Timeout occurs, function returns -1
7. **Parser is left in `State::Escape`, NOT reset to `State::Ground`**

The next call to `wgetch()` feeds a fresh byte to a parser that's still mid-sequence:

```
User presses: ESC (standalone)
Parser: Ground -> Escape
Timeout -> return -1 (but parser still in Escape state!)

User presses: 'h'
Parser: (still in Escape state) + 'h' -> confused, drops or misinterprets
```

After a few such occurrences, the parser state becomes corrupted and stops accepting
valid input. The application appears frozen because `getch()` always returns -1.

## The Fix

**RULE: Always call `state.parser.reset()` before returning -1 on timeout or error.**

This resets the parser to `State::Ground` and clears all accumulated parameters,
ensuring the next `getch()` starts with a clean state.

## Affected Code Paths

All fixed in `userspace/libs/oxide-ncurses/src/input.rs`:

1. **`read_escape_sequence()` timeout** (line 206-212)
   - Poll timeout waiting for sequence completion
   - Added `state.parser.reset()` before `return -1`

2. **ESC with no follow-up** (line 116-120)
   - ESC byte received but poll shows no more data within 50ms
   - Added `state.parser.reset()` before returning 27 (raw ESC)

3. **ESC read failure** (line 122-127)
   - Poll returned ready but read() failed
   - Added `state.parser.reset()` before returning 27

4. **Escape sequence read failure** (line 217-221)
   - read() failed or too many attempts (>16) in `read_escape_sequence()`
   - Added `state.parser.reset()` before `return -1`

5. **Unexpected action type** (line 243-248)
   - Parser returned an action we don't handle
   - Added `state.parser.reset()` before `return -1`

## Why This Matters

Without parser reset:
- `top` becomes unresponsive after a few keystrokes
- Ncurses applications in nodelay mode drop or misinterpret input
- Escape sequences get mangled (arrow keys become garbage text)
- Ctrl+C may be ignored if parser is stuck

With parser reset:
- Every `getch()` call starts with clean parser state (on timeout/error paths)
- Successful sequences still work normally (parser state preserved during read)
- Applications remain responsive indefinitely

## See Also

- `docs/agents/vt-poll-drain.md` — the other half of input handling (ring buffer drain)
- `userspace/libs/vte/src/parser.rs` — VTE parser state machine
