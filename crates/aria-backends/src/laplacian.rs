//! Graph Laplacian Infrastructure & Spectral Market Mapping (UT-4, UT-5, UT-6).
//!
//! Provides in-repo discrete spectral graph theory algorithms for knowledge maps,
//! market sector clustering, Fiedler spectral bisection, Personalised PageRank (PPR),
//! Laplacian Positional Encodings (LapPE), and effective resistance distances $\Omega(u,v)$.
//!
//! Grounded in:
//! - docs/Aria-v3.0.0-PRD.tex (UT-4 Laplacian Solver, UT-5 Partitioning, UT-6 Resistance Merge)
//! - docs/supporting-references/graph-ml-and-transformers/
//! - docs/supporting-references/spectral-graph-theory/

#![allow(clippy::needless_range_loop)] // dense index algebra on L, A, attention

use std::collections::{BTreeMap, BTreeSet};
use serde::{Deserialize, Serialize};

use aria_engine_core::graph::{Graph, NodeId};
use crate::sedenion::Sedenion;

/// Fiedler spectral decomposition result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FiedlerResult {
    /// Algebraic connectivity $\lambda_2$ (second smallest eigenvalue of $L$).
    /// $\lambda_2 > 0$ if and only if the graph is connected.
    pub lambda_2: f64,
    /// Fiedler eigenvector $v_2 \in \mathbb{R}^n$, normalized to unit length.
    pub fiedler_vector: Vec<f64>,
    /// Node IDs in index order.
    pub node_ids: Vec<NodeId>,
}

/// A node in a hierarchical market map tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketMapNode {
    /// Cluster name / category identifier.
    pub name: String,
    /// Depth in the hierarchy (0 = root).
    pub depth: usize,
    /// Node IDs belonging to this cluster.
    pub node_ids: Vec<NodeId>,
    /// Centroid in latent space (mean of node embeddings).
    pub centroid: Vec<f64>,
    /// Intra-cluster variance (mean squared distance to centroid).
    pub variance: f64,
    /// Algebraic connectivity of this sub-cluster.
    pub connectivity: f64,
    /// Child sub-clusters (empty if leaf).
    pub children: Vec<MarketMapNode>,
}

/// Discrete Graph Laplacian and Spectral Matrix representation.
#[derive(Debug, Clone)]
pub struct GraphLaplacian {
    /// Ordered list of node IDs corresponding to matrix rows/cols.
    pub node_ids: Vec<NodeId>,
    /// Node ID to 0-based index lookup.
    pub node_index: BTreeMap<NodeId, usize>,
    /// Adjacency matrix $A \in \mathbb{R}^{n \times n}$.
    pub adj: Vec<Vec<f64>>,
    /// Degrees $d_i = \sum_j A_{ij}$.
    pub degrees: Vec<f64>,
}

impl GraphLaplacian {
    /// Construct Laplacian from an experience graph using edge connectivity and latent cosine affinity.
    pub fn from_graph(g: &Graph) -> Self {
        let node_ids: Vec<NodeId> = g.nodes.keys().copied().collect();
        let n = node_ids.len();
        let mut node_index = BTreeMap::new();
        for (i, &id) in node_ids.iter().enumerate() {
            node_index.insert(id, i);
        }

        let mut adj = vec![vec![0.0; n]; n];

        // 1. Add structural edge weights (symmetric)
        for edge in &g.edges {
            let (u, v) = (edge.from, edge.to);
            if let (Some(&iu), Some(&iv)) = (node_index.get(&u), node_index.get(&v)) {
                if iu != iv {
                    adj[iu][iv] = 1.0;
                    adj[iv][iu] = 1.0;
                }
            }
        }

        // 2. If disconnected or sparse, add latent cosine affinity floor
        if n >= 2 {
            for i in 0..n {
                let id_i = node_ids[i];
                let emb_i = &g.nodes[&id_i].embedding;
                let norm_i = norm2(emb_i);
                for j in (i + 1)..n {
                    let id_j = node_ids[j];
                    let emb_j = &g.nodes[&id_j].embedding;
                    let norm_j = norm2(emb_j);
                    if norm_i > 1e-12 && norm_j > 1e-12 {
                        let dot: f64 = emb_i.iter().zip(emb_j).map(|(&x, &y)| x * y).sum();
                        let cos_sim: f64 = (dot / (norm_i * norm_j)).clamp(0.0, 1.0);
                        // Add affinity if above threshold or if no structural edge exists
                        if cos_sim > 0.3 {
                            adj[i][j] = f64::max(adj[i][j], cos_sim);
                            adj[j][i] = f64::max(adj[j][i], cos_sim);
                        }
                    }
                }
            }
        }

        let mut degrees = vec![0.0; n];
        for i in 0..n {
            degrees[i] = adj[i].iter().sum();
        }

        GraphLaplacian {
            node_ids,
            node_index,
            adj,
            degrees,
        }
    }

