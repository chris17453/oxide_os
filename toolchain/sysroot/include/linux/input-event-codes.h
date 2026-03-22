/* OXIDE OS linux/input-event-codes.h — Input event type and code definitions
 * — InputShade: Matches Linux evdev codes. GTK's Wayland backend uses these
 * for mouse button and key identification.
 */

#ifndef _LINUX_INPUT_EVENT_CODES_H
#define _LINUX_INPUT_EVENT_CODES_H

/* Event types */
#define EV_SYN          0x00
#define EV_KEY          0x01
#define EV_REL          0x02
#define EV_ABS          0x03
#define EV_MSC          0x04

/* Mouse buttons */
#define BTN_MISC        0x100
#define BTN_LEFT        0x110
#define BTN_RIGHT       0x111
#define BTN_MIDDLE      0x112
#define BTN_SIDE        0x113
#define BTN_EXTRA       0x114
#define BTN_FORWARD     0x115
#define BTN_BACK        0x116
#define BTN_TASK        0x117

/* Stylus buttons */
#define BTN_TOOL_PEN    0x140
#define BTN_TOOL_RUBBER 0x141
#define BTN_TOOL_BRUSH  0x142
#define BTN_TOOL_PENCIL 0x143
#define BTN_TOOL_AIRBRUSH 0x144
#define BTN_TOOL_FINGER 0x145
#define BTN_TOOL_MOUSE  0x146
#define BTN_TOOL_LENS   0x147
#define BTN_TOUCH       0x14a
#define BTN_STYLUS      0x14b
#define BTN_STYLUS2     0x14c

/* Relative axes */
#define REL_X           0x00
#define REL_Y           0x01
#define REL_WHEEL       0x08
#define REL_HWHEEL      0x06

/* Absolute axes */
#define ABS_X           0x00
#define ABS_Y           0x01
#define ABS_PRESSURE    0x18
#define ABS_TILT_X      0x1a
#define ABS_TILT_Y      0x1b

/* Key codes (partial — add more as needed) */
#define KEY_ESC         1
#define KEY_ENTER       28
#define KEY_SPACE       57
#define KEY_BACKSPACE   14
#define KEY_TAB         15

#endif /* _LINUX_INPUT_EVENT_CODES_H */
