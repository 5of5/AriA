//! T1R — deterministic structure inference from a literal spreadsheet.
//!
//! This is the differentiator, and its whole claim is one sentence:
//! **structure is derived from counted facts, never from names, values, or a
//! model.**
//!
//! # The role law
//!
//! For each column `c` over `n` rows the pass measures exactly four numbers —
//! `present`, `distinct`, `singletons`, and their two ratios — and the role
//! follows from those numbers alone:
//!
//! | Role | Condition |
//! |---|---|
//! | [`ColumnRole::Empty`] | `present == 0` |
//! | [`ColumnRole::Constant`] | `distinct == 1` |
//! | [`ColumnRole::KeyAnchor`] | `coverage == 1 ∧ distinct == present` |
//! | [`ColumnRole::NearKeyAnchor`] | `coverage ≥ κ ∧ distinct == present` |
//! | [`ColumnRole::Facet`] | `1 < distinct ≤ facet_cap` |
//! | [`ColumnRole::FreeAttribute`] | otherwise |
//!
//! The clause order matters: `Constant` is tested before uniqueness, because a
//! single-valued column over a single present row would otherwise satisfy
//! `distinct == present` and be mistaken for a key.
//!
//! # Three invariance properties that *are* the absence of bias
//!
//! 1. **Permutation invariance** — shuffling rows changes no role, and (because
//!    ids are assigned in content order, not arrival order) does not even
//!    change the emitted ids. A shuffled sheet produces a byte-identical
//!    envelope.
//! 2. **Rename invariance** — renaming a column changes its label and nothing
//!    else. No branch in this module reads a column name to decide a role.
//! 3. **Value-blindness** — only cardinality and dependency structure select a
//!    role. The lexical content of a cell never does.
//!
//! # What is deliberately *not* inferred
//!
//! Dates, currencies, units, entity kinds, sentiment, and any name-based
//! semantics. Parsing `"2026-01-01"` as a date would import a locale
//! assumption and make the transform value-sighted. Temporal ordering is
//! available only when a host declares it explicitly.
//!
//! # Row identity (resolves Q-2026-08-31-1)
//!
//! A sheet can have several key anchors — in the shipped fixture both `ticker`
//! and `company` are unique. Picking one by name would make identity
//! rename-sensitive, so this pass picks *none*: a row's identity is the anchor
//! of the whole row, and **every** key-anchor column is reported so the host
//! can choose its own business key. Ids are then assigned in ascending
//! `(row anchor, arrival index)` order, which is what buys permutation
//! invariance.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::ipo::{
    anchor_of, canonical_json, ColumnRole, ColumnStat, FunctionalDep, RoleThresholds,
    StructureReport,
};

/// One row of a tabular payload.
pub type Row = Map<String, Value>;

/// The structural plan a tabular payload implies, plus the measurements that
/// produced it. Ingest turns this into `G₀`; nothing here touches Φ.
#[derive(Debug, Clone, PartialEq)]
pub struct TabularPlan {
    /// Every measurement, ready to embed in the envelope.
    pub report: StructureReport,
    /// Columns whose values uniquely identify a row. Reported, never reduced
    /// to a single winner — see the module note on Q-2026-08-31-1.
    pub key_columns: Vec<String>,
    /// Columns that become shared facet nodes. This is where relations come
    /// from: two rows sharing a facet value become connected through it.
    pub facet_columns: Vec<String>,
    /// Row order after canonical sorting: `canonical_rows[i]` is the arrival
    /// index of the row that receives id `i`.
    pub canonical_rows: Vec<usize>,
}

/// Whether a cell counts as present.
///
/// Absence is a structural fact, not a semantic one: null, an all-whitespace
/// string, an empty array, and an empty object are all "no value here". Note
/// that `false` and `0` are *present* — they are data.
pub fn is_present(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::String(s) => !s.trim().is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::Bool(_) | Value::Number(_) => true,
    }
}

/// The present value at `column`, or `None`.
fn cell<'a>(row: &'a Row, column: &str) -> Option<&'a Value> {
    row.get(column).filter(|v| is_present(v))
}

/// Canonical byte key for value counting. Using canonical JSON bytes rather
/// than `to_string` means `1` and `1.0` compare as serde sees them, and no
/// UTF-8 assumption is imposed on string cells.
fn value_key(v: &Value) -> Vec<u8> {
    canonical_json(v)
}

