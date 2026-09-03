//! Relocated unit suite: the 4-term objective — analytic gradients vs central
//! differences, the stop-gradient proof, the spectral hinge direction, the
//! graph term on real edges, and ℒ_NLL isolation + runtime parity.

use std::path::PathBuf;

use aria_engine_backends::data::dataset_from_bytes;
use aria_engine_backends::readout::DiscreteReadout;
use aria_engine_backends::spectral::{power_iteration_with_vectors, DEFAULT_ITERATIONS};
use aria_engine_core::config::LossLambdas;
use aria_training::dataset::{align_corpus, KgEdge, KgEdges, PreparedDataset, KG_EDGES_FORMAT};
use aria_training::linalg::{l2_sq, matvec, zeros};
use aria_training::loss::{compute_batch, nll_loss_and_grad, Grads, ModelParams, ReadoutParams};
use aria_training::{AdamWParams, Lcg, TrainingConfig};

fn cfg(n_modes: usize) -> TrainingConfig {
    TrainingConfig {
        data_path: PathBuf::from("unused"),
        corpus_path: None,
        edges_path: None,
        stride: n_modes,
        n_modes,
        latent_dim: 8,
        eps: 1.0,
        lambdas: LossLambdas {
            jepa: 0.70,
            nll: 0.0,
            spectral: 0.15,
            graph: 0.15,
        },
        vocab_size: 256,
        epochs: 1,
        batch_size: 4,
        adamw: AdamWParams::default(),
        seed: 42,
        holdout_frac: 0.4,
        trajectory_len: 4,
        gamma_tau: 0.5,
        min_rankme_frac: 0.30,
        allow_sub_spec_dims: true,
        output_path: None,
        output_v2_path: None,
        readout_out: None,
    }
}

/// Real corpus bytes: the checked-in docs-corpus excerpt (D4).
fn corpus() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/docs_excerpt_32k.txt"
    ))
    .expect("fixture present")
}

fn prepared(n_modes: usize) -> PreparedDataset {
    let bytes = corpus();
    let ds = dataset_from_bytes("loss-tests", &bytes, n_modes, n_modes).unwrap();
    PreparedDataset::from_field_dataset(&ds, "x".into(), &cfg(n_modes)).unwrap()
}

fn seeded_params(d: usize, input_dim: usize, seed: u64) -> ModelParams {
    let mut rng = Lcg(seed);
    let embed = (0..d)
        .map(|_| (0..input_dim).map(|_| rng.unit() * 0.2).collect())
        .collect();
    let pred = (0..d)
        .map(|_| (0..d).map(|_| rng.unit() * 0.2).collect())
        .collect();
    ModelParams { embed, pred }
}

/// Central-difference derivative of a scalar loss with respect to one matrix
/// entry.
fn central_diff(
    mut f: impl FnMut(&ModelParams) -> f64,
    params: &ModelParams,
    which: &str,
    i: usize,
    j: usize,
    h: f64,
) -> f64 {
    let mut plus = params.clone();
    let mut minus = params.clone();
    if which == "embed" {
        plus.embed[i][j] += h;
        minus.embed[i][j] -= h;
    } else {
        plus.pred[i][j] += h;
        minus.pred[i][j] -= h;
    }
    (f(&plus) - f(&minus)) / (2.0 * h)
}

fn jepa_only() -> LossLambdas {
    LossLambdas {
        jepa: 1.0,
        nll: 0.0,
        spectral: 0.0,
        graph: 0.0,
    }
}

