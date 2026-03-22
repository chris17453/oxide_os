/* OXIDE OS EGL header — minimal for compilation */
#ifndef __egl_h_
#define __egl_h_

#include <EGL/eglplatform.h>

typedef unsigned int EGLenum;
typedef void *EGLConfig;
typedef void *EGLContext;
typedef void *EGLDisplay;
typedef void *EGLSurface;
typedef void *EGLClientBuffer;
typedef void *EGLImage;
typedef void *EGLSync;
typedef uint64_t EGLTime;
typedef void (*__eglMustCastToProperFunctionPointerType)(void);

#define EGL_SUCCESS             0x3000
#define EGL_NOT_INITIALIZED     0x3001
#define EGL_FALSE               0
#define EGL_TRUE                1
#define EGL_NO_CONTEXT          ((EGLContext)0)
#define EGL_NO_DISPLAY          ((EGLDisplay)0)
#define EGL_NO_SURFACE          ((EGLSurface)0)
#define EGL_DEFAULT_DISPLAY     ((EGLNativeDisplayType)0)
#define EGL_NONE                0x3038

#endif /* __egl_h_ */
