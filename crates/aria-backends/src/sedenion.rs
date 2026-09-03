//! 16-Dimensional Sedenion Algebra & Annihilator Geometry (𝔸7, ℙ7, UT-10).
//!
//! Sedenions ($\mathbb{S} = \mathbb{O} \oplus \mathbb{O}$) are constructed via the
//! Cayley–Dickson process doubling the octonions. They are power-associative,
//! flexible, non-commutative, non-associative, and crucially possess **zero divisors**:
//! pairs of non-zero unit elements $a, b \in \mathbb{S}^{14}$ such that $a \cdot b = 0$.
//!
//! The zero-divisor variety $\mathcal{ZD}(\mathbb{S}^{14}) \cong V_2(\mathbb{R}^7) \cong \mathrm{G}_2/\mathrm{SO}(3)$
//! gives an exact, $O(1)$-verifiable certificate for topological non-interference
//! between distinct conceptual/market clusters in latent space.
//!
//! Grounded in:
//! - docs/supporting-references/lnspp-g2-spectral-geometry/
//! - docs/Aria-v3.0.0-PRD.tex (UT-2 Spectral Attention & UT-10 Certified Annihilators)

#![allow(clippy::needless_range_loop)] // basis-index algebra, not a slice walk

use serde::{Deserialize, Serialize};

/// Precomputed Cayley–Dickson basis multiplication table for all 16 sedenion basis vectors.
///
/// `RAW_MUL[i][j] = (sign, k)` represents $e_i \cdot e_j = \mathrm{sign} \cdot e_k$.
pub const RAW_MUL: [[(i8, usize); 16]; 16] = [
    [(1, 0), (1, 1), (1, 2), (1, 3), (1, 4), (1, 5), (1, 6), (1, 7),
     (1, 8), (1, 9), (1, 10), (1, 11), (1, 12), (1, 13), (1, 14), (1, 15)],
    [(1, 1), (-1, 0), (1, 3), (-1, 2), (1, 5), (-1, 4), (-1, 7), (1, 6),
     (1, 9), (-1, 8), (-1, 11), (1, 10), (-1, 13), (1, 12), (1, 15), (-1, 14)],
    [(1, 2), (-1, 3), (-1, 0), (1, 1), (1, 6), (1, 7), (-1, 4), (-1, 5),
     (1, 10), (1, 11), (-1, 8), (-1, 9), (-1, 14), (-1, 15), (1, 12), (1, 13)],
    [(1, 3), (1, 2), (-1, 1), (-1, 0), (1, 7), (-1, 6), (1, 5), (-1, 4),
     (1, 11), (-1, 10), (1, 9), (-1, 8), (-1, 15), (1, 14), (-1, 13), (1, 12)],
    [(1, 4), (-1, 5), (-1, 6), (-1, 7), (-1, 0), (1, 1), (1, 2), (1, 3),
     (1, 12), (1, 13), (1, 14), (1, 15), (-1, 8), (-1, 9), (-1, 10), (-1, 11)],
    [(1, 5), (1, 4), (-1, 7), (1, 6), (-1, 1), (-1, 0), (-1, 3), (1, 2),
     (1, 13), (-1, 12), (1, 15), (-1, 14), (1, 9), (-1, 8), (1, 11), (-1, 10)],
    [(1, 6), (1, 7), (1, 4), (-1, 5), (-1, 2), (1, 3), (-1, 0), (-1, 1),
     (1, 14), (-1, 15), (-1, 12), (1, 13), (1, 10), (-1, 11), (-1, 8), (1, 9)],
    [(1, 7), (-1, 6), (1, 5), (1, 4), (-1, 3), (-1, 2), (1, 1), (-1, 0),
     (1, 15), (1, 14), (-1, 13), (-1, 12), (1, 11), (1, 10), (-1, 9), (-1, 8)],
    [(1, 8), (-1, 9), (-1, 10), (-1, 11), (-1, 12), (-1, 13), (-1, 14), (-1, 15),
     (-1, 0), (1, 1), (1, 2), (1, 3), (1, 4), (1, 5), (1, 6), (1, 7)],
    [(1, 9), (1, 8), (-1, 11), (1, 10), (-1, 13), (1, 12), (1, 15), (-1, 14),
     (-1, 1), (-1, 0), (-1, 3), (1, 2), (-1, 5), (1, 4), (1, 7), (-1, 6)],
    [(1, 10), (1, 11), (1, 8), (-1, 9), (-1, 14), (-1, 15), (1, 12), (1, 13),
     (-1, 2), (1, 3), (-1, 0), (-1, 1), (-1, 6), (-1, 7), (1, 4), (1, 5)],
    [(1, 11), (-1, 10), (1, 9), (1, 8), (-1, 15), (1, 14), (-1, 13), (1, 12),
     (-1, 3), (-1, 2), (1, 1), (-1, 0), (-1, 7), (1, 6), (-1, 5), (1, 4)],
    [(1, 12), (1, 13), (1, 14), (1, 15), (1, 8), (-1, 9), (-1, 10), (-1, 11),
     (-1, 4), (1, 5), (1, 6), (1, 7), (-1, 0), (-1, 1), (-1, 2), (-1, 3)],
    [(1, 13), (-1, 12), (1, 15), (-1, 14), (1, 9), (1, 8), (1, 11), (-1, 10),
     (-1, 5), (-1, 4), (1, 7), (-1, 6), (1, 1), (-1, 0), (1, 3), (-1, 2)],
    [(1, 14), (-1, 15), (-1, 12), (1, 13), (1, 10), (-1, 11), (1, 8), (1, 9),
     (-1, 6), (-1, 7), (-1, 4), (1, 5), (1, 2), (-1, 3), (-1, 0), (1, 1)],
    [(1, 15), (1, 14), (-1, 13), (-1, 12), (1, 11), (1, 10), (-1, 9), (1, 8),
     (-1, 7), (1, 6), (-1, 5), (-1, 4), (1, 3), (1, 2), (-1, 1), (-1, 0)],
];

