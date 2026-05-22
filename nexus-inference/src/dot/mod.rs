#[allow(dead_code)]
mod scalar;

#[cfg(all(
    target_arch = "x86_64",
    target_feature = "avx2",
    target_feature = "fma"
))]
mod avx2;

#[inline]
pub(crate) fn dot_f64(a: &[f64], b: &[f64]) -> f64 {
    debug_assert_eq!(a.len(), b.len());

    #[cfg(all(
        target_arch = "x86_64",
        target_feature = "avx2",
        target_feature = "fma"
    ))]
    {
        avx2::dot_f64(a, b)
    }

    #[cfg(not(all(
        target_arch = "x86_64",
        target_feature = "avx2",
        target_feature = "fma"
    )))]
    {
        scalar::dot_f64(a, b)
    }
}

#[inline]
pub(crate) fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());

    #[cfg(all(
        target_arch = "x86_64",
        target_feature = "avx2",
        target_feature = "fma"
    ))]
    {
        avx2::dot_f32(a, b)
    }

    #[cfg(not(all(
        target_arch = "x86_64",
        target_feature = "avx2",
        target_feature = "fma"
    )))]
    {
        scalar::dot_f32(a, b)
    }
}

/// Compute 4 dot products simultaneously: dot(rows[k*n..], input) for k in 0..4.
/// `rows` layout: [row0 | row1 | row2 | row3], each row has `input.len()` elements.
#[inline]
pub(crate) fn dot4_f64(rows: &[f64], input: &[f64]) -> [f64; 4] {
    debug_assert_eq!(rows.len(), 4 * input.len());

    #[cfg(all(
        target_arch = "x86_64",
        target_feature = "avx2",
        target_feature = "fma"
    ))]
    {
        avx2::dot4_f64(rows, input)
    }

    #[cfg(not(all(
        target_arch = "x86_64",
        target_feature = "avx2",
        target_feature = "fma"
    )))]
    {
        scalar::dot4_f64(rows, input)
    }
}

/// Compute 4 dot products simultaneously: dot(rows[k*n..], input) for k in 0..4.
/// `rows` layout: [row0 | row1 | row2 | row3], each row has `input.len()` elements.
#[inline]
pub(crate) fn dot4_f32(rows: &[f32], input: &[f32]) -> [f32; 4] {
    debug_assert_eq!(rows.len(), 4 * input.len());

    #[cfg(all(
        target_arch = "x86_64",
        target_feature = "avx2",
        target_feature = "fma"
    ))]
    {
        avx2::dot4_f32(rows, input)
    }

    #[cfg(not(all(
        target_arch = "x86_64",
        target_feature = "avx2",
        target_feature = "fma"
    )))]
    {
        scalar::dot4_f32(rows, input)
    }
}
