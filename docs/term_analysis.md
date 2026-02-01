# OXIDE OS Terminal CSI/VT100 Support Analysis

**Date:** 2026-02-01  
**Component:** `crates/terminal/`

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Terminal Emulator Architecture                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Application Output (write syscall)                                         │
│           │                                                                 │
│           ▼                                                                 │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ Parser (parser.rs) - VT100 State Machine                            │   │
│  │   States: Ground → Escape → CsiEntry → CsiParam → CsiIntermediate   │   │
│  │           OscString, DcsEntry, DesignateG0/G1                       │   │
│  │   Output: Action::Print | Execute | CsiDispatch | EscDispatch | Osc │   │
│  └────────────────────────────┬────────────────────────────────────────┘   │
│                               │                                             │
│                               ▼                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ Handler (handler.rs) - Sequence Processing                          │   │
│  │   - CSI sequences (cursor, erase, scroll, modes, colors)            │   │
│  │   - ESC sequences (charset, cursor save/restore)                    │   │
│  │   - SGR (colors, attributes)                                        │   │
│  │   - Private modes (mouse, alt screen, bracketed paste)              │   │
│  └────────────────────────────┬────────────────────────────────────────┘   │
│                               │                                             │
│                               ▼                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ ScreenBuffer (buffer.rs) - Cell Storage                             │   │
│  │   - Primary buffer (cols × rows cells)                              │   │
│  │   - Alternate buffer (for vim, less, etc.)                          │   │
│  │   - Scrollback buffer (10,000 lines default)                        │   │
│  └────────────────────────────┬────────────────────────────────────────┘   │
│                               │                                             │
│                               ▼                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ Renderer (renderer.rs) - Framebuffer Output                         │   │
│  │   - PSF2 font rendering                                             │   │
│  │   - Double buffering                                                │   │
│  │   - Dirty region tracking                                           │   │
│  │   - Cursor drawing (block/underline/bar)                            │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Implemented Features ✅

### CSI Sequences (Control Sequence Introducer)

| Sequence | Name | Status | Notes |
|----------|------|--------|-------|
| `CSI n A` | CUU - Cursor Up | ✅ | Respects scroll region |
| `CSI n B` | CUD - Cursor Down | ✅ | Respects scroll region |
| `CSI n C` | CUF - Cursor Forward | ✅ | |
| `CSI n D` | CUB - Cursor Back | ✅ | |
| `CSI n E` | CNL - Cursor Next Line | ✅ | |
| `CSI n F` | CPL - Cursor Previous Line | ✅ | |
| `CSI n G` | CHA - Cursor Horizontal Absolute | ✅ | |
| `CSI r;c H` | CUP - Cursor Position | ✅ | 1-indexed |
| `CSI r;c f` | HVP - Horizontal Vertical Position | ✅ | Same as CUP |
| `CSI n J` | ED - Erase Display | ✅ | 0=to end, 1=to start, 2=all, 3=+scrollback |
| `CSI n K` | EL - Erase Line | ✅ | 0=to end, 1=to start, 2=all |
| `CSI n L` | IL - Insert Lines | ✅ | |
| `CSI n M` | DL - Delete Lines | ✅ | |
| `CSI n P` | DCH - Delete Character | ✅ | |
| `CSI n S` | SU - Scroll Up | ✅ | Saves to scrollback |
| `CSI n T` | SD - Scroll Down | ✅ | |
| `CSI n X` | ECH - Erase Character | ✅ | |
| `CSI n @` | ICH - Insert Character | ✅ | |
| `CSI n d` | VPA - Vertical Position Absolute | ✅ | |
| `CSI n \`` | HPA - Horizontal Position Absolute | ✅ | |
| `CSI t;b r` | DECSTBM - Set Scroll Region | ✅ | Homes cursor |
| `CSI s` | SCOSC - Save Cursor | ✅ | |
| `CSI u` | SCORC - Restore Cursor | ✅ | |
| `CSI n q` | DECSCUSR - Set Cursor Style | ✅ | Block/underline/bar |
| `CSI ... m` | SGR - Select Graphic Rendition | ✅ | Full support |
| `CSI n n` | DSR - Device Status Report | ✅ | Mode 5, 6 |
| `CSI c` | DA - Device Attributes | ✅ | Primary & Secondary |
| `CSI ? n h` | DECSET - Set Private Mode | ✅ | Many modes |
| `CSI ? n l` | DECRST - Reset Private Mode | ✅ | Many modes |

