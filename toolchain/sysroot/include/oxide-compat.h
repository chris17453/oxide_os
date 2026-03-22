/* OXIDE OS Platform Compatibility Header
 *
 * — NeonRoot: Auto-included by oxide-cc for all compiled code.
 * Declares OXIDE platform capabilities so upstream packages detect
 * features correctly without source modification.
 *
 * OXIDE ABI v1: Linux-compatible syscalls, structs, and conventions.
 * When OXIDE ships a native ABI, this header becomes the switch point.
 */

#ifndef _OXIDE_COMPAT_H
#define _OXIDE_COMPAT_H

#ifdef __oxide__

/* OXIDE ABI v1 — Linux-compatible feature set */
#define __unix__  1
#define __ELF__   1
#define __linux__ 1

#endif /* __oxide__ */

#endif /* _OXIDE_COMPAT_H */
