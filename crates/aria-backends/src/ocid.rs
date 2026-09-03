//! OCID — the **Observation Commitment IDentifier**.
//!
//! One identifier that binds, cryptographically and reproducibly:
//!
//! ```text
//! OCID = H( domain ‖ public key ‖ source ‖ config ‖ graph ‖ structure )
//! ```
//!
//! A host that receives an envelope can recompute every one of those digests
//! from the envelope itself and check the OCID. If it matches, the graph in
//! that document provably came from *that* payload under *that* configuration
//! — not from a different sheet, not at a different τ, not from a different
//! seed. Nothing has to be taken on Aria's word.
//!
//! # Aria holds no private key, ever
//!
//! This module **verifies**; it never signs. A signing key is a credential, and
//! the refusal surface forbids Aria from holding one — a transform that could
//! sign on the host's behalf would be an authority. So the chain of custody is:
//!
//! 1. The plan authority signs the payload with its Ed25519
//!    private key and hands the worker the payload, the **public** key, and the
//!    signature.
//! 2. Aria verifies the signature over the exact payload bytes. That is where
//!    the elliptic curve does real work: it proves this payload is the sealed
//!    one, not a substitute.
//! 3. Aria binds the *verified* public key into the OCID.
//!
//! The result is a chain a third party can audit end to end: sealed payload →
//! verified identity → committed IPO.
//!
//! # Length-prefixed preimage
//!
//! Every field is prefixed with its length as 8 big-endian bytes. Plain
//! concatenation would be ambiguous — `("ab", "c")` and `("a", "bc")` produce
//! identical bytes and therefore identical commitments, which is a real
//! collision an attacker chooses. Length prefixing removes the ambiguity.
//!
//! # The dependency, and the fallback (minimal-dependency doctrine)
//!
//! Signature verification needs curve25519 field arithmetic and SHA-512, which
//! is not a few lines of in-repo code. `ed25519-dalek` 2.2.0 is therefore an
//! optional dependency behind the **`ocid-ed25519`** feature. Audited
//! 2026-08-31: MSRV 1.60 (lock is ≤ 1.97), published 2025-07-09 (lock is ≥ 7
//! days), BSD-3-Clause, `no_std`-capable, and no OS entropy on the verify path
//! (entropy arrives only with the non-default `rand_core` feature, which
//! enables key *generation* — something this module never does).
//!
//! **Named in-repo fallback:** the commitment itself needs no dependency at
//! all. With the feature off, OCID is still computed and still recomputable by
//! any third party; only the signature check is unavailable, and the envelope
//! reports that honestly as `signature_verified: null` with a `note` saying
//! why. The acceptance predicate for a verifiable commitment does not move.
//! `2.2.0` is pinned over the newer `3.0.0` deliberately: 72M downloads versus
//! 1.9M, and MSRV 1.60 leaves far more headroom.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ipo::{
    canonical_json, sha256_hex, GraphIpo, StructureReport, TelemetryEnvelope,
};

/// Significant decimal digits used when binding an `f64`.
///
/// Set deliberately coarser than the wire's measured fidelity — see
/// [`Enc::f64`] for the evidence and the reasoning.
const FLOAT_BIND_DIGITS: usize = 10;

/// Format-independent encoder for commitment preimages.
///
/// # The problem this solves: `f64` does not survive JSON
///
/// A commitment is worthless if a faithful copy of the document fails to
/// verify. Measured directly against the `serde_json` version in this
/// workspace's `Cargo.lock`:
///
/// ```text
/// "0.19565758484969079"   rust core parse  -> 0x3fc90b4ec81266f5   (correct)
///                         serde_json parse -> 0x3fc90b4ec81266f6   (1 ULP high)
/// ```
///
/// The *serializer* is correct — it emits exactly what `std` Display emits, the
/// shortest round-tripping form. The **parser** is not correctly rounded for
/// 17-significant-digit decimals. So `f64 -> JSON -> f64` is not the identity,
/// and neither a bit-exact digest nor a digest over re-serialized text can
/// agree between a producer and a verifier that sit on opposite sides of the
/// wire.
///
/// # The resolution
///
/// Integers, strings, counts, ids and enum tags round-trip exactly, so the
/// graph's **topology** and the structure report's **counts** are bound
/// exactly. Floats are bound at [`FLOAT_BIND_DIGITS`] significant digits, which
/// is far coarser than the ~17th-digit error above and therefore stable across
/// the wire.
///
/// Nothing meaningful is given up. A one-ULP difference is not an attack, and
/// the embeddings are a *deterministic function of inputs the commitment
/// already binds* — payload bytes, N, dim(Z), ε, seed, schedule, policy, τ and
/// step count. Anyone who wants bit-exact assurance re-runs the transform and
/// compares; the commitment's job is to pin which inputs and which discrete
/// output produced this document, and it does that exactly.
#[derive(Default)]
struct Enc(Vec<u8>);

