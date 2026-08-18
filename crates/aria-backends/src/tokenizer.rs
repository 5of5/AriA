//! Deterministic BPE tokenizer — the discrete vocabulary beside a readout.
//!
//! HuggingFace `tokenizers` is pinned in the workspace but is **not** used
//! here: its default `onig` backend is C and does not compile for
//! `wasm32-unknown-unknown` (the same wall that rejected instant-distance /
//! hnsw_rs in WS3). A byte-level BPE is small, seeded-free (training is a
//! pure function of the corpus), and identical on every surface.
//!
//! `|V_o| = 256` is the identity (raw bytes) — the spec minimum. Larger
//! vocabularies add greedy pair-merges, ties broken by `(left, right)` so
//! two trainings on the same bytes produce the same merge table.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::readout::{ReadoutError, VOCAB_MAX, VOCAB_MIN};

/// On-disk tag.
pub const TOKENIZER_FORMAT: &str = "aria-tokenizer-v1";

/// Byte-level BPE.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BpeTokenizer {
    format: String,
    vocab_size: usize,
    /// `merges[i]` = `(left, right)` that produced id `256 + i`.
    merges: Vec<(u32, u32)>,
}

impl BpeTokenizer {
    /// Spec-minimum identity tokenizer: id `i` ↔ byte `i`.
    pub fn bytes() -> Self {
        Self {
            format: TOKENIZER_FORMAT.into(),
            vocab_size: VOCAB_MIN,
            merges: Vec::new(),
        }
    }

    /// Train BPE on `corpus` up to `vocab_size` (in `[256, 128000]`).
    ///
    /// Pair selection is totally ordered: highest count, then smallest
    /// `(left, right)`. Replacement is left-to-right, non-overlapping.
    pub fn train(corpus: &[u8], vocab_size: usize) -> Result<Self, ReadoutError> {
        if !(VOCAB_MIN..=VOCAB_MAX).contains(&vocab_size) {
            return Err(ReadoutError::Invalid(format!(
                "vocab_size {vocab_size} is outside the spec domain [{VOCAB_MIN}, {VOCAB_MAX}]"
            )));
        }
        if corpus.is_empty() && vocab_size > VOCAB_MIN {
            return Err(ReadoutError::Invalid(
                "cannot train merges on an empty corpus".into(),
            ));
        }

        let mut seq: Vec<u32> = corpus.iter().copied().map(u32::from).collect();
        let mut merges = Vec::with_capacity(vocab_size - VOCAB_MIN);

        while VOCAB_MIN + merges.len() < vocab_size {
            let mut counts: HashMap<(u32, u32), usize> = HashMap::new();
            for w in seq.windows(2) {
                *counts.entry((w[0], w[1])).or_insert(0) += 1;
            }
            let Some((&pair, _)) = counts
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| a.0.cmp(b.0).reverse()))
            else {
                break; // corpus too short to produce another pair
            };
            let new_id = u32::try_from(VOCAB_MIN + merges.len())
                .map_err(|_| ReadoutError::Invalid("vocab exceeds u32".into()))?;
            seq = apply_merge(&seq, pair, new_id);
            merges.push(pair);
        }

        Ok(Self {
            format: TOKENIZER_FORMAT.into(),
            vocab_size: VOCAB_MIN + merges.len(),
            merges,
        })
    }

    pub fn vocab_size(&self) -> usize {
        self.vocab_size
    }

    /// Encode bytes with the trained merges (greedy, left-to-right).
    pub fn encode(&self, text: &[u8]) -> Vec<u32> {
        let mut seq: Vec<u32> = text.iter().copied().map(u32::from).collect();
        for (i, &pair) in self.merges.iter().enumerate() {
            let new_id = u32::try_from(VOCAB_MIN + i).expect("merge index fits u32");
            seq = apply_merge(&seq, pair, new_id);
        }
        seq
    }

    /// Decode a single id to its byte string (UTF-8 lossy for display).
    pub fn decode_one(&self, id: u32) -> Result<String, ReadoutError> {
        let bytes = self.piece(id)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Decode a sequence of ids to bytes.
    pub fn decode(&self, ids: &[u32]) -> Result<Vec<u8>, ReadoutError> {
        let mut out = Vec::new();
        for &id in ids {
            out.extend(self.piece(id)?);
        }
        Ok(out)
    }

    fn piece(&self, id: u32) -> Result<Vec<u8>, ReadoutError> {
        let idx = id as usize;
        if idx < VOCAB_MIN {
            return Ok(vec![u8::try_from(id).expect("id < 256")]);
        }
        let merge_i = idx - VOCAB_MIN;
        if merge_i >= self.merges.len() {
            return Err(ReadoutError::Invalid(format!(
                "token id {id} is outside vocab {}",
                self.vocab_size
            )));
        }
        // Recurse through the merge table; depth is |merges| in the worst
        // case and |V_o| ≤ 128000, so a heap stack is the honest bound.
        let (a, b) = self.merges[merge_i];
        let mut out = self.piece(a)?;
        out.extend(self.piece(b)?);
        Ok(out)
    }

    pub fn to_json(&self) -> Result<String, ReadoutError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| ReadoutError::Invalid(format!("tokenizer json: {e}")))
    }

    pub fn from_json(src: &str) -> Result<Self, ReadoutError> {
        let t: Self = serde_json::from_str(src)
            .map_err(|e| ReadoutError::Invalid(format!("tokenizer json: {e}")))?;
        if t.format != TOKENIZER_FORMAT {
            return Err(ReadoutError::Invalid(format!(
                "unsupported tokenizer format '{}' (expected '{TOKENIZER_FORMAT}')",
                t.format
            )));
        }
        if !(VOCAB_MIN..=VOCAB_MAX).contains(&t.vocab_size) {
            return Err(ReadoutError::Invalid(format!(
                "tokenizer vocab_size {} is outside [{VOCAB_MIN}, {VOCAB_MAX}]",
                t.vocab_size
            )));
        }
        if t.vocab_size != VOCAB_MIN + t.merges.len() {
            return Err(ReadoutError::Invalid(
                "tokenizer vocab_size does not match merge count".into(),
            ));
        }
        Ok(t)
    }

    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<(), ReadoutError> {
        std::fs::write(path, self.to_json()?)?;
        Ok(())
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ReadoutError> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }
}