### SGR (Select Graphic Rendition)

| Code | Attribute | Status |
|------|-----------|--------|
| 0 | Reset all | ✅ |
| 1 | Bold | ✅ |
| 2 | Dim/Faint | ✅ |
| 3 | Italic | ✅ |
| 4 | Underline | ✅ |
| 5, 6 | Blink | ✅ |
| 7 | Reverse | ✅ |
| 8 | Hidden | ✅ |
| 9 | Strikethrough | ✅ |
| 21 | Double underline | ✅ (as underline) |
| 22-29 | Reset specific | ✅ |
| 30-37 | Foreground 8-color | ✅ |
| 38;5;n | Foreground 256-color | ✅ |
| 38;2;r;g;b | Foreground RGB | ✅ |
| 39 | Default foreground | ✅ |
| 40-47 | Background 8-color | ✅ |
| 48;5;n | Background 256-color | ✅ |
| 48;2;r;g;b | Background RGB | ✅ |
| 49 | Default background | ✅ |
| 90-97 | Bright foreground | ✅ |
| 100-107 | Bright background | ✅ |

### Private Modes (DEC)

| Mode | Name | Status | Notes |
|------|------|--------|-------|
| ?1 | DECCKM - Application Cursor Keys | ✅ | |
| ?6 | DECOM - Origin Mode | ✅ | |
| ?7 | DECAWM - Auto-wrap Mode | ✅ | Default on |
| ?9 | X10 Mouse | ✅ | Button press only |
| ?25 | DECTCEM - Cursor Visible | ✅ | |
| ?1000 | Normal Mouse | ✅ | Press + release |
| ?1002 | Button-event Mouse | ✅ | Motion while held |
| ?1003 | Any-event Mouse | ✅ | All motion |
| ?1004 | Focus Events | ✅ | |
| ?1005 | UTF-8 Mouse Encoding | ✅ | |
| ?1006 | SGR Mouse Encoding | ✅ | |
| ?1015 | Urxvt Mouse Encoding | ✅ | |
| ?1049 | Alt Screen Buffer | ✅ | With cursor save |
| ?2004 | Bracketed Paste | ✅ | |

### ESC Sequences

| Sequence | Name | Status |
|----------|------|--------|
| `ESC 7` | DECSC - Save Cursor | ✅ |
| `ESC 8` | DECRC - Restore Cursor | ✅ |
| `ESC D` | IND - Index | ✅ |
| `ESC E` | NEL - Next Line | ✅ |
| `ESC H` | HTS - Horizontal Tab Set | ✅ |
| `ESC M` | RI - Reverse Index | ✅ |
| `ESC c` | RIS - Reset | ✅ |
| `ESC # 8` | DECALN - Screen Alignment | ✅ |
| `ESC ( B` | G0 ASCII | ✅ |
| `ESC ( 0` | G0 DEC Graphics | ✅ |
| `ESC ( <` | G0 DEC Supplemental | ✅ |
| `ESC ( >` | G0 DEC Technical | ✅ |
| `ESC ( A` | G0 UK | ✅ |
| `ESC ) X` | G1 charsets | ✅ |

### Character Sets

| Set | Name | Status | Notes |
|-----|------|--------|-------|
| ASCII | Default | ✅ | Fast path |
| DEC Special Graphics | Box drawing | ✅ | Complete VT100 mapping |
| DEC Supplemental | Double lines | ✅ | |
| DEC Technical | Math symbols | ✅ | |
| UK | Pound sign | ✅ | |

