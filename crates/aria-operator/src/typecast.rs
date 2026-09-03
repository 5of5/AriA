//! Type-cast: free text → closed-vocabulary TAG tokens.
//!
//! Listed DEEP_TAG slugs only. No LLM. No new nodes. Φ is not involved.

use aria_engine_backends::ipo::NodeRecord;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use crate::catalog;

/// Designated property keys scanned for cast phrases (00c fields).
pub(crate) const CAST_PROPS: &[&str] = &[
    "title",
    "role",
    "function",
    "industry",
    "sector",
    "category",
    "persona",
    "partner_role",
    "axis_x",
    "axis_y",
    "type",
    "kind",
];

/// Single-value fields that may emit `uncast_token` when they match nothing.
/// `type` / `kind` are scan-only (they name graph kinds, not vocabulary gaps).
pub(crate) const UNCAST_PROPS: &[&str] = &[
    "title",
    "role",
    "function",
    "industry",
    "sector",
    "category",
    "persona",
    "partner_role",
    "axis_x",
    "axis_y",
];

const PREFIXES: &[&str] = &[
    "PERSONA_", "PERSON_", "CO_", "IND_", "CAT_", "ECO_", "LANG_",
];

/// Phrase (lowercase, spaces) → catalog tags in catalog order.
pub fn cast_lexicon() -> &'static BTreeMap<String, Vec<String>> {
    static LEX: OnceLock<BTreeMap<String, Vec<String>>> = OnceLock::new();
    LEX.get_or_init(build_lexicon)
}

fn build_lexicon() -> BTreeMap<String, Vec<String>> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for spec in catalog() {
        if spec.layer != "DEEP_TAG" || spec.taxonomy.is_none() {
            continue;
        }
        let Some(tag) = spec.anchor_tags.first() else { continue };
        let phrase = phrase_from_tag(tag);
        if phrase.chars().count() < 3 {
            continue;
        }
        for key in inflections(&phrase) {
            let entry = map.entry(key).or_default();
            if !entry.iter().any(|t| t == tag) {
                entry.push(tag.clone());
            }
        }
    }
    map
}

fn inflections(phrase: &str) -> Vec<String> {
    let mut out = vec![phrase.to_string()];
    if phrase.contains(' ') {
        return out;
    }
    out.push(format!("{phrase}s"));
    if let Some(stem) = phrase.strip_suffix("er") {
        if stem.chars().count() >= 3 {
            out.push(format!("{stem}ed"));
        }
    }
    out
}

/// Catalog tag → lowercase phrase the lexicon matches (`PERSON_FOUNDER` → `founder`).
#[must_use]
pub fn tag_phrase(tag: &str) -> String {
    phrase_from_tag(tag)
}

pub(crate) fn phrase_from_tag(tag: &str) -> String {
    let mut rest = tag;
    let mut prefixes: Vec<&str> = PREFIXES.to_vec();
    prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for p in prefixes {
        if let Some(r) = rest.strip_prefix(p) {
            rest = r;
            break;
        }
    }
    rest.replace('_', " ").to_ascii_lowercase()
}

pub(crate) fn tokenize(s: &str) -> Vec<String> {
    s.to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Longest phrase (in tokens) in the lexicon — bounds the n-gram window.
fn max_phrase_len() -> usize {
    static LEN: OnceLock<usize> = OnceLock::new();
    *LEN.get_or_init(|| {
        cast_lexicon()
            .keys()
            .map(|p| p.split_whitespace().count())
            .max()
            .unwrap_or(1)
    })
}

/// Single left-to-right pass: every n-gram (n ≤ longest phrase) is one hash
/// lookup, so a record costs O(tokens · L) instead of O(|lexicon| · tokens).
/// Returns matched phrases sorted (same order the lexicon iterates in).
fn matched_phrases<'l>(texts: &[Vec<String>], lex: &'l BTreeMap<String, Vec<String>>) -> BTreeSet<&'l str> {
    let l = max_phrase_len();
    let mut hits: BTreeSet<&'l str> = BTreeSet::new();
    let mut gram = String::new();
    for toks in texts {
        for i in 0..toks.len() {
            gram.clear();
            for n in 0..l.min(toks.len() - i) {
                if n > 0 {
                    gram.push(' ');
                }
                gram.push_str(&toks[i + n]);
                if let Some((k, _)) = lex.get_key_value(gram.as_str()) {
                    hits.insert(k.as_str());
                }
            }
        }
    }
    hits
}

