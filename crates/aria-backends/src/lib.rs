//! Aria simulated backends — electronic simulation of Spec operators.
//!
//! All operators are trait implementations of aria-core's backend traits.
//! Phase 1 uses ideal/simulated operators; later phases add GPU/hardware backends.
//!
//! [`runner`] holds the single reference run path shared by the CLI, the Python
//! extension, and the WASM module (Phase 2 parity).

pub mod data;
pub mod dev_seed;
pub mod diffuser;
pub mod graph;
pub mod growth;
pub mod index;
pub mod ingest;
pub mod ipo;
pub mod laplacian;
pub mod observer;
pub mod ocid;
pub mod optical;
pub mod predictor;
pub mod readout;
pub mod runner;
pub mod sedenion;
pub mod spectral;
pub mod structure;
pub mod telemetry;
pub mod tokenizer;
pub mod trained;
pub mod verify;

pub use data::{
    dataset_from_bytes, dataset_from_file, decode_columnar, encode_columnar, encode_corpus,
    encode_window, ingest_columnar, FieldDataset, COLUMNAR_MAGIC,
};
pub use dev_seed::{graph_from_dev_seed, load_seed_graph, DevSeed, DEV_SEED_FORMAT};
pub use diffuser::SimDiffuser;
pub use graph::SimGraphBackend;
pub use growth::{fit_growth_exponent, log_checkpoints, GrowthFit};
pub use index::{HnswIndex, HnswParams, NearestStats, VectorIndex};
pub use ingest::{ingest, Ingested, PayloadShape};
pub use ipo::{
    anchor_of, binary_type_for_edge, binary_type_for_node, canonical_json, sha256_hex,
    validate_envelope, Cluster, ColumnRole, ColumnStat, FunctionalDep, GraphIpo, IpoEdge, IpoError,
    IpoNode, Limits, NodeOrigin, NodeRecord, OriginIndex, QueryMatch, QueryWhere, RoleThresholds,
    StructureReport, TaggingState, TelemetryEnvelope, TelemetryQuery, TelemetryReceipt,
    BINARY_TYPE_CUSTOM, GRAPH_IPO_V1, TELEMETRY_QUERY_V1, TELEMETRY_VERSION,
};
pub use laplacian::{
    cd_path_signature, cd_spectral_attention, FiedlerResult, GraphLaplacian, MarketMapNode,
};
pub use observer::{
    evaluate_functional, sha256, CollapsePoint, ObserverLedger, ObserverFunctional, PassiveObserver,
    BoundaryCertificate, MobiusVector, RESIDUAL_WINDOW, UNCONSTRAINED_PHASE_BITS, ZETA_HALF_LINE_OFFSET,
    DISSIPATION_SCALE, RATIONAL_PHASE_STEP, COHERENCE_FLOOR, OCTONION_MISALIGNMENT, RATIONAL_PI,
    SHADOW_SECTOR_DIM, BOUNDARY_CERT_BYTES, VISIBLE_SECTOR_DIM, FIXED_PHASE_OFFSET,
};
pub use ocid::{
    commit as ocid_commit, config_digest, signature_verification_available,
    verify_envelope as verify_ocid, Ocid, OcidBinds, OcidConfig, OcidError, OcidRequest, OCID_V1,
};
pub use optical::{FftOptical, RefOptical, SimOptical};
pub use predictor::SimPredictor;
pub use readout::{
    ContinuousReadout, DiscreteReadout, Readout, ReadoutError, ReadoutKind, READOUT_FORMAT,
    VOCAB_MAX, VOCAB_MIN,
};
pub use runner::{
    engine_with, latents_of, latents_with, run, run_observed, run_observed_with_graph, run_with,
    run_with_graph, sim_engine, ObservedRun, RefPredictor, RunOutcome, RunSummary, SimEngine,
};
pub use sedenion::{
    canonical_zero_divisor_pair, certified_context_mask, nullity_energy, AnnihilatorCertificate,
    Sedenion,
};
pub use spectral::{
    power_iteration, power_iteration_with_vectors, project_spectral, Matrix, SpectralError,
    SpectralReport, DEFAULT_ITERATIONS,
};
pub use structure::{analyze, distinct_values, facet_cap, is_present, Row, TabularPlan};
pub use telemetry::{
    apply_return_keys, node_profile_config, transform, TelemetryRequest,
};
pub use tokenizer::{BpeTokenizer, TOKENIZER_FORMAT};
pub use trained::{
    PredictorWeights, TrainedPredictor, WeightsError, PREDICTOR_V1_FORMAT, PREDICTOR_V2_FORMAT,
};
pub use verify::{
    audit_stream, verify, AuditConfig, TraceAudit, VerifyOpts, VerifyReceipt, RECEIPT_FORMAT,
};

/// Convenience constructor for a full simulated backend suite.
pub fn sim_backends(
    n_modes: usize,
    latent_dim: usize,
) -> (SimOptical, SimPredictor, SimGraphBackend, SimDiffuser) {
    (
        SimOptical::new(n_modes),
        SimPredictor::new(n_modes, latent_dim),
        SimGraphBackend::new(latent_dim),
        SimDiffuser::new(latent_dim),
    )
}
