//! Math functions (libm)

/// Mathematical constants
pub mod consts {
    pub const E: f64 = 2.718281828459045;
    pub const PI: f64 = 3.141592653589793;
    pub const PI_2: f64 = 1.5707963267948966;
    pub const PI_4: f64 = 0.7853981633974483;
    pub const SQRT_2: f64 = 1.4142135623730951;
    pub const LN_2: f64 = 0.6931471805599453;
    pub const LN_10: f64 = 2.302585092994046;
    pub const LOG2_E: f64 = 1.4426950408889634;
    pub const LOG10_E: f64 = 0.4342944819032518;
}

/// Absolute value (f64)
pub fn fabs(x: f64) -> f64 {
    if x < 0.0 { -x } else { x }
}

/// Absolute value (f32)
pub fn fabsf(x: f32) -> f32 {
    if x < 0.0 { -x } else { x }
}

/// Floor function
pub fn floor(x: f64) -> f64 {
    let i = x as i64;
    if x < 0.0 && x != i as f64 {
        (i - 1) as f64
    } else {
        i as f64
    }
}

/// Floor function (f32)
pub fn floorf(x: f32) -> f32 {
    let i = x as i32;
    if x < 0.0 && x != i as f32 {
        (i - 1) as f32
    } else {
        i as f32
    }
}

/// Ceiling function
pub fn ceil(x: f64) -> f64 {
    let i = x as i64;
    if x > 0.0 && x != i as f64 {
        (i + 1) as f64
    } else {
        i as f64
    }
}

/// Ceiling function (f32)
pub fn ceilf(x: f32) -> f32 {
    let i = x as i32;
    if x > 0.0 && x != i as f32 {
        (i + 1) as f32
    } else {
        i as f32
    }
}

/// Round to nearest integer
pub fn round(x: f64) -> f64 {
    floor(x + 0.5)
}

/// Round (f32)
pub fn roundf(x: f32) -> f32 {
    floorf(x + 0.5)
}

/// Truncate toward zero
pub fn trunc(x: f64) -> f64 {
    x as i64 as f64
}

/// Truncate (f32)
pub fn truncf(x: f32) -> f32 {
    x as i32 as f32
}

/// Modulo operation
pub fn fmod(x: f64, y: f64) -> f64 {
    x - trunc(x / y) * y
}

/// Modulo (f32)
pub fn fmodf(x: f32, y: f32) -> f32 {
    x - truncf(x / y) * y
}

/// Square root using Newton-Raphson
pub fn sqrt(x: f64) -> f64 {
    if x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }

    let mut guess = x / 2.0;
    for _ in 0..20 {
        guess = (guess + x / guess) / 2.0;
    }
    guess
}

/// Square root (f32)
pub fn sqrtf(x: f32) -> f32 {
    if x < 0.0 {
        return f32::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }

    let mut guess = x / 2.0;
    for _ in 0..15 {
        guess = (guess + x / guess) / 2.0;
    }
    guess
}

/// Cube root
pub fn cbrt(x: f64) -> f64 {
    if x == 0.0 {
        return 0.0;
    }

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let abs_x = fabs(x);

    let mut guess = abs_x / 3.0;
    for _ in 0..20 {
        guess = (2.0 * guess + abs_x / (guess * guess)) / 3.0;
    }

    sign * guess
}

/// Power function
pub fn pow(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return 1.0;
    }
    if x == 0.0 {
        return 0.0;
    }
    if x == 1.0 {
        return 1.0;
    }

    // Use exp(y * ln(x))
    exp(y * ln(x))
}

/// Power (f32)
pub fn powf(x: f32, y: f32) -> f32 {
    pow(x as f64, y as f64) as f32
}

