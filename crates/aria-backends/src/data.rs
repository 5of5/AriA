//! Real-data ingestion: bytes → optical fields.
//!
//! The encoding is native to the substrate rather than bolted on. A field is
//! the *spectrum* of a text window: each byte contributes a phasor at every
//! mode, so the inner product between two fields is precisely the spectral
//! similarity of their windows. That is the quantity an optical interferometer
//! computes in one pass — the encoding exists so that interference is meaning.
//!
//! The Spec does not know about this path, and does not need to: data enters
//! only as training fields and (optionally) initial conditions. It defines no
//! action and enlarges nothing.
//!
//! Invariants of the encoding, all of which the tests assert:
//!
//! * deterministic — same bytes in, same fields out, on every platform up to
//!   the last ulp of `sin`/`cos`;
//! * unit norm — `‖ψ‖ = 1`, so the worst-case Inv2 jump after an OpticalStep
//!   stays `≤ 2·Lip(P)` exactly as for the synthetic path;
//! * no information loss at window granularity — the DFT is invertible, so the
//!   window is recoverable from the field (up to normalization and the global
//!   phase lost to byte-centering).

use num_complex::Complex64;
use std::cell::RefCell;
use std::f64::consts::TAU;
use std::path::Path;

/// A dataset of real-data field sequences, in the training format.
///
/// Shares its shape with the synthetic optical dataset so the training loop
/// consumes both unchanged; `format` records which one it is.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FieldDataset {
    pub format: String,
    pub n_modes: usize,
    /// Which encoder produced the frames.
    pub encoding: String,
    /// What the bytes were read from.
    pub source: String,
    /// How many source bytes were consumed.
    pub source_bytes: usize,
    /// One trajectory: `[frame][2·n_modes]` flattened as [re₀, im₀, …].
    pub trajectories: Vec<Vec<Vec<f64>>>,
}

/// Encode one window of bytes as a unit-norm optical field on `n_modes` modes.
///
/// `ψ[m] ∝ Σⱼ xⱼ e^{−2πi·mj/N}` where `xⱼ` is the centered byte value. The
/// result is the DFT spectrum of the window, normalized to ‖ψ‖ = 1.
pub fn encode_window(window: &[u8], n_modes: usize) -> Vec<Complex64> {
    debug_assert!(n_modes >= 1);
    let n = n_modes as f64;
    let phasors: Vec<Vec<Complex64>> = (0..n_modes)
        .map(|m| {
            (0..window.len())
                .map(|j| {
                    let angle = -TAU * (m as f64) * (j as f64) / n;
                    Complex64::new(libm::cos(angle), libm::sin(angle)) / n.sqrt()
                })
                .collect()
        })
        .collect();
    encode_window_with(window, n_modes, &phasors)
}

/// Encode with a precomputed phasor table (`phasors[m][j] = e^{−2πi·mj/N}/√N`).
fn encode_window_with(window: &[u8], n_modes: usize, phasors: &[Vec<Complex64>]) -> Vec<Complex64> {
    let n = n_modes as f64;
    let mut psi: Vec<Complex64> = phasors
        .iter()
        .map(|row| {
            row.iter()
                .zip(window)
                .map(|(p, &b)| {
                    let x = (f64::from(b) - 127.5) / 127.5;
                    Complex64::new(p.re * x, p.im * x)
                })
                .sum()
        })
        .collect();

    // Unit norm: keeps the Inv2 worst case at 2·Lip(P) regardless of content.
    let norm = psi
        .iter()
        .map(num_complex::Complex::norm_sqr)
        .sum::<f64>()
        .sqrt();
    if norm > 0.0 {
        for c in &mut psi {
            *c /= Complex64::new(norm, 0.0);
        }
    } else {
        // A window whose centered bytes cancel exactly has no spectral energy;
        // emit the unit-norm alternating field instead of a zero vector.
        let a = 1.0 / n.sqrt();
        for (m, c) in psi.iter_mut().enumerate() {
            *c = Complex64::new(if m % 2 == 0 { a } else { -a }, 0.0);
        }
    }
    psi
}