### Control Characters (C0)

| Code | Name | Status |
|------|------|--------|
| 0x07 | BEL | ✅ (ignored) |
| 0x08 | BS - Backspace | ✅ |
| 0x09 | HT - Tab | ✅ |
| 0x0A | LF - Line Feed | ✅ |
| 0x0B | VT - Vertical Tab | ✅ (as LF) |
| 0x0C | FF - Form Feed | ✅ (as LF) |
| 0x0D | CR - Carriage Return | ✅ |
| 0x0E | SO - Shift Out (G1) | ✅ |
| 0x0F | SI - Shift In (G0) | ✅ |
| 0x18 | CAN - Cancel | ✅ |
| 0x1A | SUB - Substitute | ✅ |
| 0x1B | ESC | ✅ |

---

## Missing/Incomplete Features 🔴

### 1. OSC (Operating System Commands) - NOT IMPLEMENTED 🔴 HIGH

**Location:** `crates/terminal/src/lib.rs` line 261-263

```rust
Action::OscDispatch(_data) => {
    // OSC commands (title, colors, etc.) - mostly ignored for now
}
```

**Missing sequences:**
- `OSC 0 ; title ST` - Set window title
- `OSC 1 ; title ST` - Set icon name  
- `OSC 2 ; title ST` - Set window title
- `OSC 4 ; n ; color ST` - Set ANSI color
- `OSC 10 ; color ST` - Set foreground color
- `OSC 11 ; color ST` - Set background color
- `OSC 12 ; color ST` - Set cursor color
- `OSC 52 ; clipboard ST` - Clipboard access (security sensitive)
- `OSC 104` - Reset color palette
- `OSC 110-119` - Reset various colors

**Impact:** Programs can't set window title. Color scheme customization broken.

---

### 2. DCS (Device Control String) - NOT IMPLEMENTED 🔴 MEDIUM

**Location:** `crates/terminal/src/parser.rs` line 377-384

```rust
fn handle_dcs_entry(&mut self, byte: u8) -> Action {
    // For now, just wait for ST
    if byte == 0x1B || byte == 0x9C {
        self.reset();
    }
    Action::None
}
```

**Missing sequences:**
- `DCS $ q Pt ST` - DECRQSS (Request Status String)
- `DCS + Q Pt ST` - XTGETTCAP (Get terminfo)
- `DCS + p Pt ST` - XTSETTCAP (Set terminfo)
- Sixel graphics (`DCS Pn ; Pn q ...`)
- ReGIS graphics

**Impact:** Advanced terminal features unavailable. Sixel images won't work.

---

### 3. UTF-8 Support - INCOMPLETE 🟠 HIGH

**Location:** `crates/terminal/src/parser.rs` line 150-158

```rust
} else if byte >= 0x80 {
    // Could be UTF-8 or C1 control
    // For now, try to print extended ASCII
    if byte >= 0xC0 {
        // Start of UTF-8 sequence - simplified handling
        Action::Print(byte as char)
    } else {
        Action::None
    }
}
```

**Issues:**
- Multi-byte UTF-8 sequences not properly decoded
- Continuation bytes (0x80-0xBF) ignored
- No validation of UTF-8 sequences
- Wide characters (CJK) not handled

**Impact:** Non-ASCII text may render incorrectly or not at all.

---

### 4. Wide Characters - NOT IMPLEMENTED 🔴 MEDIUM

**Missing:**
- Double-width character support (CJK)
- `wcwidth()` equivalent for character width
- Cell merging for wide chars
- Cursor advancement by character width

**Impact:** CJK text and emoji render incorrectly (overlap or gaps).

---

### 5. Soft Reset (DECSTR) - NOT IMPLEMENTED 🟡 LOW

`CSI ! p` - Soft Terminal Reset

Should reset modes, colors, charset but not clear screen.