    /// Laplacian from **structural edges only** — no latent-affinity floor.
    ///
    /// [`Self::from_graph`] adds a cosine-similarity edge for every pair above
    /// 0.3, which is the right call when the graph is sparse and the latents
    /// carry the signal. It is the wrong call for a map whose edges are already
    /// meaningful: an ingested spreadsheet is a bipartite row/facet graph, and
    /// because a contractive predictor confines latents to a narrow cone, the
    /// affinity floor connects nearly every pair and drives the graph toward
    /// complete. Fiedler then has no cut to find — measured on the shipped
    /// fixture the affinity form bisected 21 nodes as 1 / 20, hiding the two
    /// sectors the sheet obviously contains.
    ///
    /// Use this when the caller wants clusters of the structure the host
    /// actually asserted.
    pub fn from_graph_structural(g: &Graph) -> Self {
        let node_ids: Vec<NodeId> = g.nodes.keys().copied().collect();
        let n = node_ids.len();
        let mut node_index = BTreeMap::new();
        for (i, &id) in node_ids.iter().enumerate() {
            node_index.insert(id, i);
        }

        let mut adj = vec![vec![0.0; n]; n];
        for edge in &g.edges {
            if let (Some(&iu), Some(&iv)) =
                (node_index.get(&edge.from), node_index.get(&edge.to))
            {
                if iu != iv {
                    adj[iu][iv] = 1.0;
                    adj[iv][iu] = 1.0;
                }
            }
        }

        let mut degrees = vec![0.0; n];
        for i in 0..n {
            degrees[i] = adj[i].iter().sum();
        }

        GraphLaplacian {
            node_ids,
            node_index,
            adj,
            degrees,
        }
    }

    /// Size $n = |V|$.
    pub fn size(&self) -> usize {
        self.node_ids.len()
    }

    /// Multiply combinatorial Laplacian by a vector: $y = L x = (D - A) x$.
    #[allow(clippy::needless_range_loop)]
    pub fn multiply_l(&self, x: &[f64], y: &mut [f64]) {
        let n = self.size();
        for i in 0..n {
            let mut ax = 0.0;
            for j in 0..n {
                ax += self.adj[i][j] * x[j];
            }
            y[i] = self.degrees[i] * x[i] - ax;
        }
    }

    /// Compute Fiedler vector and algebraic connectivity ($\lambda_2$) via
    /// Rayleigh quotient iteration with exact $\mathbf{1}$-deflation.
    #[allow(clippy::needless_range_loop)]
    pub fn fiedler_vector(&self, max_iter: usize, tol: f64) -> Option<FiedlerResult> {
        let n = self.size();
        if n < 2 {
            return None;
        }

        // Initialize deterministic unit vector orthogonal to 1
        let mut v = vec![0.0; n];
        for i in 0..n {
            v[i] = if i % 2 == 0 { 1.0 } else { -1.0 };
        }
        deflate_constant(&mut v);
        normalize_in_place(&mut v);

        let mut lambda_2 = 0.0;
        let mut lv = vec![0.0; n];

        for _ in 0..max_iter {
            // Shifted power iteration: M = (2 * d_max) * I - L
            let d_max = self.degrees.iter().copied().fold(1.0, f64::max);
            let shift = 2.0 * d_max;

            self.multiply_l(&v, &mut lv);
            let mut v_next = vec![0.0; n];
            for i in 0..n {
                v_next[i] = shift * v[i] - lv[i];
            }

            deflate_constant(&mut v_next);
            let norm = normalize_in_place(&mut v_next);
            if norm < 1e-14 {
                break;
            }

            // Rayleigh quotient: lambda_2 = vᵀ L v
            self.multiply_l(&v_next, &mut lv);
            let next_lambda: f64 = v_next.iter().zip(&lv).map(|(&x, &y)| x * y).sum();

            let diff = (next_lambda - lambda_2).abs();
            lambda_2 = next_lambda;
            v = v_next;

            if diff < tol {
                break;
            }
        }

        Some(FiedlerResult {
            lambda_2: lambda_2.max(0.0),
            fiedler_vector: v,
            node_ids: self.node_ids.clone(),
        })
    }