/// The facet cardinality ceiling for `n` rows.
///
/// Two guards, both necessary. The ratio stops a near-unique column from
/// exploding into thousands of facet nodes; the absolute cap stops a large
/// sheet from doing the same. The `max(2, …)` floor keeps facets reachable on
/// small sheets — without it a 3-row sheet admits no facet at all, since
/// `floor(0.5 · 3) = 1` and a single distinct value is a `Constant`.
pub fn facet_cap(n_rows: usize, thresholds: RoleThresholds) -> usize {
    let ceiling = thresholds.facet_max_distinct.max(2);
    let target = thresholds.facet_max_ratio.clamp(0.0, 1.0) * n_rows as f64;
    if !target.is_finite() {
        return 2;
    }
    // Largest `k ≤ ceiling` with `k ≤ target`, found by a bounded integer walk
    // rather than a float→integer cast. The absolute cap makes this at most
    // `facet_max_distinct` iterations, and only int→float conversions appear —
    // so there is no truncation or sign behaviour left to argue about.
    let mut cap = 2usize;
    for k in 3..=ceiling {
        if k as f64 <= target {
            cap = k;
        } else {
            break;
        }
    }
    cap.min(ceiling)
}

/// Per-column tallies. Nothing here looks at the column's name.
struct Tally {
    present: usize,
    distinct: usize,
    singletons: usize,
}

fn tally(rows: &[Row], column: &str) -> Tally {
    let mut counts: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
    let mut present = 0usize;
    for row in rows {
        if let Some(v) = cell(row, column) {
            present += 1;
            *counts.entry(value_key(v)).or_insert(0) += 1;
        }
    }
    Tally {
        present,
        distinct: counts.len(),
        singletons: counts.values().filter(|&&c| c == 1).count(),
    }
}

/// The outcome of the role law for one column.
struct Assigned {
    role: ColumnRole,
    /// The predicate text that fired. Part of the contract: it is what makes
    /// the decision falsifiable by a host that recomputes the same counts.
    predicate: String,
    coverage: f64,
    uniqueness: f64,
}

/// Assign a role from counts alone.
fn assign_role(t: &Tally, n_rows: usize, thresholds: RoleThresholds) -> Assigned {
    let coverage = if n_rows == 0 {
        0.0
    } else {
        t.present as f64 / n_rows as f64
    };
    let uniqueness = if t.present == 0 {
        0.0
    } else {
        t.distinct as f64 / t.present as f64
    };
    let cap = facet_cap(n_rows, thresholds);
    let done = |role: ColumnRole, predicate: String| Assigned {
        role,
        predicate,
        coverage,
        uniqueness,
    };

    // Order is load-bearing: Constant must precede the uniqueness clauses.
    if t.present == 0 {
        return done(ColumnRole::Empty, "present == 0".into());
    }
    if t.distinct == 1 {
        return done(ColumnRole::Constant, "distinct == 1".into());
    }
    if t.distinct == t.present {
        if t.present == n_rows {
            return done(
                ColumnRole::KeyAnchor,
                format!(
                    "coverage == 1 && distinct == present ({} == {})",
                    t.distinct, t.present
                ),
            );
        }
        if coverage >= thresholds.near_key_coverage {
            return done(
                ColumnRole::NearKeyAnchor,
                format!(
                    "coverage {coverage:.4} >= {:.4} && distinct == present ({} == {})",
                    thresholds.near_key_coverage, t.distinct, t.present
                ),
            );
        }
        // Unique wherever present, but too sparse to identify rows.
        return done(
            ColumnRole::FreeAttribute,
            format!(
                "distinct == present but coverage {coverage:.4} < {:.4}",
                thresholds.near_key_coverage
            ),
        );
    }
    if t.distinct <= cap {
        return done(
            ColumnRole::Facet,
            format!("1 < distinct {} <= facet_cap {cap}", t.distinct),
        );
    }
    done(
        ColumnRole::FreeAttribute,
        format!("distinct {} > facet_cap {cap}", t.distinct),
    )
}

/// Test `a → b`: does every distinct value of `a` map to exactly one value of
/// `b`, over the rows where both are present?
///
/// Returns `(support, distinct_from, distinct_to)` when the dependency holds.
fn functional_dep(rows: &[Row], a: &str, b: &str) -> Option<(usize, usize, usize)> {
    let mut witness: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
    let mut b_values: BTreeSet<Vec<u8>> = BTreeSet::new();
    let mut support = 0usize;

    for row in rows {
        let (Some(av), Some(bv)) = (cell(row, a), cell(row, b)) else {
            continue;
        };
        support += 1;
        let bk = value_key(bv);
        b_values.insert(bk.clone());
        match witness.entry(value_key(av)) {
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(bk);
            }
            std::collections::btree_map::Entry::Occupied(e) => {
                if *e.get() != bk {
                    // One `a` value with two `b` values: not a dependency.
                    return None;
                }
            }
        }
    }
    if support == 0 {
        return None;
    }
    Some((support, witness.len(), b_values.len()))
}

