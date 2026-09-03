//! T1R — the role law and its invariance properties (L6).
//!
//! These are the tests that make "no bias" a checkable claim rather than a
//! slogan. If any of the three invariance tests fails, the transform has
//! started reading something other than counted facts.

use aria_engine_backends::ipo::{ColumnRole, RoleThresholds};
use aria_engine_backends::structure::{analyze, canonical_row_order, distinct_values, facet_cap, is_present, Row};
use serde_json::{json, Value};

const PAIRS: usize = 4_096;

fn rows_from(v: &Value) -> Vec<Row> {
    v.as_array()
        .expect("fixture is an array")
        .iter()
        .map(|r| r.as_object().expect("row is an object").clone())
        .collect()
}

fn sheet() -> Vec<Row> {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/tabular_market_sheet.json"),
    )
    .expect("fixture must be tracked");
    rows_from(&serde_json::from_str::<Value>(&text).unwrap())
}

fn role_of(plan: &aria_engine_backends::structure::TabularPlan, column: &str) -> ColumnRole {
    plan.report
        .columns
        .iter()
        .find(|c| c.column == column)
        .unwrap_or_else(|| panic!("no stat for column '{column}'"))
        .role
}

// ---------------------------------------------------------------------------
// Presence is structural, not semantic
// ---------------------------------------------------------------------------

/// `false` and `0` are data. Only null, blank text, and empty containers are
/// absent. Treating `0` as missing would silently delete real measurements.
#[test]
fn presence_counts_false_and_zero_as_data() {
    assert!(is_present(&json!(0)));
    assert!(is_present(&json!(false)));
    assert!(is_present(&json!("x")));
    assert!(is_present(&json!([1])));
    assert!(is_present(&json!({ "a": 1 })));

    assert!(!is_present(&Value::Null));
    assert!(!is_present(&json!("")));
    assert!(!is_present(&json!("   ")));
    assert!(!is_present(&json!("\t\n")));
    assert!(!is_present(&json!([])));
    assert!(!is_present(&json!({})));
}

// ---------------------------------------------------------------------------
// The role law on the shipped sheet
// ---------------------------------------------------------------------------

#[test]
fn the_shipped_sheet_yields_the_documented_roles() {
    let plan = analyze(&sheet(), RoleThresholds::default(), PAIRS);
    assert_eq!(plan.report.n_rows, 8);

    // Unique on every row -> candidate keys. Both, not one.
    assert_eq!(role_of(&plan, "ticker"), ColumnRole::KeyAnchor);
    assert_eq!(role_of(&plan, "company"), ColumnRole::KeyAnchor);

    // Low cardinality, repeated -> shared facet nodes.
    for column in ["sector", "region", "country", "stage"] {
        assert_eq!(role_of(&plan, column), ColumnRole::Facet, "{column}");
    }

    // 7 distinct of 8 (two rows share "payments infrastructure") -> above the
    // facet cap but not unique, so it stays a plain property.
    assert_eq!(role_of(&plan, "note"), ColumnRole::FreeAttribute);
}

#[test]
fn every_key_anchor_is_reported_none_is_crowned() {
    let plan = analyze(&sheet(), RoleThresholds::default(), PAIRS);
    assert_eq!(plan.key_columns, vec!["company".to_string(), "ticker".to_string()]);
    assert_eq!(
        plan.facet_columns,
        vec![
            "country".to_string(),
            "region".to_string(),
            "sector".to_string(),
            "stage".to_string()
        ]
    );
}

/// Every role ships the counts that produced it, so a host can recompute the
/// predicate and falsify the claim. A role without its numbers would be an
/// unfalsifiable assertion.
#[test]
fn every_stat_is_self_auditing() {
    let plan = analyze(&sheet(), RoleThresholds::default(), PAIRS);
    for c in &plan.report.columns {
        assert!(!c.rule.is_empty(), "{}: empty rule", c.column);
        assert_eq!(c.n_rows, 8);
        assert!(c.present <= c.n_rows);
        assert!(c.distinct <= c.present);
        assert!(c.singletons <= c.distinct);

        let coverage = c.present as f64 / c.n_rows as f64;
        assert!((c.coverage - coverage).abs() < 1e-12, "{}", c.column);
        if c.present > 0 {
            let uniqueness = c.distinct as f64 / c.present as f64;
            assert!((c.uniqueness - uniqueness).abs() < 1e-12, "{}", c.column);
        }
    }
}

// ---------------------------------------------------------------------------
// L6 — the three invariance properties
// ---------------------------------------------------------------------------