/// A 16-dimensional sedenion $s = (s_0, s_1, \dots, s_{15}) \in \mathbb{R}^{16}$.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sedenion(pub [f64; 16]);

impl Sedenion {
    /// Zero sedenion.
    pub const ZERO: Sedenion = Sedenion([0.0; 16]);

    /// Real unit (1, 0, ..., 0).
    pub const ONE: Sedenion = {
        let mut arr = [0.0; 16];
        arr[0] = 1.0;
        Sedenion(arr)
    };

    /// Create from a 16-element slice.
    pub fn new(coords: [f64; 16]) -> Self {
        Sedenion(coords)
    }

    /// Embed a latent vector $z \in \mathbb{R}^d$ onto the pure-imaginary unit sphere $\mathbb{S}^{14} \subset \mathbb{S}$.
    ///
    /// The real part $s_0$ is set to 0; coordinates $1..15$ are filled from $z$
    /// (wrapped / tiled if $d < 15$, or truncated if $d > 15$) and normalized to unit length.
    pub fn from_latent(z: &[f64]) -> Self {
        let mut s = [0.0; 16];
        if z.is_empty() {
            s[1] = 1.0;
            return Sedenion(s);
        }
        for i in 0..15 {
            s[i + 1] = z[i % z.len()];
        }
        let norm_sq: f64 = s[1..].iter().map(|&x| x * x).sum();
        if norm_sq > 1e-300 {
            let inv_norm = 1.0 / libm::sqrt(norm_sq);
            for x in &mut s[1..] {
                *x *= inv_norm;
            }
        } else {
            s[1] = 1.0;
        }
        Sedenion(s)
    }

    /// Squared Euclidean norm $\|s\|_2^2$.
    pub fn norm_sq(&self) -> f64 {
        self.0.iter().map(|&x| x * x).sum()
    }