#[test]
fn jepa_gradients_match_central_differences_with_frozen_targets() {
    let ds = prepared(16);
    let params = seeded_params(8, 32, 7);
    let ids: Vec<usize> = (0..3.min(ds.n_train)).collect();
    let (_, grads) = compute_batch(&params, &ds, &ids, None, &jepa_only(), 0.49, 0.5).unwrap();

    // The loss function whose analytic gradient compute_batch returns:
    // stop-grad means the *target* branch is frozen at the base params.
    let base = params.clone();
    let frozen_loss = |p: &ModelParams| {
        let mut loss = 0.0;
        let mut m = 0usize;
        for &ci in &ids {
            for pair in ds.chunks[ci].windows(2) {
                let z_t = matvec(&p.embed, &ds.frames[pair[0]]);
                let target = matvec(&base.embed, &ds.frames[pair[1]]); // frozen
                let pred = matvec(&p.pred, &z_t);
                loss += l2_sq(&pred, &target);
                m += 1;
            }
        }
        loss / m as f64
    };

    for (i, j) in [(0usize, 0usize), (3, 17), (7, 31)] {
        let fd = central_diff(frozen_loss, &params, "embed", i, j, 1e-6);
        let rel = (grads.embed[i][j] - fd).abs() / fd.abs().max(1e-9);
        assert!(
            rel < 1e-7,
            "embed[{i}][{j}]: analytic {} vs fd {fd}",
            grads.embed[i][j]
        );
    }
    for (i, j) in [(0usize, 1usize), (4, 4), (7, 0)] {
        let fd = central_diff(frozen_loss, &params, "pred", i, j, 1e-6);
        let rel = (grads.pred[i][j] - fd).abs() / fd.abs().max(1e-9);
        assert!(
            rel < 1e-7,
            "pred[{i}][{j}]: analytic {} vs fd {fd}",
            grads.pred[i][j]
        );
    }
}

#[test]
fn stop_gradient_makes_the_analytic_embed_grad_differ_from_full_backprop() {
    // If the target branch were NOT stopped, ∂ℒ/∂W_I would include the
    // −2 rᵀ·(∂z̄/∂W_I) term. Verify the analytic gradient equals the
    // frozen-target FD (previous test) but *differs* from the full FD —
    // measured evidence the stop-gradient is real.
    let ds = prepared(16);
    let params = seeded_params(8, 32, 11);
    let ids: Vec<usize> = (0..3.min(ds.n_train)).collect();
    let (_, grads) = compute_batch(&params, &ds, &ids, None, &jepa_only(), 0.49, 0.5).unwrap();

    let full_loss = |p: &ModelParams| {
        let mut loss = 0.0;
        let mut m = 0usize;
        for &ci in &ids {
            for pair in ds.chunks[ci].windows(2) {
                let z_t = matvec(&p.embed, &ds.frames[pair[0]]);
                let target = matvec(&p.embed, &ds.frames[pair[1]]); // NOT frozen
                let pred = matvec(&p.pred, &z_t);
                loss += l2_sq(&pred, &target);
                m += 1;
            }
        }
        loss / m as f64
    };

    let mut max_rel_gap = 0.0f64;
    for (i, j) in [(0usize, 0usize), (2, 9), (5, 20), (7, 31)] {
        let fd_full = central_diff(full_loss, &params, "embed", i, j, 1e-6);
        let gap = (grads.embed[i][j] - fd_full).abs() / fd_full.abs().max(1e-9);
        max_rel_gap = max_rel_gap.max(gap);
    }
    assert!(
        max_rel_gap > 1e-3,
        "stop-grad must make the analytic W_I gradient differ from full backprop \
         (max relative gap {max_rel_gap})"
    );
}

