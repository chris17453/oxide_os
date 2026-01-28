//! C-callable math function exports
//!
//! Wraps the Rust math module with extern "C" #[no_mangle] functions.

use crate::math;

// Double precision
#[unsafe(no_mangle)] pub extern "C" fn sin(x: f64) -> f64 { math::sin(x) }
#[unsafe(no_mangle)] pub extern "C" fn cos(x: f64) -> f64 { math::cos(x) }
#[unsafe(no_mangle)] pub extern "C" fn tan(x: f64) -> f64 { math::tan(x) }
#[unsafe(no_mangle)] pub extern "C" fn asin(x: f64) -> f64 { math::asin(x) }
#[unsafe(no_mangle)] pub extern "C" fn acos(x: f64) -> f64 { math::acos(x) }
#[unsafe(no_mangle)] pub extern "C" fn atan(x: f64) -> f64 { math::atan(x) }
#[unsafe(no_mangle)] pub extern "C" fn atan2(y: f64, x: f64) -> f64 { math::atan2(y, x) }
#[unsafe(no_mangle)] pub extern "C" fn sinh(x: f64) -> f64 { math::sinh(x) }
#[unsafe(no_mangle)] pub extern "C" fn cosh(x: f64) -> f64 { math::cosh(x) }
#[unsafe(no_mangle)] pub extern "C" fn tanh(x: f64) -> f64 { math::tanh(x) }
#[unsafe(no_mangle)] pub extern "C" fn exp(x: f64) -> f64 { math::exp(x) }
#[unsafe(no_mangle)] pub extern "C" fn log(x: f64) -> f64 { math::ln(x) }
#[unsafe(no_mangle)] pub extern "C" fn log10(x: f64) -> f64 { math::log10(x) }
#[unsafe(no_mangle)] pub extern "C" fn log2(x: f64) -> f64 { math::log2(x) }
#[unsafe(no_mangle)] pub extern "C" fn pow(x: f64, y: f64) -> f64 { math::pow(x, y) }
#[unsafe(no_mangle)] pub extern "C" fn sqrt(x: f64) -> f64 { math::sqrt(x) }
#[unsafe(no_mangle)] pub extern "C" fn cbrt(x: f64) -> f64 { math::cbrt(x) }
#[unsafe(no_mangle)] pub extern "C" fn fabs(x: f64) -> f64 { math::fabs(x) }
#[unsafe(no_mangle)] pub extern "C" fn ceil(x: f64) -> f64 { math::ceil(x) }
#[unsafe(no_mangle)] pub extern "C" fn floor(x: f64) -> f64 { math::floor(x) }
#[unsafe(no_mangle)] pub extern "C" fn round(x: f64) -> f64 { math::round(x) }
#[unsafe(no_mangle)] pub extern "C" fn trunc(x: f64) -> f64 { math::trunc(x) }
#[unsafe(no_mangle)] pub extern "C" fn fmod(x: f64, y: f64) -> f64 { math::fmod(x, y) }
#[unsafe(no_mangle)] pub extern "C" fn copysign(x: f64, y: f64) -> f64 { math::copysign(x, y) }
#[unsafe(no_mangle)] pub extern "C" fn hypot(x: f64, y: f64) -> f64 { math::hypot(x, y) }
#[unsafe(no_mangle)] pub extern "C" fn fmin(x: f64, y: f64) -> f64 { math::fmin(x, y) }
#[unsafe(no_mangle)] pub extern "C" fn fmax(x: f64, y: f64) -> f64 { math::fmax(x, y) }
#[unsafe(no_mangle)] pub extern "C" fn ldexp(x: f64, exp: i32) -> f64 { math::ldexp(x, exp) }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn frexp(x: f64, exp: *mut i32) -> f64 {
    math::frexp(x, &mut *exp)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn modf(x: f64, iptr: *mut f64) -> f64 {
    math::modf(x, &mut *iptr)
}