/// Natural logarithm using Taylor series
pub fn ln(x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NAN;
    }
    if x == 1.0 {
        return 0.0;
    }

    // Reduce x to [0.5, 1.5] range
    let mut k = 0i32;
    let mut x = x;

    while x > 1.5 {
        x /= 2.0;
        k += 1;
    }
    while x < 0.5 {
        x *= 2.0;
        k -= 1;
    }

    // Use Taylor series: ln(1+y) = y - y²/2 + y³/3 - ...
    let y = (x - 1.0) / (x + 1.0);
    let y2 = y * y;

    let mut sum = y;
    let mut term = y;

    for i in 1..30 {
        term *= y2;
        sum += term / (2 * i + 1) as f64;
    }

    2.0 * sum + k as f64 * consts::LN_2
}

/// Natural log (f32)
pub fn logf(x: f32) -> f32 {
    ln(x as f64) as f32
}

/// Log base 10
pub fn log10(x: f64) -> f64 {
    ln(x) * consts::LOG10_E
}

/// Log base 2
pub fn log2(x: f64) -> f64 {
    ln(x) * consts::LOG2_E
}

/// Exponential function using Taylor series
pub fn exp(x: f64) -> f64 {
    if x == 0.0 {
        return 1.0;
    }

    // Reduce range using e^x = 2^(x*log2(e)) = 2^k * e^r
    let k = floor(x * consts::LOG2_E) as i32;
    let r = x - k as f64 * consts::LN_2;

    // Taylor series for e^r where r is small
    let mut sum = 1.0;
    let mut term = 1.0;

    for i in 1..30 {
        term *= r / i as f64;
        sum += term;
    }

    // Multiply by 2^k
    if k >= 0 {
        sum * (1u64 << k as u64) as f64
    } else {
        sum / (1u64 << (-k) as u64) as f64
    }
}

/// Exponential (f32)
pub fn expf(x: f32) -> f32 {
    exp(x as f64) as f32
}

/// Sine using Taylor series
pub fn sin(x: f64) -> f64 {
    // Reduce to [-π, π]
    let x = fmod(x, 2.0 * consts::PI);
    let x = if x > consts::PI { x - 2.0 * consts::PI }
            else if x < -consts::PI { x + 2.0 * consts::PI }
            else { x };

    // Taylor series: sin(x) = x - x³/3! + x⁵/5! - ...
    let x2 = x * x;
    let mut term = x;
    let mut sum = term;

    for i in 1..15 {
        term *= -x2 / ((2 * i) * (2 * i + 1)) as f64;
        sum += term;
    }

    sum
}

/// Sine (f32)
pub fn sinf(x: f32) -> f32 {
    sin(x as f64) as f32
}

/// Cosine using Taylor series
pub fn cos(x: f64) -> f64 {
    // Reduce to [-π, π]
    let x = fmod(x, 2.0 * consts::PI);
    let x = if x > consts::PI { x - 2.0 * consts::PI }
            else if x < -consts::PI { x + 2.0 * consts::PI }
            else { x };

    // Taylor series: cos(x) = 1 - x²/2! + x⁴/4! - ...
    let x2 = x * x;
    let mut term = 1.0;
    let mut sum = term;

    for i in 1..15 {
        term *= -x2 / ((2 * i - 1) * (2 * i)) as f64;
        sum += term;
    }

    sum
}

/// Cosine (f32)
pub fn cosf(x: f32) -> f32 {
    cos(x as f64) as f32
}

/// Tangent
pub fn tan(x: f64) -> f64 {
    sin(x) / cos(x)
}

/// Tangent (f32)
pub fn tanf(x: f32) -> f32 {
    sinf(x) / cosf(x)
}

/// Arcsine
pub fn asin(x: f64) -> f64 {
    if x < -1.0 || x > 1.0 {
        return f64::NAN;
    }
    atan2(x, sqrt(1.0 - x * x))
}

/// Arcsine (f32)
pub fn asinf(x: f32) -> f32 {
    asin(x as f64) as f32
}

/// Arccosine
pub fn acos(x: f64) -> f64 {
    if x < -1.0 || x > 1.0 {
        return f64::NAN;
    }
    atan2(sqrt(1.0 - x * x), x)
}