#[test]
fn spectral_hinge_pushes_an_out_of_ball_matrix_toward_the_ball() {
    let ds = prepared(16);
    let mut params = seeded_params(8, 32, 13);
    // Inflate pred far outside the ε/2 ball.
    for row in &mut params.pred {
        for v in row.iter_mut() {
            *v *= 30.0;
        }
    }
    let lambdas = LossLambdas {
        jepa: 0.0,
        nll: 0.0,
        spectral: 1.0,
        graph: 0.0,
    };
    let ids = [0usize];
    let (breakdown, grads) = compute_batch(&params, &ds, &ids, None, &lambdas, 0.49, 0.5).unwrap();
    assert!(breakdown.spectral > 0.0, "hinge must be active");

    // A small step against the gradient must reduce σ_max(pred).
    let (sigma_before, _, _) =
        power_iteration_with_vectors(&params.pred, DEFAULT_ITERATIONS).unwrap();
    let mut stepped = params.clone();
    for (row, grow) in stepped.pred.iter_mut().zip(&grads.pred) {
        for (v, g) in row.iter_mut().zip(grow) {
            *v -= 1e-2 * g;
        }
    }
    let (sigma_after, _, _) =
        power_iteration_with_vectors(&stepped.pred, DEFAULT_ITERATIONS).unwrap();
    assert!(
        sigma_after < sigma_before,
        "σ must fall along −∇ℒ_Spectral: {sigma_before} → {sigma_after}"
    );
}

fn kg_for_corpus() -> KgEdges {
    // Real tokens that occur in the docs-corpus excerpt fixture
    // (measured: graph ×27, energy ×14, optical ×24).
    KgEdges {
        format: KG_EDGES_FORMAT.into(),
        source: "test".into(),
        retrieved: "2026-08-17".into(),
        filter: "test".into(),
        license: "CC-BY-SA 4.0".into(),
        concepts: vec!["graph".into(), "energy".into(), "optical".into()],
        relations: vec!["/r/RelatedTo".into()],
        edges: vec![
            KgEdge {
                s: 0,
                e: 1,
                r: 0,
                w: 1.0,
            },
            KgEdge {
                s: 1,
                e: 2,
                r: 0,
                w: 1.0,
            },
        ],
    }
}

#[test]
fn graph_term_is_active_on_real_edges_and_matches_central_differences() {
    let bytes = corpus();
    let n_modes = 64;
    let ds_raw = dataset_from_bytes("g", &bytes, n_modes, n_modes).unwrap();
    let c = {
        let mut c = cfg(n_modes);
        c.latent_dim = 8;
        c
    };
    let ds = PreparedDataset::from_field_dataset(&ds_raw, "x".into(), &c).unwrap();
    let kg = kg_for_corpus();
    let align = align_corpus(&bytes, n_modes, n_modes, ds.frames.len(), &kg).unwrap();
    assert!(
        align.coverage > 0.0,
        "the docs excerpt mentions graph/energy/optical"
    );
    assert!(!align.usable_edges.is_empty());

    let params = seeded_params(8, 2 * n_modes, 17);
    // Frames 0..32 cover the first fixture occurrences of graph (window 3),
    // optical (window 2), and energy (window 22), so both edges are live.
    let ids: Vec<usize> = (0..8.min(ds.n_train)).collect();
    let lambdas = LossLambdas {
        jepa: 0.0,
        nll: 0.0,
        spectral: 0.0,
        graph: 1.0,
    };
    // γ small enough that hinges activate on an untrained embedding.
    let gamma = 1e-4;
    let (breakdown, grads) =
        compute_batch(&params, &ds, &ids, Some(&align), &lambdas, 0.49, gamma).unwrap();
    assert!(
        breakdown.graph > 0.0,
        "graph hinge must be active at γ = {gamma}"
    );

    let graph_loss = |p: &ModelParams| {
        let (b, _) = compute_batch(p, &ds, &ids, Some(&align), &lambdas, 0.49, gamma).unwrap();
        b.graph
    };
    for (i, j) in [(0usize, 0usize), (3, 40), (7, 127)] {
        let fd = central_diff(graph_loss, &params, "embed", i, j, 1e-6);
        let g = grads.embed[i][j];
        let rel = (g - fd).abs() / fd.abs().max(1e-9);
        assert!(
            rel < 1e-6,
            "graph ∂/∂embed[{i}][{j}]: analytic {g} vs fd {fd}"
        );
    }
    // The graph term never trains the predictor.
    assert!(grads.pred.iter().flatten().all(|&g| g == 0.0));
}