fn field_texts(rec: &NodeRecord) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(l) = rec.label.as_deref() {
        if !l.is_empty() {
            out.push(l.to_string());
        }
    }
    if let Some(n) = rec.notes.as_deref() {
        if !n.is_empty() {
            out.push(n.to_string());
        }
    }
    for key in CAST_PROPS {
        if let Some(Value::String(s)) = rec.properties.get(*key) {
            if !s.is_empty() {
                out.push(s.clone());
            }
        }
    }
    out
}

/// Closed-vocabulary tags found in this record's 00c fields. Catalog order.
#[must_use]
pub fn cast_tags(rec: &NodeRecord) -> Vec<String> {
    let texts: Vec<Vec<String>> = field_texts(rec).iter().map(|t| tokenize(t)).collect();
    if texts.iter().all(Vec::is_empty) {
        return Vec::new();
    }
    let lex = cast_lexicon();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for phrase in matched_phrases(&texts, lex) {
        for tag in &lex[phrase] {
            if seen.insert(tag.clone()) {
                out.push(tag.clone());
            }
        }
    }
    out
}

pub(crate) fn casts_in_value(value: &str) -> bool {
    let toks = tokenize(value);
    !toks.is_empty() && !matched_phrases(&[toks], cast_lexicon()).is_empty()
}

/// Designated single-value fields whose text produced zero lexicon hits.
#[must_use]
pub fn uncast_fields(rec: &NodeRecord) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for key in UNCAST_PROPS {
        let Some(Value::String(s)) = rec.properties.get(*key) else { continue };
        if s.trim().is_empty() {
            continue;
        }
        if !casts_in_value(s) {
            out.push(((*key).to_string(), s.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference scanner (the historical O(|lexicon| · windows) form).
    fn reference_phrases(texts: &[Vec<String>]) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for phrase in cast_lexicon().keys() {
            let p: Vec<&str> = phrase.split_whitespace().collect();
            let hit = texts.iter().any(|toks| {
                toks.len() >= p.len()
                    && toks
                        .windows(p.len())
                        .any(|w| w.iter().map(String::as_str).eq(p.iter().copied()))
            });
            if hit {
                out.insert(phrase.clone());
            }
        }
        out
    }

    #[test]
    fn ngram_scan_equals_reference_scanner() {
        let samples = [
            "Ada founded Acme; she is a founder and operator in ai infrastructure",
            "payments infrastructure in fintech — analyst firm coverage",
            "qwerty asdf garbage dump 🍕 not a person, not a company",
            "chief executive officer; ceo; co founder; angel investor",
            "",
            "adjacent adjacent adjacent core bridge band",
        ];
        for s in samples {
            let texts = vec![tokenize(s)];
            let fast: BTreeSet<String> = matched_phrases(&texts, cast_lexicon())
                .into_iter()
                .map(str::to_string)
                .collect();
            assert_eq!(fast, reference_phrases(&texts), "sample: {s}");
        }
        // Every lexicon phrase must find itself (n-gram window is wide enough).
        for phrase in cast_lexicon().keys() {
            let texts = vec![tokenize(phrase)];
            assert!(
                matched_phrases(&texts, cast_lexicon()).contains(phrase.as_str()),
                "phrase not self-matching: {phrase}"
            );
        }
    }
}
