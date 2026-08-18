//! Deterministic vector index — ℙ5 `O(log |V|)` metric retrieval, ℂ3.
//!
//! [`VectorIndex`] is the seam the plan (WS3 item 3) puts between the Match
//! policy and whatever structure answers "which existing node is nearest to
//! this latent?". The index is **policy layer**: it proposes candidates, it
//! never decides admissibility. A merge only happens after the caller
//! re-verifies the candidate distance against τ in the same compensated f64
//! arithmetic the rest of the engine uses, so an approximate (or even wrong)
//! candidate can change *which* admissible edit is proposed, never *whether*
//! the result satisfies Inv3.
//!
//! # Why this is implemented in-repo (WS3 §0 decision gate)
//!
//! Both third-party candidates the plan named were audited and rejected on
//! measured evidence (CHANGELOG 2026-08-13 INSIGHT):
//!
//! - `instant-distance` 0.6.1 — does not compile for
//!   `wasm32-unknown-unknown` (`getrandom` 0.2 `compile_error!`; the crate
//!   links it unconditionally because its default seed path is
//!   `rand::random()`), and its public API has **no** incremental `insert`:
//!   a Match-per-step workload must rebuild, measured at 85.99 s for the
//!   first 1024 inserts alone.
//! - `hnsw_rs` 0.3.4 — has incremental `insert` but fails the same wasm32
//!   wall (`getrandom` 0.3) and additionally pulls `mmap-rs`/`libc`.
//!
//! The only wasm escape for either is `getrandom`'s JS backend, i.e. OS/JS
//! entropy, which the repo forbids (every backend is seeded explicitly so the
//! same code runs on `wasm32` and so runs are reproducible). `usearch` stays
//! wired as the spec-normative §5.3 engine behind a native-only feature.
//!
//! [`HnswIndex`] therefore implements Malkov & Yashunin's Hierarchical
//! Navigable Small World graph (arXiv:1603.09320 — bibliography
//! `graphs-partitioning-retrieval.md`) directly, with three properties the
//! crates could not offer together:
//!
//! 1. **`f64` end-to-end.** Both crates compute distances in `f32`. Aria's
//!    embeddings, the τ merge threshold and Inv3 are `f64`; ranking candidates
//!    in a narrower arithmetic than the decision consuming them is avoidable
//!    imprecision.
//! 2. **Deterministic by construction.** Single-threaded insertion (no
//!    work-stealing), the repo's seeded MMIX LCG for level draws, the pure-Rust
//!    `libm` logarithm (WS2's parity rule — the host libm differs between
//!    execution environments by ulps), and a *total* order on candidates
//!    `(distance, internal index)` so no tie is ever broken by hash iteration
//!    order.
//! 3. **`wasm32`-clean.** No entropy, no threads, no `libc`.
//!
//! # Parameters
//!
//! Defaults mirror the spec §5.3 normative `usearch` configuration:
//! `connectivity = 16`, `expansion_add = 128`, `expansion_search = 64`.

use std::cell::RefCell;
use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap};

// Per-thread query visited buffer — the `&self` counterpart of
// `HnswIndex::insert_visited`.
//
// `nearest` / `nearest_probed` cannot mutate `insert_visited` (they take
// `&self`) and cannot take a `Mutex` (the engine is single-threaded; a lock
// is wasted work and a poison path). A *fresh* `Visited` per query would
// `resize` a `u32` stamp of length `|V|` — 4 MB and a memset at `|V| = 10⁶`,
// measured ~200 µs, which is the entire ℙ5 budget on bookkeeping. The
// thread-local lives as long as the thread, grows once, and resets in O(1)
// via the epoch stamp. No `unsafe`, no OS entropy, wasm32-safe (one thread).
thread_local! {
    static QUERY_VISITED: RefCell<Visited> = const { RefCell::new(Visited::new()) };
}

/// Candidate retrieval over `f64` embeddings (plan WS3 item 3).
///
/// # Contract
///
/// `add` requires `v.len() == dim` and all components finite — guaranteed by
/// the caller's Inv3/Inv4 checks before any graph op is committed. Violations
/// are logged and ignored rather than panicking, because an index desync is a
/// policy-quality problem, never a safety one.
pub trait VectorIndex: std::fmt::Debug + Send + Sync {
    /// Insert or resurrect `id` with embedding `v`.
    fn add(&mut self, id: u64, v: &[f64]);
    /// Tombstone `id`. Idempotent; unknown ids are a no-op (ℙ3 idempotency).
    fn remove(&mut self, id: u64);
    /// The `k` nearest live ids to `v`, ascending by distance.
    fn nearest(&self, v: &[f64], k: usize) -> Vec<(u64, f64)>;
}

