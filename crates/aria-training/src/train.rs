//! The main training loop (WS-A1): seeded init → epoch loop over seeded
//! batches → analytic gradients → AdamW with embedded projection → holdout
//! evaluation against the persistence baseline → `aria-predictor-v1` export.
//!
//! Every stochastic choice flows from `TrainingConfig::seed` through the
//! crate-local LCG; two runs with equal config are bit-identical, including
//! the exported weights (asserted by integration test).
//!
//! Phase-1 semantics (training PRD §Two-Phase Schedule): one isometry W_I and
//! one linear predictor W_P are trained on windowed DFT spectra; at export the
//! single P fills the three conditioned slots (token / diffusion /
//! world_model) — the same artifact shape WS5 shipped (σ = 0.49 × 3).
//! Conditioned differentiation joins in Phase 2 (WS-A3 graph corpus).

use aria_engine_backends::spectral::{power_iteration, DEFAULT_ITERATIONS};
use aria_engine_backends::trained::{ConditionedWeights, PredictorWeights, PREDICTOR_V1_FORMAT};

use crate::dataset::{align_corpus, EdgeAlignment, KgEdges, PreparedDataset};
use crate::linalg::{l2_sq, matvec, matvec_into};
use crate::loss::{compute_batch_with, LossBreakdown, ModelParams, SpectralEval};
use crate::optimizer::AdamW;
use crate::{Lcg, TrainOutcome, TrainingConfig, TrainingError, TRAIN_OUTCOME_FORMAT};