/// Permutation invariance. Shuffling rows must change no role, no count, and
/// no dependency — and because ids follow content order, not arrival order,
/// even the row ordering comes out identical.
#[test]
fn roles_are_invariant_under_row_permutation() {
    let original = sheet();
    let mut shuffled = original.clone();
    shuffled.reverse();
    shuffled.swap(0, 3);
    shuffled.swap(1, 6);

    let a = analyze(&original, RoleThresholds::default(), PAIRS);
    let b = analyze(&shuffled, RoleThresholds::default(), PAIRS);

    assert_eq!(a.report, b.report, "row order must not reach the report");
    assert_eq!(a.key_columns, b.key_columns);
    assert_eq!(a.facet_columns, b.facet_columns);
}

/// The stronger consequence: the *canonical row order* itself is
/// permutation-invariant, which is what makes the whole envelope
/// byte-identical for a shuffled sheet.
#[test]
fn canonical_row_order_selects_the_same_rows_regardless_of_arrival() {
    let original = sheet();
    let mut shuffled = original.clone();
    shuffled.reverse();

    let order_a = canonical_row_order(&original);
    let order_b = canonical_row_order(&shuffled);

    let seq_a: Vec<&Row> = order_a.iter().map(|&i| &original[i]).collect();
    let seq_b: Vec<&Row> = order_b.iter().map(|&i| &shuffled[i]).collect();
    assert_eq!(seq_a, seq_b, "content order must not depend on arrival order");
}

/// Rename invariance. A column's label may change; its role may not. No branch
/// in the role law is allowed to read a column name.
#[test]
fn roles_are_invariant_under_column_rename() {
    let original = sheet();
    let renamed: Vec<Row> = original
        .iter()
        .map(|r| {
            let mut out = serde_json::Map::new();
            for (k, v) in r {
                let key = match k.as_str() {
                    "ticker" => "zzz_opaque_1",
                    "sector" => "aaa_opaque_2",
                    "note" => "mmm_opaque_3",
                    other => other,
                };
                out.insert(key.to_string(), v.clone());
            }
            out
        })
        .collect();

    let a = analyze(&original, RoleThresholds::default(), PAIRS);
    let b = analyze(&renamed, RoleThresholds::default(), PAIRS);

    let role_in = |plan: &aria_engine_backends::structure::TabularPlan, col: &str| {
        plan.report.columns.iter().find(|c| c.column == col).unwrap().role
    };
    assert_eq!(role_in(&a, "ticker"), role_in(&b, "zzz_opaque_1"));
    assert_eq!(role_in(&a, "sector"), role_in(&b, "aaa_opaque_2"));
    assert_eq!(role_in(&a, "note"), role_in(&b, "mmm_opaque_3"));

    // And the multiset of roles is untouched.
    let mut roles_a: Vec<ColumnRole> = a.report.columns.iter().map(|c| c.role).collect();
    let mut roles_b: Vec<ColumnRole> = b.report.columns.iter().map(|c| c.role).collect();
    roles_a.sort_by_key(|r| r.as_str());
    roles_b.sort_by_key(|r| r.as_str());
    assert_eq!(roles_a, roles_b);
}

/// Value-blindness. Replacing every cell with an opaque token, preserving only
/// the equality pattern, must produce the same roles. If a role changed, the
/// law was reading content.
#[test]
fn roles_are_invariant_under_value_substitution() {
    let original = sheet();
    let mut dictionary: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut next = 0usize;
    let opaque: Vec<Row> = original
        .iter()
        .map(|r| {
            let mut out = serde_json::Map::new();
            for (k, v) in r {
                let key = format!("{k}\u{1}{v}");
                let token = dictionary.entry(key).or_insert_with(|| {
                    next += 1;
                    format!("tok{next:08}")
                });
                out.insert(k.clone(), Value::from(token.clone()));
            }
            out
        })
        .collect();

    let a = analyze(&original, RoleThresholds::default(), PAIRS);
    let b = analyze(&opaque, RoleThresholds::default(), PAIRS);

    for (ca, cb) in a.report.columns.iter().zip(&b.report.columns) {
        assert_eq!(ca.column, cb.column);
        assert_eq!(ca.role, cb.role, "{} changed role on substitution", ca.column);
        assert_eq!(ca.distinct, cb.distinct);
        assert_eq!(ca.present, cb.present);
        assert_eq!(ca.singletons, cb.singletons);
    }
    assert_eq!(
        a.report.functional_deps.len(),
        b.report.functional_deps.len(),
        "dependency structure must survive value substitution"
    );
}

// ---------------------------------------------------------------------------
// Functional dependencies
// ---------------------------------------------------------------------------

