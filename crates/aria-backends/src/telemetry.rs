//! T6 — the callable telemetry transform. This is the whole product in one
//! function.
//!
//! ```text
//! Node(payload, config, seed) = E( I(payload, config), Run_Φ(config, seed), Obs )
//! ```
//!
//! [`transform`] *is* that composition, evaluated left to right exactly once:
//!
//! ```text
//! validate limits → ingest (Init) → node profile → run with G₀
//!   → project IPO → tag → receipt → validate envelope → return
//! ```
//!
//! # What this function is not
//!
//! It holds no network client, no database driver, no credential, and no
//! clock. It writes no file — it *returns a value*, and the caller decides
//! where bytes land. Those are not stylistic choices:
//!
//! - No socket or driver means Aria **cannot** write host accepted state, so
//!   it cannot become a second command authority however it is called (L7).
//! - No clock means the envelope can hash equal twice, which is the only way
//!   byte-determinism (L4) is achievable at all.
//! - No sink means a failed run cannot leave a partial envelope behind (L8).
//!
//! `scripts/check_telemetry_boundary.sh` enforces all three by grep.
//!
//! # Where Aria sits
//!
//! A worker binary calls this once it has been released,
//! inside a sealed Observation Plan. The return value is a *proposal* the
//! Any reader may read and need not believe. Trust, Use, and Goal
//! completion stay with the authorized human. Nothing here assigns them, and
//! the envelope has no field in which to try.

use aria_engine_core::config::AriaConfig;
use aria_engine_core::error::AriaError;
use aria_engine_core::graph::{EdgeType, NodeId};
use aria_engine_core::policy::MatchPolicy;
use aria_engine_core::state::euclidean_distance;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use crate::ingest::{ingest, Ingested, PayloadShape};
use crate::ipo::{
    binary_type_for_edge, validate_envelope, Cluster, GraphIpo, IpoEdge, Limits, NodeOrigin,
    TaggingState, TelemetryEnvelope, TelemetryQuery, TelemetryReceipt, TELEMETRY_QUERY_V1,
    TELEMETRY_VERSION,
};
use crate::index::VectorIndex;
use crate::laplacian::GraphLaplacian;
use crate::ocid::OcidRequest;
use crate::runner::{run_observed_with_graph, run_with_graph, RefPredictor};
use crate::trained::TrainedPredictor;
use crate::SimPredictor;

/// One telemetry invocation.
#[derive(Debug)]
pub struct TelemetryRequest {
    /// Exact payload bytes as the host supplied them. Hashed before parsing so
    /// `source_sha256` is provenance for what was actually sent.
    pub payload: Vec<u8>,
    /// Engine configuration. `match_policy` is overridden by the node profile
    /// unless [`Self::respect_config_policy`] is set.
    pub config: AriaConfig,
    /// Φ steps to run. Clamped by `limits.max_steps`.
    pub steps: u64,
    /// Optional trained weights. Absent means `SimPredictor` — **no prior
    /// training is required for a valid body**.
    pub predictor: Option<TrainedPredictor>,
    /// Attach the passive observer ledger.
    pub observe: bool,
    /// Deterministic ceilings.
    pub limits: Limits,
    /// Keep the caller's `match_policy` instead of forcing the node profile.
    /// Off by default: an identity run produces a saturated map (issue `#11`).
    pub respect_config_policy: bool,
    /// Query the host wants applied. `None` uses the permissive default.
    pub query: Option<TelemetryQuery>,
    /// Optional OCID request: a public Ed25519 key to bind, and a signature
    /// over the payload to verify. Empty means no commitment is emitted.
    pub ocid: OcidRequest,
}

impl TelemetryRequest {
    /// A request over `payload` with the documented node-profile defaults.
    pub fn new(payload: Vec<u8>) -> Self {
        TelemetryRequest {
            payload,
            config: AriaConfig::default(),
            steps: 32,
            predictor: None,
            observe: false,
            limits: Limits::default(),
            respect_config_policy: false,
            query: None,
            ocid: OcidRequest::default(),
        }
    }
}

/// The node profile: map-shaped runs need a Match policy that can grow `G`.
///
/// `MatchPolicy::Identity` absorbs `G ⊕ z` once per Match, so `|V|` sticks and
/// a host gets an empty map (issue `#11`). Merge is therefore the node default
/// — but `MatchPolicy::default()` is deliberately left alone, so CI runs and
/// the committed `aria run` goldens keep their spec-minimal behavior.
pub fn node_profile_config(base: &AriaConfig) -> AriaConfig {
    AriaConfig {
        match_policy: MatchPolicy::Merge,
        ..base.clone()
    }
}