#[test]
fn nll_grad_reaches_theta_d_only_and_matches_central_differences() {
    let mut rng = Lcg(23);
    let d = 8;
    let vocab = 16;
    let readout = ReadoutParams {
        weight: (0..vocab)
            .map(|_| (0..d).map(|_| rng.unit() * 0.3).collect())
            .collect(),
        temperature: 1.0,
    };
    let z: Vec<Vec<f64>> = (0..5)
        .map(|_| (0..d).map(|_| rng.unit()).collect())
        .collect();
    let targets: Vec<u32> = vec![3, 0, 15, 7, 3];

    let (loss, grad) = nll_loss_and_grad(&readout, &z, &targets).unwrap();
    assert!(loss > 0.0);
    assert_eq!(grad.len(), vocab);

    for (v, j) in [(3usize, 0usize), (0, 4), (15, 7)] {
        let h = 1e-6;
        let mut plus = readout.clone();
        plus.weight[v][j] += h;
        let mut minus = readout.clone();
        minus.weight[v][j] -= h;
        let (lp, _) = nll_loss_and_grad(&plus, &z, &targets).unwrap();
        let (lm, _) = nll_loss_and_grad(&minus, &z, &targets).unwrap();
        let fd = (lp - lm) / (2.0 * h);
        let rel = (grad[v][j] - fd).abs() / fd.abs().max(1e-9);
        assert!(
            rel < 1e-6,
            "∂NLL/∂W_D[{v}][{j}]: analytic {} vs fd {fd}",
            grad[v][j]
        );
    }
    // 𝔸5 structurally: the signature exposes no W_I / W_P gradient at all.
}

#[test]
fn nll_forward_is_parity_exact_with_the_discrete_readout() {
    // Same weight, identity LN affine, same temperature ⇒ the probability
    // this loss consumes must match DiscreteReadout::probs bit-for-bit
    // (both paths use libm::exp and the LN_EPS = 1e-5 floor).
    let mut rng = Lcg(29);
    let d = 8;
    let vocab = 256; // readout.rs enforces the spec floor |V_o| ≥ 256
    let weight: Vec<Vec<f64>> = (0..vocab)
        .map(|_| (0..d).map(|_| rng.unit() * 0.3).collect())
        .collect();
    let flat: Vec<f64> = weight.iter().flatten().copied().collect();
    let head = DiscreteReadout::new(d, vocab, 0.8, vec![1.0; d], vec![0.0; d], flat).unwrap();
    let z: Vec<f64> = (0..d).map(|_| rng.unit()).collect();

    let ours = ReadoutParams {
        weight,
        temperature: 0.8,
    }
    .probs(&z)
    .unwrap();
    let theirs = head.probs(&z).unwrap();
    for (a, b) in ours.iter().zip(&theirs) {
        assert_eq!(a.to_bits(), b.to_bits(), "probability parity must be exact");
    }
}

#[test]
fn nll_rejects_malformed_inputs() {
    let readout = ReadoutParams {
        weight: zeros(4, 4),
        temperature: 1.0,
    };
    assert!(nll_loss_and_grad(&readout, &[], &[]).is_err());
    let z = vec![vec![0.0; 4]];
    assert!(
        nll_loss_and_grad(&readout, &z, &[9]).is_err(),
        "target outside vocab"
    );
    let bad = ReadoutParams {
        weight: zeros(4, 4),
        temperature: 0.0,
    };
    assert!(nll_loss_and_grad(&bad, &z, &[1]).is_err());
    // probs() enforces the same domain.
    assert!(bad.probs(&[0.0; 4]).is_err());
    assert!(readout.probs(&[0.0; 3]).is_err());
}

#[test]
fn grads_struct_is_publicly_constructible_for_optimizer_consumers() {
    // The optimizer's public contract takes Grads — keep it constructible.
    let g = Grads {
        embed: zeros(2, 4),
        pred: zeros(2, 2),
    };
    assert_eq!(g.embed.len(), 2);
}