/// `region → country` holds; `country → region` does not. A real dependency is
/// directional, and the scan must not report the converse.
#[test]
fn the_measured_dependency_is_directional() {
    let plan = analyze(&sheet(), RoleThresholds::default(), PAIRS);
    let has = |from: &str, to: &str| {
        plan.report
            .functional_deps
            .iter()
            .any(|d| d.from == from && d.to == to)
    };
    assert!(has("region", "country"), "region determines country");
    assert!(
        !has("country", "region"),
        "US maps to both us-west and us-east, so country cannot determine region"
    );
}

#[test]
fn dependencies_carry_their_support_counts() {
    let plan = analyze(&sheet(), RoleThresholds::default(), PAIRS);
    let dep = plan
        .report
        .functional_deps
        .iter()
        .find(|d| d.from == "region" && d.to == "country")
        .expect("region -> country");
    assert_eq!(dep.support, 8, "all eight rows have both cells");
    assert_eq!(dep.distinct_from, 4, "us-west, us-east, eu-benelux, eu-france");
    assert_eq!(dep.distinct_to, 3, "US, NL, FR");
}

/// A key trivially determines every other column, so reporting `ticker →
/// sector` would flood the list with restatements of "ticker is a key". Only
/// facet-to-facet hierarchy is interesting.
#[test]
fn key_anchors_are_not_reported_as_determinants() {
    let plan = analyze(&sheet(), RoleThresholds::default(), PAIRS);
    for dep in &plan.report.functional_deps {
        assert!(
            !plan.key_columns.contains(&dep.from),
            "{} is a key; its dependencies are implied, not news",
            dep.from
        );
    }
}

#[test]
fn a_broken_dependency_is_not_reported() {
    // `a` no longer determines `b`: the value "x" maps to both "1" and "2".
    let rows = rows_from(&json!([
        { "a": "x", "b": "1" },
        { "a": "x", "b": "2" },
        { "a": "y", "b": "1" },
        { "a": "z", "b": "2" }
    ]));
    let plan = analyze(&rows, RoleThresholds::default(), PAIRS);
    assert!(
        !plan
            .report
            .functional_deps
            .iter()
            .any(|d| d.from == "a" && d.to == "b"),
        "a -> b must not be reported when a witness contradicts it"
    );
}

/// The scan is bounded. When the budget runs out the report says so rather
/// than presenting a truncated list as complete.
#[test]
fn an_exhausted_dependency_budget_is_reported_honestly() {
    let plan = analyze(&sheet(), RoleThresholds::default(), 0);
    assert!(
        !plan.report.dependency_scan_complete,
        "a zero budget cannot have completed the scan"
    );
    assert!(plan.report.functional_deps.is_empty());

    let full = analyze(&sheet(), RoleThresholds::default(), PAIRS);
    assert!(full.report.dependency_scan_complete);
}

// ---------------------------------------------------------------------------
// Facet cap and role-law edge cases
// ---------------------------------------------------------------------------

/// Without the `max(2, …)` floor a small sheet admits no facet at all, because
/// `floor(0.5 · 3) = 1` and a single distinct value is a Constant. Facets are
/// where relations come from, so a 3-row sheet would produce an edgeless map.
#[test]
fn the_facet_cap_stays_reachable_on_small_sheets() {
    let t = RoleThresholds::default();
    assert_eq!(facet_cap(3, t), 2, "small sheets must still admit facets");
    assert_eq!(facet_cap(8, t), 4, "floor(0.5 * 8)");
    assert_eq!(facet_cap(0, t), 2);
    // The absolute cap dominates once the ratio would exceed it.
    assert_eq!(facet_cap(10_000, t), t.facet_max_distinct);
}

/// A single-valued column must be Constant, never a key — even when it is the
/// only present row, where `distinct == present` would otherwise match the key
/// predicate. This is why the clause order in the role law is load-bearing.
#[test]
fn a_constant_column_is_never_mistaken_for_a_key() {
    let rows = rows_from(&json!([
        { "k": "only", "other": 1 },
        { "k": "only", "other": 2 }
    ]));
    let plan = analyze(&rows, RoleThresholds::default(), PAIRS);
    assert_eq!(role_of(&plan, "k"), ColumnRole::Constant);
    assert!(
        !plan.key_columns.contains(&"k".to_string()),
        "a constant must never be offered as an identity"
    );
    // `other` holds 1 and 2 over two rows, so it *is* a candidate key here —
    // the law counts, it does not consult the column's name or plausibility.
    assert_eq!(role_of(&plan, "other"), ColumnRole::KeyAnchor);

    let single = rows_from(&json!([{ "k": "only" }]));
    let plan = analyze(&single, RoleThresholds::default(), PAIRS);
    assert_eq!(
        role_of(&plan, "k"),
        ColumnRole::Constant,
        "one row, one value: still a constant, not an identity"
    );
}

