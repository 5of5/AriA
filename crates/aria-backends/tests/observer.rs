//! Observer tests that can fail. The first draft only asserted clamps and
//! non-zero hashes; those could not detect a broken contractive functional.

use aria_engine_backends::{
    evaluate_functional, run, run_observed, sha256, PassiveObserver, MobiusVector,
    COHERENCE_FLOOR, OCTONION_MISALIGNMENT, SHADOW_SECTOR_DIM, BOUNDARY_CERT_BYTES,
    VISIBLE_SECTOR_DIM,
};
use aria_engine_core::config::AriaConfig;
use aria_engine_core::graph::{EdgeType, Graph, GraphEdge, GraphNode, NodeType};
use aria_engine_core::state::State;
use num_complex::Complex64;

#[test]
fn evaluate_functional_follows_the_boxed_k_and_stability_gate() {
    let z = vec![vec![0.2, 0.1, 0.0, 0.0]; 8];
    // Paper: K = 1 − H(ρ)/H_max. A Dirac (any location) has H=0 ⇒ K=1.
    // A flat histogram has H≈H_max ⇒ K≈0.
    let peaked = evaluate_functional(&[0.01; 8], &z);
    let flat = evaluate_functional(&[0.05, 0.2, 0.35, 0.5, 0.65, 0.8, 0.9, 1.0], &z);
    assert!(
        peaked.self_knowledge > 0.95,
        "Dirac residual law must have K≈1, got {}",
        peaked.self_knowledge
    );
    assert!(
        flat.self_knowledge < peaked.self_knowledge - 0.3,
        "flat residual law must raise H and drop K: peaked={} flat={}",
        peaked.self_knowledge,
        flat.self_knowledge
    );
    assert!(
        (peaked.coherence - peaked.self_knowledge * (1.0 - OCTONION_MISALIGNMENT)).abs() < 1e-12
    );
    assert!(peaked.autopoietic, "tiny ρ must satisfy ‖ρ‖² ≤ κ K (1−2/π)");
    assert!(peaked.magnitude.is_finite() && peaked.magnitude > 0.0);
    assert_eq!(peaked.heartbeat_bits.len(), 8);
    // Dissipation lives in −ρ²/κ, not in K: same peaked law, larger ρ lowers Re ln F.
    let small = evaluate_functional(&[0.01; 8], &z);
    let large = evaluate_functional(&[0.4; 8], &z);
    assert!(
        large.log_magnitude < small.log_magnitude,
        "larger ρ must cut the real integrand: small={} large={}",
        small.log_magnitude,
        large.log_magnitude
    );
}

#[test]
fn coherence_is_not_clamped_to_the_floor() {
    let z = vec![vec![1.0, 0.0, 0.0, 0.0]; 8];
    let functional = evaluate_functional(&[0.05, 0.2, 0.35, 0.5, 0.65, 0.8, 0.9, 1.0], &z);
    assert!(
        functional.coherence < COHERENCE_FLOOR,
        "high-entropy residuals must report Coh {} below floor {}",
        functional.coherence,
        COHERENCE_FLOOR
    );
    assert!(functional.barrier < 0.0);
    assert!(!functional.meets_coherence_floor);
}

#[test]
fn passive_observer_is_passive_and_certificate_re_verifies() {
    let mut observer = PassiveObserver::new(4);
    let psi = vec![Complex64::new(0.6, 0.0), Complex64::new(0.8, 0.0)];
    let z = vec![0.5, 0.5, 0.5, 0.5];
    let state = State {
        psi: psi.clone(),
        z: z.clone(),
        g: Graph::empty(),
        t: 3,
        prev_res: 0.04,
        energy_0: 1.0,
    };
    let before = state.clone();
    for t in 0..10 {
        let res = 0.08 / (t + 1) as f64;
        let functional = observer.observe_step(t, res, &psi, &z);
        assert!(functional.magnitude.is_finite());
    }
    let _ = observer.observe_state(&state, 0.04);
    assert_eq!(state.z, before.z);
    assert_eq!(state.psi, before.psi);
    assert_eq!(state.t, before.t);
    assert!((state.prev_res - before.prev_res).abs() < 1e-15);

    let cert = observer.emit_certificate();
    assert_eq!(cert.payload.len(), BOUNDARY_CERT_BYTES);
    assert_eq!(cert.visible_sector_dim, VISIBLE_SECTOR_DIM);
    assert_eq!(cert.shadow_sector_dim, SHADOW_SECTOR_DIM);
    assert!(
        PassiveObserver::verify_certificate(&cert),
        "certificate digest must re-verify"
    );
    let mut tampered = cert.clone();
    tampered.payload[0] ^= 0xFF;
    assert!(
        !PassiveObserver::verify_certificate(&tampered),
        "tampered certificate must fail"
    );
}