/// HNSW construction and search parameters (spec §5.3 normative values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HnswParams {
    /// `M` — neighbours kept per node per layer above zero.
    pub connectivity: usize,
    /// `ef_construction` — candidate width while inserting.
    pub expansion_add: usize,
    /// `ef_search` — candidate width while querying.
    pub expansion_search: usize,
    /// Seed for the level-draw LCG. No OS entropy (repo lock).
    pub seed: u64,
}

impl Default for HnswParams {
    fn default() -> Self {
        Self {
            connectivity: 16,
            expansion_add: 128,
            expansion_search: 64,
            seed: LEVEL_SEED,
        }
    }
}

/// Seed for the level-draw LCG — golden-ratio bits, matching the convention
/// [`crate::spectral::START_VECTOR_SEED`] established in WS1.
pub const LEVEL_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// Hard cap on layer count. `P(level ≥ 8) ≈ 2·10⁻¹⁰` at `M = 16`, so 16 is
/// unreachable in practice and bounds per-node memory regardless of seed.
const MAX_LEVEL: usize = 16;

/// One indexed point.
///
/// Vectors are stored in a flat `Vec<f64>` arena (`HnswIndex::vectors`) at
/// stride `dim`, not per-node — Structure of Arrays.  At `|V| = 10⁶` and
/// `dim = 64` that arena is 512 MB in one contiguous block rather than 10⁶
/// scattered 512-byte heap allocations, which removes one pointer chase (one
/// cache miss) from every distance evaluation in the search hot path.
#[derive(Debug, Clone)]
struct Node {
    id: u64,
    /// Offset into `vectors` — `vectors[offset..offset+dim]`.
    vec_off: usize,
    /// `links[l]` — neighbours at layer `l`; `links.len()` is the node's level + 1.
    links: Vec<Vec<u32>>,
    /// Tombstoned nodes still route searches but never appear in results.
    deleted: bool,
}

/// A `(distance², internal index)` pair with a **total** order.
///
/// `f64::total_cmp` gives a total order even across `-0.0`/NaN, and the index
/// tiebreak makes every heap pop and every sort deterministic — the property
/// the third-party crates could not guarantee across thread counts.
#[derive(Debug, Clone, Copy)]
struct Cand {
    d2: f64,
    idx: u32,
}

impl Ord for Cand {
    fn cmp(&self, other: &Self) -> Ordering {
        self.d2.total_cmp(&other.d2).then(self.idx.cmp(&other.idx))
    }
}
impl PartialOrd for Cand {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Cand {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Cand {}

/// Epoch-stamped visited set: `O(1)` reset, zero per-query allocation once warm.
///
/// A fresh `Vec<u32>` per query would memset 4 MB at `|V| = 10⁶` (~200 µs) and
/// blow the ℙ5 latency gate on bookkeeping alone; a `HashSet` would pay SipHash
/// on every one of the ~10³ visits.  The buffer is passed by `&mut` into
/// `search_layer` — no `Mutex`: the engine is single-threaded, and the lock
/// cost (~80 ns per acquire) was measurable at `|V| = 10⁶`.
#[derive(Debug, Default)]
struct Visited {
    stamp: Vec<u32>,
    epoch: u32,
}

impl Visited {
    const fn new() -> Self {
        Self {
            stamp: Vec::new(),
            epoch: 0,
        }
    }

    fn begin(&mut self, len: usize) {
        if self.stamp.len() < len {
            self.stamp.resize(len, 0);
        }
        // Wrapping the epoch would alias stale stamps; clear on the rare wrap.
        if self.epoch == u32::MAX {
            self.stamp.iter_mut().for_each(|s| *s = 0);
            self.epoch = 0;
        }
        self.epoch += 1;
    }