#[test]
fn an_all_null_column_is_empty_not_constant() {
    let rows = rows_from(&json!([
        { "a": 1, "blank": null },
        { "a": 2, "blank": "  " }
    ]));
    let plan = analyze(&rows, RoleThresholds::default(), PAIRS);
    assert_eq!(role_of(&plan, "blank"), ColumnRole::Empty);
    let stat = plan
        .report
        .columns
        .iter()
        .find(|c| c.column == "blank")
        .unwrap();
    assert_eq!(stat.present, 0);
    assert_eq!(stat.distinct, 0);
    assert!((stat.uniqueness - 0.0).abs() < 1e-15, "no division by zero");
}

/// Unique wherever present, but too sparse to identify rows. Calling this a
/// key would let a mostly-empty column define row identity.
#[test]
fn a_sparse_unique_column_is_not_a_key() {
    let rows = rows_from(&json!([
        { "id": "a", "sparse": "p" },
        { "id": "b", "sparse": null },
        { "id": "c", "sparse": null },
        { "id": "d", "sparse": "q" }
    ]));
    let plan = analyze(&rows, RoleThresholds::default(), PAIRS);
    assert_eq!(role_of(&plan, "id"), ColumnRole::KeyAnchor);
    assert_eq!(
        role_of(&plan, "sparse"),
        ColumnRole::FreeAttribute,
        "50% coverage is below the near-key threshold"
    );
}

#[test]
fn a_near_key_column_is_recognized_above_the_coverage_threshold() {
    // 19 of 20 present, all distinct -> near key at the default 0.90.
    let mut rows = Vec::new();
    for i in 0..20 {
        let mut row = serde_json::Map::new();
        row.insert("seq".into(), Value::from(i));
        if i != 7 {
            row.insert("almost".into(), Value::from(format!("u{i}")));
        }
        rows.push(row);
    }
    let plan = analyze(&rows, RoleThresholds::default(), PAIRS);
    assert_eq!(role_of(&plan, "almost"), ColumnRole::NearKeyAnchor);
    assert!(plan.key_columns.contains(&"almost".to_string()));
}

/// A ragged sheet is not an error: a missing key is simply an absent cell.
/// Rejecting it would make Aria stricter than the spreadsheets it exists to read.
#[test]
fn a_ragged_sheet_is_read_as_sparse_columns() {
    let rows = rows_from(&json!([
        { "a": 1 },
        { "a": 2, "b": "x" },
        { "c": true }
    ]));
    let plan = analyze(&rows, RoleThresholds::default(), PAIRS);
    let columns: Vec<&str> = plan.report.columns.iter().map(|c| c.column.as_str()).collect();
    assert_eq!(columns, vec!["a", "b", "c"], "union of keys, ascending");
    assert_eq!(plan.report.n_rows, 3);
}

#[test]
fn an_empty_sheet_measures_nothing_and_does_not_panic() {
    let plan = analyze(&[], RoleThresholds::default(), PAIRS);
    assert_eq!(plan.report.n_rows, 0);
    assert!(plan.report.columns.is_empty());
    assert!(plan.report.functional_deps.is_empty());
    assert!(plan.canonical_rows.is_empty());
}

// ---------------------------------------------------------------------------
// Distinct-value enumeration (fixes facet ids)
// ---------------------------------------------------------------------------

#[test]
fn distinct_values_are_deduplicated_and_canonically_ordered() {
    let rows = rows_from(&json!([
        { "s": "b" }, { "s": "a" }, { "s": "b" }, { "s": null }, { "s": "c" }
    ]));
    let values = distinct_values(&rows, "s");
    assert_eq!(values, vec![json!("a"), json!("b"), json!("c")]);
}

#[test]
fn distinct_values_are_order_independent() {
    let a = rows_from(&json!([{ "s": "x" }, { "s": "y" }, { "s": "z" }]));
    let b = rows_from(&json!([{ "s": "z" }, { "s": "x" }, { "s": "y" }]));
    assert_eq!(distinct_values(&a, "s"), distinct_values(&b, "s"));
}

/// Numeric and string cells that look alike must not collide: `1` and `"1"`
/// are different values, and canonical JSON bytes keep them apart.
#[test]
fn lookalike_scalars_are_counted_separately() {
    let rows = rows_from(&json!([
        { "v": 1 }, { "v": "1" }, { "v": true }, { "v": "true" }
    ]));
    let plan = analyze(&rows, RoleThresholds::default(), PAIRS);
    let stat = plan.report.columns.iter().find(|c| c.column == "v").unwrap();
    assert_eq!(stat.distinct, 4, "1, \"1\", true, \"true\" are four values");
}