    /// 2-Way Spectral Bisection: partition nodes into two clusters based on the sign of $v_2$.
    pub fn spectral_bisection(&self) -> (Vec<NodeId>, Vec<NodeId>) {
        let n = self.size();
        if n <= 1 {
            return (self.node_ids.clone(), Vec::new());
        }

        if let Some(fiedler) = self.fiedler_vector(64, 1e-6) {
            let mut left = Vec::new();
            let mut right = Vec::new();
            for (i, &id) in self.node_ids.iter().enumerate() {
                if fiedler.fiedler_vector[i] >= 0.0 {
                    left.push(id);
                } else {
                    right.push(id);
                }
            }
            if left.is_empty() {
                left.push(right.pop().unwrap());
            } else if right.is_empty() {
                right.push(left.pop().unwrap());
            }
            (left, right)
        } else {
            let mid = n / 2;
            (self.node_ids[0..mid].to_vec(), self.node_ids[mid..].to_vec())
        }
    }

    /// Personalised PageRank (PPR) power iteration with restart probability $\alpha$.
    #[allow(clippy::needless_range_loop)]
    pub fn personalized_pagerank(&self, seeds: &[NodeId], alpha: f64, steps: usize) -> BTreeMap<NodeId, f64> {
        let n = self.size();
        let mut p = vec![0.0; n];
        let mut seed_indices = Vec::new();
        for &s in seeds {
            if let Some(&idx) = self.node_index.get(&s) {
                seed_indices.push(idx);
            }
        }

        if seed_indices.is_empty() {
            for x in &mut p {
                *x = 1.0 / n as f64;
            }
        } else {
            for &idx in &seed_indices {
                p[idx] = 1.0 / seed_indices.len() as f64;
            }
        }

        let e_seed = p.clone();
        let mut p_next = vec![0.0; n];

        for _ in 0..steps {
            p_next.fill(0.0);
            for i in 0..n {
                let deg = self.degrees[i];
                if deg > 1e-12 {
                    let share = p[i] / deg;
                    for j in 0..n {
                        if self.adj[i][j] > 0.0 {
                            p_next[j] += self.adj[i][j] * share;
                        }
                    }
                } else {
                    p_next[i] += p[i];
                }
            }

            for i in 0..n {
                p[i] = alpha * p_next[i] + (1.0 - alpha) * e_seed[i];
            }
        }

        let mut out = BTreeMap::new();
        for (i, &id) in self.node_ids.iter().enumerate() {
            out.insert(id, p[i]);
        }
        out
    }

