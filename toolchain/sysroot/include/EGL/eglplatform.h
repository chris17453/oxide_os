/* OXIDE OS EGL Platform Definitions
 * — NeonVale: Minimal EGL platform types for Wayland backend.
 * Based on Khronos EGL specification headers (MIT licensed).
 */

#ifndef __eglplatform_h_
#define __eglplatform_h_

#include <stdint.h>

typedef int32_t khronos_int32_t;
typedef uint32_t khronos_uint32_t;
typedef int64_t khronos_int64_t;
typedef uint64_t khronos_uint64_t;
typedef float khronos_float_t;
typedef int64_t khronos_stime_nanoseconds_t;
typedef uint64_t khronos_utime_nanoseconds_t;
typedef intptr_t khronos_intptr_t;
typedef intptr_t khronos_ssize_t;

/* EGL types for Wayland */
typedef void *EGLNativeDisplayType;
typedef void *EGLNativeWindowType;
typedef void *EGLNativePixmapType;

typedef EGLNativeDisplayType NativeDisplayType;
typedef EGLNativeWindowType NativeWindowType;
typedef EGLNativePixmapType NativePixmapType;

typedef int EGLint;

#endif /* __eglplatform_h_ */