/// Split bytes into windows of `n_modes` with the given stride and encode each.
///
/// `stride` smaller than the window overlaps windows; the default should be
/// the window itself (non-overlapping). The final partial window is kept when
/// it holds at least 8 bytes — short tails carry almost no signal but do keep
/// real corpus length faithful.
pub fn encode_corpus(
    bytes: &[u8],
    n_modes: usize,
    stride: usize,
) -> Result<Vec<Vec<Complex64>>, String> {
    if n_modes < 2 {
        return Err("n_modes must be ≥ 2 for a spectral encoding".into());
    }
    if stride == 0 {
        return Err("stride must be ≥ 1".into());
    }
    if bytes.len() < 8 {
        return Err(format!(
            "corpus too small: {} bytes (need ≥ 8 for a single frame)",
            bytes.len()
        ));
    }

    // The phasor table e^{−2πi·mj/N} depends only on (n_modes, window length),
    // not on the window contents — computing it per window would be billions of
    // redundant trig calls on a large corpus.
    let n = n_modes as f64;
    let phasors: Vec<Vec<Complex64>> = (0..n_modes)
        .map(|m| {
            (0..n_modes)
                .map(|j| {
                    let angle = -TAU * (m as f64) * (j as f64) / n;
                    Complex64::new(libm::cos(angle), libm::sin(angle)) / n.sqrt()
                })
                .collect()
        })
        .collect();

    let mut fields = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let end = (start + n_modes).min(bytes.len());
        if end - start < 8 {
            break;
        }
        fields.push(encode_window_with(&bytes[start..end], n_modes, &phasors));
        start += stride;
    }
    Ok(fields)
}

/// Encode a whole corpus file into a training dataset.
pub fn dataset_from_bytes(
    source: &str,
    bytes: &[u8],
    n_modes: usize,
    stride: usize,
) -> Result<FieldDataset, String> {
    let fields = encode_corpus(bytes, n_modes, stride)?;
    let frames: Vec<Vec<f64>> = fields.iter().map(|f| flatten(f)).collect();
    Ok(FieldDataset {
        format: "aria-text-dataset-v1".into(),
        n_modes,
        encoding: "spectral-dft".into(),
        source: source.into(),
        source_bytes: bytes.len(),
        trajectories: vec![frames],
    })
}

/// Read and encode a corpus file.
pub fn dataset_from_file(
    path: &Path,
    n_modes: usize,
    stride: usize,
) -> Result<FieldDataset, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    dataset_from_bytes(&path.display().to_string(), &bytes, n_modes, stride)
}

fn flatten(psi: &[Complex64]) -> Vec<f64> {
    let mut v = Vec::with_capacity(psi.len() * 2);
    for c in psi {
        v.push(c.re);
        v.push(c.im);
    }
    v
}

/// In-repo columnar artifact magic (`aria-text-dataset-v1` frames, raw LE f64).
/// UT-0 fallback: no polars, no new crate, wasm-clean.
pub const COLUMNAR_MAGIC: &[u8; 8] = b"ARIACOL1";

/// Encode a [`FieldDataset`]'s frames as a compact little-endian columnar
/// blob. The JSON `aria-text-dataset-v1` path remains the identity referee;
/// this is the fast write the 10× ingest predicate times against.
fn put_bytes(buf: &mut [u8], pos: &mut usize, src: &[u8]) {
    let end = *pos + src.len();
    buf[*pos..end].copy_from_slice(src);
    *pos = end;
}

