use serde::{Deserialize, Serialize};

use crate::condition::Condition;
use crate::error::AriaError;
use crate::gates::GateConfig;
use crate::policy::{DiffPolicy, MatchPolicy};

/// Multi-task loss weights λ = (λ_JEPA, λ_NLL, λ_Spectral, λ_Graph) — the
/// probability simplex Δ³ of spec §0.4 / §6.0: each term ≥ 0, Σ λᵢ = 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LossLambdas {
    /// λ_JEPA — weight of the stop-gradient latent-prediction term.
    #[serde(default = "quarter")]
    pub jepa: f64,
    /// λ_NLL — weight of the decoupled output term (trained strictly outside Φ).
    #[serde(default = "quarter")]
    pub nll: f64,
    /// λ_Spectral — weight of the spectral (Lipschitz) penalty.
    #[serde(default = "quarter")]
    pub spectral: f64,
    /// λ_Graph — weight of the graph-structure penalty.
    #[serde(default = "quarter")]
    pub graph: f64,
}

/// Uniform point of Δ³: the only assumption-free default.
fn quarter() -> f64 {
    0.25
}

impl Default for LossLambdas {
    fn default() -> Self {
        Self {
            jepa: quarter(),
            nll: quarter(),
            spectral: quarter(),
            graph: quarter(),
        }
    }
}

/// Aria runtime configuration — TOML (PRD FR-10).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AriaConfig {
    /// Number of optical modes N (𝔸1)
    #[serde(default = "default_n_modes")]
    pub n_modes: usize,

    /// Latent dimension dim(Z)
    #[serde(default = "default_latent_dim")]
    pub latent_dim: usize,

    /// Contractivity tolerance ε (ℙ2, Inv2)
    #[serde(default = "default_eps")]
    pub eps: f64,

    /// Stutter budget K (𝐂5)
    #[serde(default = "default_stutter_k")]
    pub stutter_k: u64,

    /// Preferred schedule: "opmd" or custom action list
    #[serde(default = "default_schedule")]
    pub schedule: String,

    /// Match policy
    #[serde(default)]
    pub match_policy: MatchPolicy,

    /// Diff policy
    #[serde(default)]
    pub diff_policy: DiffPolicy,

    /// Conditioning
    #[serde(default)]
    pub condition: Condition,

    /// Which invariants to check: `["inv1","inv2","inv3","inv4"]`
    #[serde(default = "default_check_inv")]
    pub check_inv: Vec<String>,

    /// Optional operating gates Inv5–Inv11 (PRD §4.3). Off by default; these
    /// are monitors over a run, never Spec enlargement.
    #[serde(default)]
    pub gates: GateConfig,

    /// Maximum graph size |G| (nodes + edges)
    #[serde(default = "default_max_graph_size")]
    pub max_graph_size: usize,

    /// Optical backend selection: `"fft"` (ℙ1 O(N log N) phase-mask unitary,
    /// spec §5.2) or `"householder"` (the v0.1.0 cached-unitary reference).
    /// `None` = automatic: FFT for N ≥ 256 (spec mandate), Householder below.
    #[serde(default)]
    pub optical: Option<String>,

    /// Inv1 energy-drift check tolerance (spec §0.2): admissible (0, 1e-6].
    /// The winning-condition audit still demands ≤ 1e-7 regardless (WS6).
    #[serde(default = "default_eps_energy")]
    pub eps_energy: f64,

    /// Graph merge distance threshold τ (spec §0.4): admissible (0, 1].
    /// Consumed by the merge Match policy (plan WS3, 𝕃3).
    #[serde(default = "default_merge_tau")]
    pub merge_tau: f64,

    /// Multi-task loss weights λ ∈ Δ³ (spec §0.4, ℙ6): Σ λᵢ = 1, λᵢ ≥ 0.
    #[serde(default)]
    pub loss_lambdas: LossLambdas,

    /// Discrete output vocabulary size |V_o| (spec §0.1): 256 ≤ |V_o| ≤ 128000.
    #[serde(default = "default_vocab_size")]
    pub vocab_size: usize,

    /// Escape hatch: skip the 𝒮 dimension bounds
    /// (N ∈ {2^k : k ∈ [4,14]}, 8 ≤ d ≤ 2N) in [`AriaConfig::validate`].
    /// This exists so tests can run sub-spec dimensions (N = 8); every use is
    /// logged loudly. Never set this in a production config — it relaxes a
    /// spec hard bound, not an implementation tolerance.
    #[serde(default)]
    pub allow_sub_spec_dims: bool,

    /// Seed for deterministic mode (None = random)
    pub seed: Option<u64>,

    /// Strict mode: invariant violations are hard errors
    #[serde(default = "default_true")]
    pub strict: bool,
}