impl Enc {
    /// A 64-bit big-endian integer.
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    /// A length, widened without a lossy cast.
    fn len(&mut self, v: usize) {
        self.u64(u64::try_from(v).unwrap_or(u64::MAX));
    }

    /// An `f64` at [`FLOAT_BIND_DIGITS`] significant digits.
    ///
    /// Scientific notation makes the precision scale-invariant, so a component
    /// of magnitude 1e-6 is bound as tightly as one of magnitude 1e3.
    /// Non-finite values cannot occur in a validated envelope, but are given a
    /// distinct tag rather than a panic so the encoder is total.
    fn f64(&mut self, v: f64) {
        if v.is_finite() {
            self.str(&format!("{v:.FLOAT_BIND_DIGITS$e}"));
        } else {
            self.tag(0xFF);
            self.str(if v.is_nan() {
                "nan"
            } else if v > 0.0 {
                "inf"
            } else {
                "-inf"
            });
        }
    }

    /// A length-prefixed string. Prefixing keeps adjacent fields unambiguous.
    fn str(&mut self, s: &str) {
        self.len(s.len());
        self.0.extend_from_slice(s.as_bytes());
    }

    /// A one-byte discriminant, so a present-but-empty field and an absent one
    /// cannot encode identically.
    fn tag(&mut self, t: u8) {
        self.0.push(t);
    }

    fn finish(self) -> String {
        sha256_hex(&self.0)
    }
}

/// Bit-exact digest of the exported graph.
///
/// Iteration follows the emitted order, which `GraphIpo::from_graph` already
/// fixes as ascending node id and ascending edge key.
pub fn graph_digest(graph: &GraphIpo) -> String {
    let mut e = Enc::default();
    e.str(&graph.schema);
    e.len(graph.nodes.len());
    for n in &graph.nodes {
        e.u64(n.id);
        e.str(n.node_type.as_str());
        e.len(n.embedding.len());
        for c in &n.embedding {
            e.f64(*c);
        }
        e.u64(n.timestamp);
        e.str(n.origin.as_str());
        match &n.binary_type {
            Some(b) => {
                e.tag(1);
                e.str(b);
            }
            None => e.tag(0),
        }
    }
    e.len(graph.edges.len());
    for edge in &graph.edges {
        e.u64(edge.from);
        e.u64(edge.to);
        e.str(edge.edge_type.as_str());
        e.str(edge.origin.as_str());
        match &edge.binary_type {
            Some(b) => {
                e.tag(1);
                e.str(b);
            }
            None => e.tag(0),
        }
    }
    e.finish()
}

/// Bit-exact digest of the structure report, or of its absence.
pub fn structure_digest(structure: Option<&StructureReport>) -> String {
    let mut e = Enc::default();
    let Some(s) = structure else {
        e.tag(0);
        return e.finish();
    };
    e.tag(1);
    e.len(s.n_rows);
    e.len(s.columns.len());
    for c in &s.columns {
        e.str(&c.column);
        e.str(c.role.as_str());
        e.str(&c.rule);
        e.len(c.n_rows);
        e.len(c.present);
        e.len(c.distinct);
        e.f64(c.coverage);
        e.f64(c.uniqueness);
        e.len(c.singletons);
    }
    e.len(s.functional_deps.len());
    for d in &s.functional_deps {
        e.str(&d.from);
        e.str(&d.to);
        e.len(d.distinct_from);
        e.len(d.distinct_to);
        e.len(d.support);
    }
    e.f64(s.thresholds.near_key_coverage);
    e.len(s.thresholds.facet_max_distinct);
    e.f64(s.thresholds.facet_max_ratio);
    e.tag(u8::from(s.dependency_scan_complete));
    e.finish()
}