    /// `true` the first time `i` is seen this epoch.
    fn visit(&mut self, i: u32) -> bool {
        let slot = &mut self.stamp[i as usize];
        if *slot == self.epoch {
            return false;
        }
        *slot = self.epoch;
        true
    }
}

/// Deterministic single-threaded HNSW over `f64` embeddings.
#[derive(Debug)]
pub struct HnswIndex {
    dim: usize,
    params: HnswParams,
    nodes: Vec<Node>,
    /// Flat SoA vector arena: `vectors[i*dim .. (i+1)*dim]` is node `i`'s
    /// embedding.  One contiguous block, not 10⁶ heap allocations.
    vectors: Vec<f64>,
    by_id: HashMap<u64, u32>,
    entry: Option<u32>,
    live: usize,
    rng: u64,
    /// Reused across `add` calls (`&mut self`) so construction does not
    /// allocate a stamp buffer per inserted node. Query search uses
    /// [`QUERY_VISITED`] instead — `nearest` is `&self`.
    insert_visited: Visited,
}

/// What a query actually touched — the measurable form of the ℙ5 claim.
///
/// Wall-clock latency depends on the machine; `visited` is the algorithmic
/// quantity that must grow like `log |V|`, so tests assert on it directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NearestStats {
    /// Distinct nodes whose distance was evaluated.
    pub visited: usize,
    /// Layers descended, including layer zero.
    pub layers: usize,
}

impl HnswIndex {
    /// Empty index for `dim`-dimensional embeddings with spec §5.3 defaults.
    pub fn new(dim: usize) -> Self {
        Self::with_params(dim, HnswParams::default())
    }

    /// Empty index with explicit parameters.
    pub fn with_params(dim: usize, params: HnswParams) -> Self {
        let params = HnswParams {
            connectivity: params.connectivity.max(2),
            expansion_add: params.expansion_add.max(params.connectivity.max(2)),
            expansion_search: params.expansion_search.max(1),
            seed: params.seed,
        };
        Self {
            dim,
            params,
            nodes: Vec::new(),
            vectors: Vec::new(),
            by_id: HashMap::new(),
            entry: None,
            live: 0,
            rng: params.seed,
            insert_visited: Visited::default(),
        }
    }

    /// Embedding dimension.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Live (non-tombstoned) point count.
    pub fn len(&self) -> usize {
        self.live
    }

    /// Whether the index holds no live points.
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Total arena size including tombstones — the memory-side counterpart of
    /// [`Self::len`], and what a compaction pass would reclaim.
    pub fn arena_len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether `id` is present and live.
    pub fn contains(&self, id: u64) -> bool {
        self.by_id
            .get(&id)
            .is_some_and(|&i| !self.nodes[i as usize].deleted)
    }

    /// Neighbour capacity at `layer` — `2M` at layer zero, `M` above (the
    /// asymmetry in the HNSW paper: the base layer carries the recall).
    fn capacity(&self, layer: usize) -> usize {
        if layer == 0 {
            self.params.connectivity * 2
        } else {
            self.params.connectivity
        }
    }

    /// `level = ⌊−ln(u)·mL⌋`, `mL = 1/ln(M)` — the paper's exponentially
    /// decaying layer assignment, drawn from the repo's MMIX LCG.
    ///
    /// `libm::log` (pure Rust, compiled into this rlib) rather than `f64::ln`
    /// (host libm) so the level sequence is bit-identical on every surface —
    /// WS2 measured a 1-ulp host-libm divergence between the CLI and the
    /// Python extension, which here would change the graph's shape.
    fn draw_level(&mut self) -> usize {
        self.rng = self
            .rng
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // u ∈ (0, 1]: 53-bit mantissa, shifted off zero so ln(u) is finite.
        let u = (((self.rng >> 11) as f64) + 1.0) / ((1u64 << 53) as f64 + 1.0);
        let ml = 1.0 / libm::log(self.params.connectivity as f64);
        // Clamped *before* the cast: `u ∈ (0,1]` ⇒ `−ln u ≥ 0` and finite, and
        // `min` caps it at MAX_LEVEL, so truncation is exact and sign loss is
        // unreachable — the cast is precisely the paper's floor.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let level = (-libm::log(u) * ml).min(MAX_LEVEL as f64) as usize;
        level
    }

    /// Squared Euclidean distance — monotone with the metric, so ranking is
    /// identical while every comparison avoids a `sqrt`.
    ///
    /// A plain loop (not `zip`/`map`/`sum`) so LLVM sees a tight f64 reduction
    /// on the SoA slices. Inlined: this is the inner body of every visit.
    /// Branch-free on purpose: a per-element cap check de-vectorizes this
    /// loop and was measured *slower* (100k p99 233 → 283 µs).
    #[inline]
    fn dist2(a: &[f64], b: &[f64]) -> f64 {
        let mut s = 0.0;
        for (x, y) in a.iter().zip(b) {
            let d = x - y;
            s += d * d;
        }
        s
    }

    /// Borrow node `idx`'s embedding from the flat arena — one slice, no copy.
    #[inline]
    fn vec_of(&self, idx: u32) -> &[f64] {
        let off = self.nodes[idx as usize].vec_off;
        &self.vectors[off..off + self.dim]
    }

