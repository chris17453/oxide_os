/* OXIDE OS Math Functions */

#ifndef _MATH_H
#define _MATH_H

/* Constants */
#define M_E         2.7182818284590452354
#define M_LOG2E     1.4426950408889634074
#define M_LOG10E    0.43429448190325182765
#define M_LN2       0.69314718055994530942
#define M_LN10      2.30258509299404568402
#define M_PI        3.14159265358979323846
#define M_PI_2      1.57079632679489661923
#define M_PI_4      0.78539816339744830962
#define M_1_PI      0.31830988618379067154
#define M_2_PI      0.63661977236758134308
#define M_2_SQRTPI  1.12837916709551257390
#define M_SQRT2     1.41421356237309504880
#define M_SQRT1_2   0.70710678118654752440

#define HUGE_VAL    __builtin_huge_val()
#define HUGE_VALF   __builtin_huge_valf()
#define HUGE_VALL   __builtin_huge_vall()
#define INFINITY    __builtin_inff()
#define NAN         __builtin_nanf("")

/* Classification macros */
#define FP_NAN       0
#define FP_INFINITE  1
#define FP_ZERO      2
#define FP_SUBNORMAL 3
#define FP_NORMAL    4

#define fpclassify(x) __builtin_fpclassify(FP_NAN, FP_INFINITE, FP_NORMAL, FP_SUBNORMAL, FP_ZERO, x)
#define isfinite(x)   __builtin_isfinite(x)
#define isinf(x)      __builtin_isinf(x)
#define isnan(x)      __builtin_isnan(x)
#define isnormal(x)   __builtin_isnormal(x)
#define signbit(x)    __builtin_signbit(x)

/* Trigonometric */
double sin(double x);
double cos(double x);
double tan(double x);
double asin(double x);
double acos(double x);
double atan(double x);
double atan2(double y, double x);
float sinf(float x);
float cosf(float x);
float tanf(float x);
float asinf(float x);
float acosf(float x);
float atanf(float x);
float atan2f(float y, float x);
long double sinl(long double x);
long double cosl(long double x);
long double tanl(long double x);
long double asinl(long double x);
long double acosl(long double x);
long double atanl(long double x);
long double atan2l(long double y, long double x);

/* Hyperbolic */
double sinh(double x);
double cosh(double x);
double tanh(double x);
double asinh(double x);
double acosh(double x);
double atanh(double x);
float sinhf(float x);
float coshf(float x);
float tanhf(float x);
float asinhf(float x);
float acoshf(float x);
float atanhf(float x);

/* Exponential and logarithmic */
double exp(double x);
double exp2(double x);
double expm1(double x);
double log(double x);
double log2(double x);
double log10(double x);
double log1p(double x);
float expf(float x);
float exp2f(float x);
float expm1f(float x);
float logf(float x);
float log2f(float x);
float log10f(float x);
float log1pf(float x);

/* Power */
double pow(double x, double y);
double sqrt(double x);
double cbrt(double x);
double hypot(double x, double y);
float powf(float x, float y);
float sqrtf(float x);
float cbrtf(float x);
float hypotf(float x, float y);

/* Rounding */
double ceil(double x);
double floor(double x);
double trunc(double x);
double round(double x);
double nearbyint(double x);
double rint(double x);
long lrint(double x);
long long llrint(double x);
long lround(double x);
long long llround(double x);
float ceilf(float x);
float floorf(float x);
float truncf(float x);
float roundf(float x);
float nearbyintf(float x);
float rintf(float x);

/* Remainder */
double fmod(double x, double y);
double remainder(double x, double y);
double remquo(double x, double y, int *quo);
float fmodf(float x, float y);
float remainderf(float x, float y);

/* Manipulation */
double copysign(double x, double y);
double nextafter(double x, double y);
double fabs(double x);
double fdim(double x, double y);
double fmax(double x, double y);
double fmin(double x, double y);
double fma(double x, double y, double z);
float copysignf(float x, float y);
float nextafterf(float x, float y);
float fabsf(float x);
float fdimf(float x, float y);
float fmaxf(float x, float y);
float fminf(float x, float y);
float fmaf(float x, float y, float z);

/* Decomposition */
double frexp(double x, int *exp);
double ldexp(double x, int exp);
double modf(double x, double *iptr);
double scalbn(double x, int n);
double scalbln(double x, long n);
int ilogb(double x);
double logb(double x);
float frexpf(float x, int *exp);
float ldexpf(float x, int exp);
float modff(float x, float *iptr);

/* Error and gamma */
double erf(double x);
double erfc(double x);
double lgamma(double x);
double tgamma(double x);
float erff(float x);
float erfcf(float x);

/* Long double versions (map to double) */
long double fabsl(long double x);
long double ceill(long double x);
long double floorl(long double x);
long double roundl(long double x);
long double truncl(long double x);
long double sqrtl(long double x);
long double logl(long double x);
long double log10l(long double x);
long double log2l(long double x);
long double expl(long double x);
long double powl(long double x, long double y);
long double fmodl(long double x, long double y);
long double copysignl(long double x, long double y);
long double frexpl(long double x, int *exp);
long double ldexpl(long double x, int exp);
long double modfl(long double x, long double *iptr);

#endif /* _MATH_H */