    /// Euclidean norm $\|s\|_2$.
    pub fn norm(&self) -> f64 {
        libm::sqrt(self.norm_sq())
    }

    /// Unit-length copy; zero maps to [`Self::ZERO`].
    #[must_use]
    pub fn normalize(&self) -> Self {
        let n = self.norm();
        if n <= 1e-300 {
            return Self::ZERO;
        }
        let mut out = self.0;
        let inv = 1.0 / n;
        for x in &mut out {
            *x *= inv;
        }
        Sedenion(out)
    }

    /// Conjugate: $\bar{s} = (s_0, -s_1, -s_2, \dots, -s_{15})$.
    #[must_use]
    pub fn conjugate(&self) -> Self {
        let mut out = self.0;
        for x in &mut out[1..] {
            *x = -*x;
        }
        Sedenion(out)
    }

    /// Inference product: precomputed [`RAW_MUL`] table, $O(1)$ in the
    /// 16-basis. This is the compiled Cayley–Dickson algebra — the same
    /// multiplication, not a different one.
    #[must_use]
    pub fn mul_table(&self, other: &Self) -> Self {
        let mut res = [0.0; 16];
        for i in 0..16 {
            let vi = self.0[i];
            if vi.abs() < 1e-18 {
                continue;
            }
            for j in 0..16 {
                let wj = other.0[j];
                if wj.abs() < 1e-18 {
                    continue;
                }
                let (sign, k) = RAW_MUL[i][j];
                res[k] += f64::from(sign) * vi * wj;
            }
        }
        Sedenion(res)
    }

    /// Recursive Cayley–Dickson walk: $(a_1,a_2)(b_1,b_2) =
    /// (a_1 b_1 - \bar b_2 a_2,\ b_2 a_1 + a_2 \bar b_1)$ down through
    /// octonions, quaternions, and complexes. Same algebra as
    /// [`Self::mul_table`]; this is the *proof* the table compiled.
    #[must_use]
    pub fn mul_walk(&self, other: &Self) -> Self {
        let (a1, a2) = split_8(&self.0);
        let (b1, b2) = split_8(&other.0);
        let a1_b1 = mul_oct(&a1, &b1);
        let b2_conj_a2 = mul_oct(&conj_oct(&b2), &a2);
        let lo = sub_8(&a1_b1, &b2_conj_a2);
        let b2_a1 = mul_oct(&b2, &a1);
        let a2_b1_conj = mul_oct(&a2, &conj_oct(&b1));
        let hi = add_8(&b2_a1, &a2_b1_conj);
        Sedenion(join_8(&lo, &hi))
    }

    /// Default product is the table (inference). Use [`Self::mul_walk`]
    /// when you need the recursive certificate.
    #[must_use]
    pub fn mul(&self, other: &Self) -> Self {
        self.mul_table(other)
    }

    /// Both products plus the two CD octonion-fibre norms.
    /// `agrees` is true when table and walk match within `eps` in $L^2$.
    #[must_use]
    pub fn certify(&self, other: &Self, eps: f64) -> AnnihilatorCertificate {
        let table = self.mul_table(other);
        let walk = self.mul_walk(other);
        let table_norm_sq = table.norm_sq();
        let walk_norm_sq = walk.norm_sq();
        let mut diff = 0.0;
        for i in 0..16 {
            let d = table.0[i] - walk.0[i];
            diff += d * d;
        }
        let (a1, a2) = split_8(&self.0);
        let (b1, b2) = split_8(&other.0);
        let lo = sub_8(&mul_oct(&a1, &b1), &mul_oct(&conj_oct(&b2), &a2));
        let hi = add_8(&mul_oct(&b2, &a1), &mul_oct(&a2, &conj_oct(&b1)));
        AnnihilatorCertificate {
            table_norm_sq,
            walk_norm_sq,
            fibre_lo_norm_sq: lo.iter().map(|x| x * x).sum(),
            fibre_hi_norm_sq: hi.iter().map(|x| x * x).sum(),
            agrees: diff <= eps,
        }
    }