/// Run the whole telemetry operation.
///
/// The only seam. The CLI, Python, WASM, and a host binary all call this, so
/// cross-surface parity is a serialization property rather than four
/// implementations kept in step by hand.
pub fn transform(req: TelemetryRequest) -> Result<TelemetryEnvelope, AriaError> {
    let TelemetryRequest {
        payload,
        config,
        steps,
        predictor,
        observe,
        limits,
        respect_config_policy,
        query,
        ocid: ocid_request,
    } = req;

    if steps > limits.max_steps {
        return Err(AriaError::Config(format!(
            "steps {steps} exceeds max_steps {} — refusing before Φ",
            limits.max_steps
        )));
    }

    // --- I: Init. Ingest cannot be a transition, so it runs to completion
    // before an engine exists at all.
    let mut config = if respect_config_policy {
        config
    } else {
        node_profile_config(&config)
    };
    let ingested = ingest(&payload, config.n_modes, config.latent_dim, limits)?;

    let (predictor, predictor_kind) = resolve_predictor(predictor, &config)?;
    let match_policy_label = format!("{:?}", config.match_policy).to_lowercase();
    let tau = config.merge_tau;
    let seed = config.seed;
    let (n_modes, latent_dim, eps) = (config.n_modes, config.latent_dim, config.eps);
    let schedule = config.schedule.clone();
    // `|G|` grows with facets and Φ absorption; the engine's own bookkeeping
    // cap must not be tighter than what ingest already admitted.
    let admitted = ingested.g0.size() + usize::try_from(steps).unwrap_or(usize::MAX);
    if config.max_graph_size < admitted {
        config.max_graph_size = admitted;
    }

    // --- Run_Φ: the sealed machine. Untouched, and the only place any
    // invariant is decided.
    let (outcome, ledger) = if observe {
        let observed = run_observed_with_graph(config, steps, predictor, ingested.g0.clone())?;
        (observed.outcome, Some(observed.ledger))
    } else {
        (
            run_with_graph(config, steps, predictor, ingested.g0.clone())?,
            None,
        )
    };

    // --- E: pure projection. Everything below reads; nothing writes back.
    project(
        &outcome,
        ledger,
        ingested,
        query,
        ProjectionInputs {
            predictor_kind,
            match_policy_label,
            tau,
            seed,
            n_modes,
            latent_dim,
            eps,
            schedule,
            limits,
        },
        &payload,
        &ocid_request,
    )
}

/// Scalar run parameters the projection reports but does not interpret.
struct ProjectionInputs {
    predictor_kind: &'static str,
    match_policy_label: String,
    tau: f64,
    seed: Option<u64>,
    n_modes: usize,
    latent_dim: usize,
    eps: f64,
    schedule: String,
    limits: Limits,
}