    /// Effective resistance distance $\Omega(u, v) = (e_u - e_v)^\top L^\dagger (e_u - e_v)$.
    #[allow(clippy::many_single_char_names)]
    pub fn effective_resistance(&self, u_id: NodeId, v_id: NodeId) -> f64 {
        if u_id == v_id {
            return 0.0;
        }
        let (Some(&u_idx), Some(&v_idx)) = (self.node_index.get(&u_id), self.node_index.get(&v_id)) else {
            return f64::INFINITY;
        };

        let n = self.size();
        if n < 2 {
            return 0.0;
        }

        // rhs = e_u - e_v
        let mut b_vec = vec![0.0; n];
        b_vec[u_idx] = 1.0;
        b_vec[v_idx] = -1.0;

        // Solve L x = b using Jacobi preconditioned CG
        let mut x_sol = vec![0.0; n];
        let mut r_res = b_vec;
        let mut p_dir = r_res.clone();
        let mut rs_old: f64 = r_res.iter().map(|&val| val * val).sum();

        let mut lp = vec![0.0; n];
        for _ in 0..32 {
            self.multiply_l(&p_dir, &mut lp);
            let p_lp: f64 = p_dir.iter().zip(&lp).map(|(&pi, &lpi)| pi * lpi).sum();
            if p_lp.abs() < 1e-14 {
                break;
            }
            let step_alpha = rs_old / p_lp;
            for i in 0..n {
                x_sol[i] += step_alpha * p_dir[i];
                r_res[i] -= step_alpha * lp[i];
            }
            let rs_new: f64 = r_res.iter().map(|&val| val * val).sum();
            if libm::sqrt(rs_new) < 1e-6 {
                break;
            }
            let beta = rs_new / rs_old;
            for i in 0..n {
                p_dir[i] = r_res[i] + beta * p_dir[i];
            }
            rs_old = rs_new;
        }

        // Omega(u, v) = (e_u - e_v) · x = x[u] - x[v]
        (x_sol[u_idx] - x_sol[v_idx]).max(0.0)
    }

    /// Hierarchical Market Map decomposition.
    pub fn hierarchical_market_map(&self, g: &Graph, max_depth: usize) -> MarketMapNode {
        decompose_cluster(self, g, &self.node_ids, 0, max_depth, "Market_Root")
    }

    /// $\mathrm{G}_2$-Calibrated Sedenion Spectral Walk (Unified Cayley–Dickson Diffusion):
    ///
    /// Merges graph Laplacian effective resistance $\Omega(u, v)$ with non-associative
    /// sedenion algebra on $\mathbb{S}^{14}$ and zero-divisor topological resonance.
    ///
    /// At each step from node $u$ to neighbor $v$, transition weight is:
    /// $$W(u, v) = \exp(-\Omega(u, v) / \tau) \cdot \left(1.0 - \frac{\|S(u) \cdot S(v)\|^2}{8}\right)$$
    /// Accumulated path state is the non-associative Cayley–Dickson product:
    /// $$S_{t+1} = \mathrm{Normalize}(S(v) \cdot S_t)$$
    pub fn cd_spectral_walk(
        &self,
        g: &Graph,
        start_node: NodeId,
        steps: usize,
        tau: f64,
    ) -> Vec<(NodeId, Sedenion)> {
        let mut trajectory = Vec::with_capacity(steps + 1);
        let Some(start_node_data) = g.nodes.get(&start_node) else {
            return trajectory;
        };

        let mut curr_node = start_node;
        let mut curr_sedenion = Sedenion::from_latent(&start_node_data.embedding);
        trajectory.push((curr_node, curr_sedenion));

        let effective_tau = if tau <= 1e-6 { 0.5 } else { tau };

        for _ in 0..steps {
            // Find incident neighbors
            let mut neighbors = Vec::new();
            for edge in &g.edges {
                if edge.from == curr_node && g.nodes.contains_key(&edge.to) {
                    neighbors.push(edge.to);
                } else if edge.to == curr_node && g.nodes.contains_key(&edge.from) {
                    neighbors.push(edge.from);
                }
            }

            if neighbors.is_empty() {
                // If isolated, walk to nearest node in latent space
                let curr_emb = &g.nodes[&curr_node].embedding;
                let mut best_id = curr_node;
                let mut min_d = f64::INFINITY;
                for (&id, node) in &g.nodes {
                    if id != curr_node {
                        let d: f64 = curr_emb.iter().zip(&node.embedding).map(|(&x, &y)| (x - y) * (x - y)).sum();
                        if d < min_d {
                            min_d = d;
                            best_id = id;
                        }
                    }
                }
                if best_id == curr_node {
                    break;
                }
                neighbors.push(best_id);
            }

            // Score neighbors by resistance decay × sedenion zero-divisor resonance
            let mut best_neighbor = neighbors[0];
            let mut max_weight = -1.0;

            for &v in &neighbors {
                let v_node = &g.nodes[&v];
                let v_sedenion = Sedenion::from_latent(&v_node.embedding);
                let r_eff = self.effective_resistance(curr_node, v);
                let ann_norm = curr_sedenion.annihilation_norm_sq(&v_sedenion);

                // Zero divisor resonance: maximum when ann_norm == 0 (orthogonal associative fibers)
                let resonance = (1.0 - (ann_norm / 8.0)).clamp(0.01, 1.0);
                let weight = libm::exp(-r_eff / effective_tau) * resonance;

                if weight > max_weight {
                    max_weight = weight;
                    best_neighbor = v;
                }
            }

            let next_node_data = &g.nodes[&best_neighbor];
            let next_s = Sedenion::from_latent(&next_node_data.embedding);
            curr_sedenion = next_s.mul(&curr_sedenion).normalize();
            curr_node = best_neighbor;
            trajectory.push((curr_node, curr_sedenion));
        }

        trajectory
    }
}