pub fn encode_columnar(ds: &FieldDataset) -> Vec<u8> {
    let frames: &[Vec<f64>] = ds.trajectories.first().map_or(&[], Vec::as_slice);
    let n_frames = frames.len() as u64;
    let n_modes = ds.n_modes as u64;
    let frame_dim = ds.n_modes.saturating_mul(2);
    let src = ds.source.as_bytes();
    let enc = ds.encoding.as_bytes();
    let payload = frames.len().saturating_mul(frame_dim).saturating_mul(8);
    let total = 8 + 8 + 8 + 8 + 8 + src.len() + 8 + enc.len() + payload;
    let mut out = vec![0u8; total];
    let mut pos = 0usize;
    put_bytes(&mut out, &mut pos, COLUMNAR_MAGIC);
    put_bytes(&mut out, &mut pos, &n_modes.to_le_bytes());
    put_bytes(&mut out, &mut pos, &(ds.source_bytes as u64).to_le_bytes());
    put_bytes(&mut out, &mut pos, &n_frames.to_le_bytes());
    put_bytes(&mut out, &mut pos, &(src.len() as u64).to_le_bytes());
    put_bytes(&mut out, &mut pos, src);
    put_bytes(&mut out, &mut pos, &(enc.len() as u64).to_le_bytes());
    put_bytes(&mut out, &mut pos, enc);
    for frame in frames {
        for &x in frame {
            put_bytes(&mut out, &mut pos, &x.to_le_bytes());
        }
    }
    debug_assert_eq!(pos, total);
    out
}

fn read_u64_le(cur: &mut &[u8], what: &str) -> Result<u64, String> {
    if cur.len() < 8 {
        return Err(format!("columnar {what}: truncated"));
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&cur[..8]);
    *cur = &cur[8..];
    Ok(u64::from_le_bytes(buf))
}

fn read_exact<'a>(cur: &mut &'a [u8], n: usize, what: &str) -> Result<&'a [u8], String> {
    if cur.len() < n {
        return Err(format!("columnar {what}: truncated"));
    }
    let (head, rest) = cur.split_at(n);
    *cur = rest;
    Ok(head)
}

/// Decode a blob produced by [`encode_columnar`] back to a [`FieldDataset`].
/// Frames are the raw f64 bit patterns — no decimal round-trip.
pub fn decode_columnar(bytes: &[u8]) -> Result<FieldDataset, String> {
    let mut cur = bytes;
    if read_exact(&mut cur, 8, "magic")? != COLUMNAR_MAGIC {
        return Err("columnar magic mismatch (expected ARIACOL1)".into());
    }
    let n_modes = usize::try_from(read_u64_le(&mut cur, "n_modes")?)
        .map_err(|_| "columnar n_modes overflow".to_string())?;
    let source_bytes = usize::try_from(read_u64_le(&mut cur, "source_bytes")?)
        .map_err(|_| "columnar source_bytes overflow".to_string())?;
    let n_frames = usize::try_from(read_u64_le(&mut cur, "n_frames")?)
        .map_err(|_| "columnar n_frames overflow".to_string())?;
    let src_len = usize::try_from(read_u64_le(&mut cur, "source_len")?)
        .map_err(|_| "columnar source_len overflow".to_string())?;
    let source = std::str::from_utf8(read_exact(&mut cur, src_len, "source")?)
        .map_err(|e| format!("columnar source: {e}"))?
        .to_owned();
    let enc_len = usize::try_from(read_u64_le(&mut cur, "encoding_len")?)
        .map_err(|_| "columnar encoding_len overflow".to_string())?;
    let encoding = std::str::from_utf8(read_exact(&mut cur, enc_len, "encoding")?)
        .map_err(|e| format!("columnar encoding: {e}"))?
        .to_owned();
    let frame_dim = n_modes.saturating_mul(2);
    let need = n_frames.saturating_mul(frame_dim).saturating_mul(8);
    if cur.len() != need {
        return Err(format!(
            "columnar payload: got {} bytes, expected {need}",
            cur.len()
        ));
    }
    let mut frames = Vec::with_capacity(n_frames);
    let mut off = 0usize;
    for _ in 0..n_frames {
        let mut frame = Vec::with_capacity(frame_dim);
        for _ in 0..frame_dim {
            let mut word = [0u8; 8];
            word.copy_from_slice(&cur[off..off + 8]);
            frame.push(f64::from_le_bytes(word));
            off += 8;
        }
        frames.push(frame);
    }
    Ok(FieldDataset {
        format: "aria-text-dataset-v1".into(),
        n_modes,
        encoding,
        source,
        source_bytes,
        trajectories: vec![frames],
    })
}