/// Run one training job to completion. Returns the measured outcome and the
/// exported weights (also written to `cfg.output_path` when set).
///
/// One coherent procedure — load → init → epochs → export → measure — kept
/// linear on purpose so the invariant-relevant order (projection before
/// export, holdout after export) is visible at a glance.
#[allow(clippy::too_many_lines)]
pub fn train(cfg: &TrainingConfig) -> Result<(TrainOutcome, PredictorWeights), TrainingError> {
    cfg.validate()?;

    // WS-A1 trains Φ-side parameters only. The ℒ_NLL machinery is implemented
    // and tested (loss::nll_loss_and_grad), but wiring readout heads and
    // next-byte targets into this loop is WS-A2 scope (checkpoint.rs owns the
    // aria-readout-v1 artifact). A silent ignore would fake the term; reject.
    if cfg.lambdas.nll > 0.0 {
        return Err(TrainingError::Config(
            "λ_NLL > 0 requires readout-head training, which lands in WS-A2 \
             (eval.rs/checkpoint.rs wiring). WS-A1 accepts λ_NLL = 0 only."
                .into(),
        ));
    }

    let ds = PreparedDataset::load(cfg)?;

    // Raw corpus bytes: consumed by edge alignment and the readout pass.
    let corpus_bytes: Option<Vec<u8>> = match &cfg.corpus_path {
        Some(path) => Some(std::fs::read(path)?),
        None => None,
    };
    let corpus_sha256 = corpus_bytes.as_deref().map(crate::sha256::sha256_hex);

    // Optional ℒ_Graph substrate (amendment D3).
    let mut edges_checksum = None;
    let mut edges_sha256 = None;
    let mut corpus_checksum = None;
    let mut alignment: Option<EdgeAlignment> = None;
    if let Some(edges_path) = &cfg.edges_path {
        let (kg, kg_fnv, kg_sha) = KgEdges::load(edges_path)?;
        let corpus = corpus_bytes
            .as_deref()
            .expect("validated: edges_path ⇒ corpus_path");
        let align = align_corpus(corpus, cfg.n_modes, cfg.stride, ds.frames.len(), &kg)?;
        edges_checksum = Some(kg_fnv);
        edges_sha256 = Some(kg_sha);
        corpus_checksum = Some(align.corpus_checksum.clone());
        alignment = Some(align);
    }

    // ---- Seeded init: orthonormal-row embed (σ = 1 exactly) deflated
    // against the train-split mean frame, pred strictly inside the ε/2 ball.
    //
    // Why the deflation (measured 2026-08-17, WS-A1 smoke): docs-corpus
    // latents decompose as z = μ + δ with a large common mean (‖μ_z‖² ≈ 36×
    // the variance). Reproducing μ needs predictor gain ≈ 1 — infeasible
    // inside the ℙ2 ball σ ≤ ε/2 — while the *centered* lag-1 correlation is
    // C/V ≈ 0.49, i.e. the optimal centered predictor is feasible (and is
    // exactly the Lip(P) = 0.49 the WS5 receipt measured). Initializing W_I
    // orthogonal to the train-mean direction removes the infeasible-gain
    // direction from step zero; JEPA training then fits the centered AR
    // structure. This is a deterministic, train-split-only initialization
    // (no holdout leakage, no loss change, no runtime centering, no format
    // change — the exported artifact stays a plain linear map).
    //
    // Honest caveat (same measurement): the σ-cap training dynamics re-grow
    // the mean direction on mean-heavy corpora (the ℒ_JEPA mean attractor —
    // see optimizer.rs). The deflation fixes the starting basin and the
    // short-horizon/fixture regime; the standing anti-collapse mechanism is
    // WS-A2's Q-2026-08-17-6.
    let input_dim = ds.frame_dim;
    let d = cfg.latent_dim;
    let lip_bound = cfg.lipschitz_bound();
    let train_mean = train_mean_frame(&ds);
    let mut params = ModelParams {
        embed: orthonormal_rows(
            d,
            input_dim,
            cfg.seed ^ 0xA5A5_A5A5_A5A5_A5A5,
            Some(&train_mean),
        ),
        pred: seeded_in_ball(d, cfg.seed ^ 0x5A5A_5A5A_5A5A_5A5A, 0.5 * lip_bound)?,
    };
    // The deflation constraint is enforced by Π_𝒮 on every step, not only at
    // init — stop-grad would otherwise re-grow the mean direction (see
    // optimizer.rs::with_deflation for the measured drift law).
    let mut opt = AdamW::new(cfg.adamw, &params).with_deflation(&train_mean);

    // Baseline before any step: the seeded model's holdout residual.
    let (holdout_initial, _) = holdout_residuals(&params, &ds);

    // ---- Epoch loop. ----
    let mut epoch_loss: Vec<LossBreakdown> = Vec::with_capacity(cfg.epochs);
    for epoch in 0..cfg.epochs {
        let order = ds.epoch_order(cfg.seed, epoch);
        let mut sums = LossBreakdown::default();
        let mut batches = 0usize;
        for batch in order.chunks(cfg.batch_size) {
            let (breakdown, grads) = compute_batch_with(
                &params,
                &ds,
                batch,
                alignment.as_ref(),
                &cfg.lambdas,
                lip_bound,
                cfg.gamma_tau,
                SpectralEval::CertifiedInBall,
            )?;
            opt.step(&mut params, &grads, lip_bound)?;
            sums.jepa += breakdown.jepa;
            sums.nll += breakdown.nll;
            sums.spectral += breakdown.spectral;
            sums.graph += breakdown.graph;
            sums.total += breakdown.total;
            batches += 1;
        }
        let n = batches.max(1) as f64;
        epoch_loss.push(LossBreakdown {
            jepa: sums.jepa / n,
            nll: sums.nll / n,
            spectral: sums.spectral / n,
            graph: sums.graph / n,
            total: sums.total / n,
        });
    }

    // ---- Export-side fixed point: the exported estimate σ̂ must sit at or
    // under its bound so the loader's defense-in-depth projection is a
    // measured no-op (bit-exact load). A single projection can land 1 ulp
    // above the bound because scaling is not exactly distributive in fp;
    // iterate to the fixed point (converges in ≤ 2 in practice). ----
    params.embed = project_fixed_point(params.embed, 1.0)?;
    params.pred = project_fixed_point(params.pred, lip_bound)?;

    // ---- Final measurements (on the exported parameters). ----
    // One embed pass produces residual, persistence, variance, RankMe Z,
    // and the Wilcoxon pairs — the previous four walks were identical math.
    let holdout = evaluate_holdout(&params, &ds);
    let holdout_final = holdout.residual_model;
    let persistence = holdout.residual_persist;
    let latent_variance = holdout.latent_variance;
    let latent_variance_mean = if latent_variance.is_empty() {
        0.0
    } else {
        latent_variance.iter().sum::<f64>() / latent_variance.len() as f64
    };

    // The three conditioned slots are the same trained P (Phase-1 export).
    let sigma_pred = power_iteration(&params.pred, DEFAULT_ITERATIONS)?;
    let sigma = aria_engine_backends::spectral::SpectralReport {
        embed: power_iteration(&params.embed, DEFAULT_ITERATIONS)?,
        token: sigma_pred,
        diffusion: sigma_pred,
        world_model: sigma_pred,
    };

    // ---- Quality gates (WS-A2). ----
    // RankMe on the holdout latents — a hard abort per the PRD when it fires.
    let rankme = crate::collapse::check_rankme(&holdout.latents, d, cfg.min_rankme_frac)?;
    let rankme_gate = cfg.min_rankme_frac * d as f64;

    // Paired Wilcoxon vs persistence over per-trajectory mean residuals —
    // applicable only at the PRD floor of ≥ 30 holdout trajectories.
    let (per_traj_model, per_traj_persist) = (holdout.per_traj_model, holdout.per_traj_persist);
    let wilcoxon = if per_traj_model.len() >= crate::eval::MIN_TRAJECTORIES {
        Some(crate::eval::wilcoxon_paired(
            &per_traj_model,
            &per_traj_persist,
            cfg.seed,
        )?)
    } else {
        None
    };
    let gates_pass = wilcoxon
        .as_ref()
        .map(|w| w.p_one_sided < 0.01 && w.median_improvement > 0.0);

    // ---- Decoupled readout pass (D7): θ_D only, frozen latents. ----
    let readout = match (&cfg.readout_out, corpus_bytes.as_deref()) {
        (Some(out), Some(corpus)) => Some(readout_pass(cfg, &ds, &params, corpus, out)?),
        _ => None,
    };

    // ---- Artifacts. ----
    let weights = PredictorWeights {
        format: PREDICTOR_V1_FORMAT.into(),
        n_modes: cfg.n_modes,
        latent_dim: cfg.latent_dim,
        lipschitz_bound: lip_bound,
        embed: params.embed.clone(),
        predict: ConditionedWeights {
            token: params.pred.clone(),
            diffusion: params.pred.clone(),
            world_model: params.pred.clone(),
        },
    };
    if let Some(path) = &cfg.output_path {
        crate::checkpoint::write_v1(path, &weights)?;
    }
    let artifact_v2_sha256 = match &cfg.output_v2_path {
        Some(path) => {
            let provenance = crate::checkpoint::Provenance {
                git_sha: git_sha_best_effort(),
                crate_version: env!("CARGO_PKG_VERSION").into(),
                seed: cfg.seed,
                dataset_sha256: ds.checksum_sha256.clone(),
                corpus_sha256: corpus_sha256.clone(),
                edges_sha256: edges_sha256.clone(),
                lambdas: cfg.lambdas.clone(),
                protocol: format!(
                    "n_modes={} d={} stride={} L={} holdout={} epochs={} batch={} lr={} \
                     gamma_tau={} deflation=train-mean",
                    cfg.n_modes,
                    cfg.latent_dim,
                    cfg.stride,
                    cfg.trajectory_len,
                    cfg.holdout_frac,
                    cfg.epochs,
                    cfg.batch_size,
                    cfg.adamw.lr,
                    cfg.gamma_tau
                ),
                map_revision: None,
                ccv_hash: None,
            };
            Some(crate::checkpoint::write_v2(path, &weights, &provenance)?)
        }
        None => None,
    };

    let outcome = TrainOutcome {
        format: TRAIN_OUTCOME_FORMAT.into(),
        seed: cfg.seed,
        epochs_run: cfg.epochs,
        final_loss: epoch_loss.last().map_or(0.0, |b| b.total),
        epoch_loss,
        residual_metric: "mean squared L2 per transition (holdout, final embedding)".into(),
        holdout_residual_initial: holdout_initial,
        holdout_residual_final: holdout_final,
        persistence_residual: persistence,
        sigma,
        lipschitz_bound: lip_bound,
        latent_variance_mean,
        latent_variance,
        edge_coverage: alignment.as_ref().map(|a| a.coverage),
        usable_edges: alignment.as_ref().map(|a| a.usable_edges.len()),
        rankme,
        rankme_gate,
        wilcoxon,
        gates_pass,
        readout,
        dataset_checksum: ds.checksum.clone(),
        dataset_sha256: ds.checksum_sha256.clone(),
        edges_checksum,
        edges_sha256,
        corpus_checksum,
        corpus_sha256,
        artifact_v2_sha256,
        dataset_format: ds.format.clone(),
        dataset_source: ds.source.clone(),
        source_bytes: ds.source_bytes,
        n_train_trajectories: ds.n_train,
        n_holdout_trajectories: ds.chunks.len() - ds.n_train,
        transitions_per_trajectory: cfg.trajectory_len - 1,
    };

    Ok((outcome, weights))
}