// Float precision
#[unsafe(no_mangle)] pub extern "C" fn sinf(x: f32) -> f32 { math::sinf(x) }
#[unsafe(no_mangle)] pub extern "C" fn cosf(x: f32) -> f32 { math::cosf(x) }
#[unsafe(no_mangle)] pub extern "C" fn tanf(x: f32) -> f32 { math::tanf(x) }
#[unsafe(no_mangle)] pub extern "C" fn asinf(x: f32) -> f32 { math::asinf(x) }
#[unsafe(no_mangle)] pub extern "C" fn acosf(x: f32) -> f32 { math::acosf(x) }
#[unsafe(no_mangle)] pub extern "C" fn atanf(x: f32) -> f32 { math::atanf(x) }
#[unsafe(no_mangle)] pub extern "C" fn atan2f(y: f32, x: f32) -> f32 { math::atan2f(y, x) }
#[unsafe(no_mangle)] pub extern "C" fn expf(x: f32) -> f32 { math::expf(x) }
#[unsafe(no_mangle)] pub extern "C" fn logf(x: f32) -> f32 { math::logf(x) }
#[unsafe(no_mangle)] pub extern "C" fn powf(x: f32, y: f32) -> f32 { math::powf(x, y) }
#[unsafe(no_mangle)] pub extern "C" fn sqrtf(x: f32) -> f32 { math::sqrtf(x) }
#[unsafe(no_mangle)] pub extern "C" fn fabsf(x: f32) -> f32 { math::fabsf(x) }
#[unsafe(no_mangle)] pub extern "C" fn ceilf(x: f32) -> f32 { math::ceilf(x) }
#[unsafe(no_mangle)] pub extern "C" fn floorf(x: f32) -> f32 { math::floorf(x) }
#[unsafe(no_mangle)] pub extern "C" fn roundf(x: f32) -> f32 { math::roundf(x) }
#[unsafe(no_mangle)] pub extern "C" fn truncf(x: f32) -> f32 { math::truncf(x) }
#[unsafe(no_mangle)] pub extern "C" fn fmodf(x: f32, y: f32) -> f32 { math::fmodf(x, y) }

// Additional math functions CPython needs
#[unsafe(no_mangle)]
pub extern "C" fn asinh(x: f64) -> f64 {
    // asinh(x) = ln(x + sqrt(x^2 + 1))
    math::ln(x + math::sqrt(x * x + 1.0))
}

#[unsafe(no_mangle)]
pub extern "C" fn acosh(x: f64) -> f64 {
    if x < 1.0 { return f64::NAN; }
    math::ln(x + math::sqrt(x * x - 1.0))
}

#[unsafe(no_mangle)]
pub extern "C" fn atanh(x: f64) -> f64 {
    if x <= -1.0 || x >= 1.0 { return f64::NAN; }
    0.5 * math::ln((1.0 + x) / (1.0 - x))
}

#[unsafe(no_mangle)]
pub extern "C" fn asinhf(x: f32) -> f32 { asinh(x as f64) as f32 }
#[unsafe(no_mangle)]
pub extern "C" fn acoshf(x: f32) -> f32 { acosh(x as f64) as f32 }
#[unsafe(no_mangle)]
pub extern "C" fn atanhf(x: f32) -> f32 { atanh(x as f64) as f32 }

#[unsafe(no_mangle)]
pub extern "C" fn sinhf(x: f32) -> f32 { math::sinh(x as f64) as f32 }
#[unsafe(no_mangle)]
pub extern "C" fn coshf(x: f32) -> f32 { math::cosh(x as f64) as f32 }
#[unsafe(no_mangle)]
pub extern "C" fn tanhf(x: f32) -> f32 { math::tanh(x as f64) as f32 }

#[unsafe(no_mangle)]
pub extern "C" fn exp2(x: f64) -> f64 {
    math::pow(2.0, x)
}

#[unsafe(no_mangle)]
pub extern "C" fn exp2f(x: f32) -> f32 {
    math::powf(2.0, x)
}

#[unsafe(no_mangle)]
pub extern "C" fn expm1(x: f64) -> f64 {
    math::exp(x) - 1.0
}

#[unsafe(no_mangle)]
pub extern "C" fn expm1f(x: f32) -> f32 {
    (math::exp(x as f64) - 1.0) as f32
}

#[unsafe(no_mangle)]
pub extern "C" fn log1p(x: f64) -> f64 {
    math::ln(1.0 + x)
}

#[unsafe(no_mangle)]
pub extern "C" fn log1pf(x: f32) -> f32 {
    math::ln(1.0 + x as f64) as f32
}

#[unsafe(no_mangle)]
pub extern "C" fn log2f(x: f32) -> f32 {
    math::log2(x as f64) as f32
}

#[unsafe(no_mangle)]
pub extern "C" fn log10f(x: f32) -> f32 {
    math::log10(x as f64) as f32
}

#[unsafe(no_mangle)]
pub extern "C" fn cbrtf(x: f32) -> f32 {
    math::cbrt(x as f64) as f32
}

