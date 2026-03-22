/* OXIDE OS X11/Xlib.h — Minimal X11 type definitions for GLX header compat
 * — NeonVale: OXIDE doesn't support X11. These types exist only so that
 * libepoxy's GLX headers compile. No X11 functions are implemented.
 */

#ifndef _X11_XLIB_H
#define _X11_XLIB_H

typedef unsigned long XID;
typedef XID Window;
typedef XID Drawable;
typedef XID Pixmap;
typedef XID Colormap;
typedef XID Font;
typedef XID Cursor;
typedef struct _XDisplay Display;
typedef unsigned long VisualID;
typedef int Bool;
typedef int Status;

#define True  1
#define False 0
#define None  0L

/* Visual info (for GLX compat) */
typedef struct {
    void *visual;
    VisualID visualid;
    int screen;
    int depth;
    int c_class;
    unsigned long red_mask;
    unsigned long green_mask;
    unsigned long blue_mask;
    int colormap_size;
    int bits_per_rgb;
} XVisualInfo;

#endif /* _X11_XLIB_H */