/// Holdout measurements from a single embed pass.
struct HoldoutEval {
    residual_model: f64,
    residual_persist: f64,
    latent_variance: Vec<f64>,
    latents: Vec<Vec<f64>>,
    per_traj_model: Vec<f64>,
    per_traj_persist: Vec<f64>,
}

/// Embed every holdout frame once; derive residual, persistence, per-dimension
/// variance, RankMe's Z, and the Wilcoxon pairs from that cache. Frame order
/// is holdout-chunk then in-chunk index — the same order the previous four
/// independent walks used.
fn evaluate_holdout(params: &ModelParams, ds: &PreparedDataset) -> HoldoutEval {
    let d = params.embed.len();
    let mut latents = Vec::new();
    let mut sum = vec![0.0; d];
    let mut sum_sq = vec![0.0; d];
    let mut n_frames = 0usize;
    let mut model = 0.0;
    let mut persist = 0.0;
    let mut n_trans = 0usize;
    let mut per_traj_model = Vec::new();
    let mut per_traj_persist = Vec::new();
    let mut pred = vec![0.0; d];

    for ci in ds.holdout() {
        let chunk = &ds.chunks[ci];
        let start = latents.len();
        for &f in chunk {
            let mut z = vec![0.0; d];
            matvec_into(&params.embed, &ds.frames[f], &mut z);
            for ((s, ss), v) in sum.iter_mut().zip(sum_sq.iter_mut()).zip(&z) {
                *s += v;
                *ss += v * v;
            }
            n_frames += 1;
            latents.push(z);
        }
        let mut traj_m = 0.0;
        let mut traj_p = 0.0;
        let mut traj_n = 0usize;
        for pair in latents[start..].windows(2) {
            matvec_into(&params.pred, &pair[0], &mut pred);
            let mr = l2_sq(&pred, &pair[1]);
            let pr = l2_sq(&pair[0], &pair[1]);
            model += mr;
            persist += pr;
            traj_m += mr;
            traj_p += pr;
            traj_n += 1;
            n_trans += 1;
        }
        let nf = traj_n.max(1) as f64;
        per_traj_model.push(traj_m / nf);
        per_traj_persist.push(traj_p / nf);
    }

    let n = n_trans.max(1) as f64;
    let latent_variance = if n_frames == 0 {
        vec![0.0; d]
    } else {
        let nf = n_frames as f64;
        sum.iter()
            .zip(&sum_sq)
            .map(|(s, ss)| (ss / nf - (s / nf) * (s / nf)).max(0.0))
            .collect()
    };

    HoldoutEval {
        residual_model: model / n,
        residual_persist: persist / n,
        latent_variance,
        latents,
        per_traj_model,
        per_traj_persist,
    }
}

