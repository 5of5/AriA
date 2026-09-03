//! Local dump: garbage-collect every catalog binary and score the kit.
//!
//! ```bash
//! cargo run -p aria-json-telemetry --example dump -- [dump-dir] [--obsidian <vault>]
//! ```
//!
//! Default dump-dir is `dump/output_{YYMMDD_HHMM}` (UTC). With `--obsidian`,
//! SCORE.md / found.md / forgot.md / original.md / graph.md + entities/
//! are copied into `<vault>/Aria-Telemetry/<dir-name>` (human verification
//! copy, not Trust). `entities/` serializes the graph for vault Graph
//! View: one note per node, `[[wikilink]]` per relation, `#aria/{kind}` tags.

use aria_engine_backends::ipo::sha256_hex;
use aria_operator::{
    callback_results, catalog, execute_work, run_many, token_stat, OperatorEnvelope, RunOpts,
    WorkRequest, WORK_V1,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Identify/filter dump path: ingest only. Projectors tag the ingested graph;
/// Match is unused (plan-3 E6). Scale still exercises Φ at `scale_opts`.
fn dump_opts() -> RunOpts {
    RunOpts {
        steps: 0,
        seed: Some(1),
        n_modes: Some(16),
        latent_dim: Some(16),
        allow_sub_spec_dims: true,
        include_telemetry: false,
        ..RunOpts::default()
    }
}

fn scale_opts() -> RunOpts {
    RunOpts {
        steps: 8,
        seed: Some(1),
        n_modes: Some(16),
        latent_dim: Some(16),
        allow_sub_spec_dims: true,
        include_telemetry: false,
        ..RunOpts::default()
    }
}

fn all_ids() -> Vec<String> {
    catalog().iter().map(|s| s.binary_id.clone()).collect()
}

/// `YYMMDD_HHMM` in UTC — calendar math after Hinnant, so no date crate.
fn ts_label() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = i64::try_from(secs / 86_400).unwrap_or_default();
    let tod = secs % 86_400;
    let z = days + 719_468; // > 0 for any wall clock past 1970-03-01
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = u64::try_from(z - era * 146_097).unwrap_or_default();
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = i64::try_from(yoe).unwrap_or_default() + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:02}{:02}{:02}_{:02}{:02}", y % 100, m, d, tod / 3600, (tod / 60) % 60)
}

fn parse_args() -> (Option<String>, Option<PathBuf>) {
    let mut dir = None;
    let mut obsidian = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--obsidian" {
            obsidian = args.next().map(PathBuf::from);
        } else {
            dir = Some(a);
        }
    }
    (dir, obsidian)
}

/// Human-review numbers for SCORE.md — every value comes from the report.
#[allow(clippy::too_many_lines)] // flat table writer
fn score_md(report: &Value, ts: &str) -> String {
    let inv = &report["invariants"];
    let sc = &report["scores"];
    let gate = &report["stress_gate"];
    let mixed = &report["mixed_gate"];
    let company = &report["company_gate"];
    let mut s = String::new();
    let _ = writeln!(s, "# SCORE — {ts}");
    let _ = writeln!(
        s,
        "\ngit `{}` · catalog {} binaries (sha `{}`) · workbook: {}\n",
        report["git_sha"].as_str().unwrap_or("-"),
        inv["catalog"],
        report["catalog_sha256"].as_str().unwrap_or("-").get(..12).unwrap_or("-"),
        report["workbook"].as_str().unwrap_or("-")
    );
    let _ = writeln!(s, "| readiness gate | value |\n|---|---|");
    let _ = writeln!(s, "| envelopes | {} / {} |", inv["catalog"], inv["catalog"]);
    let _ = writeln!(s, "| trust hits | {} |", inv["trust_hits"]);
    let _ = writeln!(s, "| garbage-person | {} |", inv["guessed_person_on_garbage"]);
    let _ = writeln!(s, "| host on research graphs | {} |", inv["host_envelopes_with_nodes_on_mixed"]);
    let _ = writeln!(s, "| missing content_hash | {} |", inv["missing_content_hash"]);
    let _ = writeln!(
        s,
        "| stress sent | {} nodes, {} edges |",
        gate["nodes_sent"], gate["edges_sent"]
    );
    let _ = writeln!(
        s,
        "| stress expected vs matched | {} / {} |",
        gate["expected_proposals"], gate["matched"]
    );
    let _ = writeln!(
        s,
        "| stress BREAK (missing) | {} ({}) |",
        gate["missing"].as_array().map_or(0, Vec::len),
        if gate["pass"] == true { "pass" } else { "FAIL" }
    );
    let _ = writeln!(
        s,
        "| untagged tag lures on-node | {} / {} (target 0 after S1) |",
        gate["lures_lit"].as_array().map_or(0, Vec::len),
        gate["lures_sent"]
    );
    let _ = writeln!(
        s,
        "| mixed role-tag false positives | {} ({}) |",
        mixed["role_tag_false_positives"].as_array().map_or(0, Vec::len),
        if mixed["pass"] == true { "pass" } else { "FAIL" }
    );
    let _ = writeln!(
        s,
        "| mixed expected research | {} / {} |",
        mixed["matched"], mixed["expected"]
    );
    let _ = writeln!(
        s,
        "| company_typed COMPANY/PEOPLE | {} / {} |",
        company["company_state"].as_str().unwrap_or("-"),
        company["people_state"].as_str().unwrap_or("-")
    );
    let tc = &report["typecast_gate"];
    let _ = writeln!(
        s,
        "| type-cast company_notes DEEP_TAG | {} (people {}) |",
        tc["company_notes_deep_tags"].as_array().map_or(0, Vec::len),
        tc["company_notes_people"].as_str().unwrap_or("-")
    );
    let _ = writeln!(
        s,
        "| type-cast garbage DEEP_TAG | {} (target 0) |",
        tc["garbage_casts"].as_array().map_or(0, Vec::len)
    );
    if let Some(mixed_case) = report["cases"].get("mixed") {
        let _ = writeln!(
            s,
            "| mixed production callback | {} envelopes / {} B (full catalog wire {} B) |",
            mixed_case["callback_ops"],
            mixed_case["callback_bytes"],
            mixed_case["total_envelope_bytes"]
        );
    }
    let wk = &report["workers"];
    let _ = writeln!(
        s,
        "| worker fleet (64 callbacks) | with-data {} · skeletons {} · trust {} · mean {}µs · p95 {}µs |",
        wk["callbacks_with_data"], wk["skeletons"], wk["trust_hits"], wk["mean_us"], wk["p95_us"]
    );
    let _ = writeln!(
        s,
        "| stateless proof (seq vs {} threads) | bytes identical {} · {:.1} → {:.1} ops/s · wall {}ms → {}ms |",
        wk["threads"],
        wk["parallel_identical"],
        wk["sequential"]["ops_per_s"].as_f64().unwrap_or(0.0),
        wk["parallel"]["ops_per_s"].as_f64().unwrap_or(0.0),
        wk["sequential"]["wall_us"].as_u64().unwrap_or(0) / 1000,
        wk["parallel"]["wall_us"].as_u64().unwrap_or(0) / 1000
    );
    let vg = &report["viral_gate"];
    let _ = writeln!(
        s,
        "| viral K_mix (mixers / callback) | {} (d1 {} → d2 {}, pass {}) |",
        vg["K_mix"], vg["mixer_working_d1"], vg["mixer_working_d2"], vg["pass"]
    );
    let _ = writeln!(
        s,
        "| viral K_reuse (one payload → N answers) | {} |",
        vg["K_reuse"]
    );
    let obs = &report["obsidian_serialization"];
    let _ = writeln!(
        s,
        "| obsidian graph view | {} entity notes, {} rel wikilinks, {} anchor hubs |",
        obs["entity_notes"], obs["graph_links"], obs["anchor_hubs"]
    );
    let _ = writeln!(
        s,
        "| common vs uncommon (emitted) | kinds {} / {} · rels {} / {} · tags {} / {} |",
        obs["kinds_common"], obs["kinds_uncommon"],
        obs["rels_common"], obs["rels_uncommon"], obs["tags_common"], obs["tags_uncommon"]
    );
    let _ = writeln!(s, "\n| score | % |\n|---|---|");
    for k in [
        "completeness",
        "semantic_completeness",
        "quality_no_guess_no_trust",
        "invariants",
        "time_to_scale",
    ] {
        let _ = writeln!(s, "| {k} | {} |", sc[k]);
    }
    let _ = writeln!(s, "\n| case | bytes | sha256 | asked | callback_ops | callback_bytes | envelope_bytes | states |\n|---|---|---|---|---|---|---|---|");
    if let Some(cases) = report["cases"].as_object() {
        for (name, c) in cases {
            if let Some(err) = c["engine_error"].as_str() {
                let _ = writeln!(
                    s,
                    "| {name} | {} | — | — | — | — | **engine-error: {}** |",
                    c["payload_bytes"],
                    err.get(..90).unwrap_or(err)
                );
                continue;
            }
            let sha = c["payload_sha256"].as_str().unwrap_or("");
            let states = c["by_state"].as_object().map_or_else(String::new, |m| {
                m.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join(", ")
            });
            let _ = writeln!(
                s,
                "| {name} | {} | `{}` | {} | {} | {} | {} | {states} |",
                c["payload_bytes"],
                sha.get(..12).unwrap_or(sha),
                c["ops"],
                c["callback_ops"],
                c["callback_bytes"],
                c["total_envelope_bytes"]
            );
        }
    }
    let _ = writeln!(s, "\n| scale ops | ms | µs/op |\n|---|---|---|");
    if let Some(rows) = report["scale"].as_array() {
        for r in rows {
            let _ = writeln!(s, "| {} | {} | {:.2} |", r["ops"], r["ms"], r["us_per_op"].as_f64().unwrap_or(0.0));
        }
    }
    s
}

