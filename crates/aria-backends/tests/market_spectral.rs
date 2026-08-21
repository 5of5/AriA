//! Tests for Graph Laplacian, Market Mapping, and Sedenion Annihilator Geometry.

use std::collections::BTreeMap;
use aria_engine_backends::{
    canonical_zero_divisor_pair, cd_path_signature, cd_spectral_attention, certified_context_mask,
    nullity_energy, GraphLaplacian, Sedenion,
};
use aria_engine_core::graph::{EdgeType, Graph, GraphEdge, GraphNode, NodeId, NodeType};

fn build_two_cluster_market_graph() -> Graph {
    let mut nodes = BTreeMap::new();
    let mut edges = std::collections::BTreeSet::new();

    // Cluster 1 (Fintech / Banking): nodes 1, 2, 3 (close in latent space)
    nodes.insert(
        1,
        GraphNode {
            id: 1,
            embedding: vec![1.0, 0.9, 0.0, 0.0],
            node_type: NodeType::Observation,
            timestamp: 1,
        },
    );
    nodes.insert(
        2,
        GraphNode {
            id: 2,
            embedding: vec![0.95, 1.0, 0.05, 0.0],
            node_type: NodeType::Observation,
            timestamp: 2,
        },
    );
    nodes.insert(
        3,
        GraphNode {
            id: 3,
            embedding: vec![0.9, 0.95, 0.0, 0.05],
            node_type: NodeType::Observation,
            timestamp: 3,
        },
    );

    // Cluster 2 (Healthcare / Biotech): nodes 4, 5, 6 (close to each other, far from Cluster 1)
    nodes.insert(
        4,
        GraphNode {
            id: 4,
            embedding: vec![0.0, 0.05, 1.0, 0.9],
            node_type: NodeType::Observation,
            timestamp: 4,
        },
    );
    nodes.insert(
        5,
        GraphNode {
            id: 5,
            embedding: vec![0.05, 0.0, 0.95, 1.0],
            node_type: NodeType::Observation,
            timestamp: 5,
        },
    );
    nodes.insert(
        6,
        GraphNode {
            id: 6,
            embedding: vec![0.0, 0.0, 0.9, 0.95],
            node_type: NodeType::Observation,
            timestamp: 6,
        },
    );

    // Intra-cluster edges
    edges.insert(GraphEdge { from: 1, to: 2, edge_type: EdgeType::Refines });
    edges.insert(GraphEdge { from: 2, to: 3, edge_type: EdgeType::Refines });
    edges.insert(GraphEdge { from: 4, to: 5, edge_type: EdgeType::Refines });
    edges.insert(GraphEdge { from: 5, to: 6, edge_type: EdgeType::Refines });

    // Weak cross-cluster bridge
    edges.insert(GraphEdge { from: 3, to: 4, edge_type: EdgeType::CausallyPrecedes });

    let mut g = Graph::empty();
    g.nodes = nodes;
    g.edges = edges;
    g
}

#[test]
fn test_sedenion_zero_divisor_annihilation() {
    let (a, b) = canonical_zero_divisor_pair();
    assert!((a.norm() - 1.0).abs() < 1e-12, "||a|| = 1");
    assert!((b.norm() - 1.0).abs() < 1e-12, "||b|| = 1");

    let prod = a.mul(&b);
    let norm_sq = prod.norm_sq();
    assert!(
        norm_sq < 1e-14,
        "Exact zero divisor pair must produce zero product: ||a*b||^2 = {norm_sq}"
    );
    assert!(a.is_annihilator_pair(&b, 1e-10));
}

#[test]
fn test_sedenion_non_associativity() {
    // Construct 3 orthogonal unit sedenions e_1, e_2, e_4
    let mut s1 = [0.0; 16];
    s1[1] = 1.0;
    let mut s2 = [0.0; 16];
    s2[2] = 1.0;
    let mut s4 = [0.0; 16];
    s4[4] = 1.0;

    let e1 = Sedenion(s1);
    let e2 = Sedenion(s2);
    let e4 = Sedenion(s4);

    let left = (e1 * e2) * e4;
    let right = e1 * (e2 * e4);

    let diff: f64 = left
        .0
        .iter()
        .zip(&right.0)
        .map(|(x, y)| (x - y) * (x - y))
        .sum();
    // In octonions/sedenions, basis elements (e1*e2)*e4 = -(e1*(e2*e4))
    assert!(diff > 1.0, "Sedenions are strictly non-associative");
}

#[test]
fn test_graph_laplacian_and_fiedler_bisection() {
    let graph = build_two_cluster_market_graph();
    let lap = GraphLaplacian::from_graph(&graph);
    assert_eq!(lap.size(), 6);

    let fiedler = lap.fiedler_vector(128, 1e-7).expect("Fiedler vector exists");
    assert!(
        fiedler.lambda_2 > 0.0,
        "Connected graph has positive algebraic connectivity"
    );

    // Bisection should cleanly separate Cluster 1 ({1,2,3}) from Cluster 2 ({4,5,6})
    let (left, right) = lap.spectral_bisection();
    assert_eq!(left.len(), 3);
    assert_eq!(right.len(), 3);

    let left_set: std::collections::BTreeSet<NodeId> = left.into_iter().collect();
    let is_cluster1 = left_set == [1, 2, 3].into_iter().collect();
    let is_cluster2 = left_set == [4, 5, 6].into_iter().collect();
    assert!(
        is_cluster1 || is_cluster2,
        "Spectral bisection must isolate the market clusters"
    );
}