fn default_n_modes() -> usize {
    256
}
fn default_latent_dim() -> usize {
    64
}
fn default_eps() -> f64 {
    1.0
}
fn default_stutter_k() -> u64 {
    2
}
fn default_schedule() -> String {
    "opmd".into()
}
fn default_check_inv() -> Vec<String> {
    vec!["inv1".into(), "inv2".into(), "inv3".into(), "inv4".into()]
}
fn default_max_graph_size() -> usize {
    10_000
}
fn default_true() -> bool {
    true
}
fn default_eps_energy() -> f64 {
    // Tighter than the spec's 1e-7 winning-condition bound (spec §0.2); the
    // implementation-default tolerance is 1e-10, matching the pre-WS2
    // hardcoded check_inv1 literal.
    1e-10
}
fn default_merge_tau() -> f64 {
    0.5
}
fn default_vocab_size() -> usize {
    // Spec minimum |V_o| = 256 — the least presumptuous in-domain default.
    // The real value comes from training on the corpus (plan WS4).
    256
}

impl Default for AriaConfig {
    fn default() -> Self {
        AriaConfig {
            n_modes: default_n_modes(),
            latent_dim: default_latent_dim(),
            eps: default_eps(),
            stutter_k: default_stutter_k(),
            schedule: default_schedule(),
            match_policy: MatchPolicy::default(),
            diff_policy: DiffPolicy::default(),
            condition: Condition::default(),
            check_inv: default_check_inv(),
            gates: GateConfig::default(),
            max_graph_size: default_max_graph_size(),
            optical: None,
            eps_energy: default_eps_energy(),
            merge_tau: default_merge_tau(),
            loss_lambdas: LossLambdas::default(),
            vocab_size: default_vocab_size(),
            allow_sub_spec_dims: false,
            seed: None,
            strict: true,
        }
    }
}

impl AriaConfig {
    /// Quick test config with small dimensions.
    ///
    /// N = 8 lies outside the spec's 𝒮 domain (N ∈ {2^k : k ∈ [4,14]}), so the
    /// config sets `allow_sub_spec_dims` — the test-only escape validated (and
    /// logged loudly) by [`AriaConfig::validate`]. Production configs never do.
    pub fn test_config() -> Self {
        AriaConfig {
            n_modes: 8,
            latent_dim: 16,
            eps: 1.0,
            stutter_k: 2,
            schedule: "opmd".into(),
            match_policy: MatchPolicy::Identity,
            diff_policy: DiffPolicy::Identity,
            condition: Condition::Token,
            check_inv: default_check_inv(),
            gates: GateConfig::default(),
            max_graph_size: 5000,
            optical: None,
            eps_energy: default_eps_energy(),
            merge_tau: default_merge_tau(),
            loss_lambdas: LossLambdas::default(),
            vocab_size: default_vocab_size(),
            allow_sub_spec_dims: true,
            seed: Some(42),
            strict: true,
        }
    }