    /// Left-multiplication matrix $L_v \in \mathbb{R}^{16 \times 16}$ such that $(L_v x)_k = (v \cdot x)_k$.
    pub fn left_matrix(&self) -> [[f64; 16]; 16] {
        let mut l = [[0.0f64; 16]; 16];
        for i in 0..16 {
            let vi = self.0[i];
            if vi == 0.0 {
                continue;
            }
            for j in 0..16 {
                let (sign, k) = RAW_MUL[i][j];
                l[k][j] += f64::from(sign) * vi;
            }
        }
        l
    }

    /// $\mathrm{G}_2$ calibrated 3-form evaluation: $\phi_3(v) = v_1 v_9 - v_2 v_8 + v_3 v_{11} - v_4 v_{10}$.
    pub fn calibrated_3form(&self) -> f64 {
        self.0[1] * self.0[9] - self.0[2] * self.0[8] + self.0[3] * self.0[11] - self.0[4] * self.0[10]
    }

    /// Annihilator product norm $\|a \cdot b\|_2^2$.
    ///
    /// Equals 0 if and only if $(a, b)$ is an exact zero-divisor pair (LNSPP-G2).
    pub fn annihilation_norm_sq(&self, other: &Self) -> f64 {
        self.mul(other).norm_sq()
    }

    /// Check if two sedenions form a zero-divisor / annihilator pair within numerical tolerance $\epsilon$.
    pub fn is_annihilator_pair(&self, other: &Self, eps: f64) -> bool {
        self.annihilation_norm_sq(other) <= eps
    }

    /// Cayley–Dickson doubling involution $\sigma(a + b e_8) = a - b e_8$.
    #[must_use]
    pub fn doubling_involution(&self) -> Self {
        let mut out = self.0;
        for x in &mut out[8..] {
            *x = -*x;
        }
        Sedenion(out)
    }

    /// Certified 1-bit fibre parity: $h = 1$ iff the upper octonion fibre
    /// carries at least as much energy as the lower fibre.
    #[must_use]
    pub fn fibre_parity_bit(&self) -> u8 {
        let lo: f64 = self.0[..8].iter().map(|x| x * x).sum();
        let hi: f64 = self.0[8..].iter().map(|x| x * x).sum();
        u8::from(hi >= lo)
    }
}

impl std::ops::Mul for Sedenion {
    type Output = Sedenion;
    fn mul(self, rhs: Sedenion) -> Sedenion {
        Sedenion::mul(&self, &rhs)
    }
}

/// Compute the True Nullity Energy $E(v) \in [0, 8]$: sum of the 4 smallest eigenvalues of $L_v^\top L_v$.
///
/// $E(v) = 0$ if and only if $v \in \mathcal{ZD}(\mathbb{S}^{14})$ is on the zero-divisor manifold.
pub fn nullity_energy(v: &Sedenion) -> f64 {
    let l = v.left_matrix();
    let mut g = [[0.0f64; 16]; 16];
    for i in 0..16 {
        for j in i..16 {
            let mut s = 0.0;
            for k in 0..16 {
                s += l[k][i] * l[k][j];
            }
            g[i][j] = s;
            g[j][i] = s;
        }
    }
    let mut ev = sym_eigenvalues_16(g);
    ev.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ev[0].max(0.0) + ev[1].max(0.0) + ev[2].max(0.0) + ev[3].max(0.0)
}