    /// Issue independent loads of one scalar per cache line so the M2's
    /// miss-status registers fill the rest of the vector while we score
    /// the previous neighbour. `unsafe_code = forbid` rules out
    /// `_mm_prefetch` / `prefetch`; a plain load is the safe equivalent.
    ///
    /// 128 B lines (Apple) ⇒ stride 16 f64; 64 B lines (x86) still get
    /// every other line, which is enough to overlap the gather.
    #[inline]
    fn touch_vec(&self, idx: u32) -> f64 {
        let v = self.vec_of(idx);
        let mut tap = 0.0;
        let mut i = 0;
        while i < v.len() {
            tap += v[i];
            i += 16;
        }
        tap
    }

    /// Top layer currently present in the index.
    fn top_layer(&self) -> usize {
        self.entry
            .map_or(0, |e| self.nodes[e as usize].links.len() - 1)
    }

    /// Greedy single-best descent at `layer` (the `ef = 1` case).
    fn greedy(&self, q: &[f64], mut cur: u32, layer: usize) -> u32 {
        let mut cur_d = Self::dist2(q, self.vec_of(cur));
        loop {
            let mut improved = false;
            let Some(links) = self.nodes[cur as usize].links.get(layer) else {
                return cur;
            };
            // Scanned in stored order; ties resolved by strict `<` plus the
            // index tiebreak below, so the walk is deterministic.
            for &n in links {
                let d = Self::dist2(q, self.vec_of(n));
                // Same total order as `Cand::cmp`: distance first, then the
                // internal index, so the walk never depends on scan order.
                let better = match d.total_cmp(&cur_d) {
                    Ordering::Less => true,
                    Ordering::Equal => n < cur,
                    Ordering::Greater => false,
                };
                if better {
                    cur_d = d;
                    cur = n;
                    improved = true;
                }
            }
            if !improved {
                return cur;
            }
        }
    }

    /// The paper's `SEARCH-LAYER`: best-first expansion bounded to `ef`.
    ///
    /// Returns candidates ascending by `(distance², index)`.
    fn search_layer(
        &self,
        q: &[f64],
        entries: &[u32],
        ef: usize,
        layer: usize,
        vis: &mut Visited,
        visited_count: &mut usize,
    ) -> Vec<Cand> {
        let mut frontier: BinaryHeap<Reverse<Cand>> = BinaryHeap::new();
        let mut best: BinaryHeap<Cand> = BinaryHeap::new();

        for &e in entries {
            if !vis.visit(e) {
                continue;
            }
            *visited_count += 1;
            let c = Cand {
                d2: Self::dist2(q, self.vec_of(e)),
                idx: e,
            };
            frontier.push(Reverse(c));
            best.push(c);
        }
        while best.len() > ef {
            best.pop();
        }

        while let Some(Reverse(cur)) = frontier.pop() {
            // Stop once the nearest unexpanded candidate is worse than the
            // worst kept result and the result set is already full.
            if best.len() >= ef && best.peek().is_some_and(|w| cur.d2 > w.d2) {
                break;
            }
            let Some(links) = self.nodes[cur.idx as usize].links.get(layer) else {
                continue;
            };
            // Two-pass over this node's neighbours: first issue a tap load
            // per cache line of every fresh neighbour (independent misses),
            // then score. Same visit-set, same distances, more MLP.
            // Stack buffer: layer-0 cap is 2M = 32, so 64 is a hard ceiling
            // for a well-formed index (no heap traffic on the query path).
            let mut fresh = [0u32; 64];
            let mut taps = [0.0f64; 64];
            let mut n_fresh = 0usize;
            for &n in links {
                if vis.visit(n) && n_fresh < fresh.len() {
                    // Independent stores — a summed tap would serialize the
                    // misses on the accumulator and defeat MLP.
                    taps[n_fresh] = self.touch_vec(n);
                    fresh[n_fresh] = n;
                    n_fresh += 1;
                }
            }
            std::hint::black_box(taps);
            for &n in &fresh[..n_fresh] {
                *visited_count += 1;
                let c = Cand {
                    d2: Self::dist2(q, self.vec_of(n)),
                    idx: n,
                };
                let worst = best.peek().map_or(f64::INFINITY, |w| w.d2);
                if best.len() < ef || c.d2 < worst {
                    frontier.push(Reverse(c));
                    best.push(c);
                    if best.len() > ef {
                        best.pop();
                    }
                }
            }
        }

        let mut out = best.into_vec();
        out.sort_unstable();
        out
    }

