//! Simulated graph backend — proposes the atomic ops of one Match (ℙ3, 𝕃3).
//!
//! # Ops, not graphs (plan WS3)
//!
//! v0.1.0 returned a rebuilt `Graph` per Match, which cost two clones of `G`
//! per step and made a long run `O(T²)` in graph size. This backend proposes
//! [`GraphOp`]s; the engine commits them transactionally against a snapshot
//! journal, so a step costs its edit.
//!
//! # The index is a proposer, never a decider
//!
//! [`MatchPolicy::Merge`] asks an [`HnswIndex`] for the nearest existing node
//! (ℙ5, `O(log |V|)`), then **re-verifies** that candidate's distance against
//! τ in the engine's own compensated `f64` metric before emitting a merge. An
//! approximate — or even wrong — candidate can therefore only change *which*
//! admissible edit is proposed, never whether the committed graph satisfies
//! Inv3. That is what keeps an approximate structure outside the Spec surface.

use std::sync::Mutex;

use aria_engine_core::engine::GraphBackend;
use aria_engine_core::graph::{EdgeType, Graph, GraphOp, NodeId, NodeType, UndoOp};
use aria_engine_core::policy::MatchPolicy;
use aria_engine_core::state::euclidean_distance;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;

use crate::index::{HnswIndex, VectorIndex};

/// Seed for the `one_edit` choice stream. Fixed and documented — no OS entropy.
const ONE_EDIT_SEED: u64 = 789;

/// Simulated graph backend.
///
/// Implements the ℙ3 elementary edits plus the 𝕃3 τ-merge, and owns the metric
/// index that the merge policy queries.
#[derive(Debug)]
pub struct SimGraphBackend {
    latent_dim: usize,
    /// τ — merge radius in 𝒵 (spec §0.4: `τ ∈ (0, 1]`).
    merge_tau: f64,
    /// Metric index over live node embeddings.
    ///
    /// `Mutex` because [`GraphBackend`] is `&self` throughout (it must be
    /// `Sync`) while the index mutates on commit. The engine is single
    /// threaded, so the lock is uncontended.
    index: Mutex<HnswIndex>,
}

impl SimGraphBackend {
    /// Backend with the default merge radius.
    pub fn new(latent_dim: usize) -> Self {
        Self::with_merge_tau(latent_dim, 0.5)
    }

    /// Backend with an explicit merge radius τ.
    pub fn with_merge_tau(latent_dim: usize, merge_tau: f64) -> Self {
        SimGraphBackend {
            latent_dim,
            merge_tau,
            index: Mutex::new(HnswIndex::new(latent_dim)),
        }
    }

    /// Backend with an explicit seed (kept for call-site compatibility; the
    /// edit stream is seeded by construction, so the seed is not needed).
    pub fn with_seed(latent_dim: usize, _seed: u64) -> Self {
        Self::new(latent_dim)
    }

    /// τ currently in force.
    pub fn merge_tau(&self) -> f64 {
        self.merge_tau
    }

    /// Live entries in the metric index — equals `|V|` when in sync.
    pub fn index_len(&self) -> usize {
        self.lock_index().len()
    }

    fn lock_index(&self) -> std::sync::MutexGuard<'_, HnswIndex> {
        self.index
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Rebuild the index from `g` when the two disagree.
    ///
    /// Self-healing rather than trusting: an engine initialised with a
    /// non-empty `G₀`, or a revert that mirrored imperfectly, would otherwise
    /// let the proposer read a stale structure forever. In the steady state
    /// this is one integer comparison; the rebuild is `O(|V| log |V|)` and
    /// happens only on an actual divergence.
    fn ensure_synced(&self, g: &Graph) {
        let mut index = self.lock_index();
        if index.len() == g.node_count() {
            return;
        }
        log::debug!(
            "graph/index desync: |V| = {}, index = {} — rebuilding",
            g.node_count(),
            index.len()
        );
        let mut fresh = HnswIndex::new(self.latent_dim);
        for (id, node) in &g.nodes {
            fresh.add(*id, &node.embedding);
        }
        *index = fresh;
    }