#[test]
fn test_hierarchical_market_map_decomposition() {
    let graph = build_two_cluster_market_graph();
    let lap = GraphLaplacian::from_graph(&graph);
    let map = lap.hierarchical_market_map(&graph, 2);

    assert_eq!(map.depth, 0);
    assert_eq!(map.node_ids.len(), 6);
    assert_eq!(map.children.len(), 2, "Root decomposes into two market sectors");
    assert_eq!(map.children[0].node_ids.len(), 3);
    assert_eq!(map.children[1].node_ids.len(), 3);
}

#[test]
fn test_effective_resistance_distance() {
    let graph = build_two_cluster_market_graph();
    let lap = GraphLaplacian::from_graph(&graph);

    // Intra-cluster resistance vs cross-cluster resistance
    let r_intra_1 = lap.effective_resistance(1, 2);
    let r_cross = lap.effective_resistance(1, 6);

    assert!(r_intra_1 < r_cross, "Cross-sector resistance ({r_cross}) must exceed intra-sector resistance ({r_intra_1})");
}

#[test]
fn test_personalized_pagerank_diffusion() {
    let graph = build_two_cluster_market_graph();
    let lap = GraphLaplacian::from_graph(&graph);

    // Seeded diffusion from node 1 (Fintech)
    let ppr = lap.personalized_pagerank(&[1], 0.85, 20);
    assert!(ppr[&1] > ppr[&6], "Node 1 must have higher PPR probability than distant node 6");
    assert!(ppr[&2] > ppr[&6], "Adjacent Node 2 must receive more diffused probability than node 6");
}

#[test]
fn test_lnspp_g2_certified_context_mask_and_nullity_energy() {
    let (zd_a, zd_b) = canonical_zero_divisor_pair();
    let e_a = nullity_energy(&zd_a);
    let e_b = nullity_energy(&zd_b);
    assert!(e_a < 1e-6, "Nullity energy on ZD(S) must be 0: {e_a}");
    assert!(e_b < 1e-6, "Nullity energy on ZD(S) must be 0: {e_b}");

    // Convert to latents and test context mask
    let q_latents = vec![zd_a.0[1..17.min(zd_a.0.len())].to_vec()];
    let k_latents = vec![
        zd_b.0[1..17.min(zd_b.0.len())].to_vec(),
        vec![1.0; 16],
    ];

    let mask = certified_context_mask(&q_latents, &k_latents, 1e-6);
    assert_eq!(mask.len(), 1);
    assert_eq!(mask[0].len(), 2);
    assert!(!mask[0][0], "Zero divisor pair must be masked out by the LNSPP-G2 filter");
    assert!(mask[0][1], "Non-zero-divisor pair must remain active in context");
}

#[test]
fn test_cd_path_signature_sequence_sensitivity() {
    let z_a = [1.0, 0.0, 0.0, 0.0];
    let z_b = [0.0, 1.0, 0.0, 0.0];
    let z_c = [0.0, 0.0, 1.0, 0.0];

    // Path 1: A -> B -> C
    let sig_abc = cd_path_signature(&[&z_a, &z_b, &z_c]);
    // Path 2: B -> A -> C
    let sig_bac = cd_path_signature(&[&z_b, &z_a, &z_c]);

    let diff: f64 = sig_abc
        .0
        .iter()
        .zip(&sig_bac.0)
        .map(|(x, y)| (x - y) * (x - y))
        .sum();
    assert!(
        diff > 0.01,
        "Non-commutative Cayley-Dickson multiplication gives trajectory order memory without positional encoding"
    );
}

#[test]
fn test_cd_spectral_walk_and_attention() {
    let graph = build_two_cluster_market_graph();
    let lap = GraphLaplacian::from_graph(&graph);

    // 1. Test Cayley-Dickson Spectral Walk
    let walk = lap.cd_spectral_walk(&graph, 1, 4, 0.5);
    assert_eq!(walk.len(), 5);
    // Walking from 1 should visit within Cluster 1 (2 or 3) before crossing the weak bridge
    assert!(walk[1].0 == 2 || walk[1].0 == 3);

    // 2. Test Unified Sedenion-Spectral Attention
    let q = vec![vec![1.0, 0.9, 0.0, 0.0]]; // Query in Cluster 1
    let k = vec![
        vec![0.95, 1.0, 0.05, 0.0],  // Key in Cluster 1 (Node 2)
        vec![0.0, 0.0, 0.9, 0.95],    // Key in Cluster 2 (Node 6)
    ];

    let attn = cd_spectral_attention(&q, &k, Some(&lap), 0.5);
    assert_eq!(attn.len(), 1);
    assert_eq!(attn[0].len(), 2);
    assert!(
        attn[0][0] > attn[0][1],
        "Attention weight for intra-cluster node must be higher than cross-cluster node: {} vs {}",
        attn[0][0],
        attn[0][1]
    );
}