    /// Keep the `capacity` nearest neighbours of `owner`, deterministically.
    ///
    /// The paper's simple heuristic (nearest-first). `select_neighbors_heuristic`
    /// would improve recall on clustered data; it is deliberately not used here
    /// because nearest-first is order-independent, which is what makes the
    /// index reproducible.
    fn prune(&mut self, owner: u32, layer: usize) {
        let cap = self.capacity(layer);
        if self.nodes[owner as usize].links[layer].len() <= cap {
            return;
        }
        // Borrow the owner's vector from the flat arena instead of cloning it
        // — the clone was 512 bytes per call and `prune` is called once per
        // new neighbour per insert, so at M = 16 that is ~8 KB of memcpy per
        // node, ~8 GB across a 10⁶ build.
        let owner_off = self.nodes[owner as usize].vec_off;
        // Split the borrow: immutable `vectors` for the owner, mutable `nodes`
        // for the link list being truncated.  The two are disjoint fields so
        // this borrows cleanly without `clone`.
        let owner_vec = &self.vectors[owner_off..owner_off + self.dim];
        let links = std::mem::take(&mut self.nodes[owner as usize].links[layer]);
        let mut scored: Vec<Cand> = links
            .iter()
            .map(|&n| {
                let off = self.nodes[n as usize].vec_off;
                Cand {
                    d2: Self::dist2(owner_vec, &self.vectors[off..off + self.dim]),
                    idx: n,
                }
            })
            .collect();
        scored.sort_unstable();
        scored.truncate(cap);
        self.nodes[owner as usize].links[layer] = scored.into_iter().map(|c| c.idx).collect();
    }

    /// Symmetric link insertion, skipping duplicates (ℙ3 idempotency).
    fn connect(&mut self, a: u32, b: u32, layer: usize) {
        if a == b {
            return;
        }
        if let Some(links) = self.nodes[a as usize].links.get_mut(layer) {
            if !links.contains(&b) {
                links.push(b);
            }
        }
        if let Some(links) = self.nodes[b as usize].links.get_mut(layer) {
            if !links.contains(&a) {
                links.push(a);
            }
        }
    }

    /// [`VectorIndex::nearest`] plus the traversal statistics.
    pub fn nearest_probed(&self, q: &[f64], k: usize) -> (Vec<(u64, f64)>, NearestStats) {
        let mut stats = NearestStats {
            visited: 0,
            layers: 0,
        };
        if k == 0 || self.live == 0 || q.len() != self.dim {
            return (Vec::new(), stats);
        }
        let Some(entry) = self.entry else {
            return (Vec::new(), stats);
        };

        let top = self.top_layer();
        let mut cur = entry;
        for layer in (1..=top).rev() {
            cur = self.greedy(q, cur, layer);
            stats.layers += 1;
        }
        stats.layers += 1;

        // Widen `ef` to at least `k`, and enough beyond it that tombstones
        // cannot starve the result set.
        let ef = self.params.expansion_search.max(k).min(self.nodes.len());
        let out = QUERY_VISITED.with(|cell| {
            let mut vis = cell.borrow_mut();
            vis.begin(self.nodes.len());
            let found = self.search_layer(q, &[cur], ef, 0, &mut vis, &mut stats.visited);
            found
                .into_iter()
                .filter(|c| !self.nodes[c.idx as usize].deleted)
                .take(k)
                .map(|c| (self.nodes[c.idx as usize].id, c.d2.sqrt()))
                .collect()
        });
        (out, stats)
    }
}

impl VectorIndex for HnswIndex {
    fn add(&mut self, id: u64, v: &[f64]) {
        if v.len() != self.dim || v.iter().any(|x| !x.is_finite()) {
            // Unreachable through the engine: Inv3/Inv4 validate embeddings
            // before an op is committed. Logged, never a panic — see the trait
            // contract.
            log::warn!(
                "VectorIndex::add({id}) rejected: expected {} finite components, got {}",
                self.dim,
                v.len()
            );
            return;
        }

        // Resurrect-or-update: keeps `add` idempotent and makes journal undo of
        // a DeleteNode/MergeNodes exact — the arena slot, its links and its
        // level are the pre-image, so restoring is not an approximation.
        if let Some(&existing) = self.by_id.get(&id) {
            let ei = existing as usize;
            if self.nodes[ei].deleted {
                self.nodes[ei].deleted = false;
                self.live += 1;
            }
            let off = self.nodes[ei].vec_off;
            let dest = &mut self.vectors[off..off + self.dim];
            if dest != v {
                dest.copy_from_slice(v);
            }
            return;
        }

        let level = self.draw_level();
        let idx = u32::try_from(self.nodes.len()).expect("index arena exceeds u32 (|V| > 4·10⁹)");
        let vec_off = self.vectors.len();
        self.vectors.extend_from_slice(v);
        self.nodes.push(Node {
            id,
            vec_off,
            links: vec![Vec::new(); level + 1],
            deleted: false,
        });
        self.by_id.insert(id, idx);
        self.live += 1;

        let Some(entry) = self.entry else {
            self.entry = Some(idx);
            return;
        };

        let top = self.top_layer();
        let mut cur = entry;
        // Descend the layers above the new node's level with ef = 1.
        for layer in ((level + 1)..=top).rev() {
            cur = self.greedy(v, cur, layer);
        }

        // `search_layer` needs `&self` and `&mut Visited`. `insert_visited` is
        // a field of `self`, so we take it out for the layer walk and put it
        // back — no Mutex, no overlapping borrow.
        let mut vis = std::mem::take(&mut self.insert_visited);
        let mut entries = vec![cur];
        let mut ignored = 0usize;
        for layer in (0..=level.min(top)).rev() {
            vis.begin(self.nodes.len());
            let found = self.search_layer(
                v,
                &entries,
                self.params.expansion_add,
                layer,
                &mut vis,
                &mut ignored,
            );

            let take = self.capacity(layer);
            let chosen: Vec<u32> = found
                .iter()
                .filter(|c| c.idx != idx)
                .take(take)
                .map(|c| c.idx)
                .collect();
            for &n in &chosen {
                self.connect(idx, n, layer);
                self.prune(n, layer);
            }
            self.prune(idx, layer);

            entries = if chosen.is_empty() { vec![cur] } else { chosen };
        }
        self.insert_visited = vis;

        if level > top {
            self.entry = Some(idx);
        }
    }