#[unsafe(no_mangle)]
pub extern "C" fn hypotf(x: f32, y: f32) -> f32 {
    math::hypot(x as f64, y as f64) as f32
}

#[unsafe(no_mangle)]
pub extern "C" fn copysignf(x: f32, y: f32) -> f32 {
    let ax = if x < 0.0 { -x } else { x };
    if y < 0.0 { -ax } else { ax }
}

#[unsafe(no_mangle)]
pub extern "C" fn nearbyint(x: f64) -> f64 { math::round(x) }
#[unsafe(no_mangle)]
pub extern "C" fn nearbyintf(x: f32) -> f32 { math::roundf(x) }
#[unsafe(no_mangle)]
pub extern "C" fn rint(x: f64) -> f64 { math::round(x) }
#[unsafe(no_mangle)]
pub extern "C" fn rintf(x: f32) -> f32 { math::roundf(x) }
#[unsafe(no_mangle)]
pub extern "C" fn lrint(x: f64) -> i64 { math::round(x) as i64 }
#[unsafe(no_mangle)]
pub extern "C" fn llrint(x: f64) -> i64 { math::round(x) as i64 }
#[unsafe(no_mangle)]
pub extern "C" fn lround(x: f64) -> i64 { math::round(x) as i64 }
#[unsafe(no_mangle)]
pub extern "C" fn llround(x: f64) -> i64 { math::round(x) as i64 }

#[unsafe(no_mangle)]
pub extern "C" fn remainder(x: f64, y: f64) -> f64 {
    let q = math::round(x / y);
    x - q * y
}