    /// Parse from TOML string.
    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    /// Validate this config against the 𝒮 hard bounds (spec §0.1 / §0.4).
    ///
    /// Clauses, in order:
    ///
    /// 1. `N ∈ {2^k : k ∈ [4,14]}`  ⟺  16 ≤ N ≤ 16384, a power of two.
    /// 2. `8 ≤ d ≤ 2N`.
    /// 3. `K ∈ {1,2,3,4}` (stutter budget).
    /// 4. `τ ∈ (0,1]` (merge distance threshold).
    /// 5. `λ ∈ Δ³`: four non-negative finite weights summing to 1 (within
    ///    1e-9 — the f64 accumulation tolerance for user-supplied weights;
    ///    the default quarter weights sum exactly).
    /// 6. `256 ≤ |V_o| ≤ 128000` (output vocabulary).
    ///
    /// Reject-with-detail, never clamp silently — Inv4's zero-coercion
    /// discipline applied to configuration. Clauses 1–2 are dimension bounds;
    /// they are skipped (with a loud log) when `allow_sub_spec_dims` is set —
    /// a test-only escape. Clauses 3–6 and the absolute floors (N ≥ 1, d ≥ 1)
    /// are never relaxed.
    ///
    /// Called from `Engine::init` and from the shared runner behind every
    /// CLI/Python/WASM entry.
    pub fn validate(&self) -> Result<(), AriaError> {
        // Absolute floors — never relaxed, even under the test escape.
        if self.n_modes == 0 {
            return Err(AriaError::Config("n_modes = 0 violates 𝒮: N ≥ 1".into()));
        }
        if self.latent_dim == 0 {
            return Err(AriaError::Config("latent_dim = 0 violates 𝒮: d ≥ 1".into()));
        }

        // Dimension bounds (spec §0.1) — the only clauses the escape relaxes.
        if self.allow_sub_spec_dims {
            eprintln!(
                "aria: allow_sub_spec_dims = true — skipping the 𝒮 dimension bounds \
                 (N ∈ {{2^k : k ∈ [4,14]}}, 8 ≤ d ≤ 2N) for N = {}, d = {}. \
                 Test-only escape; never set this in a production config.",
                self.n_modes, self.latent_dim
            );
        } else {
            if !self.n_modes.is_power_of_two() || !(16..=16384).contains(&self.n_modes) {
                return Err(AriaError::Config(format!(
                    "n_modes = {} violates 𝒮: N must be a power of two in [16, 16384] \
                     (N = 2^k, k ∈ [4, 14]) — spec §0.1",
                    self.n_modes
                )));
            }
            if !(8..=2 * self.n_modes).contains(&self.latent_dim) {
                return Err(AriaError::Config(format!(
                    "latent_dim = {} violates 𝒮: 8 ≤ d ≤ 2N = {} — spec §0.1",
                    self.latent_dim,
                    2 * self.n_modes
                )));
            }
        }

        // Stutter budget (spec §0.4, 𝐂5).
        if !(1..=4).contains(&self.stutter_k) {
            return Err(AriaError::Config(format!(
                "stutter_k = {} violates 𝒮: K ∈ {{1,2,3,4}} — spec §0.4",
                self.stutter_k
            )));
        }

        // Merge threshold (spec §0.4, ℙ3/𝕃3). NaN fails both comparisons.
        if !(self.merge_tau > 0.0 && self.merge_tau <= 1.0) {
            return Err(AriaError::Config(format!(
                "merge_tau = {} violates 𝒮: τ ∈ (0, 1] — spec §0.4",
                self.merge_tau
            )));
        }

        // Loss weights λ ∈ Δ³ (spec §0.4, ℙ6): λᵢ ≥ 0, finite, Σ λᵢ = 1.
        let terms = [
            ("jepa", self.loss_lambdas.jepa),
            ("nll", self.loss_lambdas.nll),
            ("spectral", self.loss_lambdas.spectral),
            ("graph", self.loss_lambdas.graph),
        ];
        for (name, w) in terms {
            if !w.is_finite() {
                return Err(AriaError::Config(format!(
                    "loss_lambdas.{name} = {w} violates 𝒮: λᵢ must be finite (λ ∈ Δ³) — spec §0.4"
                )));
            }
            if w < 0.0 {
                return Err(AriaError::Config(format!(
                    "loss_lambdas.{name} = {w} violates 𝒮: λᵢ ≥ 0 (λ ∈ Δ³) — spec §0.4"
                )));
            }
        }
        let sum: f64 = [
            self.loss_lambdas.jepa,
            self.loss_lambdas.nll,
            self.loss_lambdas.spectral,
            self.loss_lambdas.graph,
        ]
        .iter()
        .sum();
        if (sum - 1.0).abs() > 1e-9 {
            return Err(AriaError::Config(format!(
                "loss_lambdas sum = {sum} violates 𝒮: Σ λᵢ = 1 (λ ∈ Δ³) — spec §0.4"
            )));
        }

        // Output vocabulary bound |V_o| (spec §0.1).
        if !(256..=128_000).contains(&self.vocab_size) {
            return Err(AriaError::Config(format!(
                "vocab_size = {} violates 𝒮: 256 ≤ |V_o| ≤ 128000 — spec §0.1",
                self.vocab_size
            )));
        }

        // Optical backend (plan WS2): only the two shipped backends are
        // admissible; unknown values are rejected, never silently remapped.
        if let Some(ref optical) = self.optical {
            if optical != "fft" && optical != "householder" {
                return Err(AriaError::Config(format!(
                    "optical = {optical:?} is not a backend: use \"fft\" or \"householder\" (plan WS2)"
                )));
            }
            // Spec §0.2 mandates O(N log N) optical kernels for N ≥ 256 —
            // choosing the O(N²) reference there is allowed for research but
            // warned about loudly, mirroring the test-escape discipline.
            if optical == "householder" && self.n_modes >= 256 {
                eprintln!(
                    "aria: optical = \"householder\" at N = {} — the spec mandates O(N log N) \
                     for N ≥ 256 (§0.2, ℙ1); using the O(N²) reference backend anyway.",
                    self.n_modes
                );
            }
        }

        // Inv1 drift tolerance (spec §0.2): admissible (0, 1e-6].
        if !(self.eps_energy > 0.0 && self.eps_energy <= 1e-6) {
            return Err(AriaError::Config(format!(
                "eps_energy = {} violates 𝒮: admissible (0, 1e-6] — spec §0.2",
                self.eps_energy
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err_of(cfg: &AriaConfig) -> String {
        match cfg.validate() {
            Ok(()) => panic!("config unexpectedly validated: {cfg:?}"),
            Err(e) => e.to_string(),
        }
    }

    /// Config with a given (N, d); everything else at validated defaults.
    fn with_dims(n_modes: usize, latent_dim: usize) -> AriaConfig {
        AriaConfig {
            n_modes,
            latent_dim,
            ..AriaConfig::default()
        }
    }

    #[test]
    fn default_config_validates() {
        AriaConfig::default().validate().unwrap();
    }

    #[test]
    fn power_of_two_boundaries_validate() {
        // N = 2^4 and N = 2^14 are the inclusive ends of the admissible range.
        with_dims(16, 8).validate().unwrap();
        with_dims(16384, 64).validate().unwrap();
        // d = 2N is the inclusive upper end of the latent bound.
        with_dims(256, 512).validate().unwrap();
    }

    #[test]
    fn n_modes_rejection_table() {
        // Absolute floor (never relaxed, even under the escape).
        assert!(err_of(&with_dims(0, 16)).contains("n_modes = 0"));
        // k = 3 < 4: too small, though a power of two.
        assert!(err_of(&with_dims(8, 16)).contains("n_modes = 8"));
        // Not powers of two.
        assert!(err_of(&with_dims(12, 16)).contains("n_modes = 12"));
        assert!(err_of(&with_dims(24, 16)).contains("n_modes = 24"));
        // k = 15 > 14: too large.
        assert!(err_of(&with_dims(32768, 64)).contains("n_modes = 32768"));
    }

    #[test]
    fn latent_dim_rejection_table() {
        // Absolute floor (never relaxed, even under the escape).
        assert!(err_of(&with_dims(256, 0)).contains("latent_dim = 0"));
        // Below the spec floor 8.
        assert!(err_of(&with_dims(256, 7)).contains("latent_dim = 7"));
        // Above 2N.
        assert!(err_of(&with_dims(256, 513)).contains("latent_dim = 513"));
    }

    #[test]
    fn stutter_k_rejection_table() {
        let mut cfg = AriaConfig {
            stutter_k: 0,
            ..AriaConfig::default()
        };
        assert!(err_of(&cfg).contains("stutter_k = 0"));
        cfg.stutter_k = 5;
        assert!(err_of(&cfg).contains("stutter_k = 5"));
        for k in [1, 4] {
            cfg.stutter_k = k;
            cfg.validate().unwrap();
        }
    }

    #[test]
    fn merge_tau_rejection_table() {
        let mut cfg = AriaConfig::default();
        for bad in [0.0, -0.1, 1.000_000_1, f64::NAN] {
            cfg.merge_tau = bad;
            assert!(err_of(&cfg).contains("merge_tau"));
        }
        for good in [f64::MIN_POSITIVE, 0.5, 1.0] {
            cfg.merge_tau = good;
            cfg.validate().unwrap();
        }
    }

    #[test]
    fn loss_lambdas_rejection_table() {
        let mut cfg = AriaConfig::default();

        cfg.loss_lambdas.jepa = -0.1;
        assert!(err_of(&cfg).contains("loss_lambdas.jepa = -0.1"));

        cfg.loss_lambdas = LossLambdas::default();
        cfg.loss_lambdas.nll = f64::NAN;
        assert!(err_of(&cfg).contains("loss_lambdas.nll"));

        cfg.loss_lambdas = LossLambdas::default();
        cfg.loss_lambdas.spectral = 0.3;
        assert!(err_of(&cfg).contains("loss_lambdas sum"));

        // Non-uniform but exact: 0.2 + 0.3 + 0.25 + 0.25 = 1.
        cfg.loss_lambdas = LossLambdas {
            jepa: 0.2,
            nll: 0.3,
            spectral: 0.25,
            graph: 0.25,
        };
        cfg.validate().unwrap();

        // Within the 1e-9 accumulation tolerance.
        cfg.loss_lambdas = LossLambdas {
            jepa: 0.25,
            nll: 0.25,
            spectral: 0.25,
            graph: 0.250_000_000_4,
        };
        cfg.validate().unwrap();
    }

    #[test]
    fn vocab_size_rejection_table() {
        let mut cfg = AriaConfig::default();
        for bad in [0, 255, 128_001] {
            cfg.vocab_size = bad;
            assert!(err_of(&cfg).contains("vocab_size"));
        }
        for good in [256, 4096, 128_000] {
            cfg.vocab_size = good;
            cfg.validate().unwrap();
        }
    }

    #[test]
    fn test_config_uses_the_escape_and_logs_loudly() {
        // N = 8 is sub-spec; the escape makes the test config valid.
        AriaConfig::test_config().validate().unwrap();

        // Without the escape the same dims are rejected.
        let mut cfg = AriaConfig::test_config();
        cfg.allow_sub_spec_dims = false;
        assert!(err_of(&cfg).contains("n_modes = 8"));
    }

    #[test]
    fn escape_relaxes_only_the_dimension_clauses() {
        let mut cfg = AriaConfig::test_config();
        cfg.stutter_k = 9;
        assert!(err_of(&cfg).contains("stutter_k = 9"));

        let mut cfg = AriaConfig::test_config();
        cfg.merge_tau = 0.0;
        assert!(err_of(&cfg).contains("merge_tau"));

        let mut cfg = AriaConfig::test_config();
        cfg.vocab_size = 1;
        assert!(err_of(&cfg).contains("vocab_size"));

        let mut cfg = AriaConfig::test_config();
        cfg.loss_lambdas.jepa = 1.5;
        assert!(err_of(&cfg).contains("loss_lambdas sum"));
    }

    #[test]
    fn toml_defaults_for_the_new_fields() {
        let src = "n_modes = 256\nlatent_dim = 64\n";
        let cfg = AriaConfig::from_toml(src).unwrap();
        assert!((cfg.merge_tau - 0.5).abs() < 1e-12);
        assert_eq!(cfg.vocab_size, 256);
        assert_eq!(cfg.loss_lambdas, LossLambdas::default());
        assert!(!cfg.allow_sub_spec_dims);
        cfg.validate().unwrap();
    }

    #[test]
    fn toml_parses_the_new_fields_and_validates() {
        let src = r"
n_modes = 256
latent_dim = 64
merge_tau = 0.7
vocab_size = 8192
allow_sub_spec_dims = false

[loss_lambdas]
jepa = 0.4
nll = 0.2
spectral = 0.2
graph = 0.2
";
        let cfg = AriaConfig::from_toml(src).unwrap();
        assert!((cfg.merge_tau - 0.7).abs() < 1e-12);
        assert_eq!(cfg.vocab_size, 8192);
        assert!((cfg.loss_lambdas.jepa - 0.4).abs() < 1e-12);
        assert!((cfg.loss_lambdas.graph - 0.2).abs() < 1e-12);
        cfg.validate().unwrap();
    }

    #[test]
    fn toml_escape_path_is_expressible() {
        // The test-only escape must round-trip through the TOML surface —
        // otherwise test configs cannot be expressed on any runtime surface.
        let src = "n_modes = 8\nlatent_dim = 16\nallow_sub_spec_dims = true\n";
        let cfg = AriaConfig::from_toml(src).unwrap();
        assert!(cfg.allow_sub_spec_dims);
        cfg.validate().unwrap();
    }
}

#[cfg(test)]
mod ws2_tests {
    use super::*;

    fn err_of(cfg: &AriaConfig) -> String {
        match cfg.validate() {
            Ok(()) => panic!("config unexpectedly validated: {cfg:?}"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn eps_energy_rejection_table() {
        for bad in [0.0, -1e-12, 1.000_001e-6, f64::NAN] {
            let cfg = AriaConfig {
                eps_energy: bad,
                ..AriaConfig::default()
            };
            assert!(err_of(&cfg).contains("eps_energy"));
        }
        for good in [f64::MIN_POSITIVE, 1e-10, 1e-7, 1e-6] {
            let cfg = AriaConfig {
                eps_energy: good,
                ..AriaConfig::default()
            };
            cfg.validate().unwrap();
        }
        // Default stays the tight implementation tolerance.
        assert!((AriaConfig::default().eps_energy - 1e-10).abs() < 1e-20);
    }

    #[test]
    fn optical_backend_value_table() {
        let cfg = AriaConfig {
            optical: Some("fourier".into()),
            ..AriaConfig::default()
        };
        assert!(err_of(&cfg).contains("optical"));
        for good in ["fft", "householder"] {
            let cfg = AriaConfig {
                optical: Some(good.into()),
                ..AriaConfig::default()
            };
            cfg.validate().unwrap();
        }
        assert!(
            AriaConfig::default().optical.is_none(),
            "default is automatic"
        );
    }

    #[test]
    fn toml_parses_the_ws2_fields() {
        let src = r"
n_modes = 256
latent_dim = 64
optical = 'fft'
eps_energy = 1e-7
";
        let cfg = AriaConfig::from_toml(src).unwrap();
        assert_eq!(cfg.optical.as_deref(), Some("fft"));
        assert!((cfg.eps_energy - 1e-7).abs() < 1e-20);
        cfg.validate().unwrap();

        let minimal = AriaConfig::from_toml("n_modes = 256\nlatent_dim = 64\n").unwrap();
        assert!(minimal.optical.is_none());
        assert!((minimal.eps_energy - 1e-10).abs() < 1e-20);
    }
}
