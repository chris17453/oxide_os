# Error: Boot menu displays ????? characters and scrunched layout — UTF-8 multi-byte stri

| Field | Value |
|-------|-------|
| ID | `54371fb9d24b` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T13:20:12.624050+00:00 |
| Keywords | bootloader, font-renderer, utf-8, unicode, glyph, display, boot-menu |

## Error

Boot menu displays ????? characters and scrunched layout — UTF-8 multi-byte strings in custom bitmap font renderer

## Cause

The custom bootloader font renderer (font.rs) processes bytes individually via text.as_bytes(). Unicode characters like ─ (U+2500, 3 bytes: 0xE2 0x94 0x80) get split into individual bytes, each > 127, each rendered as '?' glyph. A 32-char divider "────" becomes 96 '?' characters (768px wide), breaking the entire layout.

## Fix

Replace all UTF-8 box-drawing characters in rendered strings with ASCII equivalents or custom font glyphs (positions 16-22 in our font). Divider: [21u8; 32] (custom ─ glyph). Console borders: +==+ and |..| instead of ╔══╗. Footer: 'Up/Dn' instead of ↑↓ control characters. UTF-8 in comments is fine since they never render.