    /// Ops that absorb the current latent as a fresh node — Match's `G ⊕ z`.
    fn absorb(g: &Graph, z: &[f64], t: u64) -> (NodeId, GraphOp) {
        let id = g.next_id();
        (
            id,
            GraphOp::AddNode {
                id,
                ntype: NodeType::Observation,
                emb: z.to_vec(),
                ts: t,
            },
        )
    }

    /// One seeded elementary edit (ℙ3), on top of absorbing `z`.
    ///
    /// Node selection reads `g.nodes` in key order. Under v0.1.0's `HashMap`
    /// that order was randomised per process, so this policy picked a
    /// different node in different processes; the ordered container in graph
    /// v2 makes the choice reproducible.
    fn one_edit(&self, g: &Graph, z_id: NodeId, ops: &mut Vec<GraphOp>) {
        let mut rng = StdRng::seed_from_u64(ONE_EDIT_SEED);
        let ids: Vec<NodeId> = g.nodes.keys().copied().collect();
        let choice: u32 = rng.gen::<u32>() % 6;

        match choice {
            0 if !ids.is_empty() => {
                let target = ids[rng.gen::<usize>() % ids.len()];
                let emb: Vec<f64> = (0..self.latent_dim)
                    .map(|_| rng.gen::<f64>() * 2.0 - 1.0)
                    .collect();
                ops.push(GraphOp::RelabelNode { id: target, emb });
            }
            1 | 2 => {
                // The absorbed z already is the added node; link it to an
                // existing node so the edit is observable.
                if !ids.is_empty() {
                    let from = ids[rng.gen::<usize>() % ids.len()];
                    ops.push(GraphOp::AddEdge {
                        from,
                        to: z_id,
                        etype: EdgeType::CausallyPrecedes,
                    });
                }
            }
            3 if !ids.is_empty() => {
                let victim = ids[rng.gen::<usize>() % ids.len()];
                ops.push(GraphOp::DeleteNode { id: victim });
            }
            4 if ids.len() >= 2 => {
                let a = ids[rng.gen::<usize>() % ids.len()];
                let b = ids[rng.gen::<usize>() % ids.len()];
                if a != b {
                    ops.push(GraphOp::AddEdge {
                        from: a,
                        to: b,
                        etype: EdgeType::CausallyPrecedes,
                    });
                }
            }
            _ => {
                if let Some(edge) = g.edges.iter().next() {
                    ops.push(GraphOp::DeleteEdge {
                        from: edge.from,
                        to: edge.to,
                        etype: edge.edge_type.clone(),
                    });
                }
            }
        }
    }

    /// 𝕃3 τ-merge: absorb `z` into the nearest node within τ, else append.
    ///
    /// Emitted as `AddNode` + `MergeNodes` rather than a bare relabel because
    /// that is what reproduces spec §5.3's `match_and_mutate` exactly — the
    /// survivor gets the EMA embedding update **and** the candidate's
    /// timestamp — while staying inside the plan's op alphabet.
    fn merge(&self, g: &Graph, z: &[f64], z_id: NodeId, ops: &mut Vec<GraphOp>) {
        let candidate = {
            let index = self.lock_index();
            index.nearest(z, 1).first().map(|&(id, _)| id)
        };
        let Some(keep) = candidate else { return };
        if keep == z_id {
            return;
        }
        let Some(node) = g.node(keep) else { return };

        // Re-verify in the engine's own compensated metric: the index ranks
        // with plain summation, the decision must not.
        if euclidean_distance(&node.embedding, z) <= self.merge_tau {
            ops.push(GraphOp::MergeNodes { keep, merged: z_id });
        }
    }
}