/// `E` — assemble the envelope from a finished run.
///
/// Split from [`transform`] so the composition reads as its three stages, and
/// so this half is provably read-only over the run: it takes the outcome by
/// reference and returns a value.
fn project(
    outcome: &crate::runner::RunOutcome,
    ledger: Option<crate::observer::ObserverLedger>,
    ingested: Ingested,
    query: Option<TelemetryQuery>,
    inputs: ProjectionInputs,
    payload: &[u8],
    ocid_request: &OcidRequest,
) -> Result<TelemetryEnvelope, AriaError> {
    let ProjectionInputs {
        predictor_kind,
        match_policy_label,
        tau,
        seed,
        n_modes,
        latent_dim,
        eps,
        schedule,
        limits,
    } = inputs;
    let final_graph = &outcome.state.g;
    let graph = GraphIpo::from_graph(final_graph, &ingested.origins);
    let input_node_count = graph
        .nodes
        .iter()
        .filter(|n| n.origin == NodeOrigin::Input)
        .count();

    let query = normalize_query(query, tau);
    let tags = build_tags(
        final_graph,
        &ingested,
        &match_policy_label,
        tau,
        &query,
    );

    let receipt = TelemetryReceipt {
        invariants_ok: outcome.summary.invariants_ok,
        failures: outcome.summary.failures.clone(),
        steps: outcome.summary.steps,
        t: outcome.summary.t,
        node_count: final_graph.node_count(),
        edge_count: final_graph.edge_count(),
        input_node_count,
        transform_node_count: final_graph.node_count().saturating_sub(input_node_count),
        energy: outcome.summary.energy,
        residual: outcome.summary.residual,
        predictor: predictor_kind.to_string(),
        match_policy: match_policy_label,
        seed,
        n_modes,
        latent_dim,
        eps,
        schedule,
        limits,
    };

    let mut envelope = TelemetryEnvelope {
        schema: TELEMETRY_QUERY_V1.to_string(),
        version: TELEMETRY_VERSION,
        query,
        graph,
        records: ingested.records,
        source: ingested.source,
        source_sha256: ingested.source_sha256,
        structure: ingested.plan.map(|p| p.report),
        tags,
        ledger: ledger
            .map(|l| serde_json::to_value(l).unwrap_or(Value::Null))
            .filter(|v| !v.is_null()),
        receipt,
        ocid: None,
    };

    // The commitment is computed last, over the finished document, because it
    // binds the *output* as well as the input. `payload` is the exact byte
    // string the host sent — the same bytes the signature covers.
    if !ocid_request.is_empty() {
        let cfg = crate::ocid::config_of(&envelope);
        let commitment = crate::ocid::build(
            ocid_request,
            payload,
            &envelope.source_sha256,
            &cfg,
            &envelope.graph,
            envelope.structure.as_ref(),
        )
        .map_err(|e| AriaError::Config(e.to_string()))?;
        envelope.ocid = Some(commitment);

        // Self-check: a commitment that does not verify against its own
        // envelope is worse than none, because a host would trust it. Catch it
        // here rather than letting the document escape.
        crate::ocid::verify_envelope(&envelope, payload).map_err(|e| {
            AriaError::Backend(format!("OCID failed to verify against its own envelope: {e}"))
        })?;
    }

    // Self-check before the value escapes. A document that fails its own
    // validator must never reach a host: that is how a contract rots.
    let as_value = serde_json::to_value(&envelope)
        .map_err(|e| AriaError::Backend(format!("envelope serialization: {e}")))?;
    validate_envelope(&as_value)
        .map_err(|e| AriaError::Backend(format!("envelope failed its own validator: {e}")))?;

    Ok(envelope)
}

/// Pick the predictor backend and report which one it is.
///
/// A trained checkpoint fixes the dimensions it was learned for, and `G₀` was
/// already embedded at the config's dimensions. Reconciling after the fact
/// would silently invalidate every embedding in the graph, so a mismatch is an
/// error rather than a coercion.
fn resolve_predictor(
    trained: Option<TrainedPredictor>,
    config: &AriaConfig,
) -> Result<(RefPredictor, &'static str), AriaError> {
    match trained {
        Some(p) => {
            if p.n_modes() != config.n_modes || p.latent_dim() != config.latent_dim {
                return Err(AriaError::Config(format!(
                    "predictor expects N={}, dim(Z)={} but the payload was ingested at \
                     N={}, dim(Z)={}",
                    p.n_modes(),
                    p.latent_dim(),
                    config.n_modes,
                    config.latent_dim
                )));
            }
            Ok((RefPredictor::Trained(p), "trained"))
        }
        None => Ok((
            RefPredictor::Sim(SimPredictor::new(config.n_modes, config.latent_dim)),
            "sim",
        )),
    }
}

/// Fill defaults so a host that sent nothing still receives a complete query.
fn normalize_query(query: Option<TelemetryQuery>, tau: f64) -> TelemetryQuery {
    let mut q = query.unwrap_or_default();
    if q.match_clause.nodes.is_empty() {
        q.match_clause.nodes = "*".into();
    }
    if q.match_clause.edges.is_empty() {
        q.match_clause.edges = "*".into();
    }
    // τ is measured, not requested: the run already happened at this radius,
    // and echoing a different number would misreport what produced the graph.
    q.where_clause.tau = tau;
    if q.return_keys.is_empty() {
        q.return_keys = TelemetryQuery::default().return_keys;
    }
    q
}