struct Case {
    name: &'static str,
    payload: Vec<u8>,
}

/// found/forgot grouped case → family so clustering is legible to a human.
fn render_groups(groups: &BTreeMap<(&'static str, String), Vec<String>>) -> String {
    let mut s = String::new();
    let mut last = "";
    for ((case, fam), lines) in groups {
        if *case == last {
            let _ = write!(s, "\n### {fam}\n");
        } else {
            last = case;
            let _ = write!(s, "\n## case: {case}\n\n### {fam}\n");
        }
        for line in lines {
            let _ = writeln!(s, "{line}");
        }
    }
    if s.is_empty() {
        s.push_str("(none)\n");
    }
    s
}

/// vault-safe note title: strip chars vault reserves, keep it unique.
fn note_name(label: &str, id: u64, used: &mut BTreeSet<String>) -> String {
    let cleaned: String = label
        .chars()
        .map(|c| match c {
            '#' | '^' | '[' | ']' | '|' | ':' | '/' | '\\' | '%' | '?' => ' ',
            _ => c,
        })
        .collect();
    let base = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let base = if base.is_empty() {
        format!("node {id}")
    } else {
        base
    };
    let mut candidate = base.clone();
    while !used.insert(candidate.clone()) {
        candidate = format!("{base} [{id}]");
    }
    candidate
}

/// vault graph-view serialization: one note per graph node, edges become
/// `[[wikilinks]]`, kinds become nested `#aria/{kind}` tags so Graph View
/// can colour the clusters. Markdown is the view; the dump JSON is the contract.
/// Anchor hubs: EVERY expressed token gets a hub (the MAX-anchor doctrine —
/// no orphan dust); weight only orders the MOC table. Hub member lists stay
/// capped so one hub never turns into a blob.
const ANCHOR_MEMBERS_CAP: usize = 50;
const MOC_TABLE_CAP: usize = 40;

#[allow(clippy::too_many_lines)] // deterministic layout: express → rank → emit → hubs
fn write_graph_notes(dir: &Path, case_list: &[Case], gr: &Grammar) -> ObsidianStats {
    let mut stats = ObsidianStats {
        notes: 0,
        links: 0,
        hubs: 0,
        kinds: (0, 0),
        rels: (0, 0),
        tags: (0, 0),
    };
    // First pass: which anchor tokens does this run actually express?
    let mut expressed: BTreeSet<String> = BTreeSet::new();
    let mut case_spans = Vec::<(String, Vec<(u64, String, String, String, Vec<String>)>)>::new();
    let mut case_roots = Vec::<(&Case, Value)>::new();
    for c in case_list {
        // Limit battery payloads stress the engine; they are not review graphs.
        // (limit_huge alone would mint 5_000 entity notes.)
        if c.name.starts_with("limit_") {
            continue;
        }
        let Ok(root) = serde_json::from_slice::<Value>(&c.payload) else { continue };
        let Some(raw_nodes) = root["nodes"].as_array() else { continue };
        if raw_nodes.is_empty() {
            continue;
        }
        let mut used = BTreeSet::new();
        let mut nodes = Vec::<(u64, String, String, String, Vec<String>)>::new();
        for n in raw_nodes {
            let Some(id) = n["id"].as_u64() else { continue };
            let kind = n["type"]
                .as_str()
                .or_else(|| n["kind"].as_str())
                .unwrap_or("Unlabeled")
                .to_string();
            let label = n["label"].as_str().map_or_else(|| kind.clone(), str::to_string);
            let name = note_name(&label, id, &mut used);
            let sector = n["sector"].as_str().unwrap_or("").to_string();
            let tags: Vec<String> = n["tags"].as_array().map_or_else(Vec::new, |a| {
                a.iter().filter_map(Value::as_str).map(str::to_string).collect()
            });
            for t in tags.iter().filter(|t| !GENERIC_TAGS.contains(&t.as_str())) {
                expressed.insert(t.clone());
            }
            if tags.is_empty() {
                if let Some((owner_bin, _)) = gr.owner.get(&kind.to_ascii_lowercase()) {
                    for t in catalog().iter().find(|s| &s.binary_id == owner_bin).map_or(Vec::new(), |s| {
                        s.anchor_tags.iter().filter(|t| !GENERIC_TAGS.contains(&t.as_str())).cloned().collect()
                    }) {
                        expressed.insert(t);
                    }
                }
            }
            nodes.push((id, name, kind, sector, tags));
        }
        case_roots.push((c, root));
        case_spans.push((c.name.to_string(), nodes));
    }
    // Every expressed anchor gets a hub — ranked by weight for the table;
    // anything a node anchors to is a real note (no ghost links, no dust).
    let mut ranked: Vec<&String> = expressed.iter().collect();
    ranked.sort_by(|a, b| token_stat(b).0.cmp(&token_stat(a).0).then_with(|| a.cmp(b)));
    let top: BTreeSet<String> = ranked.iter().map(|s| (*s).clone()).collect();
    let mut hub_members: BTreeMap<String, Vec<String>> = top.iter().map(|t| (t.clone(), Vec::new())).collect();
    let mut review = String::from(
        "# Review — topology of the emitted knowledge graph\n\
         \nComputed from the same notes/files Graph View renders. 'Orphan' means \
         \nno link of any kind — no relation *and* no anchor hub.\n",
    );
    let mut rev_rows = Vec::<(
        String,
        BTreeMap<String, usize>,
        BTreeMap<String, usize>,
        BTreeSet<String>,
        usize,
    )>::new();

    let mut moc = String::from(
        "# Graph — labeled entities, wikilinked by catalog relations\n\
         \nFields `weight::` (category weight = declaring binaries) and `height::` \
         \n(install wave A→1…C→3) come from the catalog grammar, so this \
         \ndump is a deterministic reflection of the catalog — no run computes its own truth.\n\
         \n## Graph View groups (paste into Groups, one per line)\n\
         \n```text\n\
         tag:#aria/h/1        (wave A spine — green)\n\
         tag:#aria/h/2        (wave B residual — blue)\n\
         tag:#aria/h/3        (wave C frontier — purple)\n\
         tag:#aria/w/common   (shared grammar — yellow)\n\
         tag:#aria/w/uncommon (singleton handles — red)\n\
         tag:#aria/Anchor     (category hubs — amber)\n\
         path:entities/stress (stress graph only)\n\
         ```\n",
    );
    for ((cname, nodes), (_c, root)) in case_spans.iter().zip(case_roots.iter()) {
        let by_id: BTreeMap<u64, &str> = nodes.iter().map(|(id, name, ..)| (*id, name.as_str())).collect();
        let cdir = dir.join("entities").join(cname);
        fs::create_dir_all(&cdir).expect("entities dir");
        let _ = writeln!(moc, "\n## case: {cname}");
        let mut by_kind: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut degree = BTreeMap::<String, usize>::new();
        let targets: BTreeSet<&str> = (*root)["edges"]
            .as_array()
            .map_or_else(BTreeSet::new, |es| {
                es.iter()
                    .filter_map(|e| e["to"].as_u64().and_then(|t| by_id.get(&t)))
                    .copied()
                    .collect()
            });
        for (id, name, kind, sector, tags) in nodes {
            by_kind.entry(kind.as_str()).or_default().push(name.as_str());
            let (wk, hk, ck) = Grammar::stat(kind);
            *if ck { &mut stats.kinds.0 } else { &mut stats.kinds.1 } += 1;
            let mut body = String::new();
            let _ = writeln!(body, "# {name}\n\nkind:: {kind}");
            let _ = writeln!(body, "weight:: {wk} {}", if ck { "(common)" } else { "(uncommon)" });
            let _ = writeln!(body, "height:: {hk} (wave {})", ["-", "A", "B", "C", "D"].get(hk as usize).copied().unwrap_or("-"));
            if !sector.is_empty() {
                let _ = writeln!(body, "sector:: {sector}");
            }
            let specific: Vec<&String> = tags.iter().filter(|t| !GENERIC_TAGS.contains(&t.as_str())).collect();
            if !specific.is_empty() {
                body.push_str("tags:: ");
                for t in &specific {
                    let (_, _, ct) = Grammar::stat(t);
                    *if ct { &mut stats.tags.0 } else { &mut stats.tags.1 } += 1;
                    let _ = write!(body, "{} ", Grammar::render_anchor(t, &top));
                    if let Some(m) = hub_members.get_mut(*t) {
                        if m.len() < ANCHOR_MEMBERS_CAP && !m.contains(name) {
                            m.push(name.clone());
                        }
                    }
                }
                body.push('\n');
            } else if let Some((owner_bin, _)) = gr.owner.get(&kind.to_ascii_lowercase()) {
                let oanchors: Vec<String> = catalog()
                    .iter()
                    .find(|s| &s.binary_id == owner_bin)
                    .map_or(Vec::new(), |s| {
                        s.anchor_tags.iter().filter(|t| !GENERIC_TAGS.contains(&t.as_str())).cloned().collect()
                    });
                if !oanchors.is_empty() {
                    let _ = writeln!(body, "owner:: {owner_bin}");
                    let rendered: Vec<String> = oanchors.iter().map(|t| Grammar::render_anchor(t, &top)).collect();
                    let _ = writeln!(body, "anchor:: {}", rendered.join(" · "));
                    for t in &oanchors {
                        if let Some(m) = hub_members.get_mut(t) {
                            if m.len() < ANCHOR_MEMBERS_CAP && !m.contains(name) {
                                m.push(name.clone());
                            }
                        }
                    }
                }
            }
            if let Some(es) = (*root)["edges"].as_array() {
                let mut rel = false;
                for e in es {
                    if e["from"].as_u64() == Some(*id) {
                        if !rel {
                            let _ = writeln!(body, "\n## Relations\n");
                            rel = true;
                        }
                        let rt = e["type"].as_str().unwrap_or("?");
                        let (wr, hr, cr) = Grammar::stat(rt);
                        *if cr { &mut stats.rels.0 } else { &mut stats.rels.1 } += 1;
                        let tgt = e["to"]
                            .as_u64()
                            .and_then(|t| by_id.get(&t))
                            .copied()
                            .unwrap_or("(external)");
                        let hletter = ["-", "A", "B", "C", "D"].get(hr as usize).copied().unwrap_or("-");
                        let _ = writeln!(body, "- **{rt}** → [[{tgt}]] · w{wr} {} · wave {hletter}", if cr { "common" } else { "uncommon" });
                        stats.links += 1;
                        *degree.entry(name.clone()).or_insert(0) += 1;
                    }
                }
            }
            degree.entry(name.clone()).or_insert(0);
            // Multi-axis Graph View grammar: colour by height (wave) or
            // weight (shape), navigate by kind — all deterministic per run.
            let wshape = match wk {
                0 => "isolated",
                1 => "uncommon",
                _ => "common",
            };
            let _ = writeln!(body, "\n#aria/kind/{kind}\n#aria/h/{hk}\n#aria/w/{wshape}");
            fs::write(cdir.join(format!("{name}.md")), body).expect("entity note");
            stats.notes += 1;
        }
        // MOC indexes names as TEXT (not links) so graph view shows the real
        // entity↔entity / entity↔anchor topology — no giant MOC blob.
        for (kind, names) in &by_kind {
            let _ = writeln!(moc, "\n### {kind} ({})\n", names.len());
            for n in names {
                let _ = writeln!(moc, "- `{n}`");
            }
        }
        rev_rows.push((
            cname.clone(),
            degree,
            by_kind.iter().map(|(k, v)| ((*k).to_string(), v.len())).collect::<BTreeMap<_, _>>(),
            targets.iter().map(|s| (*s).to_string()).collect(),
            nodes.len(),
        ));
    }
    // Anchor hubs + the category-weight table (02/12 grammar surfaced as notes).
    if !top.is_empty() {
        let adir = dir.join("anchors");
        fs::create_dir_all(&adir).expect("anchors dir");
        let _ = writeln!(moc, "\n## Anchors — category weight (02/12 grammar)\n");
        let _ = writeln!(moc, "| anchor | weight | height | shape | hub |\n|---|---|---|---|---|");
        for (token, members) in &hub_members {
            let (w, h, c) = Grammar::stat(token);
            let mut hub = format!("# Anchor {token}\n\nweight:: {w}\nheight:: {h}\nshape:: {}\n", if c { "common" } else { "uncommon" });
            if let Some(fam) = gr.owner.get(&token.to_ascii_lowercase()) {
                let _ = writeln!(hub, "owning_binary:: {}", fam.0);
            }
            let _ = writeln!(hub, "\n## Members\n");
            for m in members {
                let _ = writeln!(hub, "- [[{m}]]");
            }
            let _ = writeln!(hub, "\n#aria/Anchor");
            fs::write(adir.join(format!("Anchor-{token}.md")), hub).expect("anchor hub");
            stats.hubs += 1;
            if stats.hubs <= MOC_TABLE_CAP {
                let _ = writeln!(
                    moc,
                    "| {token} | {w} | {} ({}) | {} | [[Anchor-{token}]] |",
                    h, ["-", "A", "B", "C", "D"].get(h as usize).copied().unwrap_or("-"),
                    if c { "common" } else { "uncommon" }
                );
            }
        }
    }
    // Review: orphans measured the way Graph View sees them — a note is
    // connected iff it has a relation *or* an anchor-hub link.
    let anchored: BTreeSet<&String> = hub_members.values().flatten().collect();
    for (cname, degree, kinds_counts, targets, n_nodes) in &rev_rows {
        let orphans = degree
            .iter()
            .filter(|(n, d)| **d == 0 && !targets.contains(n.as_str()) && !anchored.contains(n))
            .count();
        let mut top_deg: Vec<(&String, &usize)> = degree.iter().collect();
        top_deg.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let kinds_hist = kinds_counts
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect::<Vec<_>>()
            .join(" · ");
        let _ = writeln!(review, "\n## case: {cname}\n\n| metric | value |\n|---|---|");
        let _ = writeln!(review, "| notes | {n_nodes} |");
        let _ = writeln!(review, "| relation links | {} |", degree.values().sum::<usize>());
        let _ = writeln!(review, "| anchor-hub members | {} |", degree.keys().filter(|n| anchored.contains(n)).count());
        let _ = writeln!(review, "| orphans (no link at all) | {orphans} |");
        let _ = writeln!(review, "| kinds | {kinds_hist} |");
        let _ = writeln!(review, "\nTop degree:\n");
        for (n, d) in top_deg.iter().take(10) {
            let _ = writeln!(review, "- {n} ({d})");
        }
    }
    fs::write(dir.join("graph.md"), moc).expect("graph.md");
    fs::write(dir.join("review.md"), review).expect("review.md");
    stats
}

fn worker_opts() -> RunOpts {
    RunOpts {
        steps: 0,
        seed: Some(1),
        n_modes: Some(16),
        latent_dim: Some(16),
        allow_sub_spec_dims: true,
        include_telemetry: false,
        ..RunOpts::default()
    }
}

/// One worker = one requirement (binary) × one question, through the real
/// callback path. Returns (row, wire bytes). Pure: no shared state (𝔸T2).
fn one_worker(w: usize, binary: &str, qname: &str, qpayload: &[u8]) -> (Value, Vec<u8>) {
    let t0 = Instant::now();
    let parsed = match serde_json::from_slice::<Value>(qpayload) {
        Ok(p) => p,
        Err(e) => {
            return (
                json!({
                    "worker": w, "binary_id": binary, "question": qname,
                    "ops": 0, "us": 0, "bytes": 0,
                    "engine_error": format!("question not parseable JSON: {e}"),
                }),
                Vec::new(),
            )
        }
    };
    let req = WorkRequest {
        ops: vec![binary.to_string()],
        payload: Some(parsed),
        ..WorkRequest::default()
    };
    match execute_work(&req, &worker_opts()) {
        Ok(out) => {
            let us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
            let wire = serde_json::to_vec(&out).unwrap_or_default();
            let skeletons = out["results"].as_array().map_or(0, |rs| {
                rs.iter()
                    .filter(|r| {
                        r["nodes"].as_array().map_or(0, Vec::len)
                            + r["relationships"].as_array().map_or(0, Vec::len)
                            + r["properties"].as_object().map_or(0, serde_json::Map::len)
                            == 0
                    })
                    .count()
            });
            let text = String::from_utf8_lossy(&wire);
            let trust = usize::from(wire.is_empty())
                + usize::from(text.contains("\"trust\"") || text.contains("\"Trust\""));
            (
                json!({
                    "worker": w, "binary_id": binary, "question": qname,
                    "ops": out["ops"], "us": us, "bytes": wire.len(),
                    "skeletons": skeletons, "trust": trust,
                }),
                wire,
            )
        }
        Err(e) => {
            // Hostile questions are engine errors, not worker crashes.
            let us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
            (
                json!({
                    "worker": w, "binary_id": binary, "question": qname,
                    "ops": 0, "us": us, "bytes": 0,
                    "engine_error": e.to_string(),
                }),
                Vec::new(),
            )
        }
    }
}

fn fleet_summary(rows: &[Value], n: usize, wall_us: u64) -> Value {
    let mut micros: Vec<u64> = rows.iter().filter_map(|r| r["us"].as_u64()).collect();
    micros.sort_unstable();
    let mean_us = micros.iter().sum::<u64>() / u64::try_from(n.max(1)).unwrap_or(1);
    let p95 = micros.get(n.saturating_mul(95) / 100).copied().unwrap_or(0);
    let sum = |k: &str| rows.iter().map(|r| r[k].as_u64().unwrap_or(0)).sum::<u64>();
    let ops_per_s = if wall_us == 0 { 0.0 } else { (n as f64) * 1_000_000.0 / (wall_us as f64) };
    json!({
        "n": n,
        "wall_us": wall_us,
        "ops_per_s": ops_per_s,
        "mean_us": mean_us,
        "p95_us": p95,
        "max_us": micros.last().copied().unwrap_or(0),
        "callbacks_with_data": rows.iter().filter(|r| r["ops"].as_u64().unwrap_or(0) > 0).count(),
        "engine_errors": rows.iter().filter(|r| r.get("engine_error").is_some()).count(),
        "skeletons": sum("skeletons"),
        "trust_hits": sum("trust"),
        "bytes_total": sum("bytes"),
    })
}

/// Production worker simulation (dispatch model) run twice — sequential
/// and parallel (`std::thread::scope`, one thread per core). The node is
/// stateless, so both must produce identical bytes; the pair also yields a
/// real throughput number (𝐋T4 / ℙT5). Picks are sha256-keyed: the fleet
/// replays bit-for-bit every run.
fn run_workers(case_list: &[Case], n: usize) -> Value {
    let all_ids: Vec<String> = catalog().iter().map(|s| s.binary_id.clone()).collect();
    let questions: Vec<(&'static str, Vec<u8>)> =
        case_list.iter().map(|c| (c.name, c.payload.clone())).collect();
    let jobs: Vec<(usize, String, &'static str, &[u8])> = (0..n)
        .map(|w| {
            let (qname, qpayload) = det_pick("worker-question", w, &questions);
            (w, det_pick("worker-binary", w, &all_ids).clone(), *qname, qpayload.as_slice())
        })
        .collect();

    let t0 = Instant::now();
    let seq: Vec<(Value, Vec<u8>)> = jobs.iter().map(|(w, b, q, p)| one_worker(*w, b, q, p)).collect();
    let seq_wall = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);

    let threads = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    let t1 = Instant::now();
    let mut par: Vec<(usize, Value, Vec<u8>)> = std::thread::scope(|s| {
        let handles: Vec<_> = jobs
            .chunks(jobs.len().div_ceil(threads).max(1))
            .map(|chunk| {
                s.spawn(move || {
                    chunk
                        .iter()
                        .map(|(w, b, q, p)| {
                            let (row, wire) = one_worker(*w, b, q, p);
                            (*w, row, wire)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("worker thread")).collect()
    });
    let par_wall = u64::try_from(t1.elapsed().as_micros()).unwrap_or(u64::MAX);
    par.sort_by_key(|(w, ..)| *w);

    // Statelessness proof: identical callback bytes, sequential vs parallel.
    let identical = seq.iter().zip(par.iter()).all(|((_, a), (_, _, b))| a == b);

    let rows: Vec<Value> = seq.iter().map(|(r, _)| r.clone()).collect();
    let par_rows: Vec<Value> = par.iter().map(|(_, r, _)| r.clone()).collect();
    let seq_sum = fleet_summary(&rows, n, seq_wall);
    let par_sum = fleet_summary(&par_rows, n, par_wall);
    json!({
        "schema": "aria-workers-v1",
        "contract": "one worker = one requirement (binary) × one question; asked≠ops is the audit; skeletons must be 0; trust must be 0; parallel bytes == sequential bytes",
        "workers": rows,
        "threads": threads,
        "parallel_identical": identical,
        "parallel": par_sum,
        "sequential": seq_sum.clone(),
        // Flat fields kept for SCORE rows / older readers.
        "n": n,
        "mean_us": seq_sum["mean_us"],
        "p95_us": seq_sum["p95_us"],
        "max_us": seq_sum["max_us"],
        "callbacks_with_data": seq_sum["callbacks_with_data"],
        "skeletons": seq_sum["skeletons"],
        "trust_hits": seq_sum["trust_hits"],
        "bytes_total": seq_sum["bytes_total"],
    })
}

/// Measured virality (ℙT4): one callback re-fed to the 25 mixers, then once more.
fn viral_gate(mixed_payload: &[u8], callback: &Value, opts: &RunOpts) -> Value {
    let mixers: Vec<String> = catalog()
        .iter()
        .filter(|s| s.layer == "REFINEMENT")
        .map(|s| s.binary_id.clone())
        .collect();
    let cb_ops = callback["ops"].as_u64().unwrap_or(0);
    let cb_bytes = serde_json::to_vec(callback).unwrap_or_default();
    let d1 = run_many(&mixers, &cb_bytes, opts).unwrap_or_default();
    let d1_w: Vec<_> = d1.iter().filter(|e| e.has_working_data()).collect();
    let d1_cb = json!({
        "schema": WORK_V1,
        "phi_once": true,
        "asked": mixers.len(),
        "ops": d1_w.len(),
        "results": d1_w,
    });
    let d2 = run_many(&mixers, &serde_json::to_vec(&d1_cb).unwrap_or_default(), opts)
        .unwrap_or_default();
    let d2_w = d2.iter().filter(|e| e.has_working_data()).count();
    let research: Vec<String> = catalog()
        .iter()
        .filter(|s| s.layer != "HOST")
        .map(|s| s.binary_id.clone())
        .collect();
    let reuse = run_many(&research, mixed_payload, opts).unwrap_or_default();
    let reuse_w = reuse.iter().filter(|e| e.has_working_data()).count();
    let k_mix = if cb_ops == 0 {
        0.0
    } else {
        d1_w.len() as f64 / cb_ops as f64
    };
    json!({
        "callback_working": cb_ops,
        "mixer_working_d1": d1_w.len(),
        "mixer_working_d2": d2_w,
        "K_mix": k_mix,
        "K_reuse": reuse_w,
        "depth2_le_depth1": d2_w <= d1_w.len(),
        "pass": d2_w <= d1_w.len(),
        "mixers": mixers.len(),
    })
}

fn coverage_md(cov: &BTreeMap<String, Vec<&'static str>>) -> String {
    let mut s = String::from(
        "# Operator coverage (this dump)\n\nWorking data per binary across battery cases. Dark on every case = fine-tune queue (spec/lexicon, never a per-crate src edit).\n\n| binary | cases with working data |\n|---|---|\n",
    );
    let mut dark = Vec::new();
    for spec in catalog() {
        match cov.get(&spec.binary_id) {
            Some(cs) if !cs.is_empty() => {
                let _ = writeln!(s, "| `{}` | {} |", spec.binary_id, cs.join(", "));
            }
            _ => dark.push(spec.binary_id.as_str()),
        }
    }
    let _ = writeln!(s, "\n## Dark on every case ({})\n", dark.len());
    for id in dark {
        let _ = writeln!(s, "- `{id}`");
    }
    s
}

/// WORKERS.md — the dispatch contract + this run's measured table.
fn workers_md(w: &Value) -> String {
    let errors = w["workers"]
        .as_array()
        .map_or(0, |rs| rs.iter().filter(|r| r.get("engine_error").is_some()).count());
    let mut s = String::from(
        "# Production worker plan\n\n\
         One dispatcher spawns **one worker per requirement**: each worker is a \
         catalog binary; the question payload is the original anchor. Every row below went \
         through the real `execute_work` callback, so what you see is the production wire.\n\n\
         - absent `results` = absence (no skeletons)\n\
         - `asked` ≠ `ops` is the no-bias audit (B2 independence)\n\
         - picks are sha256-keyed: the fleet replays bit-for-bit\n\n",
    );
    let _ = writeln!(s, "| metric | value |\n|---|---|");
    let _ = writeln!(s, "| workers | {} |", w["n"]);
    let _ = writeln!(s, "| callbacks with data | {} |", w["callbacks_with_data"]);
    let _ = writeln!(s, "| engine errors (Result, no crash) | {errors} |");
    let _ = writeln!(s, "| skeletons | {} |", w["skeletons"]);
    let _ = writeln!(s, "| trust hits | {} |", w["trust_hits"]);
    let _ = writeln!(s, "| mean | {} µs |", w["mean_us"]);
    let _ = writeln!(s, "| p95 | {} µs |", w["p95_us"]);
    let _ = writeln!(s, "| max | {} µs |", w["max_us"]);
    let _ = writeln!(s, "| total wire bytes | {} |", w["bytes_total"]);
    let _ = writeln!(
        s,
        "\n## Statelessness proof\n\nThe same 64 workers ran sequentially and on {} threads (`std::thread::scope`). \
         Because the node holds no state, the callback bytes must be identical.\n\n\
         | mode | wall | ops/s | p95 |\n|---|---|---|---|\n\
         | sequential | {} ms | {:.1} | {} µs |\n\
         | parallel ×{} | {} ms | {:.1} | {} µs |\n\n\
         **bytes identical: {}**\n",
        w["threads"],
        w["sequential"]["wall_us"].as_u64().unwrap_or(0) / 1000,
        w["sequential"]["ops_per_s"].as_f64().unwrap_or(0.0),
        w["sequential"]["p95_us"],
        w["threads"],
        w["parallel"]["wall_us"].as_u64().unwrap_or(0) / 1000,
        w["parallel"]["ops_per_s"].as_f64().unwrap_or(0.0),
        w["parallel"]["p95_us"],
        w["parallel_identical"]
    );
    let _ = writeln!(s, "\n| worker | binary | question | working ops | µs | bytes |\n|---|---|---|---|---|---|");
    if let Some(rows) = w["workers"].as_array() {
        for r in rows {
            let ops = r["engine_error"].as_str().map_or_else(
                || format!("{}", r["ops"]),
                |e| format!("ERR: {}", e.split([':', '\n']).next().unwrap_or(e)),
            );
            let _ = writeln!(
                s,
                "| {} | {} | {} | {ops} | {} | {} |",
                r["worker"], r["binary_id"], r["question"], r["us"], r["bytes"]
            );
        }
    }
    s
}

fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("copy_tree dst");
    for entry in fs::read_dir(src).expect("copy_tree read_dir") {
        let entry = entry.expect("copy_tree entry");
        let p = entry.path();
        if p.is_dir() {
            copy_tree(&p, &dst.join(entry.file_name()));
        } else {
            fs::copy(&p, dst.join(entry.file_name())).expect("copy_tree file");
        }
    }
}

/// Deterministic pool pick keyed by sha256 — no PRNG dep, and the picking
/// seed is itself hash-evidence (`{key}:{i}` is verifiable after the fact).
fn det_pick<'a, T>(key: &str, i: usize, pool: &'a [T]) -> &'a T {
    let h = sha256_hex(format!("{key}:{i}").as_bytes());
    let v = u64::from_str_radix(h.get(..13).unwrap_or(&h), 16).unwrap_or(0);
    pool.get(usize::try_from(v % u64::try_from(pool.len()).unwrap_or(u64::MAX)).unwrap_or(0))
        .unwrap_or(&pool[0])
}

fn family(binary_id: &str) -> String {
    binary_id.split('.').nth(1).unwrap_or("ROOT").to_string()
}

/// Workbook grammar for the render layer: weight/height come from the lib's
/// frozen-catalog `token_stat` (same numbers carried on the wire envelope);
/// only kind→owning-binary stays local (entities need their owner binary).
struct Grammar {
    owner: BTreeMap<String, (String, String)>, // kind lc → (binary_id, wave)
}

fn grammar() -> Grammar {
    // ENTITY (family) first: its anchors are the hub-bearing ones; the NODE
    // residual only fills gaps the family didn't name (deterministic).
    let mut owner: BTreeMap<String, (String, String)> = BTreeMap::new();
    for class in ["ENTITY", "NODE"] {
        for s in catalog().iter().filter(|s| s.layer != "HOST" && s.class == class) {
            for t in &s.node_types {
                owner
                    .entry(t.to_ascii_lowercase())
                    .or_insert_with(|| (s.binary_id.clone(), s.wave.clone().unwrap_or_default()));
            }
        }
    }
    Grammar { owner }
}

impl Grammar {
    /// (weight, height, common) for one token — same numbers the wire
    /// envelope carries (`OperatorGraph`); the vault only projects them.
    fn stat(token: &str) -> (u32, u8, bool) {
        let (w, h) = token_stat(token);
        (w, h, w >= 2)
    }
    /// `Some(|—render|)` text for an anchor token: hub links for top anchors,
    /// plain text otherwise (no ghost nodes in Graph View).
    fn render_anchor(token: &str, top: &BTreeSet<String>) -> String {
        let (w, h, c) = Grammar::stat(token);
        let name = if top.contains(token) {
            format!("[[Anchor-{token}]]")
        } else {
            token.to_string()
        };
        format!("{name} (w{w}{} h{h})", if c { " common" } else { " uncommon" })
    }
}

struct ObsidianStats {
    notes: usize,
    links: usize,
    hubs: usize,
    kinds: (usize, usize),
    rels: (usize, usize),
    tags: (usize, usize),
}

fn git_sha() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_string(), |s| s.trim().to_string())
}

const FIRST: &[&str] = &[
    "Mira", "Jonas", "Priya", "Theo", "Anouk", "Ravi", "Selma", "Owen", "Lena", "Marcus",
    "Ines", "Dario", "Freya", "Tomas", "Aiko", "Nils", "Sofia", "Emeka", "Halle", "Yusuf",
    "Petra", "Gideon", "Nadia", "Callum", "Ruth", "Iker", "Amara", "Felix", "Talia", "Ruben",
    "Wren", "Matteo",
];
const LAST: &[&str] = &[
    "Okafor", "Lindqvist", "Marshak", "de Vries", "Sato", "Kowalski", "Almeida", "Brandt",
    "Haugen", "Ferreira", "Nakamura", "O'Brien", "Castellanos", "Vogel", "Adeyemi", "Mercer",
    "Halloran", "Silva", "Novak", "Tanaka", "Bergstrom", "Owusu", "Marchetti", "Falk",
    "Sundaram", "Rivera", "Lange", "Moreau", "Petrov", "Ashby", "Lemmens", "Quist",
];
const ADJ: &[&str] = &[
    "Northline", "Vantage", "Meridian", "Redline", "Clearview", "Ironwood", "Bluefield",
    "Sable", "Highpoint", "Cobalt", "Fairway", "Summit", "Harborline", "Solidground",
    "Brightmark", "Eastgate", "Stonepath", "Trueline", "Westbrook", "Upland", "Corebridge",
    "Pinnacle", "Grove", "Sterling",
];
const CORE: &[&str] = &[
    "Logistics", "Grid", "Payments", "Robotics", "Materials", "Storage", "Analytics", "Cloud",
    "Security", "Supply", "Imaging", "Commerce", "Energy", "Mapping", "Bio", "Networks",
    "Hardware", "Legal", "Climate", "Mobility", "Fabrication", "Capital", "Markets", "Talent",
];
const SUFFIX: &[&str] = &[
    "Labs", "Systems", "Works", "Dynamics", "Group", "Technologies", "Industries", "Networks",
    "Ventures", "Platforms", "Compute", "Ascent",
];
const CITY: &[&str] = &[
    "Lisbon", "Oslo", "Warsaw", "Nairobi", "Vancouver", "Ankara", "Melbourne", "Bergen",
    "Medellin", "Porto", "Tallinn", "Quebec City", "Adelaide", "Dublin", "Rotterdam", "Leipzig",
];
const SECTOR: &[&str] = &[
    "fintech", "industrial automation", "grid software", "medical imaging", "supply chain",
    "cloud infrastructure", "cybersecurity", "legal tech", "mobility", "climate data",
    "bioinstrumentation", "mapping", "payments", "energy storage", "talent analytics",
    "fabrication services",
];
/// Entity-type tokens that are always true of a typed node — a lure carrying
/// these proves nothing; a lure carrying a *specific* tag proves surgery S1.
const GENERIC_TAGS: &[&str] = &[
    "PERSON", "ACCOUNT", "COMPANY", "FUND", "INVESTOR", "CUSTOMER", "PRODUCT", "EVENT",
    "MARKET", "CATEGORY", "MARKET_SIGNAL", "MARKETSIGNAL", "OBSERVATION",
];

struct StressGen {
    payload: Vec<u8>,
    /// Binaries that MUST propose on this payload (expectation == breakage test).
    expect: Vec<String>,
    /// Untagged lures: (binary_id, node_id). Lit only if that node appears
    /// on the binary's envelope — binary-level proposal is not enough (a
    /// TAG may correctly light a *different* typed entity).
    lures: Vec<(String, u64)>,
    nodes: usize,
    edges: usize,
}

/// "Sounds true, made-up, all sent": every field fabricated from pools above,
/// but typed/tagged exactly per the catalog grammar.
/// Texture comes from hash-keyed sampling of the catalog's own declarations.
#[allow(clippy::too_many_lines)] // linear generator, four declarative passes
fn stress_gen() -> StressGen {
    let specs: Vec<_> = catalog()
        .iter()
        .filter(|s| s.layer != "HOST" && s.verify)
        .collect();
    let mut expect = Vec::new();
    let mut lures = Vec::new();
    let mut nodes: Vec<Value> = Vec::new();
    let mut edges: Vec<Value> = Vec::new();
    let mut ent_ids = Vec::new();
    let mut next_id = 1u64;

    // Pass 1 — ENTITY/NODE declarations become typed nodes with fabricated labels.
    for (i, spec) in specs.iter().enumerate() {
        if !matches!(spec.class.as_str(), "ENTITY" | "NODE") {
            continue;
        }
        let Some(ty) = spec.node_types.first() else { continue };
        let label = if ty == "Person" || ty == "Advisor" {
            format!("{} {}", det_pick("first", i, FIRST), det_pick("last", i, LAST))
        } else {
            format!(
                "{} {} {}",
                det_pick("adj", i, ADJ),
                det_pick("core", i, CORE),
                det_pick("suf", i, SUFFIX)
            )
        };
        let sector = det_pick("sec", i, SECTOR);
        nodes.push(json!({
            "id": next_id, "type": ty, "kind": ty, "label": label,
            "sector": sector,
            "notes": format!("{label} — {sector} operator, {city} (fabricated lore, dumped as sent)", city = det_pick("city", i, CITY)),
        }));
        ent_ids.push(next_id);
        next_id += 1;
        expect.push(spec.binary_id.clone());
    }

    // Pass 2 — REL declarations become edges between the entities above.
    for (i, spec) in specs.iter().enumerate() {
        if spec.class != "REL" {
            continue;
        }
        let Some(rt) = spec.relationship_types.first() else { continue };
        if ent_ids.len() < 2 {
            continue;
        }
        let a = ent_ids[i % ent_ids.len()];
        let b = ent_ids[(i * 7 + 3) % ent_ids.len()];
        edges.push(json!({"from": a, "to": b, "type": rt}));
        expect.push(spec.binary_id.clone());
    }

    // Pass 3 — PROP declarations become properties on the entities.
    for (i, spec) in specs.iter().enumerate() {
        if spec.class != "PROP" {
            continue;
        }
        let Some(key) = spec.property_key.as_deref() else { continue };
        let idx = i % ent_ids.len();
        nodes[idx]
            .as_object_mut()
            .and_then(|m| m.insert(key.to_string(), json!(format!("v{}", i + 1))));
        expect.push(spec.binary_id.clone());
    }

    // Pass 4 — TAG/DEEP_TAG: alternate expect-tagged vs. untagged lure.
    for (i, spec) in specs.iter().enumerate() {
        if !(spec.class == "TAG" || spec.layer == "DEEP_TAG") {
            continue;
        }
        let Some(tag) = spec
            .anchor_tags
            .iter()
            .map(String::as_str)
            .find(|t| !GENERIC_TAGS.contains(t))
        else {
            continue;
        };
        if i % 2 == 0 {
            let label = format!("{} {}", det_pick("first", i, FIRST), det_pick("last", i, LAST));
            let ty = spec.node_types.first().map_or("Observation", String::as_str);
            nodes.push(json!({
                "id": next_id, "type": ty, "kind": ty, "label": label,
                "tags": [tag],
                "notes": format!("{label} tagged in {sector} tooling (fabricated)", sector = det_pick("sec", i, SECTOR)),
            }));
            next_id += 1;
            expect.push(spec.binary_id.clone());
        } else {
            // Lure: a real structural Person with role-flavoured notes, no tags.
            let label = format!("{} {}", det_pick("first", i, FIRST), det_pick("last", i, LAST));
            nodes.push(json!({
                "id": next_id, "type": "Person", "label": label,
                "notes": format!("{label} leads {co} in {sector} (fabricated role-flavoured notes only, zero tags)", co = format!("{} {} {}", det_pick("adj", i, ADJ), det_pick("core", i, CORE), det_pick("suf", i, SUFFIX)), sector = det_pick("sec", i, SECTOR)),
            }));
            lures.push((spec.binary_id.clone(), next_id));
            next_id += 1;
        }
    }

    // Family TAG: every already-sent eligible entity without the firing tag
    // is a lure. COMPANY nodes must not light COMPETITOR.
    for spec in specs.iter().filter(|s| s.layer == "TAG" && s.class == "TAG") {
        let kinds: Vec<String> = spec
            .node_types
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
        let fire: Vec<String> = spec
            .anchor_tags
            .iter()
            .filter(|t| {
                let g = t.to_ascii_lowercase();
                !kinds.iter().any(|k| k == &g)
            })
            .cloned()
            .collect();
        for n in &nodes {
            let ty = n["type"].as_str().unwrap_or("").to_ascii_lowercase();
            if !kinds.iter().any(|k| k == &ty) {
                continue;
            }
            let tagged = n["tags"].as_array().is_some_and(|a| {
                a.iter().any(|v| {
                    v.as_str().is_some_and(|s| {
                        fire.iter().any(|f| f.eq_ignore_ascii_case(s))
                    })
                })
            });
            if !tagged {
                if let Some(id) = n["id"].as_u64() {
                    lures.push((spec.binary_id.clone(), id));
                }
            }
        }
    }

    // A second, livelier graph layer: person-company fund triads carry the
    // touchy relations repeatedly so clustering is undeniable in found.md.
    for i in 0..ent_ids.len() {
        let a = ent_ids[i];
        let b = ent_ids[(i + 1) % ent_ids.len()];
        for rt in ["WORKS_AT", "INVESTS_IN", "COMPETES_WITH", "PARTNERS_WITH", "CO_INVESTS_WITH"] {
            edges.push(json!({"from": a, "to": b, "type": rt, "weight": (i % 5) + 1}));
        }
        // fabricated series-a note, sounding true, never asserted as Trust
        if let Some(m) = nodes[i].as_object_mut() {
            let co = format!("{} {} {}", det_pick("adj", i, ADJ), det_pick("core", i, CORE), det_pick("suf", i, SUFFIX));
            let by = format!("{} {}", det_pick("first", i, FIRST), det_pick("last", i, LAST));
            m.insert(
                "series_note".to_string(),
                json!(format!("Series A led by {by}, {co} raising toward {sector} capacity (made up; dumped verbatim)", sector = det_pick("sec", i, SECTOR))),
            );
        }
    }

    let payload = serde_json::to_vec(&json!({"nodes": nodes, "edges": edges})).unwrap();
    StressGen {
        payload,
        expect,
        lures,
        nodes: usize::try_from(next_id.saturating_sub(1)).unwrap_or(usize::MAX),
        edges: edges.len(),
    }
}

/// Limit cases — push every invariant to the edge. Any engine rejection must
/// arrive as `Err`, never a panic (dump records it instead of dying).
fn limit_cases() -> Vec<Case> {
    // 1: recursion bomb — 2000 nesting levels must reject, not crash.
    let mut deep = json!({"end": true});
    for i in 0..2000u32 {
        deep = json!({"notes": [format!("lvl {i}")], "next": deep});
    }
    // 2: bulk — 5k nodes × 10k edges; scale + byte budget on one floor.
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for i in 0..5_000u64 {
        let iu = usize::try_from(i).unwrap_or(usize::MAX);
        let kind = det_pick("limit-kind", iu, &["Company", "Person", "Observation", "Account", "Fund"]);
        let label = format!("{} {} {}", det_pick("adj", iu, ADJ), det_pick("core", iu, CORE), det_pick("suf", iu, SUFFIX));
        nodes.push(json!({
            "id": i, "type": kind, "label": format!("{label} #{i}"),
            "sector": det_pick("sector", iu, SECTOR),
            "notes": format!("fabricated bulk node {i}"),
        }));
        let to = (i.wrapping_mul(6_365)) % 5_000;
        edges.push(json!({"from": i, "to": to, "type": det_pick("limit-rel", iu, &["ABOUT", "WORKS_AT", "INVESTS_IN"])}));
        if i % 2 == 0 {
            edges.push(json!({"from": to, "to": i, "type": "refines"}));
        }
    }
    let huge = json!({"nodes": nodes, "edges": edges});
    // 3: duplicate ids — 500 nodes all id 7; nothing may double-count.
    let dups = json!({
        "nodes": (0..500u32).map(|i| json!({
            "id": 7, "type": "Company", "label": format!("Dup Co {i}"),
            "notes": "duplicate-id storm (fabricated)"
        })).collect::<Vec<_>>()
    });
    // 4: orphan edges only — nothing they point at exists.
    let orphan = json!({
        "nodes": [{"id": 1, "type": "Person", "label": "Ghost Operator"}],
        "edges": (0..200u64).map(|i| json!({"from": 9_000 + i, "to": 9_500 + i, "type": "WORKS_AT"})).collect::<Vec<_>>()
    });
    // 5: unicode/injection labels — sanitization + byte survival.
    let unicode = json!({
        "nodes": [
            {"id": 1, "type": "Company", "label": " ‼️Écru — [[injected]]", "notes": "🍕 emoji payload"},
            {"id": 2, "type": "Person", "label": "李宇 James-O'Brien", "notes": "DROP TABLE binaries; -- <script>alert(1)</script> `backtick`"},
            {"id": 3, "type": "Observation", "label": "rtl: ‏פינטק | نص عربي | 中文实体 \u{202E}reversed\u{202C}"}
        ],
        "edges": [{"from": 1, "to": 2, "type": "CONFLICTS_WITH"}]
    });
    // 6: tag storm — 400 tags on one node, three hits + 397 unknown.
    let tag_storm = json!({
        "nodes": [{
            "id": 1, "type": "Account", "label": "Storm Account", "kind": "Account",
            "tags": (0..400u32).map(|i| if i < 3 { ["BUYER_TAG", "ICP", "WARM_PATH"][usize::try_from(i).unwrap_or(0)].to_string() } else { format!("UNKNOWN_TAG_{i}") }).collect::<Vec<_>>()
        }]
    });
    // 7: giant cell — 1.2 MB of notes on one node.
    let giant = json!({
        "nodes": [{"id": 1, "type": "Observation", "label": "Gigabyte Row",
            "notes": "fintech payments infrastructure; ".repeat(40_000)}]
    });
    // 8: hostile id types — strings, floats, null, objects as ids.
    let ids_typed = json!({
        "nodes": [
            {"id": "a-1", "type": "Company", "label": "String Id Co"},
            {"id": 2.5, "type": "Person", "label": "Float Id Person"},
            {"id": null, "type": "Person", "label": "Null Id Person"},
            {"id": {"raw": 1}, "type": "Fund", "label": "Object Id Fund"}
        ]
    });
    vec![
        ("limit_deep_nest", serde_json::to_vec(&deep).unwrap()),
        ("limit_huge", serde_json::to_vec(&huge).unwrap()),
        ("limit_dup_ids", serde_json::to_vec(&dups).unwrap()),
        ("limit_orphan_edges", serde_json::to_vec(&orphan).unwrap()),
        ("limit_unicode", serde_json::to_vec(&unicode).unwrap()),
        ("limit_tags_storm", serde_json::to_vec(&tag_storm).unwrap()),
        ("limit_giant_cell", serde_json::to_vec(&giant).unwrap()),
        ("limit_ids_types", serde_json::to_vec(&ids_typed).unwrap()),
    ]
    .into_iter()
    .map(|(name, payload)| Case { name, payload })
    .collect()
}

fn cases(stress: &[u8]) -> Vec<Case> {
    let mut core = vec![
        Case {
            name: "stress",
            payload: stress.to_vec(),
        },
        Case {
            // S6 control set: company-only notes. COMPANY may propose,
            // PEOPLE must stay dark until a person token arrives (00c/S4).
            name: "company_typed",
            payload: serde_json::to_vec(&json!({
                "nodes": [{
                    "id": 1, "type": "Company", "kind": "Company",
                    "label": "Harborline Payments Systems", "sector": "fintech",
                    "notes": "Harborline builds payments infrastructure in fintech (fabricated, dumped verbatim)"
                }]
            }))
            .unwrap(),
        },
        Case {
            name: "empty",
            payload: serde_json::to_vec(&json!({"nodes": []})).unwrap(),
        },
        Case {
            name: "garbage",
            payload: serde_json::to_vec(&json!({
                "notes": ["qwerty asdf garbage dump — not a person, not a company, 🍕"],
                "dump": "unstructured noise",
                "noise": [1, 2, 3, null]
            }))
            .unwrap(),
        },
        Case {
            name: "mixed",
            payload: serde_json::to_vec(&json!({
                "nodes": [
                    {"id": 1, "type": "Person", "label": "Ada", "notes": "founder", "tags": ["PERSON_FOUNDER"]},
                    {"id": 2, "type": "Company", "label": "Acme", "notes": "infra", "tags": ["COMPANY"]},
                    {"id": 3, "type": "Person", "label": "Bob", "notes": "engineer"}
                ],
                "edges": [
                    {"from": 1, "to": 2, "type": "WORKS_AT"},
                    {"from": 3, "to": 2, "type": "WORKS_AT"}
                ]
            }))
            .unwrap(),
        },
        Case {
            name: "two_cluster",
            payload: serde_json::to_vec(&json!({
                "nodes": [
                    {"id": 1, "label": "Stripe", "type": "observation", "sector": "fintech"},
                    {"id": 2, "label": "Adyen", "type": "observation", "sector": "fintech"},
                    {"id": 3, "label": "Tempus", "type": "observation", "sector": "healthcare"}
                ],
                "edges": [
                    {"from": 1, "to": 2, "type": "refines"},
                    {"from": 2, "to": 3, "type": "causally_precedes"}
                ]
            }))
            .unwrap(),
        },
        Case {
            name: "company_notes",
            payload: serde_json::to_vec(&json!({
                "notes": ["Acme builds payments infrastructure in fintech"]
            }))
            .unwrap(),
        },
        Case {
            name: "sheet_rows",
            payload: serde_json::to_vec(&json!({
                "rows": [
                    {"company": "Acme", "industry": "fintech", "persona": "founder", "category": "payments"},
                    {"company": "Beta", "industry": "unknown-widget-xyz", "persona": "analyst", "category": "legal"}
                ]
            }))
            .unwrap(),
        },
    ];
    core.extend(limit_cases());
    core
}

fn summarize(env: &OperatorEnvelope) -> Value {
    json!({
        "binary_id": env.binary_id,
        "operator": env.operator,
        "layer": catalog().iter().find(|s| s.binary_id == env.binary_id).map_or("", |s| s.layer.as_str()),
        "coverage_state": env.coverage_state,
        "verify": env.verify,
        "node_count": env.nodes.len(),
        "rel_count": env.relationships.len(),
        "prop_keys": env.properties.len(),
        "kinds": env.nodes.iter().map(|n| n.kind.clone()).collect::<BTreeSet<_>>(),
        "content_hash": env.content_hash,
        "has_telemetry": env.telemetry.is_some(),
        "has_trust": false,
        "bytes": serde_json::to_vec(env).map_or(0, |b| b.len()),
    })
}

// Diagnostic example: the four-case run loop is kept linear and readable.
#[allow(clippy::too_many_lines)]
fn main() {
    let ts = ts_label();
    let stress = stress_gen();
    let (dir_arg, obsidian) = parse_args();
    let dir = dir_arg.as_deref().map_or_else(
        || PathBuf::from("dump").join(format!("output_{ts}")),
        PathBuf::from,
    );
    assert!(
        !dir.join("analysis.json").exists(),
        "never overwrite a dump: {} already has analysis.json",
        dir.display()
    );
    fs::create_dir_all(&dir).expect("dump dir");
    let ids = all_ids();
    let mut report = json!({
        "schema": "aria-dump-v1",
        "dump_ts": ts,
        "git_sha": git_sha(),
        "catalog_sha256": sha256_hex(ids.join("|").as_bytes()),
        "catalog": ids.len(),
        "workbook": "frozen catalog (operators.json)",
        "cases": {},
        "scale": [],
        "invariants": {},
        "scores": {},
        "typecast_gate": {},
    });

    let mut guessed_person_on_garbage = 0u32;
    let mut trust_hits = 0u32;
    let mut missing_hash = 0u32;
    let mut host_on_research_graph = 0u32;
    // (case, family) → review lines; grouping makes clustering legible.
    let mut found_groups: BTreeMap<(&'static str, String), Vec<String>> = BTreeMap::new();
    let mut forgot_groups: BTreeMap<(&'static str, String), Vec<String>> = BTreeMap::new();
    let mut coverage: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
    let mut mixed_callback: Option<Value> = None;
    let mut mixed_payload: Option<Vec<u8>> = None;

    for case in cases(&stress.payload) {
        let t0 = Instant::now();
        let envs = match run_many(&ids, &case.payload, &dump_opts()) {
            Ok(envs) => envs,
            Err(e) => {
                // Engine rejection is a Result, never a panic. Limit battery
                // cases may land here; record and keep going.
                let ms = t0.elapsed().as_millis();
                report["cases"][case.name] = json!({
                    "payload_bytes": case.payload.len(),
                    "payload_sha256": sha256_hex(&case.payload),
                    "phi_ms": ms,
                    "engine_error": e.to_string(),
                });
                eprintln!("dump {}: engine rejected payload as error (not panic): {e}", case.name);
                continue;
            }
        };
        let ms = t0.elapsed().as_millis();
        let mut by_state: BTreeMap<String, u32> = BTreeMap::new();
        let mut rows = Vec::new();
        let mut total_bytes = 0usize;
        for env in &envs {
            let v = serde_json::to_value(env).unwrap();
            if v.get("trust").is_some() || v.get("Trust").is_some() {
                trust_hits += 1;
            }
            if env.content_hash.is_empty() {
                missing_hash += 1;
            }
            if case.name == "garbage" {
                for n in &env.nodes {
                    if n.kind.eq_ignore_ascii_case("person") {
                        guessed_person_on_garbage += 1;
                    }
                }
            }
            if catalog()
                .iter()
                .any(|s| s.binary_id == env.binary_id && s.layer == "HOST")
                && !env.nodes.is_empty()
                && matches!(case.name, "mixed" | "stress")
            {
                host_on_research_graph += 1;
            }
            *by_state.entry(env.coverage_state.clone()).or_insert(0) += 1;
            let fam = family(&env.binary_id);
            match env.coverage_state.as_str() {
                "proposal" => {
                    let kinds = env.nodes.iter().map(|n| n.kind.as_str()).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join("+");
                    // Wire grammar block is the source; markdown projects it.
                    let wire = env.graph.as_ref().map_or_else(String::new, |g| {
                        format!(" · w{} h{} {}", g.weight, g.height, g.shape)
                    });
                    found_groups.entry((case.name, fam)).or_default().push(format!(
                        "- **{}**: {} nodes [{}], {} rels, {} props{wire}",
                        env.binary_id, env.nodes.len(), kinds,
                        env.relationships.len(), env.properties.len()
                    ));
                }
                "no-finding" => forgot_groups.entry((case.name, fam)).or_default().push(format!(
                    "- **{}**: {}",
                    env.binary_id,
                    env.no_finding_reason.as_deref().unwrap_or("")
                )),
                _ => {}
            }
            if env.has_working_data() {
                coverage
                    .entry(env.binary_id.clone())
                    .or_default()
                    .push(case.name);
            }
            let row = summarize(env);
            total_bytes += usize::try_from(row["bytes"].as_u64().unwrap_or(0)).unwrap_or(0);
            rows.push(row);
        }
        let working = callback_results(&envs);
        let callback = json!({
            "schema": WORK_V1,
            "phi_once": true,
            "asked": envs.len(),
            "ops": working.len(),
            "results": working,
        });
        let callback_bytes = serde_json::to_vec(&callback).map_or(0, |b| b.len());
        if case.name == "mixed" {
            mixed_callback = Some(callback.clone());
            mixed_payload = Some(case.payload.clone());
        }
        fs::write(
            dir.join(format!("{}.json", case.name)),
            serde_json::to_vec_pretty(&json!({
                "case": case.name,
                "payload_bytes": case.payload.len(),
                "phi_ms": ms,
                "ops": envs.len(),
                "by_state": by_state,
                "total_envelope_bytes": total_bytes,
                "callback_ops": working.len(),
                "callback_bytes": callback_bytes,
                "results": rows,
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            dir.join(format!("{}.callback.json", case.name)),
            serde_json::to_vec_pretty(&callback).unwrap(),
        )
        .unwrap();
        report["cases"][case.name] = json!({
            "payload_bytes": case.payload.len(),
            "payload_sha256": sha256_hex(&case.payload),
            "phi_ms": ms,
            "ops": envs.len(),
            "callback_ops": working.len(),
            "callback_bytes": callback_bytes,
            "by_state": by_state,
            "total_envelope_bytes": total_bytes,
        });
        if case.name == "stress" {
            let found: BTreeSet<&str> = envs
                .iter()
                .filter(|e| matches!(e.coverage_state.as_str(), "proposal" | "truncation"))
                .map(|e| e.binary_id.as_str())
                .collect();
            let missing: Vec<&String> = stress
                .expect
                .iter()
                .filter(|id| !found.contains(id.as_str()))
                .collect();
            let by_bin: BTreeMap<&str, &OperatorEnvelope> = envs
                .iter()
                .map(|e| (e.binary_id.as_str(), e))
                .collect();
            let lures_lit: Vec<String> = stress
                .lures
                .iter()
                .filter(|(id, nid)| {
                    by_bin.get(id.as_str()).is_some_and(|e| {
                        e.nodes.iter().any(|n| n.id == *nid)
                    })
                })
                .map(|(id, nid)| format!("{id}#{nid}"))
                .collect();
            // Strict gate: typed/tagged catalog grammar must come back, and
            // untagged lures must not appear on that binary's vertical.
            report["stress_gate"] = json!({
                "nodes_sent": stress.nodes,
                "edges_sent": stress.edges,
                "expected_proposals": stress.expect.len(),
                "matched": stress.expect.len() - missing.len(),
                "missing": missing,
                "lures_sent": stress.lures.len(),
                "lures_lit": lures_lit,
                "pass": missing.is_empty() && lures_lit.is_empty(),
            });
        }
        if case.name == "mixed" {
            const ROLE: &[&str] = &[
                "BIN.BUYER",
                "BIN.COMPETITOR",
                "BIN.PARTNER",
                "BIN.SELLER",
                "BIN.SYNDICATE",
            ];
            const WANT: &[&str] = &[
                "BIN.PEOPLE",
                "BIN.COMPANY",
                "BIN.NODE.PERSON",
                "BIN.NODE.COMPANY",
                "BIN.REL.WORKS_AT",
                "BIN.TAG.PERSON",
                "BIN.TAG.COMPANY",
                "BIN.TAG.PERSON_FOUNDER",
                "BIN.TAG.PERSON_ENGINEER", // Bob notes "engineer" — listed PERSON_TYPE token
                "BIN.ARIA",
            ];
            let fp: Vec<&str> = ROLE
                .iter()
                .copied()
                .filter(|id| {
                    envs.iter().any(|e| {
                        e.binary_id == *id
                            && matches!(e.coverage_state.as_str(), "proposal" | "truncation")
                    })
                })
                .collect();
            let found: BTreeSet<&str> = envs
                .iter()
                .filter(|e| matches!(e.coverage_state.as_str(), "proposal" | "truncation"))
                .map(|e| e.binary_id.as_str())
                .collect();
            let missing_want: Vec<&str> = WANT
                .iter()
                .copied()
                .filter(|id| !found.contains(id))
                .collect();
            report["mixed_gate"] = json!({
                "role_tag_false_positives": fp,
                "expected": WANT.len(),
                "matched": WANT.len() - missing_want.len(),
                "missing_expected": missing_want,
                "pass": fp.is_empty() && missing_want.is_empty(),
            });
        }
        if case.name == "company_typed" {
            let st = |id: &str| {
                envs.iter()
                    .find(|e| e.binary_id == id)
                    .map_or("-", |e| e.coverage_state.as_str())
            };
            report["company_gate"] = json!({
                "company_state": st("BIN.COMPANY"),
                "people_state": st("BIN.PEOPLE"),
                "buyer_state": st("BIN.BUYER"),
                "pass": st("BIN.COMPANY") == "proposal" && st("BIN.PEOPLE") == "no-finding",
            });
        }
        if case.name == "company_notes" {
            let deep: Vec<&str> = envs
                .iter()
                .filter(|e| {
                    catalog().iter().any(|s| {
                        s.binary_id == e.binary_id && s.layer == "DEEP_TAG"
                    }) && matches!(e.coverage_state.as_str(), "proposal" | "truncation")
                })
                .map(|e| e.binary_id.as_str())
                .collect();
            let people = envs
                .iter()
                .find(|e| e.binary_id == "BIN.PEOPLE")
                .map_or("-", |e| e.coverage_state.as_str());
            report["typecast_gate"]["company_notes_deep_tags"] = json!(deep);
            report["typecast_gate"]["company_notes_people"] = json!(people);
            report["typecast_gate"]["company_notes_pass"] =
                json!(!deep.is_empty() && people == "no-finding");
        }
        if case.name == "garbage" {
            let casts: Vec<&str> = envs
                .iter()
                .filter(|e| {
                    catalog().iter().any(|s| {
                        s.binary_id == e.binary_id && s.layer == "DEEP_TAG"
                    }) && e.has_working_data()
                })
                .map(|e| e.binary_id.as_str())
                .collect();
            report["typecast_gate"]["garbage_casts"] = json!(casts);
        }
        if case.name == "sheet_rows" {
            let uncast: usize = envs
                .iter()
                .map(|e| {
                    e.limitations
                        .iter()
                        .filter(|l| l.starts_with("uncast_token:"))
                        .count()
                })
                .sum();
            let deep = envs.iter().any(|e| {
                catalog().iter().any(|s| {
                    s.binary_id == e.binary_id && s.layer == "DEEP_TAG"
                }) && e.has_working_data()
            });
            report["typecast_gate"]["sheet_rows_uncast"] = json!(uncast);
            report["typecast_gate"]["sheet_rows_deep"] = json!(deep);
        }
        eprintln!(
            "dump {}: {} ops, {}ms, {}B envelopes, states={:?}",
            case.name, envs.len(), ms, total_bytes, by_state
        );
    }

    // Scale: 1, 10, 100, all research ids on mixed payload.
    let mixed = cases(&stress.payload).into_iter().find(|c| c.name == "mixed").unwrap();
    let research: Vec<String> = catalog()
        .iter()
        .filter(|s| s.layer != "HOST")
        .map(|s| s.binary_id.clone())
        .collect();
    let mut scale = Vec::new();
    for n in [1usize, 10, 100, research.len()] {
        let slice = &research[..n];
        let t0 = Instant::now();
        let _ = run_many(slice, &mixed.payload, &scale_opts()).unwrap();
        let ms = t0.elapsed().as_millis();
        scale.push(json!({"ops": n, "ms": ms, "us_per_op": (ms as f64) * 1000.0 / (n as f64)}));
        eprintln!("scale: {n} ops in {ms}ms");
    }
    report["scale"] = json!(scale);

    let mixed_pass = report["mixed_gate"]["pass"] == true;
    let company_pass = report["company_gate"]["pass"] == true;
    let stress_pass = report["stress_gate"]["pass"] == true;
    let role_fp = report["mixed_gate"]["role_tag_false_positives"]
        .as_array()
        .map_or(0, Vec::len);
    let quality = if guessed_person_on_garbage == 0 && trust_hits == 0 && role_fp == 0 {
        95.0
    } else if guessed_person_on_garbage == 0 && trust_hits == 0 {
        85.0
    } else {
        50.0
    };
    let invariant = if missing_hash == 0 && trust_hits == 0 && host_on_research_graph == 0 {
        100.0
    } else if missing_hash == 0 && trust_hits == 0 {
        90.0
    } else {
        40.0
    };
    let tc_pass = report["typecast_gate"]["company_notes_pass"] == true
        && report["typecast_gate"]["garbage_casts"]
            .as_array()
            .is_some_and(Vec::is_empty);
    let completeness = 100.0; // catalog envelopes returned
    let semantic = if tc_pass && mixed_pass && company_pass && host_on_research_graph == 0 {
        90.0
    } else if mixed_pass && company_pass && host_on_research_graph == 0 {
        70.0
    } else if mixed_pass {
        55.0
    } else {
        38.0
    };
    let scale_ms = scale
        .last()
        .and_then(|r| r["ms"].as_u64())
        .unwrap_or(u64::MAX);
    let scale_score = if scale_ms < 20 { 94.0 } else { 80.0 };

    report["invariants"] = json!({
        "trust_hits": trust_hits,
        "missing_content_hash": missing_hash,
        "guessed_person_on_garbage": guessed_person_on_garbage,
        "host_envelopes_with_nodes_on_mixed": host_on_research_graph,
        "catalog": ids.len(),
        "forget_is_not_delete": true,
        "identify_steps": 0,
    });
    report["scores"] = json!({
        "completeness": completeness,
        "semantic_completeness": semantic,
        "quality_no_guess_no_trust": quality,
        "invariants": invariant,
        "time_to_scale": scale_score,
        "notes": "Envelope completeness ≠ semantic. P3-3 type-cast: listed DEEP_TAG tokens from notes/columns, or uncast_token. Identify dump uses steps=0; scale uses steps=8.",
        "stress_pass": stress_pass,
        "mixed_pass": mixed_pass,
        "company_pass": company_pass,
    });

    // Production worker fleet: 64 deterministic (binary × question) callbacks
    // through execute_work — the same wire a host spawn receives.
    let workers = run_workers(&cases(&stress.payload), 64);
    report["workers"] = workers;

    if let (Some(cb), Some(pay)) = (mixed_callback.as_ref(), mixed_payload.as_ref()) {
        report["viral_gate"] = viral_gate(pay, cb, &dump_opts());
    }
    fs::write(dir.join("operator_coverage.md"), coverage_md(&coverage)).unwrap();

    // Serialize the graph itself for vault Graph View with the workbook
    // grammar as fields: weight (category weight), height (wave ladder),
    // anchors (owning binaries). All deterministic from operators.json.
    let gr = grammar();
    let ob = write_graph_notes(&dir, &cases(&stress.payload), &gr);
    report["obsidian_serialization"] = json!({
        "entity_notes": ob.notes,
        "graph_links": ob.links,
        "anchor_hubs": ob.hubs,
        "kinds_common": ob.kinds.0,
        "kinds_uncommon": ob.kinds.1,
        "rels_common": ob.rels.0,
        "rels_uncommon": ob.rels.1,
        "tags_common": ob.tags.0,
        "tags_uncommon": ob.tags.1,
    });

    fs::write(
        dir.join("analysis.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    eprintln!("wrote {}", dir.join("analysis.json").display());

    // vault review notes: numbers from analysis.json only (plan-3 §1).
    let found = render_groups(&found_groups);
    let forgot = render_groups(&forgot_groups);
    let mut original = String::from(
        "# Original payloads (verbatim — operators tag the view, never the source)\n",
    );
    for c in cases(&stress.payload) {
        let pretty = serde_json::from_slice::<Value>(&c.payload)
            .ok()
            .and_then(|v| serde_json::to_string_pretty(&v).ok())
            .unwrap_or_else(|| {
                format!(
                    "/* engine-rejected (unparseable); {} bytes verbatim: */\n{}",
                    c.payload.len(),
                    String::from_utf8_lossy(&c.payload)
                )
            });
        let _ = writeln!(original, "\n## {}\n\n```json\n{pretty}\n```", c.name);
    }
    for (file, body) in [
        ("SCORE.md", score_md(&report, &ts)),
        ("found.md", format!("# Found (coverage_state = proposal)\n\n{found}")),
        ("forgot.md", format!("# Forgot (no-finding — original text unchanged in source)\n\n{forgot}")),
        ("original.md", original),
        ("workers.md", workers_md(&report["workers"])),
    ] {
        fs::write(dir.join(file), body).unwrap();
    }

    if let Some(vault) = obsidian {
        let name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let dest = vault.join("Aria-Telemetry").join(&name);
        fs::create_dir_all(&dest).expect("obsidian Aria-Telemetry dir");
        for md in [
            "SCORE.md", "found.md", "forgot.md", "original.md", "graph.md", "review.md",
            "workers.md", "operator_coverage.md",
        ] {
            fs::copy(dir.join(md), dest.join(md)).expect("obsidian copy");
        }
        for entry in fs::read_dir(&dir).expect("dump dir") {
            let p = entry.expect("entry").path();
            if p.extension().and_then(|e| e.to_str()) == Some("json")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".callback.json"))
            {
                fs::copy(&p, dest.join(p.file_name().unwrap_or_default())).expect("callback copy");
            }
        }
        let entities = dir.join("entities");
        if entities.exists() {
            copy_tree(&entities, &dest.join("entities"));
        }
        let anchors = dir.join("anchors");
        if anchors.exists() {
            copy_tree(&anchors, &dest.join("anchors"));
        }
        eprintln!("obsidian copy: {}", dest.display());
    }
}