/// Phasor table for the fused DFT: `pre[j * n_modes + m]` is the real part
/// of mode `m` tap `j` so the hot loop can walk modes contiguously.
/// Values are bit-identical to `Complex64::new(cos, sin) / n.sqrt()`.
fn fused_phasors(n_modes: usize) -> (Vec<f64>, Vec<f64>) {
    let n = n_modes as f64;
    let scale = n.sqrt();
    let mut pre = vec![0.0; n_modes * n_modes];
    let mut pim = vec![0.0; n_modes * n_modes];
    #[allow(clippy::needless_range_loop)]
    for m in 0..n_modes {
        for j in 0..n_modes {
            let angle = -TAU * (m as f64) * (j as f64) / n;
            let c = Complex64::new(libm::cos(angle), libm::sin(angle)) / scale;
            // Store tap-major so the m-inner accumulation is sequential.
            let idx = j * n_modes + m;
            pre[idx] = c.re;
            pim[idx] = c.im;
        }
    }
    (pre, pim)
}

type PhasorCache = Option<(usize, Vec<f64>, Vec<f64>)>;

thread_local! {
    static FUSED_PHASORS: RefCell<PhasorCache> =
        const { RefCell::new(None) };
}

/// Reuse the last fused phasor table. Training and ingest hit one `n_modes`
/// for the whole job; rebuilding  N²  `libm` cos/sin on every batch is waste.
fn with_fused_phasors<R>(n_modes: usize, f: impl FnOnce(&[f64], &[f64]) -> R) -> R {
    FUSED_PHASORS.with(|slot| {
        let mut cache = slot.borrow_mut();
        let miss = cache.as_ref().is_none_or(|(n, _, _)| *n != n_modes);
        if miss {
            let (pre, pim) = fused_phasors(n_modes);
            *cache = Some((n_modes, pre, pim));
        }
        let (_, pre, pim) = cache.as_ref().expect("just filled");
        f(pre, pim)
    })
}

fn count_windows(len: usize, n_modes: usize, stride: usize) -> usize {
    let mut n = 0usize;
    let mut start = 0usize;
    while start < len {
        let end = (start + n_modes).min(len);
        if end - start < 8 {
            break;
        }
        n += 1;
        start += stride;
    }
    n
}

/// Flattened-frame DFT with the same IEEE stream as [`encode_corpus`] +
/// [`flatten`]. Modes are independent: tap `j` is accumulated in order
/// `j = 0, 1, …` for every mode, matching the referee's per-mode `sum`.
/// The inner index is the mode so LLVM can vectorise across modes
/// without changing any mode's addition order.
fn encode_frames_fused(
    bytes: &[u8],
    n_modes: usize,
    stride: usize,
) -> Result<Vec<Vec<f64>>, String> {
    if n_modes < 2 {
        return Err("n_modes must be ≥ 2 for a spectral encoding".into());
    }
    if stride == 0 {
        return Err("stride must be ≥ 1".into());
    }
    if bytes.len() < 8 {
        return Err(format!(
            "corpus too small: {} bytes (need ≥ 8 for a single frame)",
            bytes.len()
        ));
    }

    let n = n_modes as f64;
    if n_modes == 64 {
        return with_fused_phasors(64, |pre, pim| {
            Ok(encode_frames_n64(bytes, stride, n, pre, pim))
        });
    }
    with_fused_phasors(n_modes, |pre, pim| {
        let n_frames = count_windows(bytes.len(), n_modes, stride);
        let mut frames = Vec::with_capacity(n_frames);
        let mut acc_re = vec![0.0; n_modes];
        let mut acc_im = vec![0.0; n_modes];

        let mut start = 0;
        while start < bytes.len() {
            let end = (start + n_modes).min(bytes.len());
            if end - start < 8 {
                break;
            }
            let window = &bytes[start..end];
            let wlen = window.len();
            acc_re.fill(0.0);
            acc_im.fill(0.0);
            // Same add order per mode as `for j in 0..wlen { re += p[m][j] * x[j] }`.
            #[allow(clippy::needless_range_loop)]
            for j in 0..wlen {
                let x = (f64::from(window[j]) - 127.5) / 127.5;
                let base = j * n_modes;
                for m in 0..n_modes {
                    acc_re[m] += pre[base + m] * x;
                    acc_im[m] += pim[base + m] * x;
                }
            }

            let mut energy = 0.0;
            #[allow(clippy::needless_range_loop)]
            for m in 0..n_modes {
                energy += acc_re[m] * acc_re[m] + acc_im[m] * acc_im[m];
            }
            let norm = energy.sqrt();
            let mut frame = vec![0.0; n_modes * 2];
            if norm > 0.0 {
                // Referee: `c /= Complex64::new(norm, 0.0)` → (re·n)/(n·n).
                let denom = norm * norm;
                #[allow(clippy::needless_range_loop)]
                for m in 0..n_modes {
                    frame[2 * m] = (acc_re[m] * norm) / denom;
                    frame[2 * m + 1] = (acc_im[m] * norm) / denom;
                }
            } else {
                let a = 1.0 / n.sqrt();
                #[allow(clippy::needless_range_loop)]
                for m in 0..n_modes {
                    frame[2 * m] = if m % 2 == 0 { a } else { -a };
                    frame[2 * m + 1] = 0.0;
                }
            }
            frames.push(frame);
            start += stride;
        }
        Ok(frames)
    })
}

