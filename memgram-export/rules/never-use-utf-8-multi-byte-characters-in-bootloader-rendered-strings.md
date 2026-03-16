# NEVER use UTF-8 multi-byte characters in bootloader rendered strings

🔴 critical | ❌ dont

| Field | Value |
|-------|-------|
| ID | `60b27fe27668` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T13:20:17.993677+00:00 |
| Keywords | bootloader, font, utf-8, unicode, rendered-strings, display |

## Details

The bootloader's bitmap font renderer (font.rs draw_string/draw_char) processes raw bytes, not Unicode codepoints. Multi-byte UTF-8 characters (─ ═ ║ ╔ ╗ ╚ ╝ ↑ ↓ etc.) get split into individual bytes > 127, each rendered as '?'. This triples or quadruples the visual width and fills the screen with question marks. Use only: ASCII 32-126, or custom glyphs at positions 16-22 in FONT_DATA (►=16, ┌=17, ┐=18, └=19, ┘=20, ─=21, │=22). UTF-8 in code comments is fine since comments never render.