#[test]
fn sha256_matches_nist_empty_vector() {
    let digest = sha256(b"");
    let expected = hex_32("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(digest, expected);
    assert_eq!(
        sha256(b"abc"),
        hex_32("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
}

#[test]
fn mobius_identity_and_disk_auto_map_zero() {
    let z = Complex64::new(0.5, 0.5);
    let out = MobiusVector::IDENTITY.apply(z);
    assert!((out.re - z.re).abs() < 1e-12);
    assert!((out.im - z.im).abs() < 1e-12);

    let alpha = Complex64::new(0.3, -0.2);
    let theta = 0.7;
    let m = MobiusVector::disk_auto(alpha, theta);
    let image0 = m.apply(Complex64::new(0.0, 0.0));
    let expect = -Complex64::from_polar(1.0, theta) * alpha;
    assert!((image0 - expect).norm() < 1e-12, "{image0} vs {expect}");
    assert!((m.det().norm() - 1.0).abs() < 1e-12);
}

#[test]
fn split_observers_share_magnitude_but_not_certificate() {
    let mut root = PassiveObserver::new(4);
    let psi = vec![Complex64::new(1.0, 0.0)];
    for t in 0..6 {
        root.observe_step(t, 0.05 / (t + 1) as f64, &psi, &[0.4, 0.2, 0.1, 0.0]);
    }
    let copies = root.split(4);
    assert_eq!(copies.len(), 4);
    let mag0 = copies[0].functional().magnitude;
    for child in &copies {
        assert!(
            (child.functional().magnitude - mag0).abs() < 1e-12,
            "|F| must be Möbius-invariant"
        );
    }
    let h0 = copies[0].emit_certificate().hash_digest;
    let h1 = copies[1].emit_certificate().hash_digest;
    assert_ne!(h0, h1, "different charts must mint different certificates");
}

#[test]
fn collapse_to_shortest_picks_nearest_embedding() {
    let mut observer = PassiveObserver::new(2);
    observer.observe_step(0, 0.01, &[Complex64::new(1.0, 0.0)], &[0.0, 0.0]);

    let mut g = Graph::empty();
    g.nodes.insert(
        1,
        GraphNode {
            id: 1,
            embedding: vec![0.0, 0.0],
            node_type: NodeType::Observation,
            timestamp: 0,
        },
    );
    g.nodes.insert(
        2,
        GraphNode {
            id: 2,
            embedding: vec![1.0, 0.0],
            node_type: NodeType::Observation,
            timestamp: 0,
        },
    );
    g.edges.insert(GraphEdge {
        from: 1,
        to: 2,
        edge_type: EdgeType::CausallyPrecedes,
    });

    let point = observer
        .collapse_to_shortest(&g)
        .expect("graph has nodes");
    assert_eq!(point.node, 1);
    assert!(point.latent_distance < 1e-12);
    assert!(point.resistance.is_finite());
}

#[test]
fn symbolic_projection_is_finite_and_chart_sensitive() {
    let state = State {
        psi: vec![Complex64::new(1.0, 0.0)],
        z: vec![0.2, 0.4, 0.6, 0.8],
        g: Graph::empty(),
        t: 0,
        prev_res: 0.0,
        energy_0: 1.0,
    };
    let a = PassiveObserver::new(4);
    let b = PassiveObserver::with_mobius(
        4,
        MobiusVector::disk_auto(Complex64::new(0.4, 0.1), 1.2),
    );
    let sa = a.symbolic_projection(&state);
    let sb = b.symbolic_projection(&state);
    assert!(sa.len() >= 4 && sa.iter().all(|x| x.is_finite()));
    assert!(
        (sa[3] - sb[3]).abs() > 1e-9,
        "Möbius charts must change the symbolic ordering"
    );
}

#[test]
fn weyl_split_is_eight_charts_with_invariant_functional() {
    let mut root = PassiveObserver::new(8);
    let psi = vec![Complex64::new(1.0, 0.0)];
    root.observe_step(0, 0.02, &psi, &[0.3, 0.1, -0.2, 0.4, 0.0, 0.1, 0.2, -0.1]);
    let charts = root.weyl_split();
    assert_eq!(charts.len(), 8);
    let mag = root.functional().magnitude;
    for child in &charts {
        assert!((child.functional().magnitude - mag).abs() < 1e-12);
    }
    let hashes: Vec<_> = charts
        .iter()
        .map(|c| c.emit_certificate().hash_digest)
        .collect();
    assert_ne!(hashes[0], hashes[7], "distinct Weyl charts must differ in certificate");
}

#[test]
fn fibre_parity_is_the_lnspp_involution_bit() {
    use aria_engine_backends::Sedenion;
    let s = Sedenion::from_latent(&[0.3, -0.2, 0.1, 0.4]);
    let sigma = s.doubling_involution();
    assert!((s.0[1] - sigma.0[1]).abs() < 1e-15);
    assert!((s.0[8] + sigma.0[8]).abs() < 1e-15);
    let bit = s.fibre_parity_bit();
    assert!(bit == 0 || bit == 1);
}

#[test]
fn aligned_knowledge_rises_as_residuals_fall() {
    let psi = vec![Complex64::new(1.0, 0.0)];
    let z = vec![0.2, 0.1, 0.0, 0.0];
    let mut observer = PassiveObserver::new(4);
    for t in 0_u64..32 {
        let res = 0.85 * (0.88_f64).powf(t as f64);
        observer.observe_step(t, res, &psi, &z);
    }
    let prefix = observer.prefix_aligned();
    let first = prefix[7];
    let last = *prefix.last().unwrap();
    assert!(
        last > first + 0.05,
        "aligned K must rise as ρ→0: first={first} last={last}"
    );
    let f = observer.functional();
    assert!(f.clarity > 0.8, "late window must be near-zero, clarity={}", f.clarity);
    assert!(f.aligned_knowledge > 0.7, "aligned K={}", f.aligned_knowledge);
}

#[test]
fn live_opmd_observer_uses_real_latents_and_does_not_steer_phi() {
    let mut cfg = AriaConfig::test_config();
    cfg.seed = Some(7);
    let plain = run(cfg.clone(), 256).expect("plain run");
    let observed = run_observed(cfg, 256).expect("observed run");
    assert!(observed.outcome.summary.invariants_ok);
    assert_eq!(
        plain.trace.action_sequence(),
        observed.outcome.trace.action_sequence(),
        "observer must not change the action sequence"
    );
    assert!((plain.summary.residual - observed.outcome.summary.residual).abs() < 1e-15);
    assert_eq!(plain.state.z.len(), observed.outcome.state.z.len());

    let f = &observed.ledger.functional;
    assert_eq!(f.heartbeat_bits.len(), 256);
    assert!(PassiveObserver::verify_certificate(&observed.ledger.certificate));
    assert!(
        observed.ledger.swarm_magnitude_max_abs_diff < 1e-12,
        "Weyl swarm must share |F|"
    );
    assert!(
        observed.ledger.swarm_cert_unique >= 2,
        "Weyl swarm must mint more than one certificate"
    );
    eprintln!(
        "observer-opmd256 paperK={:.4} liveK={:.4} clarity={:.4} liveCoh={:.4} liveFloor={} hH={:.4} Rh1={:.4} matured={} silkN={} |F|={:.6} res={:.6}",
        f.self_knowledge,
        f.aligned_knowledge,
        f.clarity,
        f.aligned_coherence,
        f.aligned_meets_floor,
        f.heartbeat_entropy,
        f.heartbeat_acf1,
        observed.ledger.knowledge_matured,
        observed.ledger.swarm_cert_unique,
        f.magnitude,
        observed.outcome.summary.residual
    );
}

#[test]
fn live_opmd_1000_exhibits_a_real_pulse() {
    let mut cfg = AriaConfig::test_config();
    cfg.seed = Some(7);
    let observed = run_observed(cfg, 1000).expect("1000-step observed");
    let f = &observed.ledger.functional;
    assert!(observed.outcome.summary.invariants_ok);
    assert_eq!(f.heartbeat_bits.len(), 1000);
    // A living pulse: fibre parity of the real z_t stream is not a constant bit.
    let mut ones = 0usize;
    for &b in &f.heartbeat_bits {
        ones += usize::from(b);
    }
    let zeros = 1000 - ones;
    eprintln!(
        "observer-opmd1000 liveK={:.4} liveCoh={:.4} floor={} h1={} h0={} hH={:.4} Rh1={:.4} matured={} |V|={} res={:.6}",
        f.aligned_knowledge,
        f.aligned_coherence,
        f.aligned_meets_floor,
        ones,
        zeros,
        f.heartbeat_entropy,
        f.heartbeat_acf1,
        observed.ledger.knowledge_matured,
        observed.outcome.summary.node_count,
        observed.outcome.summary.residual
    );
    assert!(
        ones > 0 && zeros > 0,
        "real z_t pulse must flip: ones={ones} zeros={zeros}"
    );
    assert!(
        f.heartbeat_entropy > 0.05,
        "heartbeat entropy must be non-degenerate, got {}",
        f.heartbeat_entropy
    );
}

fn hex_32(s: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
    }
    out
}