/// N = 64 specialized: `[f64; 64]` accumulators so the mode loop is a
/// closed constant and LLVM can vectorise it. Addition order per mode is
/// still tap `j = 0, 1, …` — bit-identical to the general fused path.
fn encode_frames_n64(
    bytes: &[u8],
    stride: usize,
    n: f64,
    pre: &[f64],
    pim: &[f64],
) -> Vec<Vec<f64>> {
    const N: usize = 64;
    let n_frames = count_windows(bytes.len(), N, stride);
    let mut frames = Vec::with_capacity(n_frames);
    let mut start = 0;
    while start < bytes.len() {
        let end = (start + N).min(bytes.len());
        if end - start < 8 {
            break;
        }
        let window = &bytes[start..end];
        let wlen = window.len();
        let mut acc_re = [0.0f64; N];
        let mut acc_im = [0.0f64; N];
        #[allow(clippy::needless_range_loop)]
        for j in 0..wlen {
            let x = (f64::from(window[j]) - 127.5) / 127.5;
            let base = j * N;
            for m in 0..N {
                acc_re[m] += pre[base + m] * x;
                acc_im[m] += pim[base + m] * x;
            }
        }
        let mut energy = 0.0;
        for m in 0..N {
            energy += acc_re[m] * acc_re[m] + acc_im[m] * acc_im[m];
        }
        let norm = energy.sqrt();
        let mut frame = vec![0.0; N * 2];
        if norm > 0.0 {
            let denom = norm * norm;
            for m in 0..N {
                frame[2 * m] = (acc_re[m] * norm) / denom;
                frame[2 * m + 1] = (acc_im[m] * norm) / denom;
            }
        } else {
            let a = 1.0 / n.sqrt();
            for m in 0..N {
                frame[2 * m] = if m % 2 == 0 { a } else { -a };
                frame[2 * m + 1] = 0.0;
            }
        }
        frames.push(frame);
        start += stride;
    }
    frames
}

/// Fast ingest: fused DFT (bit-identical to [`dataset_from_bytes`]) plus
/// columnar write. This is the shipped UT-0 entry — time *this* against
/// `dataset_from_bytes` + serde-JSON, not the serializer alone.
pub fn ingest_columnar(
    source: &str,
    bytes: &[u8],
    n_modes: usize,
    stride: usize,
) -> Result<(FieldDataset, Vec<u8>), String> {
    if n_modes == 64 {
        return ingest_columnar_n64(source, bytes, stride);
    }
    let frames = encode_frames_fused(bytes, n_modes, stride)?;
    let ds = FieldDataset {
        format: "aria-text-dataset-v1".into(),
        n_modes,
        encoding: "spectral-dft".into(),
        source: source.into(),
        source_bytes: bytes.len(),
        trajectories: vec![frames],
    };
    let blob = encode_columnar(&ds);
    Ok((ds, blob))
}