---

### 6. Window Manipulation - NOT IMPLEMENTED 🟡 LOW

`CSI n t` - Window operations:
- Minimize, maximize, resize
- Report window position/size
- Push/pop title

---

### 7. Tab Stops - PARTIAL 🟡 LOW

**Location:** `crates/terminal/src/handler.rs`

**Implemented:**
- `ESC H` - Set tab stop
- Tab advance

**Missing:**
- `CSI 0 g` - Clear tab at cursor
- `CSI 3 g` - Clear all tabs
- `CSI ? 5 W` - Auto tab mode

---

### 8. Printing - NOT IMPLEMENTED 🟢 LOW

- `CSI 5 i` - Start printing
- `CSI 4 i` - Stop printing  
- `CSI ? 5 i` - Auto print mode

Not relevant for OXIDE (no printer support).

---

### 9. Sixel Graphics - NOT IMPLEMENTED 🔴 MEDIUM

DCS-based inline graphics protocol. Used by:
- `img2sixel`
- libsixel apps
- Some modern terminals

---

### 10. Synchronized Output - NOT IMPLEMENTED 🟡 MEDIUM

`CSI ? 2026 h/l` - Begin/end synchronized output

Prevents tearing during rapid updates (vim, htop).

---

### 11. Cursor Report Modes - PARTIAL 🟡 LOW

**Implemented:**
- `CSI 6 n` - CPR (Cursor Position Report)
- `CSI 5 n` - DSR (Device Status)

**Missing:**
- `CSI ? 6 n` - DECXCPR (Extended CPR with page)

---

### 12. Character Protection - NOT IMPLEMENTED 🟢 LOW

- `CSI " q` - DECSCA (Select Character Protection)
- `CSI ? J` - DECSED (Selective Erase Display)
- `CSI ? K` - DECSEL (Selective Erase Line)

---

### 13. Rectangular Area Operations - NOT IMPLEMENTED 🟢 LOW

- `CSI Pt ; Pl ; Pb ; Pr ; ... $ r` - DECCARA
- `CSI Pt ; Pl ; Pb ; Pr ; ... $ t` - DECRARA
- Copy, fill, attribute operations in rectangles

---

### 14. Line Attributes - NOT IMPLEMENTED 🟡 LOW

- `ESC # 3` - Double-height top half (DECDHL)
- `ESC # 4` - Double-height bottom half
- `ESC # 5` - Single-width line (DECSWL)
- `ESC # 6` - Double-width line (DECDWL)

---

### 15. Bell (BEL) - IGNORED 🟢 LOW

**Location:** `crates/terminal/src/lib.rs` line 271-273

```rust
0x07 => {
    // BEL - Bell (ignored)
}
```

Could flash screen or trigger sound.

---

## Rendering Issues

### 1. No True Bold Font 🟡 MEDIUM

Bold is rendered by brightening color, not using bold font weight.
Need: Bold variant of PSF2 font, or synthetic bold (double-strike).

### 2. No Italic Support 🟡 LOW

Italic flag is tracked but not rendered differently.
Need: Slant transformation or italic font.

### 3. No Underline Styles 🟡 LOW

- Single underline: ✅
- Double underline: treated as single
- Curly underline: not supported
- Colored underline: not supported

### 4. Blink Not Animated 🟢 LOW

Blink flag stored but cells don't actually blink.
Need: Timer-based toggle in renderer.

---

## Terminal Response Issues

### 1. DA Reports VT220 🟡 LOW

**Location:** `crates/terminal/src/handler.rs` line 671-676

```rust
// Primary DA - report terminal capabilities
// CSI ? 6 c = VT102
// CSI ? 6 2 ; c = VT220 (more features)
// We report VT220 for better compatibility
crate::send_response(b"\x1b[?62c");
```

Reports VT220 but doesn't implement all VT220 features.
Consider: Report actual supported feature set.

### 2. No DECRQSS Support 🟡 LOW

