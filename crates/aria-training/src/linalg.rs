//! Deterministic f64 linear-algebra primitives shared by the training engine.
//!
//! Everything here is plain IEEE-754 arithmetic in fixed iteration order —
//! no SIMD dispatch, no threading — so results are bit-identical across
//! platforms and runs. These are the only vector/matrix kernels the loss,
//! optimizer, and trainer are allowed to use.
//!
//! Allocating wrappers (`matvec`, `mat_t_vec`) are thin fronts over the
//! in-place kernels. Hot loops must call the `*_into` forms so a JEPA
//! transition does not allocate `p_t` / `r` / `pᵀr` on every pair.

use aria_engine_backends::spectral::Matrix;

/// An all-zero matrix of the given shape.
pub fn zeros(rows: usize, cols: usize) -> Matrix {
    vec![vec![0.0; cols]; rows]
}

/// `M·x`.
pub fn matvec(m: &Matrix, x: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; m.len()];
    matvec_into(m, x, &mut out);
    out
}

/// `out ← M·x`. Iteration order matches a row-wise `dot` fold, so the
/// allocating [`matvec`] wrapper is bit-identical to this kernel.
pub fn matvec_into(m: &Matrix, x: &[f64], out: &mut [f64]) {
    debug_assert_eq!(out.len(), m.len());
    for (row, o) in m.iter().zip(out.iter_mut()) {
        *o = dot(row, x);
    }
}

/// `Mᵀ·x` without materializing the transpose.
pub fn mat_t_vec(m: &Matrix, x: &[f64]) -> Vec<f64> {
    let cols = m.first().map_or(0, Vec::len);
    let mut out = vec![0.0; cols];
    mat_t_vec_into(m, x, &mut out);
    out
}

/// `out ← Mᵀ·x`. `out` is zeroed then accumulated in the same (row, col)
/// order as [`mat_t_vec`], so the two are bit-identical.
pub fn mat_t_vec_into(m: &Matrix, x: &[f64], out: &mut [f64]) {
    out.fill(0.0);
    for (row, xi) in m.iter().zip(x) {
        for (o, v) in out.iter_mut().zip(row) {
            *o += v * xi;
        }
    }
}

/// Inner product, left-to-right IEEE sum.
pub fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// `‖v‖²`.
pub fn norm2_sq(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum()
}

/// `out ← a − b`.
pub fn sub_into(a: &[f64], b: &[f64], out: &mut [f64]) {
    for ((o, x), y) in out.iter_mut().zip(a).zip(b) {
        *o = x - y;
    }
}

/// `M += scale · u vᵀ`.
pub fn add_outer(m: &mut Matrix, scale: f64, u: &[f64], v: &[f64]) {
    for (row, ui) in m.iter_mut().zip(u) {
        let s = scale * ui;
        for (cell, vj) in row.iter_mut().zip(v) {
            *cell += s * vj;
        }
    }
}

/// Squared Euclidean distance `‖a − b‖²`.
pub fn l2_sq(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}