impl GraphBackend for SimGraphBackend {
    fn edit_ops(
        &self,
        g: &Graph,
        z: &[f64],
        policy: MatchPolicy,
        target: Option<&Graph>,
        t: u64,
    ) -> Vec<GraphOp> {
        self.ensure_synced(g);

        // Every policy absorbs the latent first — `G ⊕ z` is part of Match
        // (FORMAL_SPEC §6.3), and v0.1.0 did it in the engine.
        let (z_id, absorb) = Self::absorb(g, z, t);
        let mut ops = vec![absorb];

        match policy {
            MatchPolicy::Identity => {}
            MatchPolicy::OneEdit => self.one_edit(g, z_id, &mut ops),
            MatchPolicy::Merge => self.merge(g, z, z_id, &mut ops),
            MatchPolicy::RebuildGStar => {
                // ED(G ⊕ z, G*): delete everything, then rebuild G*. With no
                // target configured, G* is empty — v0.1.0's behaviour, now
                // expressed as ops instead of a wholesale replacement.
                ops.clear();
                for &id in g.nodes.keys() {
                    ops.push(GraphOp::DeleteNode { id });
                }
                if let Some(target) = target {
                    for (id, node) in &target.nodes {
                        ops.push(GraphOp::AddNode {
                            id: *id,
                            ntype: node.node_type.clone(),
                            emb: node.embedding.clone(),
                            ts: node.timestamp,
                        });
                    }
                    for edge in &target.edges {
                        ops.push(GraphOp::AddEdge {
                            from: edge.from,
                            to: edge.to,
                            etype: edge.edge_type.clone(),
                        });
                    }
                }
            }
        }

        ops
    }

    fn ok(&self, g: &Graph) -> bool {
        g.ok(self.latent_dim)
    }

    fn commit_ops(&self, ops: &[GraphOp], g: &Graph) {
        // A node added and merged away inside the same batch never enters the
        // index: indexing it only to tombstone it would leak an arena slot per
        // merge, which at 10⁵ steps is the dominant memory waste.
        let absorbed: Vec<NodeId> = ops
            .iter()
            .filter_map(|op| match op {
                GraphOp::MergeNodes { merged, .. } => Some(*merged),
                _ => None,
            })
            .collect();

        let mut index = self.lock_index();
        for op in ops {
            match op {
                GraphOp::AddNode { id, emb, .. } => {
                    if !absorbed.contains(id) {
                        index.add(*id, emb);
                    }
                }
                GraphOp::RelabelNode { id, emb } => index.add(*id, emb),
                GraphOp::DeleteNode { id } => index.remove(*id),
                GraphOp::MergeNodes { keep, merged } => {
                    index.remove(*merged);
                    if let Some(node) = g.node(*keep) {
                        // Post-state embedding: the EMA-updated survivor.
                        index.add(*keep, &node.embedding);
                    }
                }
                GraphOp::AddEdge { .. } | GraphOp::DeleteEdge { .. } => {}
            }
        }
    }