/// Build the probable, pruned, natural tagging of the whole map.
///
/// A **view** over `G`, never a mutation of it. Three parts:
///
/// 1. **Probable edges** — every host edge, plus τ-near pairs re-checked in the
///    engine's own compensated f64 metric (the same decision `SimGraphBackend`
///    makes, so the view cannot disagree with the graph).
/// 2. **Clusters** — the existing Fiedler decomposition. View-only.
/// 3. **Binary index** — relation type → binary types present, so a host can
///    route by `where.binary_types`.
fn build_tags(
    g: &aria_engine_core::graph::Graph,
    ingested: &Ingested,
    policy: &str,
    tau: f64,
    query: &TelemetryQuery,
) -> TaggingState {
    let type_filter: BTreeSet<&str> = query
        .where_clause
        .edge_types
        .iter()
        .map(String::as_str)
        .collect();
    let keep_type = |t: &EdgeType| type_filter.is_empty() || type_filter.contains(t.as_str());

    // Host edges first: a relation the host asserted is never pruned.
    let mut probable: BTreeMap<(NodeId, NodeId, String), IpoEdge> = BTreeMap::new();
    for e in &g.edges {
        if e.from == e.to || !keep_type(&e.edge_type) {
            continue;
        }
        probable.insert(
            (e.from, e.to, e.edge_type.as_str().to_string()),
            IpoEdge {
                from: e.from,
                to: e.to,
                edge_type: e.edge_type.clone(),
                origin: ingested.origins.edge(e.from, e.to),
                binary_type: Some(binary_type_for_edge(&e.edge_type)),
            },
        );
    }

    // τ-near pairs the graph does not already connect.
    if keep_type(&EdgeType::Refines) {
        for (a, b) in near_pairs(g, tau) {
            let key = (a, b, EdgeType::Refines.as_str().to_string());
            probable.entry(key).or_insert(IpoEdge {
                from: a,
                to: b,
                edge_type: EdgeType::Refines,
                // Proximity Aria measured, not a relation the host stated.
                origin: NodeOrigin::Transform,
                binary_type: Some(binary_type_for_edge(&EdgeType::Refines)),
            });
        }
    }
    let probable_edges: Vec<IpoEdge> = probable.into_values().collect();

    let clusters = build_clusters(g);

    let mut binary_index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in &g.edges {
        let entry = binary_index
            .entry(e.edge_type.as_str().to_string())
            .or_default();
        let bt = binary_type_for_edge(&e.edge_type);
        if !entry.contains(&bt) {
            entry.push(bt);
        }
    }
    for list in binary_index.values_mut() {
        list.sort_unstable();
    }

    TaggingState {
        policy: policy.to_string(),
        tau,
        pruned: true,
        clusters,
        probable_edges,
        binary_index,
    }
}

/// Neighbours queried per node when building the probable-edge view.
///
/// The view is a *sparse* summary, so a small constant is the right shape: the
/// total edge count is bounded by `PROBABLE_NEIGHBORS · |V| / 2`, and `|V|` is
/// already bounded by `Limits::max_nodes`.
const PROBABLE_NEIGHBORS: usize = 4;

/// τ-near node pairs, found through the metric index rather than by exhaustion.
///
/// # Why not a nested loop
///
/// The obvious `for a in ids { for b in ids }` is `O(|V|²)`. At the default
/// `max_nodes = 65_536` that is 2.1 billion distance evaluations for a *view* —
/// an unbounded-work defect that would breach L8 on a payload Aria had already
/// accepted. `HnswIndex` is the spec-normative retrieval path (ℙ5 / ℂ3,
/// `ef_search = 64`) and turns the scan into `O(|V| log |V|)`.
///
/// # Why τ is rescaled, and why rank alone is not enough
///
/// This is the merge-collapse finding already sealed in `spec/WINNING-V3.md`
/// §2.5 (𝔸8 / 𝕃-F): a contractive predictor confines latents to a ball of
/// radius `R_z ≤ ε/2`, so any *static* threshold `τ ≥ R_z` makes every pair
/// "near" and the view degenerates into a complete graph. The sealed
/// resolution is scale-invariant eligibility:
///
/// ```text
/// τ_eff = τ · min(1, R_z),   R_z = max ‖z‖₂ over the graph
/// ```
///
/// Measured on the shipped sheet that is necessary but *not sufficient*: the
/// static form reported 222 edges over 21 nodes, and τ_eff alone still
/// reported 111, because τ_eff = 0.2755 lands almost exactly on the median
/// pairwise distance (0.2809) — a threshold at the median produces a
/// ~50%-dense graph, which asserts that everything relates to everything.
///
/// So eligibility is the **conjunction** of two independent conditions:
///
/// 1. **Mutual k-NN** — `b` is kept for `a` only if each is among the other's
///    `PROBABLE_NEIGHBORS` nearest. Reciprocity is the standard sparsification
///    for exactly this failure mode, and it is symmetric by construction,
///    which is what an undirected "probable relation" should be.
/// 2. **Scale-invariant metric gate** — the pair must also satisfy
///    `d ≤ τ_eff`, so a mutual pair that is merely *relatively* close in a
///    sparse region is still excluded.
///
/// Distances are re-checked in the engine's own compensated `f64` metric, so
/// the view can never disagree with what `SimGraphBackend` would have merged:
/// the index ranks, the engine decides.
fn near_pairs(g: &aria_engine_core::graph::Graph, tau: f64) -> BTreeSet<(NodeId, NodeId)> {
    let mut out = BTreeSet::new();
    if g.node_count() < 2 {
        return out;
    }

    let radius = g
        .nodes
        .values()
        .map(|n| euclidean_distance(&n.embedding, &vec![0.0; n.embedding.len()]))
        .fold(0.0_f64, f64::max);
    let tau_eff = tau * radius.min(1.0);
    // `<=` rather than `!(> 0)` so a NaN τ falls through to "no pairs" instead
    // of relying on how a negated partial comparison reads.
    if tau_eff.is_nan() || tau_eff <= 0.0 {
        return out;
    }

    let dim = g.nodes.values().next().map_or(0, |n| n.embedding.len());
    let mut index = crate::index::HnswIndex::new(dim);
    for (id, node) in &g.nodes {
        index.add(*id, &node.embedding);
    }

    // Rank first: who is in whose neighbourhood.
    let mut neighbourhood: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
    for (id, node) in &g.nodes {
        let near = index
            .nearest(&node.embedding, PROBABLE_NEIGHBORS + 1)
            .into_iter()
            .filter(|(other, _)| other != id)
            .take(PROBABLE_NEIGHBORS)
            .map(|(other, _)| other)
            .collect();
        neighbourhood.insert(*id, near);
    }

    // Then keep only reciprocated pairs that also clear the metric gate.
    for (id, near) in &neighbourhood {
        for other in near {
            if !neighbourhood.get(other).is_some_and(|n| n.contains(id)) {
                continue;
            }
            let (Some(a), Some(b)) = (g.node(*id), g.node(*other)) else {
                continue;
            };
            if euclidean_distance(&a.embedding, &b.embedding) > tau_eff {
                continue;
            }
            let (lo, hi) = if id < other { (*id, *other) } else { (*other, *id) };
            out.insert((lo, hi));
        }
    }
    out
}

