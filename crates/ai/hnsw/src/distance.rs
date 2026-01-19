//! Distance metrics for vector similarity

/// Square root approximation for no_std
fn sqrt_f32(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut guess = x / 2.0;
    for _ in 0..10 {
        guess = (guess + x / guess) / 2.0;
    }
    guess
}

/// Distance metric type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distance {
    /// Cosine distance (1 - cosine similarity)
    Cosine,
    /// Euclidean (L2) distance
    Euclidean,
    /// Inner product (negative for similarity)
    InnerProduct,
}

/// Compute cosine distance between two vectors
///
/// Returns value in range [0, 2], where 0 means identical direction
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = sqrt_f32(norm_a * norm_b);
    if denom < 1e-10 {
        return 1.0;
    }

    let cosine_sim = dot / denom;
    1.0 - cosine_sim
}

/// Compute Euclidean (L2) distance between two vectors
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());

    let mut sum = 0.0f32;
    for i in 0..a.len() {
        let diff = a[i] - b[i];
        sum += diff * diff;
    }

    sqrt_f32(sum)
}

/// Compute squared Euclidean distance (faster, preserves ordering)
pub fn squared_euclidean(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());

    let mut sum = 0.0f32;
    for i in 0..a.len() {
        let diff = a[i] - b[i];
        sum += diff * diff;
    }

    sum
}

/// Compute inner product (negative for distance)
pub fn inner_product(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());

    let mut dot = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
    }

    -dot // Negative so higher similarity = lower distance
}

/// Normalize a vector in place
pub fn normalize(v: &mut [f32]) {
    let mut norm = 0.0f32;
    for x in v.iter() {
        norm += x * x;
    }
    let norm = sqrt_f32(norm);
    if norm > 1e-10 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}