#[unsafe(no_mangle)]
pub extern "C" fn remainderf(x: f32, y: f32) -> f32 {
    remainder(x as f64, y as f64) as f32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn remquo(x: f64, y: f64, quo: *mut i32) -> f64 {
    let q = math::round(x / y);
    if !quo.is_null() {
        *quo = q as i32;
    }
    x - q * y
}

#[unsafe(no_mangle)]
pub extern "C" fn fdim(x: f64, y: f64) -> f64 {
    if x > y { x - y } else { 0.0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn fdimf(x: f32, y: f32) -> f32 {
    if x > y { x - y } else { 0.0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn fmaxf(x: f32, y: f32) -> f32 {
    if x > y { x } else { y }
}

#[unsafe(no_mangle)]
pub extern "C" fn fminf(x: f32, y: f32) -> f32 {
    if x < y { x } else { y }
}

#[unsafe(no_mangle)]
pub extern "C" fn fma(x: f64, y: f64, z: f64) -> f64 {
    x * y + z
}

#[unsafe(no_mangle)]
pub extern "C" fn fmaf(x: f32, y: f32, z: f32) -> f32 {
    x * y + z
}

#[unsafe(no_mangle)]
pub extern "C" fn nextafter(x: f64, y: f64) -> f64 {
    if x == y { return y; }
    let bits = x.to_bits();
    let new_bits = if (y > x) == (x >= 0.0) { bits + 1 } else { bits - 1 };
    f64::from_bits(new_bits)
}

#[unsafe(no_mangle)]
pub extern "C" fn nextafterf(x: f32, y: f32) -> f32 {
    if x == y { return y; }
    let bits = x.to_bits();
    let new_bits = if (y > x) == (x >= 0.0) { bits + 1 } else { bits - 1 };
    f32::from_bits(new_bits)
}

#[unsafe(no_mangle)]
pub extern "C" fn scalbn(x: f64, n: i32) -> f64 {
    math::ldexp(x, n)
}

#[unsafe(no_mangle)]
pub extern "C" fn scalbln(x: f64, n: i64) -> f64 {
    math::ldexp(x, n as i32)
}

#[unsafe(no_mangle)]
pub extern "C" fn ilogb(x: f64) -> i32 {
    if x == 0.0 { return -2147483647; }
    let mut exp = 0i32;
    math::frexp(x, &mut exp);
    exp - 1
}

#[unsafe(no_mangle)]
pub extern "C" fn logb(x: f64) -> f64 {
    ilogb(x) as f64
}

#[unsafe(no_mangle)]
pub extern "C" fn logbf(x: f32) -> f32 {
    ilogb(x as f64) as f32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn frexpf(x: f32, exp: *mut i32) -> f32 {
    frexp(x as f64, exp) as f32
}

#[unsafe(no_mangle)]
pub extern "C" fn ldexpf(x: f32, exp: i32) -> f32 {
    math::ldexp(x as f64, exp) as f32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn modff(x: f32, iptr: *mut f32) -> f32 {
    let mut d: f64 = 0.0;
    let frac = math::modf(x as f64, &mut d);
    *iptr = d as f32;
    frac as f32
}

// Error/gamma functions (stubs with reasonable approximations)
#[unsafe(no_mangle)]
pub extern "C" fn erf(x: f64) -> f64 {
    // Approximation using Horner form
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = math::fabs(x);
    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * math::exp(-x * x);
    sign * y
}

#[unsafe(no_mangle)]
pub extern "C" fn erfc(x: f64) -> f64 {
    1.0 - erf(x)
}

#[unsafe(no_mangle)]
pub extern "C" fn erff(x: f32) -> f32 { erf(x as f64) as f32 }
#[unsafe(no_mangle)]
pub extern "C" fn erfcf(x: f32) -> f32 { erfc(x as f64) as f32 }

#[unsafe(no_mangle)]
pub extern "C" fn lgamma(x: f64) -> f64 {
    // Stirling approximation for lgamma
    if x <= 0.0 { return f64::INFINITY; }
    let x = x - 1.0;
    0.5 * math::ln(2.0 * math::consts::PI) + (x + 0.5) * math::ln(x + 1.0) - (x + 1.0)
        + 1.0 / (12.0 * (x + 1.0))
}

#[unsafe(no_mangle)]
pub extern "C" fn tgamma(x: f64) -> f64 {
    math::exp(lgamma(x))
}

// Long double versions (identical to double on x86_64 for our purposes)
#[unsafe(no_mangle)] pub extern "C" fn fabsl(x: f64) -> f64 { math::fabs(x) }
#[unsafe(no_mangle)] pub extern "C" fn ceill(x: f64) -> f64 { math::ceil(x) }
#[unsafe(no_mangle)] pub extern "C" fn floorl(x: f64) -> f64 { math::floor(x) }
#[unsafe(no_mangle)] pub extern "C" fn roundl(x: f64) -> f64 { math::round(x) }
#[unsafe(no_mangle)] pub extern "C" fn truncl(x: f64) -> f64 { math::trunc(x) }
#[unsafe(no_mangle)] pub extern "C" fn sqrtl(x: f64) -> f64 { math::sqrt(x) }
#[unsafe(no_mangle)] pub extern "C" fn logl(x: f64) -> f64 { math::ln(x) }
#[unsafe(no_mangle)] pub extern "C" fn log10l(x: f64) -> f64 { math::log10(x) }
#[unsafe(no_mangle)] pub extern "C" fn log2l(x: f64) -> f64 { math::log2(x) }
#[unsafe(no_mangle)] pub extern "C" fn expl(x: f64) -> f64 { math::exp(x) }
#[unsafe(no_mangle)] pub extern "C" fn powl(x: f64, y: f64) -> f64 { math::pow(x, y) }
#[unsafe(no_mangle)] pub extern "C" fn fmodl(x: f64, y: f64) -> f64 { math::fmod(x, y) }
#[unsafe(no_mangle)] pub extern "C" fn copysignl(x: f64, y: f64) -> f64 { math::copysign(x, y) }
#[unsafe(no_mangle)] pub extern "C" fn sinl(x: f64) -> f64 { math::sin(x) }
#[unsafe(no_mangle)] pub extern "C" fn cosl(x: f64) -> f64 { math::cos(x) }
#[unsafe(no_mangle)] pub extern "C" fn tanl(x: f64) -> f64 { math::tan(x) }
#[unsafe(no_mangle)] pub extern "C" fn asinl(x: f64) -> f64 { math::asin(x) }
#[unsafe(no_mangle)] pub extern "C" fn acosl(x: f64) -> f64 { math::acos(x) }
#[unsafe(no_mangle)] pub extern "C" fn atanl(x: f64) -> f64 { math::atan(x) }
#[unsafe(no_mangle)] pub extern "C" fn atan2l(y: f64, x: f64) -> f64 { math::atan2(y, x) }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn frexpl(x: f64, exp: *mut i32) -> f64 { frexp(x, exp) }
#[unsafe(no_mangle)]
pub extern "C" fn ldexpl(x: f64, exp: i32) -> f64 { math::ldexp(x, exp) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn modfl(x: f64, iptr: *mut f64) -> f64 { modf(x, iptr) }