/// Arccosine (f32)
pub fn acosf(x: f32) -> f32 {
    acos(x as f64) as f32
}

/// Arctangent
pub fn atan(x: f64) -> f64 {
    atan2(x, 1.0)
}

/// Arctangent (f32)
pub fn atanf(x: f32) -> f32 {
    atan(x as f64) as f32
}

/// Two-argument arctangent
pub fn atan2(y: f64, x: f64) -> f64 {
    if x == 0.0 {
        if y > 0.0 { return consts::PI_2; }
        if y < 0.0 { return -consts::PI_2; }
        return 0.0;
    }

    let z = y / x;
    let mut result;

    if fabs(z) < 1.0 {
        // Use series directly
        result = atan_series(z);
    } else {
        // Use identity: atan(z) = π/2 - atan(1/z)
        result = consts::PI_2 - atan_series(1.0 / z);
        if z < 0.0 {
            result = -result;
        }
    }

    if x < 0.0 {
        if y >= 0.0 {
            result += consts::PI;
        } else {
            result -= consts::PI;
        }
    }

    result
}

/// Arctangent series for |x| <= 1
fn atan_series(x: f64) -> f64 {
    let x2 = x * x;
    let mut term = x;
    let mut sum = term;

    for i in 1..25 {
        term *= -x2;
        sum += term / (2 * i + 1) as f64;
    }

    sum
}

/// Two-argument arctangent (f32)
pub fn atan2f(y: f32, x: f32) -> f32 {
    atan2(y as f64, x as f64) as f32
}

/// Hyperbolic sine
pub fn sinh(x: f64) -> f64 {
    (exp(x) - exp(-x)) / 2.0
}

/// Hyperbolic cosine
pub fn cosh(x: f64) -> f64 {
    (exp(x) + exp(-x)) / 2.0
}

/// Hyperbolic tangent
pub fn tanh(x: f64) -> f64 {
    sinh(x) / cosh(x)
}

/// Hypotenuse
pub fn hypot(x: f64, y: f64) -> f64 {
    sqrt(x * x + y * y)
}

/// Copy sign
pub fn copysign(x: f64, y: f64) -> f64 {
    fabs(x) * if y < 0.0 { -1.0 } else { 1.0 }
}

/// Sign function
pub fn signbit(x: f64) -> bool {
    x < 0.0
}

/// Check if NaN
pub fn isnan(x: f64) -> bool {
    x != x
}

/// Check if infinite
pub fn isinf(x: f64) -> bool {
    x == f64::INFINITY || x == f64::NEG_INFINITY
}

/// Check if finite
pub fn isfinite(x: f64) -> bool {
    !isnan(x) && !isinf(x)
}

/// Minimum
pub fn fmin(x: f64, y: f64) -> f64 {
    if isnan(x) { return y; }
    if isnan(y) { return x; }
    if x < y { x } else { y }
}

/// Maximum
pub fn fmax(x: f64, y: f64) -> f64 {
    if isnan(x) { return y; }
    if isnan(y) { return x; }
    if x > y { x } else { y }
}

/// Fractional and integral parts
pub fn modf(x: f64, iptr: &mut f64) -> f64 {
    *iptr = trunc(x);
    x - *iptr
}

/// Load exponent
pub fn ldexp(x: f64, exp: i32) -> f64 {
    x * pow(2.0, exp as f64)
}

/// Extract exponent
pub fn frexp(x: f64, exp: &mut i32) -> f64 {
    if x == 0.0 {
        *exp = 0;
        return 0.0;
    }

    let mut e = 0i32;
    let mut m = fabs(x);

    while m >= 1.0 {
        m /= 2.0;
        e += 1;
    }
    while m < 0.5 {
        m *= 2.0;
        e -= 1;
    }

    *exp = e;
    if x < 0.0 { -m } else { m }
}