/// Cyclic Jacobi eigenvalue solver for symmetric $16 \times 16$ Gram matrices.
fn sym_eigenvalues_16(mut a: [[f64; 16]; 16]) -> [f64; 16] {
    for _sweep in 0..40 {
        let mut off = 0.0;
        for p in 0..16 {
            for q in (p + 1)..16 {
                off += a[p][q] * a[p][q];
            }
        }
        if off < 1e-24 {
            break;
        }
        for p in 0..15 {
            for q in (p + 1)..16 {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let sgn = if theta >= 0.0 { 1.0 } else { -1.0 };
                let t = sgn / (theta.abs() + libm::sqrt(theta * theta + 1.0));
                let c = 1.0 / libm::sqrt(t * t + 1.0);
                let s = t * c;
                for k in 0..16 {
                    let akp = a[k][p];
                    let akq = a[k][q];
                    a[k][p] = c * akp - s * akq;
                    a[k][q] = s * akp + c * akq;
                }
                for k in 0..16 {
                    let apk = a[p][k];
                    let aqk = a[q][k];
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
            }
        }
    }
    let mut ev = [0.0f64; 16];
    for i in 0..16 {
        ev[i] = a[i][i];
    }
    ev
}

/// LNSPP-G2 Certified Context Mask (UT-2, UT-10):
///
/// Filters an $L_q \times L_k$ cross-attention matrix by testing whether query latent $q_i$
/// and key latent $k_j$ annihilate on $\mathbb{S}^{14}$.
///
/// If $\|L_{S(q_i)} S(k_j)\|^2 < \text{threshold}$, the pair is in an orthogonal zero-divisor
/// fiber (zero topological interference) and `mask[i][j] = false` (pruned).
/// Otherwise `mask[i][j] = true` (active context).
pub fn certified_context_mask(
    query_latents: &[Vec<f64>],
    key_latents: &[Vec<f64>],
    threshold: f64,
) -> Vec<Vec<bool>> {
    let l_q = query_latents.len();
    let l_k = key_latents.len();
    let mut mask = vec![vec![true; l_k]; l_q];

    let q_sedenions: Vec<Sedenion> = query_latents.iter().map(|z| Sedenion::from_latent(z)).collect();
    let k_sedenions: Vec<Sedenion> = key_latents.iter().map(|z| Sedenion::from_latent(z)).collect();

    for (i, q_s) in q_sedenions.iter().enumerate() {
        for (j, k_s) in k_sedenions.iter().enumerate() {
            // Table first (inference). Only a table-hit is walked:
            // prune iff the recursive CD fibre agrees it is a zero divisor.
            let table = q_s.mul_table(k_s).norm_sq();
            if table < threshold {
                let cert = q_s.certify(k_s, threshold);
                if cert.agrees && cert.walk_norm_sq < threshold {
                    mask[i][j] = false;
                }
            }
        }
    }

    mask
}

/// Dual-path certificate: table product, CD walk product, and the two
/// octonion-fibre norms of the walk. Inference reads `table_norm_sq`;
/// intelligence is `agrees` (the walk compiled the same algebra).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnnihilatorCertificate {
    pub table_norm_sq: f64,
    pub walk_norm_sq: f64,
    /// $\|a_1 b_1 - \bar b_2 a_2\|_2^2$ — lower CD octonion fibre.
    pub fibre_lo_norm_sq: f64,
    /// $\|b_2 a_1 + a_2 \bar b_1\|_2^2$ — upper CD octonion fibre.
    pub fibre_hi_norm_sq: f64,
    pub agrees: bool,
}

/// Canonical zero divisor pair generator on $\mathbb{S}^{14}$ (LNSPP-G2 canonical form):
///
/// $e_1 + e_{10}$ and $e_5 + e_{14}$ are orthogonal unit sedenions whose product is zero:
/// $(e_1 + e_{10})(e_5 + e_{14}) = 0$.
pub fn canonical_zero_divisor_pair() -> (Sedenion, Sedenion) {
    let inv_sqrt2 = 1.0 / libm::sqrt(2.0);
    let mut a = [0.0; 16];
    let mut b = [0.0; 16];
    a[1] = inv_sqrt2;
    a[10] = inv_sqrt2;
    b[5] = inv_sqrt2;
    b[14] = inv_sqrt2;
    (Sedenion(a), Sedenion(b))
}