    fn revert_ops(&self, journal: &[UndoOp], _g: &Graph) {
        let mut index = self.lock_index();
        // Reverse order, mirroring `Graph::undo_ops`, so the index passes
        // through the same intermediate states as the graph.
        for entry in journal.iter().rev() {
            match entry {
                UndoOp::DropNode(id) => index.remove(*id),
                UndoOp::RestoreNode { node, .. } => index.add(node.id, &node.embedding),
                UndoOp::RestoreEmbedding { id, emb } => index.add(*id, emb),
                UndoOp::Unmerge {
                    merged,
                    keep,
                    keep_emb,
                    ..
                } => {
                    index.add(merged.id, &merged.embedding);
                    index.add(*keep, keep_emb);
                }
                UndoOp::DropEdge(_) | UndoOp::RestoreEdge(_) | UndoOp::Noop => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aria_engine_core::graph::GraphOpError;

    const DIM: usize = 4;

    fn graph_with(points: &[(NodeId, [f64; DIM])]) -> Graph {
        let mut g = Graph::empty();
        for (id, emb) in points {
            g.apply(
                &GraphOp::AddNode {
                    id: *id,
                    ntype: NodeType::Observation,
                    emb: emb.to_vec(),
                    ts: *id,
                },
                DIM,
            )
            .expect("fixture must apply");
        }
        g
    }

    /// Apply proposed ops the way the engine does, keeping the index in step.
    fn commit(
        backend: &SimGraphBackend,
        g: &mut Graph,
        ops: &[GraphOp],
    ) -> Result<Vec<UndoOp>, GraphOpError> {
        let journal = g.apply_ops(ops, DIM)?;
        backend.commit_ops(ops, g);
        Ok(journal)
    }

    #[test]
    fn identity_absorbs_exactly_one_node_per_match() {
        let backend = SimGraphBackend::new(DIM);
        let mut g = Graph::empty();
        for t in 0..5u64 {
            let z = vec![f64::from(u32::try_from(t).unwrap()), 0.0, 0.0, 0.0];
            let ops = backend.edit_ops(&g, &z, MatchPolicy::Identity, None, t);
            assert_eq!(ops.len(), 1, "identity must emit exactly the absorb op");
            commit(&backend, &mut g, &ops).unwrap();
        }
        assert_eq!(g.node_count(), 5, "one node per Match — v0.1.0 behaviour");
        assert_eq!(g.edge_count(), 0);
        assert!(backend.ok(&g));
        assert_eq!(backend.index_len(), 5, "index must track |V|");
    }

    #[test]
    fn merge_absorbs_near_latents_and_appends_far_ones() {
        let backend = SimGraphBackend::with_merge_tau(DIM, 0.5);
        let mut g = Graph::empty();

        // First Match: nothing to merge into, so it appends.
        let ops = backend.edit_ops(&g, &[0.0; DIM], MatchPolicy::Merge, None, 0);
        commit(&backend, &mut g, &ops).unwrap();
        assert_eq!(g.node_count(), 1);

        // Near latent (‖·‖ = 0.1 < τ): merged, |V| unchanged.
        let near = vec![0.1, 0.0, 0.0, 0.0];
        let ops = backend.edit_ops(&g, &near, MatchPolicy::Merge, None, 1);
        assert!(
            ops.iter().any(|o| matches!(o, GraphOp::MergeNodes { .. })),
            "a latent inside τ must merge: {ops:?}"
        );
        commit(&backend, &mut g, &ops).unwrap();
        assert_eq!(g.node_count(), 1, "merge must not grow |V|");
        // EMA: 0.9·0.0 + 0.1·0.1 = 0.01, and the timestamp follows the latent.
        let survivor = g.nodes.values().next().unwrap();
        assert!((survivor.embedding[0] - 0.01).abs() < 1e-15);
        assert_eq!(survivor.timestamp, 1);

        // Far latent (distance 5 > τ): appended.
        let far = vec![5.0, 0.0, 0.0, 0.0];
        let ops = backend.edit_ops(&g, &far, MatchPolicy::Merge, None, 2);
        assert!(
            !ops.iter().any(|o| matches!(o, GraphOp::MergeNodes { .. })),
            "a latent outside τ must not merge: {ops:?}"
        );
        commit(&backend, &mut g, &ops).unwrap();
        assert_eq!(g.node_count(), 2);
        assert_eq!(
            backend.index_len(),
            2,
            "index must track |V| through merges"
        );
        assert!(backend.ok(&g));
    }

    #[test]
    fn merge_keeps_growth_sublinear() {
        // Latents drawn from a bounded region: 𝕃3's sphere-packing argument
        // says |V| saturates rather than growing once per Match.
        let backend = SimGraphBackend::with_merge_tau(DIM, 0.5);
        let mut g = Graph::empty();
        let mut x = 12345u64;
        for t in 0..400u64 {
            let z: Vec<f64> = (0..DIM)
                .map(|_| {
                    x = x
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    ((x >> 11) as f64) / ((1u64 << 53) as f64)
                })
                .collect();
            let ops = backend.edit_ops(&g, &z, MatchPolicy::Merge, None, t);
            commit(&backend, &mut g, &ops).unwrap();
        }
        assert!(
            g.node_count() < 400,
            "merge policy grew one node per Match ({}) — no sub-linearity",
            g.node_count()
        );
        assert_eq!(backend.index_len(), g.node_count());
        assert!(backend.ok(&g), "merging must preserve Inv3");
    }

    #[test]
    fn one_edit_preserves_graph_ok_and_is_reproducible() {
        let backend = SimGraphBackend::new(DIM);
        let g = graph_with(&[(0, [1.0, 0.0, 0.0, 0.0]), (1, [0.0, 1.0, 0.0, 0.0])]);

        let a = backend.edit_ops(&g, &[0.5; DIM], MatchPolicy::OneEdit, None, 7);
        let b = backend.edit_ops(&g, &[0.5; DIM], MatchPolicy::OneEdit, None, 7);
        assert_eq!(a, b, "one_edit must be reproducible across calls");

        let mut applied = g.clone();
        commit(&backend, &mut applied, &a).unwrap();
        assert!(backend.ok(&applied), "one_edit must preserve Inv3");
    }

    #[test]
    fn rebuild_to_no_target_empties_the_graph() {
        let backend = SimGraphBackend::new(DIM);
        let mut g = graph_with(&[(0, [1.0, 0.0, 0.0, 0.0]), (1, [0.0, 1.0, 0.0, 0.0])]);
        let ops = backend.edit_ops(&g, &[0.5; DIM], MatchPolicy::RebuildGStar, None, 3);
        commit(&backend, &mut g, &ops).unwrap();
        assert_eq!(g.node_count(), 0, "rebuild with no G* empties G");
        assert_eq!(backend.index_len(), 0);
    }

    #[test]
    fn rebuild_reaches_the_target_graph() {
        let backend = SimGraphBackend::new(DIM);
        let mut g = graph_with(&[(0, [1.0, 0.0, 0.0, 0.0])]);
        let mut target = graph_with(&[(10, [0.0, 0.0, 1.0, 0.0]), (11, [0.0, 0.0, 0.0, 1.0])]);
        target
            .apply(
                &GraphOp::AddEdge {
                    from: 10,
                    to: 11,
                    etype: EdgeType::Refines,
                },
                DIM,
            )
            .unwrap();

        let ops = backend.edit_ops(&g, &[0.5; DIM], MatchPolicy::RebuildGStar, Some(&target), 4);
        commit(&backend, &mut g, &ops).unwrap();
        assert!(g.same_content(&target), "rebuild must reach G*");
        assert!(backend.ok(&g));
    }

    #[test]
    fn revert_keeps_the_index_in_step_with_the_graph() {
        let backend = SimGraphBackend::with_merge_tau(DIM, 0.5);
        let mut g = Graph::empty();
        for t in 0..3u64 {
            let z = vec![f64::from(u32::try_from(t).unwrap()) * 3.0, 0.0, 0.0, 0.0];
            let ops = backend.edit_ops(&g, &z, MatchPolicy::Merge, None, t);
            commit(&backend, &mut g, &ops).unwrap();
        }
        let before = g.clone();
        let index_before = backend.index_len();

        // A merge, then a rollback — the engine's invariant-violation path.
        let z = vec![0.1, 0.0, 0.0, 0.0];
        let ops = backend.edit_ops(&g, &z, MatchPolicy::Merge, None, 9);
        assert!(ops.iter().any(|o| matches!(o, GraphOp::MergeNodes { .. })));
        let journal = commit(&backend, &mut g, &ops).unwrap();

        g.undo_ops(&journal);
        backend.revert_ops(&journal, &g);

        assert!(g.same_content(&before), "graph must be restored");
        assert_eq!(
            backend.index_len(),
            index_before,
            "index live count diverged from |V| after revert"
        );
        // And the restored survivor must be findable at its original embedding.
        let restored = backend.lock_index().nearest(&[0.0; DIM], 1);
        assert_eq!(restored.len(), 1);
        assert!(restored[0].1 < 1e-12, "survivor embedding was not restored");
    }

    #[test]
    fn a_desynced_index_heals_itself() {
        let backend = SimGraphBackend::new(DIM);
        // Graph built without ever telling the backend — e.g. a non-empty G₀.
        let g = graph_with(&[
            (0, [1.0, 0.0, 0.0, 0.0]),
            (1, [0.0, 1.0, 0.0, 0.0]),
            (2, [0.0, 0.0, 1.0, 0.0]),
        ]);
        assert_eq!(backend.index_len(), 0, "index starts empty");

        let ops = backend.edit_ops(&g, &[0.9, 0.0, 0.0, 0.0], MatchPolicy::Merge, None, 1);
        assert_eq!(backend.index_len(), 3, "edit_ops must resync the index");
        // With τ = 0.5 the nearest node is at distance 0.1 ⇒ merge proposed.
        assert!(ops
            .iter()
            .any(|o| matches!(o, GraphOp::MergeNodes { keep: 0, .. })));
    }
}