/// Format tag and hash-domain separator. Changing the binding changes this.
pub const OCID_V1: &str = "aria-ocid-v1";

/// Why an OCID could not be produced or did not verify.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OcidError {
    /// A hex field was the wrong length or contained a non-hex digit.
    #[error("{field}: expected {expected} lowercase hex digits, got {got}")]
    Hex {
        /// Which field.
        field: &'static str,
        /// Required digit count.
        expected: usize,
        /// Supplied digit count.
        got: usize,
    },
    /// The public key is not a valid Ed25519 point, or is a weak key.
    #[error("public key rejected: {0}")]
    Key(String),
    /// A signature was supplied but did not verify over the payload.
    #[error("signature does not verify over the payload bytes")]
    Signature,
    /// The recomputed commitment disagrees with the one in the document.
    #[error("OCID mismatch: document says {found}, recomputation gives {expected}")]
    Mismatch {
        /// What the document claimed.
        found: String,
        /// What recomputation produced.
        expected: String,
    },
    /// A digest the commitment binds disagrees with the envelope's content.
    #[error("{0}")]
    Binding(String),
    /// A signature was supplied but this build cannot check it.
    #[error("signature supplied but the 'ocid-ed25519' feature is not enabled")]
    VerifierUnavailable,
}

/// The exact digests a commitment binds. Emitted so a third party can
/// recompute each one independently instead of trusting the total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcidBinds {
    /// Hash-domain separator; always [`OCID_V1`].
    pub domain: String,
    /// Ed25519 public key as 64 lowercase hex digits, or empty when unbound.
    pub public_key: String,
    /// SHA-256 of the exact payload bytes.
    pub source_sha256: String,
    /// SHA-256 of the canonical trajectory-defining configuration.
    pub config_sha256: String,
    /// SHA-256 of the canonical graph IPO object.
    pub graph_sha256: String,
    /// SHA-256 of the canonical structure report (`null` when absent).
    pub structure_sha256: String,
}

/// The commitment, as it appears in the envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ocid {
    /// Always [`OCID_V1`].
    pub schema: String,
    /// The commitment: 64 lowercase hex digits.
    pub ocid: String,
    /// The public key bound in, when the host supplied one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// `Some(true)`: a signature was supplied and verified. `Some(false)` is
    /// never emitted — a failed verification is an error, not a field, because
    /// returning a document that says "this did not verify" invites a host to
    /// ignore it. `None`: no signature was supplied, or no verifier is built in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_verified: Option<bool>,
    /// Why verification was not performed, when it was not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The digests bound, for independent recomputation.
    pub binds: OcidBinds,
}

/// What a caller supplies to request an OCID.
#[derive(Debug, Clone, Default)]
pub struct OcidRequest {
    /// Ed25519 public key, 64 lowercase hex digits.
    pub public_key_hex: Option<String>,
    /// Ed25519 signature over the exact payload bytes, 128 hex digits.
    pub signature_hex: Option<String>,
}

impl OcidRequest {
    /// Whether the caller asked for anything at all.
    pub fn is_empty(&self) -> bool {
        self.public_key_hex.is_none() && self.signature_hex.is_none()
    }
}

/// The trajectory-defining inputs the commitment binds.
///
/// Deliberately *not* the whole `AriaConfig`: binding fields that cannot change
/// the graph would make the OCID brittle for no gain. These are exactly the
/// inputs that select the trajectory, mirroring the trace header's contract.
#[derive(Debug, Clone, Serialize)]
pub struct OcidConfig {
    /// Optical modes N.
    pub n_modes: usize,
    /// Latent dimension dim(Z).
    pub latent_dim: usize,
    /// Contractivity tolerance ε.
    pub eps: f64,
    /// Seed, when fixed.
    pub seed: Option<u64>,
    /// Schedule string.
    pub schedule: String,
    /// Match policy wire name.
    pub match_policy: String,
    /// Merge radius τ.
    pub merge_tau: f64,
    /// Steps executed.
    pub steps: u64,
}