// ---- Recursive Cayley–Dickson fibres (octonion → quaternion → complex) ----

fn split_8(arr: &[f64; 16]) -> ([f64; 8], [f64; 8]) {
    let mut a = [0.0; 8];
    let mut b = [0.0; 8];
    a.copy_from_slice(&arr[0..8]);
    b.copy_from_slice(&arr[8..16]);
    (a, b)
}

fn join_8(a: &[f64; 8], b: &[f64; 8]) -> [f64; 16] {
    let mut out = [0.0; 16];
    out[0..8].copy_from_slice(a);
    out[8..16].copy_from_slice(b);
    out
}

fn add_8(a: &[f64; 8], b: &[f64; 8]) -> [f64; 8] {
    let mut out = [0.0; 8];
    for i in 0..8 {
        out[i] = a[i] + b[i];
    }
    out
}

fn sub_8(a: &[f64; 8], b: &[f64; 8]) -> [f64; 8] {
    let mut out = [0.0; 8];
    for i in 0..8 {
        out[i] = a[i] - b[i];
    }
    out
}

fn conj_oct(a: &[f64; 8]) -> [f64; 8] {
    let mut out = *a;
    for x in &mut out[1..8] {
        *x = -*x;
    }
    out
}

fn mul_oct(a: &[f64; 8], b: &[f64; 8]) -> [f64; 8] {
    let mut a1 = [0.0; 4];
    let mut a2 = [0.0; 4];
    let mut b1 = [0.0; 4];
    let mut b2 = [0.0; 4];
    a1.copy_from_slice(&a[0..4]);
    a2.copy_from_slice(&a[4..8]);
    b1.copy_from_slice(&b[0..4]);
    b2.copy_from_slice(&b[4..8]);

    let a1_b1 = mul_quat(&a1, &b1);
    let b2_conj_a2 = mul_quat(&conj_quat(&b2), &a2);
    let mut lo = [0.0; 4];
    for i in 0..4 {
        lo[i] = a1_b1[i] - b2_conj_a2[i];
    }

    let b2_a1 = mul_quat(&b2, &a1);
    let a2_b1_conj = mul_quat(&a2, &conj_quat(&b1));
    let mut hi = [0.0; 4];
    for i in 0..4 {
        hi[i] = b2_a1[i] + a2_b1_conj[i];
    }

    let mut out = [0.0; 8];
    out[0..4].copy_from_slice(&lo);
    out[4..8].copy_from_slice(&hi);
    out
}

fn conj_quat(a: &[f64; 4]) -> [f64; 4] {
    [a[0], -a[1], -a[2], -a[3]]
}

fn mul_quat(a: &[f64; 4], b: &[f64; 4]) -> [f64; 4] {
    let a1 = [a[0], a[1]];
    let a2 = [a[2], a[3]];
    let b1 = [b[0], b[1]];
    let b2 = [b[2], b[3]];
    let a1_b1 = mul_cplx(&a1, &b1);
    let b2_conj = [b2[0], -b2[1]];
    let b2_conj_a2 = mul_cplx(&b2_conj, &a2);
    let lo = [a1_b1[0] - b2_conj_a2[0], a1_b1[1] - b2_conj_a2[1]];
    let b2_a1 = mul_cplx(&b2, &a1);
    let b1_conj = [b1[0], -b1[1]];
    let a2_b1_conj = mul_cplx(&a2, &b1_conj);
    let hi = [b2_a1[0] + a2_b1_conj[0], b2_a1[1] + a2_b1_conj[1]];
    [lo[0], lo[1], hi[0], hi[1]]
}

fn mul_cplx(a: &[f64; 2], b: &[f64; 2]) -> [f64; 2] {
    [a[0] * b[0] - a[1] * b[1], a[0] * b[1] + a[1] * b[0]]
}