/// Fiedler bisection into two labelled clusters, reusing the existing
/// spectral helpers. Empty for a graph too small to bisect — `[]`, never null.
fn build_clusters(g: &aria_engine_core::graph::Graph) -> Vec<Cluster> {
    if g.node_count() < 2 || g.edge_count() == 0 {
        return Vec::new();
    }
    // Structural edges only: the affinity floor in `from_graph` drives a
    // bipartite row/facet map toward complete and destroys the cut (see that
    // constructor's note for the measured 1/20 degeneracy).
    let lap = GraphLaplacian::from_graph_structural(g);
    let connectivity = lap
        .fiedler_vector(128, 1e-7)
        .map_or(0.0, |f| f.lambda_2);
    let (left, right) = lap.spectral_bisection();

    [("cluster_0", left), ("cluster_1", right)]
        .into_iter()
        .enumerate()
        .filter(|(_, (_, ids))| !ids.is_empty())
        .map(|(i, (label, mut ids))| {
            ids.sort_unstable();
            Cluster {
                id: i,
                label: label.to_string(),
                node_ids: ids,
                connectivity,
            }
        })
        .collect()
}

/// Apply `query.return` by dropping the optional sections a host did not ask
/// for. `query` and `receipt` are never dropped — they are guaranteed.
///
/// Kept separate from [`transform`] so the envelope is always built and
/// validated in full before anything is withheld; a projection must not be
/// able to hide a defect.
pub fn apply_return_keys(mut env: TelemetryEnvelope) -> TelemetryEnvelope {
    let wanted: BTreeSet<&str> = env.query.return_keys.iter().map(String::as_str).collect();
    if !wanted.contains("structure") {
        env.structure = None;
    }
    if !wanted.contains("ledger") {
        env.ledger = None;
    }
    if !wanted.contains("graph") {
        env.graph.nodes.clear();
        env.graph.edges.clear();
    }
    if !wanted.contains("records") {
        env.records.clear();
    }
    if !wanted.contains("source") {
        env.source = Value::Null;
    }
    if !wanted.contains("tags") {
        env.tags.clusters.clear();
        env.tags.probable_edges.clear();
        env.tags.binary_index.clear();
    }
    env
}

/// The shape the payload was read as — surfaced for diagnostics, never for a
/// behavioral decision.
pub fn payload_shape(bytes: &[u8], config: &AriaConfig, limits: Limits) -> Option<PayloadShape> {
    ingest(bytes, config.n_modes, config.latent_dim, limits)
        .ok()
        .map(|i| i.shape)
}