/// Computes the exact recursive Cayley–Dickson non-associative trajectory signature:
/// $$\mathcal{P}(z_0, z_1, \dots, z_k) = S(z_k) \cdot (\dots (S(z_1) \cdot S(z_0)))$$
///
/// Because sedenions are strictly non-associative, $\mathcal{P}$ uniquely encodes
/// the temporal-causal order of the trajectory without external positional tags.
pub fn cd_path_signature(embeddings: &[&[f64]]) -> Sedenion {
    if embeddings.is_empty() {
        return Sedenion::ZERO;
    }
    let mut acc = Sedenion::from_latent(embeddings[0]);
    for &z in &embeddings[1..] {
        let s = Sedenion::from_latent(z);
        acc = s.mul(&acc).normalize();
    }
    acc
}

/// Unified $\mathrm{G}_2$ Sedenion-Spectral Attention Kernel (UT-2, UT-10):
///
/// Modulates multi-head query-key affinities by zero-divisor topological filtering
/// and graph Laplacian resistance distance.
pub fn cd_spectral_attention(
    queries: &[Vec<f64>],
    keys: &[Vec<f64>],
    laplacian: Option<&GraphLaplacian>,
    tau: f64,
) -> Vec<Vec<f64>> {
    let l_q = queries.len();
    let l_k = keys.len();
    let mut attn = vec![vec![0.0; l_k]; l_q];
    if l_q == 0 || l_k == 0 {
        return attn;
    }

    let d = queries[0].len() as f64;
    let inv_sqrt_d = 1.0 / libm::sqrt(d.max(1.0));
    let effective_tau = if tau <= 1e-6 { 0.5 } else { tau };

    let q_s: Vec<Sedenion> = queries.iter().map(|z| Sedenion::from_latent(z)).collect();
    let k_s: Vec<Sedenion> = keys.iter().map(|z| Sedenion::from_latent(z)).collect();

    for i in 0..l_q {
        let qi = &queries[i];
        let mut row_max = -1e30;
        let mut logits = vec![0.0; l_k];

        for j in 0..l_k {
            let kj = &keys[j];
            let dot: f64 = qi.iter().zip(kj).map(|(&x, &y)| x * y).sum();
            let mut logit = dot * inv_sqrt_d;

            // 1. Table prune, CD-walk certify (hybrid LNSPP-G2).
            let table_norm = q_s[i].mul_table(&k_s[j]).norm_sq();
            if table_norm < 1e-6 {
                let cert = q_s[i].certify(&k_s[j], 1e-6);
                if cert.agrees && cert.walk_norm_sq < 1e-6 {
                    logit -= 100.0;
                } else {
                    let resonance = (1.0 - (table_norm / 8.0)).clamp(0.01, 1.0);
                    logit += libm::log(resonance);
                }
            } else {
                let resonance = (1.0 - (table_norm / 8.0)).clamp(0.01, 1.0);
                logit += libm::log(resonance);
            }

            // 2. Graph Laplacian resistance decay (if graph nodes present)
            if let Some(lap) = laplacian {
                if i < lap.node_ids.len() && j < lap.node_ids.len() {
                    let r_eff = lap.effective_resistance(lap.node_ids[i], lap.node_ids[j]);
                    if r_eff.is_finite() {
                        logit -= r_eff / effective_tau;
                    }
                }
            }

            logits[j] = logit;
            if logit > row_max {
                row_max = logit;
            }
        }

        // Softmax normalization
        let mut sum_exp = 0.0;
        for j in 0..l_k {
            let exp_val = libm::exp(logits[j] - row_max);
            attn[i][j] = exp_val;
            sum_exp += exp_val;
        }

        let inv_sum = if sum_exp > 1e-300 { 1.0 / sum_exp } else { 1.0 / l_k as f64 };
        for item in &mut attn[i] {
            *item *= inv_sum;
        }
    }

    attn
}