/// Measure a tabular payload and derive its structural plan.
///
/// Complexity: `O(n · |C|)` for the tallies plus `O(n · P)` for the dependency
/// scan, where `P ≤ max_dependency_pairs` is the number of candidate column
/// pairs actually tested. Both terms are bounded before any work begins, which
/// is what makes an untrusted sheet safe to accept (L8).
pub fn analyze(rows: &[Row], thresholds: RoleThresholds, max_dependency_pairs: usize) -> TabularPlan {
    let n_rows = rows.len();

    // Ascending column order: the union of every row's keys. A ragged sheet is
    // not an error — a missing key is simply an absent cell.
    let columns: BTreeSet<&str> = rows.iter().flat_map(|r| r.keys().map(String::as_str)).collect();

    let mut stats = Vec::with_capacity(columns.len());
    let mut key_columns = Vec::new();
    let mut facet_columns = Vec::new();

    for column in &columns {
        let t = tally(rows, column);
        let Assigned {
            role,
            predicate,
            coverage,
            uniqueness,
        } = assign_role(&t, n_rows, thresholds);
        match role {
            ColumnRole::KeyAnchor | ColumnRole::NearKeyAnchor => {
                key_columns.push((*column).to_string());
            }
            ColumnRole::Facet => facet_columns.push((*column).to_string()),
            _ => {}
        }
        stats.push(ColumnStat {
            column: (*column).to_string(),
            role,
            rule: predicate,
            n_rows,
            present: t.present,
            distinct: t.distinct,
            coverage,
            uniqueness,
            singletons: t.singletons,
        });
    }

    let (functional_deps, dependency_scan_complete) =
        scan_dependencies(rows, &stats, &facet_columns, max_dependency_pairs);

    TabularPlan {
        report: StructureReport {
            n_rows,
            columns: stats,
            functional_deps,
            thresholds,
            dependency_scan_complete,
        },
        key_columns,
        facet_columns,
        canonical_rows: canonical_row_order(rows),
    }
}

/// Bounded dependency scan over facet columns.
///
/// Key anchors are excluded as determinants on purpose: a key determines every
/// other column by definition, so reporting `ticker → sector` would flood the
/// list with consequences of a fact the report already states (that `ticker`
/// is a `key_anchor`). The interesting hierarchy is facet-to-facet — the
/// `region → country` shape.
///
/// Only pairs with `distinct(a) ≥ distinct(b)` are tested, because `a → b`
/// forces `distinct(b) ≤ distinct(a)`: a determinant partitions the rows at
/// least as finely as what it determines.
fn scan_dependencies(
    rows: &[Row],
    stats: &[ColumnStat],
    facet_columns: &[String],
    max_pairs: usize,
) -> (Vec<FunctionalDep>, bool) {
    let distinct_of: BTreeMap<&str, usize> = stats
        .iter()
        .map(|s| (s.column.as_str(), s.distinct))
        .collect();

    let mut deps = Vec::new();
    let mut tested = 0usize;
    let mut complete = true;

    'outer: for a in facet_columns {
        for b in facet_columns {
            if a == b {
                continue;
            }
            let (Some(&da), Some(&db)) = (distinct_of.get(a.as_str()), distinct_of.get(b.as_str()))
            else {
                continue;
            };
            if da < db {
                continue;
            }
            if tested >= max_pairs {
                complete = false;
                break 'outer;
            }
            tested += 1;
            if let Some((support, distinct_from, distinct_to)) = functional_dep(rows, a, b) {
                deps.push(FunctionalDep {
                    from: a.clone(),
                    to: b.clone(),
                    distinct_from,
                    distinct_to,
                    support,
                });
            }
        }
    }
    // `facet_columns` is already ascending, so `deps` is ascending in
    // `(from, to)` without an explicit sort.
    (deps, complete)
}

/// Arrival indices sorted by `(row anchor, arrival index)`.
///
/// Content-first ordering is what makes the envelope permutation-invariant:
/// two payloads differing only in row order produce the same ids and therefore
/// the same bytes. The arrival index is the tiebreak for byte-identical
/// duplicate rows, which keeps the sort total.
pub fn canonical_row_order(rows: &[Row]) -> Vec<usize> {
    let mut keyed: Vec<(String, usize)> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (anchor_of(&Value::Object(r.clone())), i))
        .collect();
    keyed.sort_unstable();
    keyed.into_iter().map(|(_, i)| i).collect()
}

/// Distinct present values of `column`, ascending by canonical bytes.
///
/// Facet node creation consumes this, so its order fixes facet ids.
pub fn distinct_values(rows: &[Row], column: &str) -> Vec<Value> {
    let mut seen: BTreeMap<Vec<u8>, Value> = BTreeMap::new();
    for row in rows {
        if let Some(v) = cell(row, column) {
            seen.entry(value_key(v)).or_insert_with(|| v.clone());
        }
    }
    seen.into_values().collect()
}

/// The values of `column` in `row`, if present. Re-exported for ingest.
pub fn present_cell<'a>(row: &'a Row, column: &str) -> Option<&'a Value> {
    cell(row, column)
}
