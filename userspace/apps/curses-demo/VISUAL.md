# Curses VGA Demo - Visual Layout

This document shows the visual layout of the curses-demo application.

## Screen Layout (80x24 terminal)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ╔═══════════════════════════════════════════════════════════════════════╗    │
│ ║  OXIDE OS - TERMINAL CURSES VGA DEMO                                  ║    │
│ ╚═══════════════════════════════════════════════════════════════════════╝    │
└──────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────┐  ┌──────────────────────────────────────┐
│     COLOR PALETTE               │  │     BOUNCING OBJECTS                 │
│                                 │  │                                      │
│ █████ (Red)                     │  │           ◆ (Red)                    │
│ █████ (Green)                   │  │                                      │
│ █████ (Yellow)                  │  │      ◆ (Green)        ◆ (Yellow)    │
│ █████ (Blue)                    │  │                                      │
│ █████ (Magenta)                 │  │                  ◆ (Blue)            │
│ █████ (Cyan)                    │  │                                      │
│ █████ (White)                   │  │                                      │
│                                 │  └──────────────────────────────────────┘
└─────────────────────────────────┘

┌─────────────────────────────────┐
│     TEXT EFFECTS                │
│                                 │
│ BLINKING TEXT (blinks)          │
│ REVERSE VIDEO (inverted)        │
│ UNDERLINED (underlined)         │
└─────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────┐
│ VGA Style Terminal Graphics Demo                                            │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Color Scheme

### Title Area (Cyan + Bold)
- Bright cyan border
- Bold text
- Unicode box-drawing characters

### Color Palette (Green)
- Green title
- 7 color samples with █ (block) characters
- Each color displayed in bold

### Animation Area (Blue)
- Blue border
- 4 bouncing diamonds (◆)
  - Red diamond moving ↗
  - Green diamond moving ↘
  - Yellow diamond moving ↖
  - Blue diamond moving ↙
- Objects bounce off walls

### Text Effects (Magenta)
- Magenta title
- Demonstrates:
  - **A_BLINK**: Text that blinks on/off
  - **A_REVERSE**: Inverted foreground/background
  - **A_UNDERLINE**: Underlined text

### Status Bar (White)
- White text on black
- Shows demo information

## Animation Behavior

The demo runs for approximately 10 seconds (200 frames):
- Frame rate: ~20 FPS (50ms per frame)
- Diamond objects update position each frame
- Collision detection against animation box boundaries
- Smooth motion with simple physics

## Terminal Requirements

- Minimum size: 80x24 characters
- Color support: 8-color ANSI (minimum)
- Unicode support: Box-drawing characters
- Terminal attributes: Bold, Blink, Reverse, Underline

## ASCII Box Drawing Characters Used

```
┌ ─ ┐   Upper corners and horizontal line
│       Vertical line  
└ ─ ┘   Lower corners and horizontal line
█       Solid block (for color bars)
◆       Diamond (for bouncing objects)
```

## Code Architecture

```
main()
  │
  ├─ initscr() - Initialize ncurses
  │
  ├─ has_colors() - Check color support
  │
  ├─ start_color() + init_pair(1-7) - Setup colors
  │
  ├─ Animation Loop (200 iterations)
  │   │
  │   ├─ Clear screen
  │   │
  │   ├─ draw_box() - Draw all borders
  │   │
  │   ├─ Draw title with attributes
  │   │
  │   ├─ Draw color palette
  │   │
  │   ├─ Ball::update() - Update positions
  │   │
  │   ├─ Ball::draw() - Draw diamonds
  │   │
  │   ├─ Draw text effects
  │   │
  │   ├─ Draw status bar
  │   │
  │   ├─ refresh() - Update screen
  │   │
  │   └─ sleep_ms(50) - Animation delay
  │
  └─ endwin() - Cleanup
```

## Example Color Output

If viewing in a terminal that supports colors:

```bash
# Run the demo
curses-demo

# You'll see:
# - Cyan borders (bright)
# - Red, green, yellow, blue, magenta, cyan, white color bars
# - Moving colored diamonds
# - Blinking, reversed, and underlined text
# - All in glorious VGA-style retro aesthetic
```

## Performance

- Binary size: 39KB (stripped)
- Memory usage: Minimal (stack-only, no heap)
- CPU usage: Low (50ms sleep between frames)
- Works on any terminal size ≥ 80x24

-- NeonVale: Retro terminal graphics brought to life