fn decompose_cluster(
    lap: &GraphLaplacian,
    g: &Graph,
    nodes: &[NodeId],
    depth: usize,
    max_depth: usize,
    name: &str,
) -> MarketMapNode {
    let d = g.nodes.values().next().map_or(0, |n| n.embedding.len());
    let mut centroid = vec![0.0; d];
    if !nodes.is_empty() && d > 0 {
        for &id in nodes {
            if let Some(n) = g.nodes.get(&id) {
                for (c, &v) in centroid.iter_mut().zip(&n.embedding) {
                    *c += v;
                }
            }
        }
        let inv_n = 1.0 / nodes.len() as f64;
        for c in &mut centroid {
            *c *= inv_n;
        }
    }

    let mut variance = 0.0;
    if !nodes.is_empty() {
        for &id in nodes {
            if let Some(n) = g.nodes.get(&id) {
                variance += n.embedding.iter().zip(&centroid).map(|(&x, &c)| (x - c) * (x - c)).sum::<f64>();
            }
        }
        variance /= nodes.len() as f64;
    }

    // Subgraph bisection
    if depth < max_depth && nodes.len() >= 4 {
        let _node_set: BTreeSet<NodeId> = nodes.iter().copied().collect();
        let sub_ids: Vec<NodeId> = nodes.to_vec();
        let k = sub_ids.len();
        let mut sub_adj = vec![vec![0.0; k]; k];
        let mut sub_map = BTreeMap::new();
        for (i, &id) in sub_ids.iter().enumerate() {
            sub_map.insert(id, i);
        }

        for (i, &u) in sub_ids.iter().enumerate() {
            for (j, &v) in sub_ids.iter().enumerate() {
                if let (Some(&iu), Some(&iv)) = (lap.node_index.get(&u), lap.node_index.get(&v)) {
                    sub_adj[i][j] = lap.adj[iu][iv];
                }
            }
        }

        let mut sub_deg = vec![0.0; k];
        for i in 0..k {
            sub_deg[i] = sub_adj[i].iter().sum();
        }

        let partition = GraphLaplacian {
            node_ids: sub_ids.clone(),
            node_index: sub_map,
            adj: sub_adj,
            degrees: sub_deg,
        };

        let (left, right) = partition.spectral_bisection();
        if !left.is_empty() && !right.is_empty() && left.len() < nodes.len() && right.len() < nodes.len() {
            let left_child = decompose_cluster(lap, g, &left, depth + 1, max_depth, &format!("{name}_SectorA"));
            let right_child = decompose_cluster(lap, g, &right, depth + 1, max_depth, &format!("{name}_SectorB"));
            return MarketMapNode {
                name: name.to_string(),
                depth,
                node_ids: nodes.to_vec(),
                centroid,
                variance,
                connectivity: partition.fiedler_vector(32, 1e-4).map_or(0.0, |f| f.lambda_2),
                children: vec![left_child, right_child],
            };
        }
    }

    MarketMapNode {
        name: name.to_string(),
        depth,
        node_ids: nodes.to_vec(),
        centroid,
        variance,
        connectivity: 0.0,
        children: Vec::new(),
    }
}

fn deflate_constant(v: &mut [f64]) {
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    for x in v {
        *x -= mean;
    }
}

fn normalize_in_place(v: &mut [f64]) -> f64 {
    let norm = norm2(v);
    if norm > 1e-300 {
        let inv = 1.0 / norm;
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
    norm
}

fn norm2(v: &[f64]) -> f64 {
    libm::sqrt(v.iter().map(|&x| x * x).sum())
}