/// N = 64: DFT into a stack frame, write LE payload in the same pass.
fn ingest_columnar_n64(
    source: &str,
    bytes: &[u8],
    stride: usize,
) -> Result<(FieldDataset, Vec<u8>), String> {
    const N: usize = 64;
    const DIM: usize = 128;
    if stride == 0 {
        return Err("stride must be ≥ 1".into());
    }
    if bytes.len() < 8 {
        return Err(format!(
            "corpus too small: {} bytes (need ≥ 8 for a single frame)",
            bytes.len()
        ));
    }
    let n = N as f64;
    with_fused_phasors(N, |pre, pim| {
        let n_frames = count_windows(bytes.len(), N, stride);
        let mut slab = vec![0.0f64; n_frames * DIM];
        fill_slab_n64(&mut slab, bytes, stride, n_frames, n, pre, pim);
        let src = source.as_bytes();
        let enc = b"spectral-dft";
        let header = 8 + 8 + 8 + 8 + 8 + src.len() + 8 + enc.len();
        let mut blob = vec![0u8; header + n_frames * DIM * 8];
        let mut pos = 0usize;
        put_bytes(&mut blob, &mut pos, COLUMNAR_MAGIC);
        put_bytes(&mut blob, &mut pos, &(N as u64).to_le_bytes());
        put_bytes(&mut blob, &mut pos, &(bytes.len() as u64).to_le_bytes());
        put_bytes(&mut blob, &mut pos, &(n_frames as u64).to_le_bytes());
        put_bytes(&mut blob, &mut pos, &(src.len() as u64).to_le_bytes());
        put_bytes(&mut blob, &mut pos, src);
        put_bytes(&mut blob, &mut pos, &(enc.len() as u64).to_le_bytes());
        put_bytes(&mut blob, &mut pos, enc);
        for &x in &slab {
            put_bytes(&mut blob, &mut pos, &x.to_le_bytes());
        }
        debug_assert_eq!(pos, blob.len());
        let mut frames = Vec::with_capacity(n_frames);
        for i in 0..n_frames {
            frames.push(slab[i * DIM..(i + 1) * DIM].to_vec());
        }
        Ok((
            FieldDataset {
                format: "aria-text-dataset-v1".into(),
                n_modes: N,
                encoding: "spectral-dft".into(),
                source: source.into(),
                source_bytes: bytes.len(),
                trajectories: vec![frames],
            },
            blob,
        ))
    })
}

fn fill_slab_n64(
    slab: &mut [f64],
    bytes: &[u8],
    stride: usize,
    n_frames: usize,
    n: f64,
    pre: &[f64],
    pim: &[f64],
) {
    const DIM: usize = 128;
    #[cfg(not(target_arch = "wasm32"))]
    {
        let threads = std::thread::available_parallelism()
            .map_or(1, |p| p.get().clamp(1, 4));
        if threads > 1 && n_frames >= threads * 8 {
            let chunk = n_frames.div_ceil(threads);
            std::thread::scope(|scope| {
                let mut rest = &mut slab[..];
                let mut frame0 = 0usize;
                for _ in 0..threads {
                    if frame0 >= n_frames {
                        break;
                    }
                    let take = chunk.min(n_frames - frame0);
                    let (head, tail) = rest.split_at_mut(take * DIM);
                    rest = tail;
                    let f0 = frame0;
                    scope.spawn(move || {
                        encode_slab_range_n64(head, bytes, stride, f0, take, n, pre, pim);
                    });
                    frame0 += take;
                }
            });
            return;
        }
    }
    encode_slab_range_n64(slab, bytes, stride, 0, n_frames, n, pre, pim);
}