Programs query terminal settings with `CSI $ q` but we don't respond.

---

## Compatibility Notes

### xterm Compatibility

| Feature | xterm | OXIDE |
|---------|-------|-------|
| 256 colors | ✅ | ✅ |
| True color | ✅ | ✅ |
| Mouse modes | ✅ | ✅ |
| Alt screen | ✅ | ✅ |
| Bracketed paste | ✅ | ✅ |
| OSC title | ✅ | ❌ |
| Sixel | Optional | ❌ |

### VT100 Compatibility

| Feature | VT100 | OXIDE |
|---------|-------|-------|
| Basic cursor | ✅ | ✅ |
| Scroll regions | ✅ | ✅ |
| Character sets | ✅ | ✅ |
| DEC graphics | ✅ | ✅ |
| 80/132 column | ✅ | ❌ |
| Double-height | ✅ | ❌ |

---

## Priority Fix List

| # | Feature | Severity | Effort | Impact |
|---|---------|----------|--------|--------|
| 1 | UTF-8 proper decoding | 🔴 HIGH | Medium | International text |
| 2 | OSC title/colors | 🔴 HIGH | Low | Many apps use this |
| 3 | Wide char support | 🔴 MEDIUM | High | CJK/emoji |
| 4 | Synchronized output | 🟡 MEDIUM | Low | Visual quality |
| 5 | Tab stop clearing | 🟡 LOW | Low | Some apps use |
| 6 | DCS/DECRQSS | 🟡 LOW | Medium | Terminal queries |
| 7 | Sixel graphics | 🔴 MEDIUM | High | Image display |
| 8 | Bold font | 🟡 MEDIUM | Medium | Visual quality |
| 9 | Soft reset | 🟡 LOW | Low | Completeness |
| 10 | Line attributes | 🟡 LOW | Medium | Legacy apps |

---

## Recommended Implementation Order

### Phase 1: Critical Fixes (1-2 days)
1. **Fix UTF-8 parsing** - Proper multi-byte sequence handling
2. **Implement OSC 0/2** - Window title (most commonly used)
3. **Add `CSI 0 g` / `CSI 3 g`** - Tab stop clearing

### Phase 2: Usability (1 week)
4. **Wide character support** - wcwidth + cell merging
5. **Synchronized output** - `CSI ? 2026 h/l`
6. **OSC colors** - 4, 10, 11, 12

### Phase 3: Advanced (2 weeks)
7. **DCS framework** - Parser support
8. **Sixel graphics** - Image protocol
9. **Bold/italic rendering** - Font variants

---

## Test Commands

```bash
# Test 256 colors
for i in {0..255}; do printf "\e[48;5;${i}m  "; done; echo -e "\e[0m"

# Test RGB colors
printf "\e[38;2;255;100;0mOrange text\e[0m\n"

# Test box drawing (DEC graphics)
printf "\e(0lqqqqqqqqqqqqqqqqk\e(B\n"
printf "\e(0x\e(B OXIDE Terminal \e(0x\e(B\n"
printf "\e(0mqqqqqqqqqqqqqqqqj\e(B\n"

# Test mouse (run in raw mode app)
printf "\e[?1000h"  # Enable
printf "\e[?1000l"  # Disable

# Test alt screen
printf "\e[?1049h"  # Enter
printf "\e[?1049l"  # Leave

# Test cursor position report
printf "\e[6n"  # Should receive CSI row ; col R

# Test UTF-8 (may fail)
echo "日本語 中文 한국어 🎉🚀"
```

---

## Files Reference

| File | Purpose |
|------|---------|
| `parser.rs` | VT100 state machine |
| `handler.rs` | Sequence processing, modes |
| `buffer.rs` | Screen/scrollback storage |
| `cell.rs` | Cell attributes, cursor |
| `color.rs` | Color conversion |
| `renderer.rs` | Framebuffer output |
| `lib.rs` | Public API, global state |
