use aria_engine_core::engine::Diffuser;
use aria_engine_core::graph::Graph;
use aria_engine_core::policy::DiffPolicy;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;

/// Simulated diffuser backend.
///
/// Implements one atomic diffusion sample:
///   z' = Diff_G(z)  — per FORMAL_SPEC §6.4 and CONTINUOUS_REFINEMENT §2.4
#[derive(Debug)]
pub struct SimDiffuser {
    latent_dim: usize,
}

impl SimDiffuser {
    pub fn new(latent_dim: usize) -> Self {
        SimDiffuser { latent_dim }
    }

    pub fn with_seed(latent_dim: usize, _seed: u64) -> Self {
        SimDiffuser { latent_dim }
    }
}

impl Diffuser for SimDiffuser {
    fn diffuse(&self, g: &Graph, z: &[f64], policy: DiffPolicy) -> Vec<f64> {
        let mut rng = StdRng::seed_from_u64(456);
        let mut z2 = z.to_vec();

        match policy {
            DiffPolicy::Identity => {
                // z unchanged
            }
            DiffPolicy::Flip => {
                // Negate all latent components (simple unitary flip)
                for v in &mut z2 {
                    *v = -*v;
                }
            }
            DiffPolicy::GraphConditioned => {
                // Graph-conditioned diffusion: small perturbation scaled by graph density
                let density = if g.node_count() > 1 {
                    (g.edge_count() as f64) / (g.node_count() as f64 * (g.node_count() - 1) as f64)
                } else {
                    0.0
                };

                let scale = (1.0 + density) * 0.1 / (self.latent_dim as f64).sqrt();

                for v in &mut z2 {
                    let noise: f64 = rng.gen::<f64>() * 2.0 - 1.0;
                    *v += noise * scale;
                    // Keep values bounded
                    *v = v.clamp(-10.0, 10.0);
                }
            }
        }

        z2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aria_engine_core::graph::{EdgeType, GraphOp, NodeType};

    #[test]
    fn identity_diffuse() {
        let d = SimDiffuser::new(8);
        let g = Graph::empty();
        let z = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let z2 = d.diffuse(&g, &z, DiffPolicy::Identity);
        assert_eq!(z, z2);
    }

    #[test]
    fn flip_diffuse() {
        let d = SimDiffuser::new(4);
        let g = Graph::empty();
        let z = vec![1.0, -2.0, 3.0, -4.0];
        let z2 = d.diffuse(&g, &z, DiffPolicy::Flip);
        assert_eq!(z2, vec![-1.0, 2.0, -3.0, 4.0]);
    }

    #[test]
    fn graph_conditioned_diffuse_changes_z() {
        let d = SimDiffuser::new(4);
        let mut g = Graph::empty();
        for (id, emb) in [(0u64, [1.0, 0.0, 0.0, 0.0]), (1, [0.0, 1.0, 0.0, 0.0])] {
            g.apply(
                &GraphOp::AddNode {
                    id,
                    ntype: NodeType::Observation,
                    emb: emb.to_vec(),
                    ts: id,
                },
                4,
            )
            .unwrap();
        }
        g.apply(
            &GraphOp::AddEdge {
                from: 0,
                to: 1,
                etype: EdgeType::CausallyPrecedes,
            },
            4,
        )
        .unwrap();

        let z = vec![1.0; 4];
        let z2 = d.diffuse(&g, &z, DiffPolicy::GraphConditioned);
        assert_ne!(z, z2); // Should be perturbed
        assert_eq!(z2.len(), 4);
        assert!(z2.iter().all(|v| v.is_finite()));
    }
}
