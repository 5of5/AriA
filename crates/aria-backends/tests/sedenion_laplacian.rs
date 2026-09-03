//! Public-API smoke for the user-supplied UT-10 / UT-4 kernels.
//! Production modules stay test-free; these drive `Sedenion` and
//! `GraphLaplacian` from the same boundary the rest of the crate uses.

use aria_engine_backends::{
    canonical_zero_divisor_pair, certified_context_mask, nullity_energy, AnnihilatorCertificate,
    FiedlerResult, GraphLaplacian, MarketMapNode, Sedenion,
};
use aria_engine_core::graph::{Graph, GraphNode, NodeType};

#[test]
fn canonical_pair_annihilates() {
    let (a, b) = canonical_zero_divisor_pair();
    assert!((a.norm() - 1.0).abs() < 1e-12);
    assert!((b.norm() - 1.0).abs() < 1e-12);
    assert!(
        a.is_annihilator_pair(&b, 1e-12),
        "‖a·b‖² = {}",
        a.annihilation_norm_sq(&b)
    );
    // Non-zero units: not the zero sedenion.
    assert_ne!(a, Sedenion::ZERO);
    assert_ne!(b, Sedenion::ZERO);
    let e = nullity_energy(&a);
    assert!(
        e < 1e-6,
        "canonical annihilator must sit on the ZD fibre, E(v) = {e}"
    );
    let cert: AnnihilatorCertificate = a.certify(&b, 1e-12);
    assert!(cert.agrees, "table and CD walk must compile the same algebra");
    assert!(cert.table_norm_sq < 1e-12);
    assert!(cert.walk_norm_sq < 1e-12);
}

#[test]
fn table_mul_matches_cayley_dickson_walk_on_basis() {
    for i in 0..16 {
        for j in 0..16 {
            let mut a = [0.0; 16];
            let mut b = [0.0; 16];
            a[i] = 1.0;
            b[j] = 1.0;
            let sa = Sedenion(a);
            let sb = Sedenion(b);
            let table = sa.mul_table(&sb);
            let walk = sa.mul_walk(&sb);
            for k in 0..16 {
                assert!(
                    (table.0[k] - walk.0[k]).abs() < 1e-12,
                    "e_{i}·e_{j} table[{k}]={} walk[{k}]={}",
                    table.0[k],
                    walk.0[k]
                );
            }
        }
    }
}

#[test]
fn certified_mask_prunes_the_canonical_annihilator_pair() {
    let inv = 1.0 / 2.0_f64.sqrt();
    // from_latent fills s[1..] from z — reconstruct the LNSPP-G2 pair.
    let mut z_a = vec![0.0; 15];
    z_a[0] = inv;
    z_a[9] = inv;
    let mut z_b = vec![0.0; 15];
    z_b[4] = inv;
    z_b[13] = inv;
    let mask = certified_context_mask(&[z_a], &[z_b], 1e-10);
    assert_eq!(mask.len(), 1);
    assert_eq!(mask[0].len(), 1);
    assert!(
        !mask[0][0],
        "annihilator pair must be pruned (mask false)"
    );
    let keep = certified_context_mask(&[vec![1.0; 4]], &[vec![0.0, 1.0, 0.0, 0.0]], 1e-18);
    assert!(keep[0][0], "generic pair must stay active");
}

#[test]
fn from_latent_is_pure_imaginary_unit() {
    let z = vec![0.3, -0.4, 0.5, 0.1];
    let s = Sedenion::from_latent(&z);
    assert!(s.0[0].abs() < 1e-15, "real part must be 0");
    assert!((s.norm() - 1.0).abs() < 1e-12);
}

#[test]
fn laplacian_from_two_node_graph_is_symmetric() {
    let mut g = Graph::empty();
    g.nodes.insert(
        1,
        GraphNode {
            id: 1,
            embedding: vec![1.0, 0.0],
            node_type: NodeType::Observation,
            timestamp: 0,
        },
    );
    g.nodes.insert(
        2,
        GraphNode {
            id: 2,
            embedding: vec![0.0, 1.0],
            node_type: NodeType::Observation,
            timestamp: 0,
        },
    );
    let lap = GraphLaplacian::from_graph(&g);
    assert_eq!(lap.size(), 2);
    assert_eq!(lap.adj.len(), 2);
    assert!((lap.adj[0][1] - lap.adj[1][0]).abs() < 1e-15);
    let _ = FiedlerResult {
        lambda_2: 0.0,
        fiedler_vector: vec![0.0; 2],
        node_ids: lap.node_ids.clone(),
    };
    let _ = MarketMapNode {
        name: "root".into(),
        depth: 0,
        node_ids: lap.node_ids.clone(),
        centroid: vec![0.5, 0.5],
        variance: 0.0,
        connectivity: 0.0,
        children: vec![],
    };
}
