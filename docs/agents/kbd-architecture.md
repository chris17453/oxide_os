# Keyboard Input Architecture (kbd module)

## Rule
ALL keyboard drivers MUST use `input::kbd::process_key_event(keycode, pressed)` for
console/VT conversion. Drivers MUST NOT implement their own modifier tracking, Ctrl
code generation, ANSI escape sequence emission, or VT switching logic.

## Architecture (like Linux's kbd.c)

```
Hardware → Driver → input::report_key()          (evdev path → /dev/input/eventN)
                  → input::kbd::process_key_event() (console path → /dev/ttyN)
                      ├── Modifier tracking (shift/ctrl/alt/capslock/numlock)
                      ├── Ctrl+A-Z → control codes (0x01-0x1A)
                      ├── Special keys → ANSI escape sequences
                      ├── Numpad → navigation or numeric mode
                      ├── Alt+F1-F6 → VT switching
                      └── Keymap lookup → character push to VT
```

## Files
- **`kernel/input/input/src/kbd.rs`** — shared keyboard→console handler
- **`kernel/input/input/src/layouts.rs`** — keyboard layouts (US/UK/DE/FR)
- **`kernel/input/input/src/keymap.rs`** — keycode→char conversion

## API
```rust
// Main entry point — called by every keyboard driver
input::kbd::process_key_event(keycode: u16, pressed: bool) -> KeyAction

// Callbacks — registered once during kernel init
input::kbd::set_console_callback(fn(&[u8]))    // pushes bytes to VT
input::kbd::set_vt_switch_callback(fn(usize))  // switches VT on Alt+Fn

// Query modifier state
input::kbd::shift_pressed() -> bool
input::kbd::ctrl_pressed() -> bool
input::kbd::alt_pressed() -> bool
input::kbd::led_state() -> LedState
```

## KeyAction return value
- `KeyAction::None` — no hardware action needed
- `KeyAction::LedChange(LedState)` — lock key toggled, driver should update hardware LEDs

## Driver responsibilities
1. Translate hardware-specific events to Linux keycodes (PS/2: scancode→keycode via Keymap)
2. Call `input::report_key()` for evdev consumers
3. Call `input::kbd::process_key_event()` for console consumers
4. Handle `KeyAction::LedChange` by updating hardware LEDs (PS/2 0xED command, VirtIO status queue)

## Keyboard layout config
- **`/etc/vconsole.conf`** — `KEYMAP=us` (read by init on boot)
- **`loadkeys`** command — runtime layout switching via `SYS_SETKEYMAP` syscall
- Layouts defined in `kernel/input/input/src/layouts.rs`