#[allow(clippy::too_many_arguments)]
fn encode_slab_range_n64(
    slab: &mut [f64],
    bytes: &[u8],
    stride: usize,
    frame0: usize,
    n_take: usize,
    n: f64,
    pre: &[f64],
    pim: &[f64],
) {
    const N: usize = 64;
    const DIM: usize = 128;
    for local in 0..n_take {
        let fi = frame0 + local;
        let start = fi * stride;
        let end = (start + N).min(bytes.len());
        if end - start < 8 {
            break;
        }
        let window = &bytes[start..end];
        let wlen = window.len();
        let mut acc_re = [0.0f64; N];
        let mut acc_im = [0.0f64; N];
        #[allow(clippy::needless_range_loop)]
        for j in 0..wlen {
            let x = (f64::from(window[j]) - 127.5) / 127.5;
            let pr = &pre[j * N..j * N + N];
            let pi = &pim[j * N..j * N + N];
            for m in 0..N {
                acc_re[m] += pr[m] * x;
                acc_im[m] += pi[m] * x;
            }
        }
        let mut energy = 0.0;
        for m in 0..N {
            energy += acc_re[m] * acc_re[m] + acc_im[m] * acc_im[m];
        }
        let norm = energy.sqrt();
        let slot = &mut slab[local * DIM..(local + 1) * DIM];
        if norm > 0.0 {
            let denom = norm * norm;
            for m in 0..N {
                slot[2 * m] = (acc_re[m] * norm) / denom;
                slot[2 * m + 1] = (acc_im[m] * norm) / denom;
            }
        } else {
            let a = 1.0 / n.sqrt();
            for m in 0..N {
                slot[2 * m] = if m % 2 == 0 { a } else { -a };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(psi: &[Complex64]) -> f64 {
        psi.iter()
            .map(num_complex::Complex::norm_sqr)
            .sum::<f64>()
            .sqrt()
    }

    #[test]
    fn windows_have_unit_norm() {
        for bytes in [
            b"hello aria".as_slice(),
            &[0u8; 64],
            b"\xff".repeat(100).as_slice(),
        ] {
            let psi = encode_window(bytes, 64);
            assert!((norm(&psi) - 1.0).abs() < 1e-12, "‖ψ‖ = {}", norm(&psi));
        }
    }

    #[test]
    fn encoding_is_deterministic() {
        let a = encode_window(b"the quick brown fox", 32);
        let b = encode_window(b"the quick brown fox", 32);
        assert_eq!(a, b);
        let c = encode_window(b"the quick brown foy", 32);
        assert_ne!(a, c, "one byte must move the field");
    }

    #[test]
    fn similar_windows_interfere_constructively() {
        // The whole point of the encoding: |⟨ψ₁, ψ₂⟩| is a similarity score.
        let a = encode_window(b"aria is an optical jepa graph dynamical system", 64);
        let b = encode_window(b"aria is an optical jepa graph dynamical system", 64);
        let c = encode_window(b"completely unrelated text about something else", 64);

        let sim = |x: &[Complex64], y: &[Complex64]| {
            let dot: Complex64 = x.iter().zip(y).map(|(p, q)| p.conj() * q).sum();
            dot.norm()
        };

        let identical = sim(&a, &b);
        let different = sim(&a, &c);
        assert!(identical > 0.999_999, "⟨ψ,ψ⟩ = {identical}");
        assert!(
            identical > different,
            "identical windows ({identical}) must interfere more than unrelated ones ({different})"
        );
    }

    #[test]
    fn corpus_encoding_validates_inputs() {
        assert!(encode_corpus(b"hi", 64, 64).is_err());
        assert!(encode_corpus(b"hello world!!", 1, 64).is_err());
        assert!(encode_corpus(b"hello world!!", 64, 0).is_err());
        assert!(encode_corpus(b"hello world, this is a real sentence.", 16, 16).is_ok());
    }

    #[test]
    fn corpus_covers_the_input() {
        let bytes: Vec<u8> = (0..1000u32)
            .map(|i| u8::try_from(i % 256).unwrap())
            .collect();
        let fields = encode_corpus(&bytes, 64, 64).unwrap();
        // 1000 bytes = 15 full 64-byte windows + one 40-byte tail (≥ 8, kept).
        assert_eq!(fields.len(), 16);
        for f in &fields {
            assert!((norm(f) - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn dataset_shape_matches_the_training_format() {
        let d = dataset_from_bytes("test", b"some actual bytes of real text here", 16, 16).unwrap();
        assert_eq!(d.format, "aria-text-dataset-v1");
        assert_eq!(d.trajectories.len(), 1);
        for frame in &d.trajectories[0] {
            assert_eq!(frame.len(), 2 * d.n_modes);
        }
    }
}