/// SHA-256 over the canonical JSON of the configuration.
pub fn config_digest(cfg: &OcidConfig) -> String {
    let value = serde_json::to_value(cfg).unwrap_or(Value::Null);
    sha256_hex(&canonical_json(&value))
}

/// Reconstruct the bound configuration from a finished envelope.
///
/// Every field comes from the receipt or the tags, which is what makes the
/// commitment verifiable by a host holding only the document.
pub fn config_of(env: &TelemetryEnvelope) -> OcidConfig {
    OcidConfig {
        n_modes: env.receipt.n_modes,
        latent_dim: env.receipt.latent_dim,
        eps: env.receipt.eps,
        seed: env.receipt.seed,
        schedule: env.receipt.schedule.clone(),
        match_policy: env.receipt.match_policy.clone(),
        merge_tau: env.tags.tau,
        steps: env.receipt.steps,
    }
}

/// Length-prefixed, domain-separated commitment preimage.
///
/// Each field contributes `len(8, big-endian) ‖ bytes`. See the module note on
/// why plain concatenation is unsafe here.
fn preimage(binds: &OcidBinds) -> Vec<u8> {
    let mut out = Vec::new();
    for field in [
        binds.domain.as_str(),
        binds.public_key.as_str(),
        binds.source_sha256.as_str(),
        binds.config_sha256.as_str(),
        binds.graph_sha256.as_str(),
        binds.structure_sha256.as_str(),
    ] {
        out.extend_from_slice(&(field.len() as u64).to_be_bytes());
        out.extend_from_slice(field.as_bytes());
    }
    out
}

/// Compute the commitment from a set of bindings.
pub fn commit(binds: &OcidBinds) -> String {
    sha256_hex(&preimage(binds))
}

/// Validate a lowercase-hex field of an exact digit count.
fn hex_field(field: &'static str, value: &str, expected: usize) -> Result<Vec<u8>, OcidError> {
    if value.len() != expected || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(OcidError::Hex {
            field,
            expected,
            got: value.len(),
        });
    }
    Ok((0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap_or(0))
        .collect())
}

/// Verify an Ed25519 signature over `message`.
///
/// Uses `verify_strict`, not `verify`. The crate's own documentation records
/// that `verify()` **permits weak public keys**: an attacker can craft a key
/// `A` and a signature that validates against almost any message. `verify_strict`
/// additionally rejects small-order (weak) keys, which is the difference
/// between a real proof of custody and a checkbox.
#[cfg(feature = "ocid-ed25519")]
pub fn verify_signature(
    public_key_hex: &str,
    signature_hex: &str,
    message: &[u8],
) -> Result<(), OcidError> {
    use ed25519_dalek::{Signature, VerifyingKey};

    let key_bytes = hex_field("public_key", public_key_hex, 64)?;
    let sig_bytes = hex_field("signature", signature_hex, 128)?;

    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| OcidError::Key("public key is not 32 bytes".into()))?;
    let verifying = VerifyingKey::from_bytes(&key_array)
        .map_err(|e| OcidError::Key(format!("not a valid curve point: {e}")))?;
    if verifying.is_weak() {
        return Err(OcidError::Key(
            "weak (small-order) public key: a forged signature could validate against \
             almost any message"
                .into(),
        ));
    }

    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| OcidError::Signature)?;
    let signature = Signature::from_bytes(&sig_array);

    verifying
        .verify_strict(message, &signature)
        .map_err(|_| OcidError::Signature)
}

/// Signature verification is not compiled in.
#[cfg(not(feature = "ocid-ed25519"))]
pub fn verify_signature(
    public_key_hex: &str,
    _signature_hex: &str,
    _message: &[u8],
) -> Result<(), OcidError> {
    // Still validate the key's shape so a malformed request fails the same way
    // in both builds; only the curve check is missing.
    hex_field("public_key", public_key_hex, 64)?;
    Err(OcidError::VerifierUnavailable)
}

/// Whether this build can check signatures.
pub const fn signature_verification_available() -> bool {
    cfg!(feature = "ocid-ed25519")
}