/// Best-effort git SHA for provenance (empty-tree marker when unavailable —
/// recorded honestly rather than fabricated).
fn git_sha_best_effort() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or_else(
            || "unavailable".into(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
        )
}

/// The decoupled readout pass (D7): train θ_D (linear head over frozen LN
/// latents) on the TRAIN split with next-window byte targets, gate on holdout
/// NLL < ln |V_o|, and write the `aria-readout-v1` artifact. Gradients cannot
/// reach W_I or W_P — the pass consumes latents as data (𝔸5, 𝕃5).
// One coherent pass (targets → split → init → early-stopped Adam → gate →
// artifact); the {train,val,hold}×{z,y} names are the clearest available.
#[allow(clippy::too_many_lines, clippy::similar_names)]
fn readout_pass(
    cfg: &TrainingConfig,
    ds: &PreparedDataset,
    params: &ModelParams,
    corpus: &[u8],
    out: &std::path::Path,
) -> Result<crate::ReadoutOutcome, TrainingError> {
    use crate::loss::{nll_loss_and_grad, ReadoutParams};

    // Window starts re-derived exactly like the encoder; target for window t
    // is the first byte of window t+1 (the next-window prediction task).
    let mut starts = Vec::new();
    let mut s = 0usize;
    while s < corpus.len() {
        let e = (s + cfg.n_modes).min(corpus.len());
        if e - s < 8 {
            break;
        }
        starts.push(s);
        s += cfg.stride;
    }
    if starts.len() != ds.frames.len() {
        return Err(TrainingError::Dataset(format!(
            "readout targets: corpus yields {} windows but the dataset holds {} frames — \
             corpus_path/stride mismatch",
            starts.len(),
            ds.frames.len()
        )));
    }

    let vocab = cfg.vocab_size;
    let d = cfg.latent_dim;
    let frozen = |f: usize| matvec(&params.embed, &ds.frames[f]);
    let split_pairs = |range: &mut dyn Iterator<Item = usize>| {
        let mut z = Vec::new();
        let mut y = Vec::new();
        for ci in range {
            for &f in &ds.chunks[ci] {
                if f + 1 < starts.len() {
                    z.push(frozen(f));
                    y.push(u32::from(corpus[starts[f + 1]]));
                }
            }
        }
        (z, y)
    };
    let (train_z_all, train_y_all) = split_pairs(&mut (0..ds.n_train));
    let (hold_z, hold_y) = split_pairs(&mut ds.holdout());
    if train_z_all.is_empty() || hold_z.is_empty() {
        return Err(TrainingError::Dataset(
            "readout pass needs non-empty train and holdout target sets".into(),
        ));
    }

    // Train-internal validation tail (last 20% of train pairs, time order):
    // early stopping selects the best-on-validation head so the pass returns
    // the generalizing iterate, not the last (measured without this: 60 steps
    // → holdout 5.589, 300 steps → 6.207 on an unlearnable split — classic
    // overfit). The holdout is never consulted during selection.
    let val_cut = (train_z_all.len() * 4) / 5;
    let val_cut = val_cut.max(1).min(train_z_all.len() - 1);
    let (train_z, val_z) = train_z_all.split_at(val_cut);
    let (train_y, val_y) = train_y_all.split_at(val_cut);

    // Seeded θ_D init (LCG discipline), Adam on the head weight only.
    let mut rng = crate::Lcg(cfg.seed ^ 0x0D0_D0D0_D0D0_D0D0);
    let scale = 1.0 / (d as f64).sqrt();
    let mut head = ReadoutParams {
        weight: (0..vocab)
            .map(|_| (0..d).map(|_| rng.unit() * scale).collect())
            .collect(),
        temperature: 1.0,
    };
    let uniform_nll = libm::log(vocab as f64);
    let holdout_nll = |head: &ReadoutParams| -> Result<f64, TrainingError> {
        Ok(nll_loss_and_grad(head, &hold_z, &hold_y)?.0)
    };
    let nll_initial = holdout_nll(&head)?;

    let max_steps = 300usize;
    let mut m1 = crate::linalg::zeros(vocab, d);
    let mut m2 = crate::linalg::zeros(vocab, d);
    let mut best = head.clone();
    let mut best_val = nll_loss_and_grad(&head, val_z, val_y)?.0;
    let mut steps_taken = 0usize;
    for t in 1..=max_steps {
        let (_, grad) = nll_loss_and_grad(&head, train_z, train_y)?;
        crate::optimizer::adamw_update(
            &mut head.weight,
            &grad,
            &mut m1,
            &mut m2,
            t as u64,
            cfg.adamw,
        );
        if t % 10 == 0 {
            let val = nll_loss_and_grad(&head, val_z, val_y)?.0;
            if val < best_val {
                best_val = val;
                best = head.clone();
                steps_taken = t;
            }
        }
    }
    let head = best;
    let nll_final = holdout_nll(&head)?;
    if nll_final >= uniform_nll {
        return Err(TrainingError::Config(format!(
            "readout gate: holdout NLL {nll_final:.4} did not beat the uniform floor \
             {uniform_nll:.4} — the latent carries no learnable next-window byte signal \
             under this protocol; head not written"
        )));
    }

    // aria-readout-v1 artifact via the runtime head (identity LN affine).
    let flat: Vec<f64> = head.weight.iter().flatten().copied().collect();
    let runtime_head = aria_engine_backends::readout::DiscreteReadout::new(
        d,
        vocab,
        head.temperature,
        vec![1.0; d],
        vec![0.0; d],
        flat,
    )
    .map_err(|e| TrainingError::Config(format!("readout artifact: {e}")))?;
    runtime_head
        .to_file(out)
        .map_err(|e| TrainingError::Config(format!("readout artifact write: {e}")))?;

    Ok(crate::ReadoutOutcome {
        holdout_nll_initial: nll_initial,
        holdout_nll_final: nll_final,
        uniform_nll,
        steps: steps_taken,
    })
}

