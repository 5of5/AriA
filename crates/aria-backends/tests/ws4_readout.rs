//! WS4 — decoupled readout heads (𝔸5, 𝕃5).
//!
//! The head lives in backends. Φ is not modified. A run with and without
//! a subsequent emit produces the same JSONL bytes.

use aria_engine_backends::runner::{self, latents_of};
use aria_engine_backends::{BpeTokenizer, DiscreteReadout, Readout};
use aria_engine_core::config::AriaConfig;

fn test_config() -> AriaConfig {
    AriaConfig {
        n_modes: 8,
        latent_dim: 16,
        seed: Some(42),
        ..AriaConfig::test_config()
    }
}

#[test]
fn a_run_then_an_emit_leaves_the_trace_byte_identical() {
    let config = test_config();
    let a = runner::run(config.clone(), 200).expect("run A");
    let before = a.trace.to_jsonl();
    // Emit path: recover z, decode. Must not be able to change `before`.
    let zs = latents_of(config.clone(), 200).expect("latents");
    assert_eq!(zs.len(), 200);
    let head = DiscreteReadout::seeded(16, 256, 1.0, 7).unwrap();
    let ids: Vec<u32> = zs.iter().map(|z| head.decode_id(z).unwrap()).collect();
    assert_eq!(ids.len(), 200);
    assert!(
        ids.iter().any(|&id| id > 0),
        "seeded head collapsed to id 0"
    );

    let b = runner::run(config, 200).expect("run B");
    assert_eq!(
        before,
        b.trace.to_jsonl(),
        "recovering z / decoding tokens changed the Φ trace"
    );
    assert_eq!(before, a.trace.to_jsonl());
}

#[test]
fn latents_match_the_trace_length_and_are_deterministic() {
    let config = test_config();
    let a = latents_of(config.clone(), 80).unwrap();
    let b = latents_of(config, 80).unwrap();
    assert_eq!(a.len(), 80);
    assert_eq!(a, b, "z-sequence is not deterministic");
    assert!(a
        .iter()
        .all(|z| z.len() == 16 && z.iter().all(|x| x.is_finite())));
}

#[test]
fn one_thousand_step_trace_decodes_to_one_thousand_tokens() {
    let config = test_config();
    let outcome = runner::run(config.clone(), 1000).expect("1000-step run");
    assert!(outcome.summary.invariants_ok);
    let zs = latents_of(config, 1000).unwrap();
    assert_eq!(zs.len(), 1000);
    let head = DiscreteReadout::seeded(16, 256, 1.0, 11).unwrap();
    let tok = BpeTokenizer::bytes();
    let mut pieces = Vec::with_capacity(1000);
    for z in &zs {
        let id = head.decode_id(z).unwrap();
        pieces.push(tok.decode_one(id).unwrap());
    }
    assert_eq!(pieces.len(), 1000);
    assert!(
        pieces.iter().any(|p| !p.is_empty()),
        "decode produced no displayable pieces"
    );
}

#[test]
fn bpe_trained_on_the_repo_readme_decodes_ids() {
    let readme = std::fs::read("README.md")
        .or_else(|_| std::fs::read("../../README.md"))
        .expect("README.md is the docs-adjacent corpus");
    assert!(
        readme.len() > 64,
        "README.md is too short to be a real corpus"
    );
    let tok = BpeTokenizer::train(&readme, 320).expect("train BPE");
    assert!(tok.vocab_size() >= 256);
    let ids = tok.encode(&readme[..64]);
    assert!(!ids.is_empty());
    assert_eq!(tok.decode(&ids).unwrap(), readme[..64]);
}

#[test]
fn safetensors_file_round_trip_through_the_readout_enum() {
    let dir = std::env::temp_dir();
    let path = dir.join("aria-ws4-readout.safetensors");
    let head = DiscreteReadout::seeded(16, 256, 0.9, 3).unwrap();
    head.to_file(&path).unwrap();
    let loaded = Readout::from_file(&path).unwrap();
    let Readout::Discrete(h) = loaded else {
        panic!("expected discrete");
    };
    let z = vec![0.05; 16];
    assert_eq!(head.decode_id(&z).unwrap(), h.decode_id(&z).unwrap());
    let _ = std::fs::remove_file(path);
}