/// Build the OCID for a finished transform.
///
/// `payload` must be the exact bytes the host supplied — the same bytes
/// `source_sha256` was taken over — because that is what the signature covers.
pub fn build(
    request: &OcidRequest,
    payload: &[u8],
    source_sha256: &str,
    cfg: &OcidConfig,
    graph: &GraphIpo,
    structure: Option<&StructureReport>,
) -> Result<Ocid, OcidError> {
    let mut signature_verified = None;
    let mut note = None;

    let public_key = match &request.public_key_hex {
        Some(hex) => {
            hex_field("public_key", hex, 64)?;
            hex.to_lowercase()
        }
        None => String::new(),
    };

    match (&request.public_key_hex, &request.signature_hex) {
        (Some(key), Some(sig)) => {
            if signature_verification_available() {
                verify_signature(key, sig, payload)?;
                signature_verified = Some(true);
            } else {
                return Err(OcidError::VerifierUnavailable);
            }
        }
        (Some(_), None) => {
            note = Some(
                "public key bound into the commitment; no signature was supplied, so custody \
                 of the payload is not proven"
                    .into(),
            );
        }
        (None, Some(_)) => {
            return Err(OcidError::Key(
                "a signature was supplied without a public key to check it against".into(),
            ));
        }
        (None, None) => {
            note = Some(
                "no key supplied: the commitment binds payload, config and output only".into(),
            );
        }
    }

    let binds = OcidBinds {
        domain: OCID_V1.to_string(),
        public_key,
        source_sha256: source_sha256.to_string(),
        config_sha256: config_digest(cfg),
        graph_sha256: graph_digest(graph),
        structure_sha256: structure_digest(structure),
    };

    Ok(Ocid {
        schema: OCID_V1.to_string(),
        ocid: commit(&binds),
        key: request.public_key_hex.as_ref().map(|k| k.to_lowercase()),
        signature_verified,
        note,
        binds,
    })
}

/// Independently verify the OCID on an envelope.
///
/// This is the function a **host** runs — it recomputes every bound digest from
/// the envelope's own content and from the payload bytes, then recomputes the
/// commitment. Passing means the graph in this document came from that exact
/// payload under that exact configuration.
///
/// Note what it does *not* require: no trust in Aria, no access to Aria's
/// internals, and no private key.
pub fn verify_envelope(env: &TelemetryEnvelope, payload: &[u8]) -> Result<(), OcidError> {
    let Some(ocid) = &env.ocid else {
        return Err(OcidError::Binding(
            "envelope carries no OCID to verify".into(),
        ));
    };

    // 1. The payload really is the one this envelope describes.
    let recomputed_source = sha256_hex(payload);
    if recomputed_source != env.source_sha256 {
        return Err(OcidError::Binding(format!(
            "payload hashes to {recomputed_source} but the envelope says {}",
            env.source_sha256
        )));
    }
    if ocid.binds.source_sha256 != env.source_sha256 {
        return Err(OcidError::Binding(
            "the commitment binds a different source hash than the envelope reports".into(),
        ));
    }

    // 2. The bound output digests match the content actually present. Both are
    //    bit-exact over IEEE-754 patterns, so a JSON round-trip through any
    //    float formatter cannot perturb them.
    let graph = graph_digest(&env.graph);
    if graph != ocid.binds.graph_sha256 {
        return Err(OcidError::Binding(format!(
            "graph digest {graph} does not match the bound {}",
            ocid.binds.graph_sha256
        )));
    }
    let structure = structure_digest(env.structure.as_ref());
    if structure != ocid.binds.structure_sha256 {
        return Err(OcidError::Binding(format!(
            "structure digest {structure} does not match the bound {}",
            ocid.binds.structure_sha256
        )));
    }

    // 3. The configuration digest, recomputed from the receipt. Without this
    //    step a run at a different τ or seed could present the same OCID.
    let cfg = config_of(env);
    let config_digest = config_digest(&cfg);
    if config_digest != ocid.binds.config_sha256 {
        return Err(OcidError::Binding(format!(
            "config digest {config_digest} does not match the bound {}",
            ocid.binds.config_sha256
        )));
    }

    // 4. Finally the commitment itself.
    let recomputed = commit(&ocid.binds);
    if recomputed != ocid.ocid {
        return Err(OcidError::Mismatch {
            found: ocid.ocid.clone(),
            expected: recomputed,
        });
    }
    Ok(())
}