/// Holdout residuals under the current parameters:
/// model = mean ‖W_P(W_I x_t) − W_I x_{t+1}‖², persistence = mean ‖W_I x_t − W_I x_{t+1}‖².
/// The persistence baseline shares the embedding — exactly the WS5 comparison.
pub(crate) fn holdout_residuals(params: &ModelParams, ds: &PreparedDataset) -> (f64, f64) {
    let eval = evaluate_holdout(params, ds);
    (eval.residual_model, eval.residual_persist)
}

/// Mean frame over the TRAIN split only (time-split discipline: the holdout
/// never informs initialization). Public: it is the deflation direction the
/// optimizer constrains against, and receipts may want to re-derive it.
pub fn train_mean_frame(ds: &PreparedDataset) -> Vec<f64> {
    let mut mean = vec![0.0; ds.frame_dim];
    let mut n = 0usize;
    for chunk in &ds.chunks[..ds.n_train] {
        for &f in chunk {
            for (m, v) in mean.iter_mut().zip(&ds.frames[f]) {
                *m += v;
            }
            n += 1;
        }
    }
    if n > 0 {
        let nf = n as f64;
        for m in &mut mean {
            *m /= nf;
        }
    }
    mean
}

/// `d` orthonormal rows in ℝ^`input_dim` via seeded Gram–Schmidt: σ_max = 1
/// exactly, so the 𝔸2 projection at load time is a no-op from step zero.
/// When `deflate` is given (and non-degenerate), every row is additionally
/// orthogonalized against it — the mean-deflated initialization documented
/// at the call site. Requires d ≤ input_dim (𝒮 guarantees d ≤ 2N; deflation
/// consumes one of the remaining input_dim − d directions). A degenerate draw
/// (norm below 1e-12 after orthogonalization) is redrawn deterministically.
/// Public: the crate's deterministic isometry initializer.
pub fn orthonormal_rows(
    d: usize,
    input_dim: usize,
    seed: u64,
    deflate: Option<&[f64]>,
) -> Vec<Vec<f64>> {
    debug_assert!(d < input_dim || (d == input_dim && deflate.is_none()));
    let unit_deflate: Option<Vec<f64>> = deflate.and_then(|v| {
        let norm = v.iter().map(|a| a * a).sum::<f64>().sqrt();
        (norm > 1e-12).then(|| v.iter().map(|a| a / norm).collect())
    });
    let mut rng = Lcg(seed);
    let mut rows: Vec<Vec<f64>> = Vec::with_capacity(d);
    while rows.len() < d {
        let mut candidate: Vec<f64> = (0..input_dim).map(|_| rng.unit()).collect();
        if let Some(u) = &unit_deflate {
            let proj: f64 = candidate.iter().zip(u).map(|(a, b)| a * b).sum();
            for (c, p) in candidate.iter_mut().zip(u) {
                *c -= proj * p;
            }
        }
        for prev in &rows {
            let proj: f64 = candidate.iter().zip(prev).map(|(a, b)| a * b).sum();
            for (c, p) in candidate.iter_mut().zip(prev) {
                *c -= proj * p;
            }
        }
        let norm = candidate.iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm > 1e-12 {
            for c in &mut candidate {
                *c /= norm;
            }
            rows.push(candidate);
        }
        // else: redraw — the LCG stream has advanced, so this terminates.
    }
    rows
}

/// A seeded d×d matrix hard-projected to σ ≤ `bound` — a definite in-ball
/// starting point for the predictor. Public: the crate's deterministic
/// contraction initializer.
pub fn seeded_in_ball(d: usize, seed: u64, bound: f64) -> Result<Vec<Vec<f64>>, TrainingError> {
    let mut rng = Lcg(seed);
    let raw: Vec<Vec<f64>> = (0..d)
        .map(|_| (0..d).map(|_| rng.unit()).collect())
        .collect();
    Ok(aria_engine_backends::spectral::project_spectral(
        raw, bound,
    )?)
}

/// Project until the audited estimate σ̂ is ≤ `bound` (loader no-op contract).
fn project_fixed_point(mut w: Vec<Vec<f64>>, bound: f64) -> Result<Vec<Vec<f64>>, TrainingError> {
    for _ in 0..8 {
        if power_iteration(&w, DEFAULT_ITERATIONS)? <= bound {
            return Ok(w);
        }
        w = aria_engine_backends::spectral::project_spectral(w, bound)?;
    }
    Err(TrainingError::Config(format!(
        "export projection did not reach σ̂ ≤ {bound} within 8 iterations — \
         numerically pathological weights"
    )))
}