fn apply_merge(seq: &[u32], pair: (u32, u32), new_id: u32) -> Vec<u32> {
    let mut out = Vec::with_capacity(seq.len());
    let mut i = 0;
    while i < seq.len() {
        if i + 1 < seq.len() && seq[i] == pair.0 && seq[i + 1] == pair.1 {
            out.push(new_id);
            i += 2;
        } else {
            out.push(seq[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_tokenizer_is_identity() {
        let t = BpeTokenizer::bytes();
        let text = b"Aria \x00\xff";
        let ids = t.encode(text);
        assert_eq!(ids, text.iter().map(|&b| u32::from(b)).collect::<Vec<_>>());
        assert_eq!(t.decode(&ids).unwrap(), text);
    }

    #[test]
    fn train_is_deterministic_and_round_trips() {
        let corpus = b"the cat sat on the mat the cat sat";
        let a = BpeTokenizer::train(corpus, 280).unwrap();
        let b = BpeTokenizer::train(corpus, 280).unwrap();
        assert_eq!(a, b);
        assert!(a.vocab_size() > VOCAB_MIN);
        assert!(a.vocab_size() <= 280);
        let ids = a.encode(corpus);
        assert_eq!(a.decode(&ids).unwrap(), corpus);
        let json = a.to_json().unwrap();
        let c = BpeTokenizer::from_json(&json).unwrap();
        assert_eq!(a, c);
    }

    #[test]
    fn train_rejects_out_of_domain_vocab() {
        assert!(BpeTokenizer::train(b"abc", 255).is_err());
        assert!(BpeTokenizer::train(b"abc", 128_001).is_err());
        assert!(BpeTokenizer::train(b"", 300).is_err());
        assert!(BpeTokenizer::train(b"", 256).is_ok());
    }
}