    fn remove(&mut self, id: u64) {
        let Some(&idx) = self.by_id.get(&id) else {
            return;
        };
        let node = &mut self.nodes[idx as usize];
        if node.deleted {
            return;
        }
        node.deleted = true;
        self.live -= 1;

        // Links are kept: a tombstone still routes searches (removing it from
        // its neighbours' lists would disconnect the graph and silently cost
        // recall). Results filter tombstones instead.
        if self.entry == Some(idx) {
            // Re-seat the entry point on the highest-level live node, else the
            // whole index becomes unreachable once the entry dies.
            self.entry = self
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| !n.deleted)
                .max_by_key(|(i, n)| (n.links.len(), std::cmp::Reverse(*i)))
                .map(|(i, _)| u32::try_from(i).expect("arena fits u32"))
                .or(Some(idx));
        }
    }

    fn nearest(&self, v: &[f64], k: usize) -> Vec<(u64, f64)> {
        self.nearest_probed(v, k).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seeded point generator — the repo's MMIX LCG, no OS entropy.
    fn points(n: usize, dim: usize, seed: u64) -> Vec<Vec<f64>> {
        let mut x = seed;
        let mut next = || {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((x >> 11) as f64) / ((1u64 << 53) as f64)
        };
        (0..n).map(|_| (0..dim).map(|_| next()).collect()).collect()
    }

    fn brute_force(pts: &[Vec<f64>], q: &[f64], k: usize) -> Vec<u64> {
        let mut scored: Vec<(f64, u64)> = pts
            .iter()
            .enumerate()
            .map(|(i, p)| (HnswIndex::dist2(p, q), i as u64))
            .collect();
        scored.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        scored.into_iter().take(k).map(|(_, i)| i).collect()
    }

    fn build(pts: &[Vec<f64>], dim: usize) -> HnswIndex {
        let mut ix = HnswIndex::new(dim);
        for (i, p) in pts.iter().enumerate() {
            ix.add(i as u64, p);
        }
        ix
    }

    #[test]
    fn recall_matches_brute_force() {
        let (n, dim, k) = (2000, 32, 10);
        let pts = points(n, dim, 42);
        let ix = build(&pts, dim);
        assert_eq!(ix.len(), n);

        let queries = points(100, dim, 7);
        let mut hits = 0usize;
        let mut top1 = 0usize;
        for q in &queries {
            let want = brute_force(&pts, q, k);
            let got: Vec<u64> = ix.nearest(q, k).into_iter().map(|(id, _)| id).collect();
            if got.first() == want.first() {
                top1 += 1;
            }
            hits += got.iter().filter(|g| want.contains(g)).count();
        }
        let recall = hits as f64 / (queries.len() * k) as f64;
        let top1_recall = top1 as f64 / queries.len() as f64;
        assert!(
            recall >= 0.95,
            "top-{k} recall {recall:.4} < 0.95 — the index is not finding true neighbours"
        );
        assert!(top1_recall >= 0.95, "top-1 recall {top1_recall:.4} < 0.95");
    }

    #[test]
    fn distances_are_exact_and_ascending() {
        let dim = 8;
        let pts = points(200, dim, 3);
        let ix = build(&pts, dim);
        let q = &points(1, dim, 11)[0];
        let got = ix.nearest(q, 5);
        for w in got.windows(2) {
            assert!(w[0].1 <= w[1].1, "results not ascending: {got:?}");
        }
        for (id, d) in got {
            let exact = HnswIndex::dist2(&pts[usize::try_from(id).unwrap()], q).sqrt();
            assert_eq!(
                d.to_bits(),
                exact.to_bits(),
                "reported distance for {id} is not the exact f64 metric"
            );
        }
    }

    #[test]
    fn identical_construction_is_bit_identical() {
        let dim = 16;
        let pts = points(500, dim, 5);
        let queries = points(20, dim, 6);
        let a = build(&pts, dim);
        let b = build(&pts, dim);
        for q in &queries {
            let (ra, sa) = a.nearest_probed(q, 10);
            let (rb, sb) = b.nearest_probed(q, 10);
            assert_eq!(sa, sb, "traversal statistics diverged");
            assert_eq!(ra.len(), rb.len());
            for (x, y) in ra.iter().zip(&rb) {
                assert_eq!(x.0, y.0, "id order diverged");
                assert_eq!(x.1.to_bits(), y.1.to_bits(), "distance bits diverged");
            }
        }
    }

    #[test]
    fn level_sequence_is_seeded_and_reproducible() {
        let mut a = HnswIndex::new(4);
        let mut b = HnswIndex::new(4);
        let seq_a: Vec<usize> = (0..64).map(|_| a.draw_level()).collect();
        let seq_b: Vec<usize> = (0..64).map(|_| b.draw_level()).collect();
        assert_eq!(seq_a, seq_b, "level draws are not reproducible");
        assert!(
            seq_a.iter().all(|&l| l <= MAX_LEVEL),
            "level exceeded MAX_LEVEL: {seq_a:?}"
        );
        assert!(
            seq_a.contains(&0),
            "degenerate level distribution: {seq_a:?}"
        );
        // A different seed must produce a different stream.
        let mut c = HnswIndex::with_params(
            4,
            HnswParams {
                seed: 12345,
                ..HnswParams::default()
            },
        );
        let seq_c: Vec<usize> = (0..64).map(|_| c.draw_level()).collect();
        assert_ne!(seq_a, seq_c, "seed had no effect on the level stream");
    }

    #[test]
    fn tombstones_are_excluded_and_restore_is_exact() {
        let dim = 12;
        let pts = points(300, dim, 21);
        let queries = points(25, dim, 22);
        let mut ix = build(&pts, dim);

        let before: Vec<Vec<(u64, f64)>> = queries.iter().map(|q| ix.nearest(q, 8)).collect();

        let victims: Vec<u64> = (0..300u64).step_by(30).collect();
        for &v in &victims {
            ix.remove(v);
        }
        assert_eq!(ix.len(), 300 - victims.len());
        for q in &queries {
            for (id, _) in ix.nearest(q, 8) {
                assert!(
                    !victims.contains(&id),
                    "tombstoned {id} appeared in results"
                );
            }
        }

        // Undo: re-adding the same id with the same vector must restore the
        // exact pre-image, because the journal replays it that way.
        for &v in &victims {
            ix.add(v, &pts[usize::try_from(v).unwrap()]);
        }
        assert_eq!(ix.len(), 300);
        let after: Vec<Vec<(u64, f64)>> = queries.iter().map(|q| ix.nearest(q, 8)).collect();
        assert_eq!(before.len(), after.len());
        for (b, a) in before.iter().zip(&after) {
            assert_eq!(b.len(), a.len(), "result cardinality changed after restore");
            for (x, y) in b.iter().zip(a) {
                assert_eq!(x.0, y.0, "restore changed neighbour ids");
                assert_eq!(x.1.to_bits(), y.1.to_bits(), "restore changed distances");
            }
        }
    }

    #[test]
    fn remove_is_idempotent_and_unknown_ids_are_noops() {
        let dim = 4;
        let pts = points(10, dim, 1);
        let mut ix = build(&pts, dim);
        ix.remove(3);
        ix.remove(3);
        ix.remove(9999);
        assert_eq!(ix.len(), 9);
        assert!(!ix.contains(3));
        assert!(ix.contains(4));
    }

    #[test]
    fn removing_the_entry_point_keeps_the_index_searchable() {
        let dim = 6;
        let pts = points(150, dim, 31);
        let mut ix = build(&pts, dim);
        // Tombstone whatever the entry currently is, repeatedly.
        for _ in 0..5 {
            let entry_id = ix.nodes[ix.entry.unwrap() as usize].id;
            ix.remove(entry_id);
            let q = &points(1, dim, 32)[0];
            assert!(
                !ix.nearest(q, 5).is_empty(),
                "index became unsearchable after removing the entry point"
            );
        }
    }

    #[test]
    fn add_is_idempotent_and_updates_embeddings() {
        let dim = 4;
        let mut ix = HnswIndex::new(dim);
        ix.add(1, &[0.0, 0.0, 0.0, 0.0]);
        ix.add(1, &[0.0, 0.0, 0.0, 0.0]);
        assert_eq!(ix.len(), 1);
        assert_eq!(ix.arena_len(), 1, "duplicate add allocated a second slot");
        ix.add(1, &[1.0, 1.0, 1.0, 1.0]);
        let got = ix.nearest(&[1.0, 1.0, 1.0, 1.0], 1);
        assert_eq!(got[0].0, 1);
        assert!(
            got[0].1 < 1e-12,
            "embedding update was not applied: {got:?}"
        );
    }

    #[test]
    fn malformed_adds_are_rejected_without_mutation() {
        let mut ix = HnswIndex::new(4);
        ix.add(1, &[0.0, 0.0]); // wrong dim
        ix.add(2, &[0.0, f64::NAN, 0.0, 0.0]); // non-finite
        ix.add(3, &[0.0, f64::INFINITY, 0.0, 0.0]);
        assert_eq!(ix.len(), 0);
        assert_eq!(ix.arena_len(), 0);
        assert!(ix.nearest(&[0.0; 4], 1).is_empty());
    }

    #[test]
    fn degenerate_queries_are_safe() {
        let dim = 4;
        let pts = points(20, dim, 2);
        let ix = build(&pts, dim);
        assert!(ix.nearest(&[0.0; 4], 0).is_empty(), "k = 0 must be empty");
        assert!(
            ix.nearest(&[0.0; 3], 1).is_empty(),
            "wrong-dim query must be empty"
        );
        assert_eq!(ix.nearest(&[0.0; 4], 1000).len(), 20, "k > |V| must clamp");
        assert!(HnswIndex::new(4).nearest(&[0.0; 4], 5).is_empty());
    }

    #[test]
    fn duplicate_embeddings_do_not_break_ordering() {
        let dim = 4;
        let mut ix = HnswIndex::new(dim);
        for id in 0..50u64 {
            ix.add(id, &[1.0, 2.0, 3.0, 4.0]);
        }
        let got = ix.nearest(&[1.0, 2.0, 3.0, 4.0], 10);
        assert_eq!(got.len(), 10);
        assert!(got.iter().all(|(_, d)| *d == 0.0));
        // Ties broken by internal index ⇒ deterministic id order.
        let again = ix.nearest(&[1.0, 2.0, 3.0, 4.0], 10);
        assert_eq!(got, again);
    }

    /// ℙ5 evidence at unit-test scale: work must grow like `log |V|`, not `|V|`.
    #[test]
    fn traversal_is_sublinear_in_graph_size() {
        let dim = 16;
        let q = &points(1, dim, 77)[0];
        let mut visited = Vec::new();
        for exp in [10u32, 12, 14] {
            let n = 1usize << exp;
            let pts = points(n, dim, 99);
            let ix = build(&pts, dim);
            let (_, stats) = ix.nearest_probed(q, 10);
            visited.push((n, stats.visited));
        }
        // 16× the points must not cost 16× the distance evaluations.
        let (n_small, v_small) = visited[0];
        let (n_big, v_big) = visited[2];
        let node_growth = n_big as f64 / n_small as f64;
        let work_growth = v_big as f64 / v_small as f64;
        assert!(
            work_growth < node_growth / 4.0,
            "visited grew {work_growth:.2}× for a {node_growth:.0}× larger index \
             (measurements: {visited:?}) — retrieval is not sub-linear"
        );
        // And the absolute work stays bounded by the ef/M budget, not |V|.
        assert!(
            v_big < n_big / 4,
            "visited {v_big} of {n_big} nodes — too close to a full scan"
        );
    }
}
