//! In-house layered (Sugiyama) mol placement for standard reaction schemes.
//!
//! Emits mol top-left positions plus long-edge **dummy waypoints** for the
//! router: multi-layer shafts follow the dummy chain instead of a tip chord
//! that cuts intermediate layers.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use crate::engine_config::LayeredLayoutConfig;
use crate::scene::Primitive;

use super::{DepictSpec, DocPaint, LayerAlign, LayoutDirection, LayoutOpts};

type Pt = (f64, f64);

/// Result of layered packing (positions in document space).
#[derive(Debug, Clone)]
pub struct LayeredPlacement {
    pub positions: HashMap<String, Pt>,
    pub width: f64,
    pub height: f64,
    /// Layer index per real mol id (after hints / longest-path).
    pub layers: HashMap<String, usize>,
    /// Intermediate dummy centers for multi-layer real→real edges, keyed by
    /// `(src_id, tgt_id)` in shallow→deep order (empty if adjacent layers).
    pub route_waypoints: HashMap<(String, String), Vec<Pt>>,
}

#[derive(Clone)]
struct NodeMetrics {
    id: String,
    w: f64,
    h: f64,
    /// Chem centroid in local tile space (origin = tile top-left).
    centroid_local: Pt,
    /// Caption baseline y in local tile space, if a name/caption exists.
    baseline_local_y: Option<f64>,
    layer_hint: Option<usize>,
    /// Opaque stable tie key (content hash of id).
    tie: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum NodeKey {
    Real(usize),
    Dummy(usize),
}

#[derive(Clone)]
struct PackNode {
    key: NodeKey,
    /// Real mol index into `metrics`, or unused for dummies.
    real: Option<usize>,
    w: f64,
    h: f64,
    layer: usize,
    /// Frozen cross-axis order from crossing minimization.
    ord: usize,
    /// Cross-axis center (document), set during coordinate assign.
    cross: f64,
    /// Flow-axis top-left (document).
    flow: f64,
}

#[derive(Clone, Copy)]
struct PackEdge {
    src: NodeKey,
    tgt: NodeKey,
}

/// Place mols for a layered scheme. Order-invariant given the same connectivity
/// and sizes (document list order is canonicalized away).
pub fn place_layered(
    spec: &DepictSpec,
    layout: &LayoutOpts,
    sizes: &[(String, f64, f64)],
    paints: &[DocPaint],
) -> Result<LayeredPlacement, String> {
    if sizes.is_empty() {
        return Ok(LayeredPlacement {
            positions: HashMap::new(),
            width: 0.0,
            height: 0.0,
            layers: HashMap::new(),
            route_waypoints: HashMap::new(),
        });
    }

    let budgets = &layout.engine().layered;
    let metrics = collect_metrics(spec, sizes, paints);
    let id_of: HashMap<String, usize> = metrics
        .iter()
        .enumerate()
        .map(|(i, m)| (m.id.clone(), i))
        .collect();

    let packing = packing_edges(spec, &id_of);
    let (feedback, forward) = break_cycles(&packing, metrics.len(), budgets);

    let mut layer_of = assign_layers(&forward, &metrics, metrics.len());
    apply_layer_hints(&mut layer_of, &metrics);
    let hinted: HashSet<usize> = metrics
        .iter()
        .enumerate()
        .filter_map(|(i, m)| m.layer_hint.map(|_| i))
        .collect();
    promote_strict_layers(&mut layer_of, &forward, &hinted);
    // Paint-only (`layout: false`) edges still express shell: dotted
    // intermediate→child should sit the child *after* the intermediate when
    // that does not cycle against hard layout edges.
    let soft = soft_order_edges(spec, &id_of);
    soft_promote_layers(&mut layer_of, &soft, &forward, &hinted);
    promote_strict_layers(&mut layer_of, &forward, &hinted);
    compact_layer_indices(&mut layer_of);

    let (mut nodes, mut edges) = insert_dummies(&metrics, &layer_of, &forward, &feedback);
    append_soft_edges_with_dummies(&mut nodes, &mut edges, &layer_of, &soft);
    order_layers(&mut nodes, &edges, budgets);
    assign_coordinates(&mut nodes, &edges, layout, &metrics, budgets);
    let dir = layout.direction_or_default();
    let vertical_flow = matches!(dir, LayoutDirection::Up | LayoutDirection::Down);
    let node_gap = layout.node_spacing_or_default();
    if vertical_flow {
        clear_long_edge_corridors(&mut nodes, &edges, node_gap, vertical_flow);
        // Slack-clamped pull toward mean real parents/children (dummies for
        // corridor targets + layer occupancy). Never free-jump — that blew
        // the pack when resolve cascaded rightward.
        for _ in 0..budgets.align_passes.max(3) {
            if !pull_cross_toward_neighbors(&mut nodes, &edges, node_gap, vertical_flow) {
                break;
            }
        }
        let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);
        for layer in 0..=max_layer {
            resolve_cross_overlaps(&mut nodes, layer, node_gap, vertical_flow);
        }
        normalize_cross_origin(&mut nodes, vertical_flow);
        separate_aabb_overlaps(&mut nodes, dir, node_gap);
        normalize_cross_origin(&mut nodes, vertical_flow);
    }

    let mut positions = HashMap::new();
    let mut layers = HashMap::new();
    let mut max_r = 0.0_f64;
    let mut max_b = 0.0_f64;
    for n in &nodes {
        let Some(ri) = n.real else {
            continue;
        };
        let m = &metrics[ri];
        let (x, y) = top_left_from_centers(n, dir);
        positions.insert(m.id.clone(), (x, y));
        layers.insert(m.id.clone(), n.layer);
        max_r = max_r.max(x + m.w);
        max_b = max_b.max(y + m.h);
    }

    // Co-endpoint stacks: share cross-axis mid (absorbs former post-ELK nudge).
    align_co_endpoint_mids(spec, &mut positions, &metrics, layout);
    separate_position_overlaps(&mut positions, &metrics, node_gap);

    let mut max_r = 0.0_f64;
    let mut max_b = 0.0_f64;
    for m in &metrics {
        let (x, y) = positions.get(&m.id).copied().unwrap_or((0.0, 0.0));
        max_r = max_r.max(x + m.w);
        max_b = max_b.max(y + m.h);
    }

    // Which edges span >1 layer (dummy chains), then place waypoints from
    // *final* mol positions into intermediate-layer gaps — Sugiyama dummy
    // pack centers go stale after corridor clear / co-endpoint / AABB nudges.
    let multi = multi_layer_edge_keys(&nodes, &edges, &metrics);
    let route_waypoints =
        gap_route_waypoints(&multi, &positions, &layers, &metrics, dir);

    Ok(LayeredPlacement {
        positions,
        width: max_r,
        height: max_b,
        layers,
        route_waypoints,
    })
}

/// Real→real edges that traverse at least one intermediate layer.
fn multi_layer_edge_keys(
    nodes: &[PackNode],
    edges: &[PackEdge],
    metrics: &[NodeMetrics],
) -> Vec<(String, String, usize, usize)> {
    let idx_of: HashMap<NodeKey, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.key, i))
        .collect();
    let mut out_adj: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    for e in edges {
        out_adj.entry(e.src).or_default().push(e.tgt);
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for n in nodes {
        let Some(si) = n.real else {
            continue;
        };
        let mut stack: Vec<(NodeKey, bool)> = out_adj
            .get(&n.key)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|k| (k, false))
            .collect();
        let mut visited = HashSet::new();
        visited.insert(n.key);
        while let Some((k, via_dummy)) = stack.pop() {
            if !visited.insert(k) {
                continue;
            }
            let Some(&j) = idx_of.get(&k) else {
                continue;
            };
            if let Some(ti) = nodes[j].real {
                if via_dummy && nodes[j].layer > n.layer {
                    let key = (metrics[si].id.clone(), metrics[ti].id.clone());
                    if seen.insert(key.clone()) {
                        out.push((key.0, key.1, n.layer, nodes[j].layer));
                    }
                }
                continue;
            }
            if let Some(nbrs) = out_adj.get(&k) {
                for &nbr in nbrs {
                    stack.push((nbr, true));
                }
            }
        }
    }
    out
}

/// Waypoints on each intermediate layer: snap the parent→child cross lerp into
/// the nearest gap between real tiles (document space after final nudges).
fn gap_route_waypoints(
    multi: &[(String, String, usize, usize)],
    positions: &HashMap<String, Pt>,
    layers: &HashMap<String, usize>,
    metrics: &[NodeMetrics],
    dir: LayoutDirection,
) -> HashMap<(String, String), Vec<Pt>> {
    let vertical_flow = matches!(dir, LayoutDirection::Up | LayoutDirection::Down);
    let size_of: HashMap<&str, (f64, f64)> = metrics
        .iter()
        .map(|m| (m.id.as_str(), (m.w, m.h)))
        .collect();

    // Per layer: sorted (cross0, cross1, flow_mid) of real tiles.
    let mut layer_tiles: HashMap<usize, Vec<(f64, f64, f64)>> = HashMap::new();
    for (id, &layer) in layers {
        let Some(&(x, y)) = positions.get(id) else {
            continue;
        };
        let Some(&(w, h)) = size_of.get(id.as_str()) else {
            continue;
        };
        let (c0, c1, fmid) = if vertical_flow {
            (x, x + w, y + h * 0.5)
        } else {
            (y, y + h, x + w * 0.5)
        };
        layer_tiles
            .entry(layer)
            .or_default()
            .push((c0, c1, fmid));
    }
    for tiles in layer_tiles.values_mut() {
        tiles.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }

    let mut out: HashMap<(String, String), Vec<Pt>> = HashMap::new();
    for (src, tgt, ls, lt) in multi {
        if *lt <= *ls + 1 {
            continue;
        }
        let Some(src_c) = mol_cross_flow(src, positions, &size_of, vertical_flow) else {
            continue;
        };
        let Some(tgt_c) = mol_cross_flow(tgt, positions, &size_of, vertical_flow) else {
            continue;
        };
        let mut wps = Vec::new();
        for layer in (ls + 1)..*lt {
            let t = (layer - ls) as f64 / (lt - ls) as f64;
            let ideal_cross = src_c.0 + (tgt_c.0 - src_c.0) * t;
            let Some(tiles) = layer_tiles.get(&layer) else {
                // Empty intermediate: keep lerp on both axes.
                let ideal_flow = src_c.1 + (tgt_c.1 - src_c.1) * t;
                wps.push(pt_from_cross_flow(ideal_cross, ideal_flow, vertical_flow));
                continue;
            };
            let (cross, flow) = snap_to_layer_gap(ideal_cross, tiles);
            wps.push(pt_from_cross_flow(cross, flow, vertical_flow));
        }
        if !wps.is_empty() {
            out.insert((src.clone(), tgt.clone()), wps);
        }
    }
    out
}

fn mol_cross_flow(
    id: &str,
    positions: &HashMap<String, Pt>,
    size_of: &HashMap<&str, (f64, f64)>,
    vertical_flow: bool,
) -> Option<(f64, f64)> {
    let &(x, y) = positions.get(id)?;
    let &(w, h) = size_of.get(id)?;
    if vertical_flow {
        Some((x + w * 0.5, y + h * 0.5))
    } else {
        Some((y + h * 0.5, x + w * 0.5))
    }
}

fn pt_from_cross_flow(cross: f64, flow: f64, vertical_flow: bool) -> Pt {
    if vertical_flow {
        (cross, flow)
    } else {
        (flow, cross)
    }
}

/// Nearest inter-tile gap mid to `ideal_cross`; fall back to layer span ends.
fn snap_to_layer_gap(ideal_cross: f64, tiles: &[(f64, f64, f64)]) -> (f64, f64) {
    let flow = {
        let sum: f64 = tiles.iter().map(|t| t.2).sum();
        sum / tiles.len() as f64
    };
    if tiles.len() < 2 {
        let mid = 0.5 * (tiles[0].0 + tiles[0].1);
        return (mid, flow);
    }
    let mut best_cross = 0.5 * (tiles[0].1 + tiles[1].0);
    let mut best_d = (best_cross - ideal_cross).abs();
    for w in tiles.windows(2) {
        let gap_lo = w[0].1;
        let gap_hi = w[1].0;
        let mid = if gap_hi >= gap_lo {
            0.5 * (gap_lo + gap_hi)
        } else {
            // Overlap: sit on the shared boundary.
            0.5 * (gap_lo + gap_hi)
        };
        let d = (mid - ideal_cross).abs();
        if d < best_d {
            best_d = d;
            best_cross = mid;
        }
    }
    // Also consider just outside the leftmost / rightmost tile when the lerp
    // lands past the row — keeps long fans from stealing an interior gap.
    let left_out = tiles[0].0 - (tiles[1].0 - tiles[0].1).abs().max(1.0) * 0.5;
    let right_out = tiles[tiles.len() - 1].1
        + (tiles[tiles.len() - 1].0 - tiles[tiles.len() - 2].1)
            .abs()
            .max(1.0)
            * 0.5;
    for cand in [left_out, right_out] {
        let d = (cand - ideal_cross).abs();
        if d < best_d {
            best_d = d;
            best_cross = cand;
        }
    }
    // Prefer an interior gap when ideal sits inside some gap span.
    for w in tiles.windows(2) {
        let gap_lo = w[0].1.min(w[1].0);
        let gap_hi = w[0].1.max(w[1].0);
        if ideal_cross >= gap_lo && ideal_cross <= gap_hi {
            return (ideal_cross.clamp(gap_lo, gap_hi), flow);
        }
    }
    let _ = best_d;
    (best_cross, flow)
}

fn collect_metrics(
    spec: &DepictSpec,
    sizes: &[(String, f64, f64)],
    paints: &[DocPaint],
) -> Vec<NodeMetrics> {
    let paint_of: HashMap<&str, &DocPaint> = paints.iter().map(|p| (p.id.as_str(), p)).collect();
    let mut layer_hint_of: HashMap<String, usize> = HashMap::new();
    for m in spec.mols() {
        if let (Some(id), Some(layer)) = (m.id.as_ref(), m.layer) {
            layer_hint_of.insert(id.clone(), layer as usize);
        }
    }

    let mut out = Vec::with_capacity(sizes.len());
    for (id, w, h) in sizes {
        let paint = paint_of.get(id.as_str()).copied();
        let centroid_local = local_chem_centroid(paint, *w, *h);
        let baseline_local_y = local_caption_baseline(paint);
        let layer_hint = layer_hint_of.get(id).copied();
        out.push(NodeMetrics {
            id: id.clone(),
            w: *w,
            h: *h,
            centroid_local,
            baseline_local_y,
            layer_hint,
            tie: stable_tie(id),
        });
    }
    // Canonical node order by id (order invariance).
    out.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.tie.cmp(&b.tie)));
    out
}

fn stable_tie(id: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    h.finish()
}

fn local_chem_centroid(paint: Option<&DocPaint>, w: f64, h: f64) -> Pt {
    let Some(paint) = paint else {
        return (w * 0.5, h * 0.5);
    };
    // Packing mark: pose-aligned common core when tagged; else shaft center.
    if let Some(dbg) = paint.molecule.align_debug.as_ref() {
        let mut sx = 0.0_f64;
        let mut sy = 0.0_f64;
        let mut n = 0_usize;
        for &(atom_idx, _) in &dbg.tags {
            if atom_idx < 0 {
                continue;
            }
            if let Some(a) = paint.molecule.atoms.get(atom_idx as usize) {
                sx += a.x;
                sy += a.y;
                n += 1;
            }
        }
        if n > 0 {
            let (ax, ay) = (sx / n as f64, sy / n as f64);
            let (lx, ly) = paint
                .scene
                .viewports
                .first()
                .map(|v| v.local_origin())
                .unwrap_or((0.0, 0.0));
            let s = paint.molecule.diagram_scale();
            let (fx, fy) = crate::depict::frame_offset(&paint.molecule);
            return (
                lx - paint.scene.x + s * (fx + ax),
                ly - paint.scene.y + s * (fy + ay),
            );
        }
    }
    paint.center()
}

fn local_caption_baseline(paint: Option<&DocPaint>) -> Option<f64> {
    let paint = paint?;
    let mut best: Option<f64> = None;
    for p in &paint.scene.overlays {
        if let Primitive::Text { y, class, .. } = p {
            let c = class.as_deref().unwrap_or("");
            if c.contains("mol-caption") && !c.contains("metrics") {
                best = Some(match best {
                    Some(b) => b.max(*y),
                    None => *y,
                });
            }
        }
    }
    // Also scan viewport layers (some captions land there).
    for vp in &paint.scene.viewports {
        for layer in &vp.layers {
            for p in &layer.primitives {
                if let Primitive::Text { y, class, .. } = p {
                    let c = class.as_deref().unwrap_or("");
                    if c.contains("mol-caption") && !c.contains("metrics") {
                        let yy = vp.y + *y;
                        best = Some(match best {
                            Some(b) => b.max(yy),
                            None => yy,
                        });
                    }
                }
            }
        }
    }
    best
}

/// One directed packing edge per layout-participating relation.
/// Multi-endpoint: every source → every target (bundle span), sorted stably.
fn packing_edges(spec: &DepictSpec, id_of: &HashMap<String, usize>) -> Vec<(usize, usize)> {
    relation_pairs(spec, id_of, true)
}

/// Relations with `layout: false` — routing-only, but still scored for crossings.
fn soft_order_edges(spec: &DepictSpec, id_of: &HashMap<String, usize>) -> Vec<(usize, usize)> {
    relation_pairs(spec, id_of, false)
}

fn relation_pairs(
    spec: &DepictSpec,
    id_of: &HashMap<String, usize>,
    layout_only: bool,
) -> Vec<(usize, usize)> {
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut seen = HashSet::new();
    for e in spec.edges() {
        let participates = e.participates_in_layout();
        if layout_only && !participates {
            continue;
        }
        if !layout_only && participates {
            continue;
        }
        for s in e.sources.as_slice() {
            for t in e.targets.as_slice() {
                let Some(&si) = id_of.get(s) else {
                    continue;
                };
                let Some(&ti) = id_of.get(t) else {
                    continue;
                };
                if si == ti {
                    continue;
                }
                if seen.insert((si, ti)) {
                    pairs.push((si, ti));
                }
            }
        }
    }
    pairs.sort_unstable();
    pairs
}

/// Push targets so every forward layout edge goes to a strictly deeper layer.
fn promote_strict_layers(
    layer_of: &mut [usize],
    forward: &[(usize, usize)],
    hinted: &HashSet<usize>,
) {
    let n = layer_of.len();
    for _ in 0..(n * n).max(1) {
        let mut changed = false;
        for &(s, t) in forward {
            if hinted.contains(&t) {
                continue;
            }
            if layer_of[t] <= layer_of[s] {
                layer_of[t] = layer_of[s] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Soft (`layout: false`) edges promote shell when they do not cycle against
/// the hard forward DAG. Intermediate→child dotted links deepen the child so
/// the intermediate sits in an earlier row with descendants after it.
fn soft_promote_layers(
    layer_of: &mut [usize],
    soft: &[(usize, usize)],
    hard_forward: &[(usize, usize)],
    hinted: &HashSet<usize>,
) {
    if soft.is_empty() {
        return;
    }
    let n = layer_of.len();
    let mut hard_adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(s, t) in hard_forward {
        hard_adj[s].push(t);
    }
    let hard_reaches = |from: usize, to: usize| -> bool {
        if from == to {
            return true;
        }
        let mut stack = vec![from];
        let mut seen = vec![false; n];
        seen[from] = true;
        while let Some(u) = stack.pop() {
            for &v in &hard_adj[u] {
                if v == to {
                    return true;
                }
                if !seen[v] {
                    seen[v] = true;
                    stack.push(v);
                }
            }
        }
        false
    };
    for _ in 0..(n * n).max(1) {
        let mut changed = false;
        for &(s, t) in soft {
            if hinted.contains(&t) {
                continue;
            }
            // Soft s→t must not reverse a hard path t ↝ s (e.g. Morphine↔Codeine).
            if hard_reaches(t, s) {
                continue;
            }
            if layer_of[t] <= layer_of[s] {
                layer_of[t] = layer_of[s] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
        promote_strict_layers(layer_of, hard_forward, hinted);
    }
}

/// Soft edges spanning multiple layers get the same dummy chain as hard edges
/// so crossing / span scoring sees the full pathway.
fn append_soft_edges_with_dummies(
    nodes: &mut Vec<PackNode>,
    edges: &mut Vec<PackEdge>,
    layer_of: &[usize],
    soft: &[(usize, usize)],
) {
    let mut dummy_i = nodes
        .iter()
        .filter_map(|n| match n.key {
            NodeKey::Dummy(i) => Some(i + 1),
            NodeKey::Real(_) => None,
        })
        .max()
        .unwrap_or(0);
    for &(s, t) in soft {
        let ls = layer_of[s];
        let lt = layer_of[t];
        if ls == lt {
            edges.push(PackEdge {
                src: NodeKey::Real(s),
                tgt: NodeKey::Real(t),
            });
            continue;
        }
        let (lo, hi, a, b) = if lt > ls {
            (ls, lt, s, t)
        } else {
            // Soft edge points up-shell; still score the pathway for crossings.
            (lt, ls, t, s)
        };
        if hi == lo + 1 {
            edges.push(PackEdge {
                src: NodeKey::Real(a),
                tgt: NodeKey::Real(b),
            });
            continue;
        }
        let mut prev = NodeKey::Real(a);
        for layer in (lo + 1)..hi {
            let key = NodeKey::Dummy(dummy_i);
            dummy_i += 1;
            nodes.push(PackNode {
                key,
                real: None,
                w: 0.0,
                h: 0.0,
                layer,
                ord: 0,
                cross: 0.0,
                flow: 0.0,
            });
            edges.push(PackEdge { src: prev, tgt: key });
            prev = key;
        }
        edges.push(PackEdge {
            src: prev,
            tgt: NodeKey::Real(b),
        });
    }
}

fn compact_layer_indices(layer_of: &mut [usize]) {
    let mut used: Vec<usize> = layer_of.to_vec();
    used.sort_unstable();
    used.dedup();
    let map: HashMap<usize, usize> = used
        .iter()
        .enumerate()
        .map(|(i, &l)| (l, i))
        .collect();
    for l in layer_of.iter_mut() {
        *l = map[l];
    }
}

/// DFS cycle break (DEPTH_FIRST semantics): keep real sources early.
/// Returns (feedback arcs, forward arcs).
fn break_cycles(
    edges: &[(usize, usize)],
    n: usize,
    _budgets: &LayeredLayoutConfig,
) -> (Vec<(usize, usize)>, Vec<(usize, usize)>) {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(s, t) in edges {
        adj[s].push(t);
    }
    for a in &mut adj {
        a.sort_unstable();
    }

    let mut color = vec![0u8; n]; // 0=white 1=gray 2=black
    let mut feedback = HashSet::new();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_unstable(); // stable visit order by index (= id-sorted metrics)

    fn dfs(
        u: usize,
        adj: &[Vec<usize>],
        color: &mut [u8],
        feedback: &mut HashSet<(usize, usize)>,
    ) {
        color[u] = 1;
        for &v in &adj[u] {
            if color[v] == 1 {
                feedback.insert((u, v));
            } else if color[v] == 0 {
                dfs(v, adj, color, feedback);
            }
        }
        color[u] = 2;
    }

    for u in order {
        if color[u] == 0 {
            dfs(u, &adj, &mut color, &mut feedback);
        }
    }

    let mut forward = Vec::new();
    let mut fb = Vec::new();
    for &e in edges {
        if feedback.contains(&e) {
            fb.push(e);
        } else {
            forward.push(e);
        }
    }
    forward.sort_unstable();
    fb.sort_unstable();
    (fb, forward)
}

fn assign_layers(forward: &[(usize, usize)], metrics: &[NodeMetrics], n: usize) -> Vec<usize> {
    let mut indeg = vec![0usize; n];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(s, t) in forward {
        adj[s].push(t);
        indeg[t] += 1;
    }
    for a in &mut adj {
        a.sort_unstable();
    }

    // Longest-path layering toward sinks (sources → low layers).
    let mut layer = vec![0usize; n];
    let mut ready: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    ready.sort_unstable_by_key(|&i| (metrics[i].tie, metrics[i].id.as_str()));
    let mut seen = 0usize;
    let mut queue = ready;
    let mut indeg_left = indeg.clone();
    while let Some(u) = {
        // pop front of sorted frontier
        if queue.is_empty() {
            None
        } else {
            Some(queue.remove(0))
        }
    } {
        seen += 1;
        for &v in &adj[u] {
            layer[v] = layer[v].max(layer[u] + 1);
            indeg_left[v] -= 1;
            if indeg_left[v] == 0 {
                queue.push(v);
                queue.sort_unstable_by_key(|&i| (metrics[i].tie, metrics[i].id.as_str()));
            }
        }
    }
    if seen < n {
        // Disconnected leftovers stay at layer 0 (already).
        let _ = seen;
    }
    layer
}

fn apply_layer_hints(layer_of: &mut [usize], metrics: &[NodeMetrics]) {
    for (i, m) in metrics.iter().enumerate() {
        if let Some(h) = m.layer_hint {
            layer_of[i] = h;
        }
    }
    // Push successors so hints do not create backward forward-edges.
    // Capped: one pass per node (bounded).
    let n = metrics.len();
    for _ in 0..n.max(1) {
        let mut changed = false;
        // Re-read edges via layer monotonicity among all pairs that had forward
        // intent is handled at insert_dummies; here only bump hinted sinks.
        for i in 0..n {
            if let Some(h) = metrics[i].layer_hint {
                if layer_of[i] != h {
                    layer_of[i] = h;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

fn insert_dummies(
    metrics: &[NodeMetrics],
    layer_of: &[usize],
    forward: &[(usize, usize)],
    feedback: &[(usize, usize)],
) -> (Vec<PackNode>, Vec<PackEdge>) {
    let mut nodes: Vec<PackNode> = metrics
        .iter()
        .enumerate()
        .map(|(i, m)| PackNode {
            key: NodeKey::Real(i),
            real: Some(i),
            w: m.w,
            h: m.h,
            layer: layer_of[i],
            ord: 0,
            cross: 0.0,
            flow: 0.0,
        })
        .collect();

    let mut oriented: Vec<(usize, usize)> = forward.to_vec();
    for &(s, t) in feedback {
        oriented.push((t, s));
    }

    let mut edges = Vec::new();
    let mut dummy_i = 0usize;
    for &(s, t) in &oriented {
        let ls = layer_of[s];
        let lt = layer_of[t];
        if lt == ls {
            edges.push(PackEdge {
                src: NodeKey::Real(s),
                tgt: NodeKey::Real(t),
            });
            continue;
        }
        let (lo, hi, a, b) = if lt > ls {
            (ls, lt, s, t)
        } else {
            (lt, ls, t, s)
        };
        if hi == lo + 1 {
            edges.push(PackEdge {
                src: NodeKey::Real(a),
                tgt: NodeKey::Real(b),
            });
            continue;
        }
        let mut prev = NodeKey::Real(a);
        for layer in (lo + 1)..hi {
            let key = NodeKey::Dummy(dummy_i);
            dummy_i += 1;
            nodes.push(PackNode {
                key,
                real: None,
                w: 0.0,
                h: 0.0,
                layer,
                ord: 0,
                cross: 0.0,
                flow: 0.0,
            });
            edges.push(PackEdge { src: prev, tgt: key });
            prev = key;
        }
        edges.push(PackEdge {
            src: prev,
            tgt: NodeKey::Real(b),
        });
    }
    (nodes, edges)
}

fn order_layers(nodes: &mut [PackNode], edges: &[PackEdge], budgets: &LayeredLayoutConfig) {
    let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);
    let deeper_deg = deeper_neighbor_degree(nodes, edges);
    for layer in 0..=max_layer {
        let mut idx: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.layer == layer)
            .map(|(i, _)| i)
            .collect();
        idx.sort_by(|&a, &b| {
            deeper_deg
                .get(&nodes[a].key)
                .copied()
                .unwrap_or(0)
                .cmp(&deeper_deg.get(&nodes[b].key).copied().unwrap_or(0))
                .then_with(|| node_tie(nodes, a).cmp(&node_tie(nodes, b)))
        });
        for (rank, &i) in idx.iter().enumerate() {
            nodes[i].cross = rank as f64;
        }
    }

    let sweeps = budgets.crossing_sweeps;
    for sweep in 0..sweeps {
        let forward = sweep % 2 == 1; // reverse-first
        let layers: Box<dyn Iterator<Item = usize>> = if forward {
            Box::new(1..=max_layer)
        } else {
            Box::new((0..max_layer).rev())
        };
        let mut any = false;
        for layer in layers {
            if barycenter_reorder(nodes, edges, layer, forward) {
                any = true;
            }
        }
        if improve_order_by_swaps(nodes, edges) {
            any = true;
        }
        if !any && sweep > 0 {
            break;
        }
    }
    for _ in 0..sweeps {
        if !improve_order_by_swaps(nodes, edges) {
            break;
        }
    }
    // Polynomial only: sift nodes, then swap whole vertical legs (root + all
    // deeper descendants move together). No joint factorial search.
    for _ in 0..sweeps.min(8) {
        let a = sift_layer_order(nodes, edges);
        let b = improve_order_by_leg_swaps(nodes, edges);
        if !a && !b {
            break;
        }
    }
    exhaustive_order_small_layers(nodes, edges);
    // Leg swaps again after small-layer polish — descendants may have moved.
    for _ in 0..4 {
        if !improve_order_by_leg_swaps(nodes, edges) && !sift_layer_order(nodes, edges) {
            break;
        }
    }
    for layer in 0..=max_layer {
        let mut idx: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.layer == layer)
            .map(|(i, _)| i)
            .collect();
        idx.sort_by(|&a, &b| {
            nodes[a]
                .cross
                .partial_cmp(&nodes[b].cross)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| node_tie(nodes, a).cmp(&node_tie(nodes, b)))
        });
        for (ord, &i) in idx.iter().enumerate() {
            nodes[i].ord = ord;
            nodes[i].cross = ord as f64;
        }
    }
}

fn deeper_neighbor_degree(
    nodes: &[PackNode],
    edges: &[PackEdge],
) -> HashMap<NodeKey, usize> {
    let layer_of: HashMap<NodeKey, usize> = nodes.iter().map(|n| (n.key, n.layer)).collect();
    let mut deg: HashMap<NodeKey, usize> = HashMap::new();
    for e in edges {
        let Some(&ls) = layer_of.get(&e.src) else { continue; };
        let Some(&lt) = layer_of.get(&e.tgt) else { continue; };
        if lt > ls {
            *deg.entry(e.src).or_default() += 1;
        } else if ls > lt {
            *deg.entry(e.tgt).or_default() += 1;
        }
    }
    deg
}

/// Collapse dummy chains to real→real chords, then count proper crossings in
/// (cross, layer) space. This is what `order_cost` minimizes: long spanning
/// edges (Morphine→deep children) compete with short ones (norcodeine→child).
/// Unit-dummy segment counts under-count those long-edge conflicts.
fn real_real_chords(nodes: &[PackNode], edges: &[PackEdge]) -> Vec<(NodeKey, NodeKey)> {
    let mut succ: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    for e in edges {
        succ.entry(e.src).or_default().push(e.tgt);
    }
    let is_real = |k: NodeKey| matches!(k, NodeKey::Real(_));
    let mut out = Vec::new();
    let mut seen: HashSet<(NodeKey, NodeKey)> = HashSet::new();
    for n in nodes {
        if !is_real(n.key) {
            continue;
        }
        let mut stack: Vec<NodeKey> = succ.get(&n.key).cloned().unwrap_or_default();
        while let Some(cur) = stack.pop() {
            if is_real(cur) {
                if cur != n.key && seen.insert((n.key, cur)) {
                    out.push((n.key, cur));
                }
                continue;
            }
            if let Some(nexts) = succ.get(&cur) {
                stack.extend(nexts.iter().copied());
            }
        }
    }
    out
}

fn total_chord_crossings(nodes: &[PackNode], edges: &[PackEdge]) -> usize {
    let pos: HashMap<NodeKey, (f64, f64)> = nodes
        .iter()
        .map(|n| (n.key, (n.cross, n.layer as f64)))
        .collect();
    let chords = real_real_chords(nodes, edges);
    let mut x = 0usize;
    for i in 0..chords.len() {
        for j in (i + 1)..chords.len() {
            let (a, b) = chords[i];
            let (c, d) = chords[j];
            if a == c || a == d || b == c || b == d {
                continue;
            }
            let Some(&pa) = pos.get(&a) else { continue; };
            let Some(&pb) = pos.get(&b) else { continue; };
            let Some(&pc) = pos.get(&c) else { continue; };
            let Some(&pd) = pos.get(&d) else { continue; };
            if (pa.0 - pb.0).abs() < 1e-12 && (pa.1 - pb.1).abs() < 1e-12 {
                continue;
            }
            if (pc.0 - pd.0).abs() < 1e-12 && (pc.1 - pd.1).abs() < 1e-12 {
                continue;
            }
            if rank_segments_cross(pa, pb, pc, pd) {
                x += 1;
            }
        }
    }
    x
}

fn rank_orient(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn rank_segments_cross(
    p: (f64, f64),
    q: (f64, f64),
    r: (f64, f64),
    s: (f64, f64),
) -> bool {
    // Use raw orientations — do NOT use f64::signum for the zero test.
    // Rust's 0.0.signum() is 1.0 (copysign), which falsely treats shared-endpoint
    // / collinear cases as proper crossings and inflates order_cost.
    let o1 = rank_orient(p, q, r);
    let o2 = rank_orient(p, q, s);
    let o3 = rank_orient(r, s, p);
    let o4 = rank_orient(r, s, q);
    o1 != 0.0
        && o2 != 0.0
        && o3 != 0.0
        && o4 != 0.0
        && o1.signum() != o2.signum()
        && o3.signum() != o4.signum()
}

/// Try every insertion rank for each node in each layer (Sugiyama sifting).
/// O(Σ n_ℓ² · cost) — fine for scheme-sized layers; no factorial search.
fn sift_layer_order(nodes: &mut [PackNode], edges: &[PackEdge]) -> bool {
    let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);
    let mut moved = false;
    for layer in 0..=max_layer {
        let mut idx: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.layer == layer)
            .map(|(i, _)| i)
            .collect();
        if idx.len() < 2 {
            continue;
        }
        idx.sort_by(|&a, &b| {
            nodes[a]
                .cross
                .partial_cmp(&nodes[b].cross)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| node_tie(nodes, a).cmp(&node_tie(nodes, b)))
        });
        let n = idx.len();
        for which in 0..n {
            let node = idx[which];
            let mut best_pos = which;
            let mut best_cost = order_cost(nodes, edges);
            // Pull `node` out and try every slot.
            let mut order = idx.clone();
            order.remove(which);
            for pos in 0..=order.len() {
                let mut trial = order.clone();
                trial.insert(pos, node);
                for (rank, &i) in trial.iter().enumerate() {
                    nodes[i].cross = rank as f64;
                }
                let cost = order_cost(nodes, edges);
                if cost < best_cost {
                    best_cost = cost;
                    best_pos = pos;
                }
            }
            let mut final_order = order;
            final_order.insert(best_pos, node);
            for (rank, &i) in final_order.iter().enumerate() {
                nodes[i].cross = rank as f64;
            }
            if best_pos != which {
                moved = true;
            }
            idx = final_order;
        }
    }
    moved
}

/// Downward closure along forward (shallower→deeper) edges, including `root`.
fn downward_leg(nodes: &[PackNode], edges: &[PackEdge], root: usize) -> HashSet<usize> {
    let key_to_idx: HashMap<NodeKey, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.key, i))
        .collect();
    let root_layer = nodes[root].layer;
    let mut children: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    for e in edges {
        let Some(&si) = key_to_idx.get(&e.src) else { continue; };
        let Some(&ti) = key_to_idx.get(&e.tgt) else { continue; };
        let (from, to) = if nodes[si].layer < nodes[ti].layer {
            (e.src, e.tgt)
        } else if nodes[ti].layer < nodes[si].layer {
            (e.tgt, e.src)
        } else {
            continue;
        };
        children.entry(from).or_default().push(to);
    }
    let mut out = HashSet::new();
    let mut stack = vec![nodes[root].key];
    out.insert(root);
    while let Some(k) = stack.pop() {
        let Some(ch) = children.get(&k) else { continue; };
        for &c in ch {
            let Some(&ci) = key_to_idx.get(&c) else { continue; };
            if nodes[ci].layer <= root_layer {
                continue;
            }
            if out.insert(ci) {
                stack.push(c);
            }
        }
    }
    out
}

/// Swap two vertical legs (root + all deeper descendants) as blocks on every
/// layer they occupy. Unequal block sizes share the combined cross slots.
fn improve_order_by_leg_swaps(nodes: &mut [PackNode], edges: &[PackEdge]) -> bool {
    let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);
    // Candidate roots: real nodes that own at least one deeper descendant.
    let mut roots: Vec<usize> = Vec::new();
    for i in 0..nodes.len() {
        if nodes[i].real.is_none() {
            continue;
        }
        let leg = downward_leg(nodes, edges, i);
        if leg.iter().any(|&j| nodes[j].layer > nodes[i].layer) {
            roots.push(i);
        }
    }
    let mut moved = false;
    for layer in 0..=max_layer {
        let layer_roots: Vec<usize> = roots
            .iter()
            .copied()
            .filter(|&i| nodes[i].layer == layer)
            .collect();
        if layer_roots.len() < 2 {
            continue;
        }
        let mut improved = true;
        while improved {
            improved = false;
            for a in 0..layer_roots.len() {
                for b in (a + 1)..layer_roots.len() {
                    if try_swap_legs(nodes, edges, layer_roots[a], layer_roots[b]) {
                        improved = true;
                        moved = true;
                    }
                }
            }
        }
    }
    moved
}

fn try_swap_legs(
    nodes: &mut [PackNode],
    edges: &[PackEdge],
    root_a: usize,
    root_b: usize,
) -> bool {
    let da = downward_leg(nodes, edges, root_a);
    let db = downward_leg(nodes, edges, root_b);
    if da.iter().any(|i| db.contains(i)) {
        return false;
    }
    let before = order_cost(nodes, edges);
    // Snapshot crosses for revert.
    let snap: Vec<(usize, f64)> = da
        .iter()
        .chain(db.iter())
        .map(|&i| (i, nodes[i].cross))
        .collect();
    let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);
    for layer in 0..=max_layer {
        let mut a_list: Vec<usize> = da
            .iter()
            .copied()
            .filter(|&i| nodes[i].layer == layer)
            .collect();
        let mut b_list: Vec<usize> = db
            .iter()
            .copied()
            .filter(|&i| nodes[i].layer == layer)
            .collect();
        if a_list.is_empty() || b_list.is_empty() {
            continue;
        }
        a_list.sort_by(|&i, &j| {
            nodes[i]
                .cross
                .partial_cmp(&nodes[j].cross)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        b_list.sort_by(|&i, &j| {
            nodes[i]
                .cross
                .partial_cmp(&nodes[j].cross)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let a_mean: f64 =
            a_list.iter().map(|&i| nodes[i].cross).sum::<f64>() / a_list.len() as f64;
        let b_mean: f64 =
            b_list.iter().map(|&i| nodes[i].cross).sum::<f64>() / b_list.len() as f64;
        let mut slots: Vec<f64> = a_list
            .iter()
            .chain(b_list.iter())
            .map(|&i| nodes[i].cross)
            .collect();
        slots.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        // Swap blocks: the side that was left becomes right and vice versa.
        if a_mean <= b_mean {
            // A left of B → B takes left slots, A right.
            for (k, &i) in b_list.iter().enumerate() {
                nodes[i].cross = slots[k];
            }
            for (k, &i) in a_list.iter().enumerate() {
                nodes[i].cross = slots[b_list.len() + k];
            }
        } else {
            for (k, &i) in a_list.iter().enumerate() {
                nodes[i].cross = slots[k];
            }
            for (k, &i) in b_list.iter().enumerate() {
                nodes[i].cross = slots[a_list.len() + k];
            }
        }
    }
    let after = order_cost(nodes, edges);
    // Prefer fewer crossings even when bundle/span regress — Morphine
    // norcodeine↔i0 is the motivating case.
    if after.0 < before.0 || after < before {
        true
    } else {
        for (i, c) in snap {
            nodes[i].cross = c;
        }
        false
    }
}

fn improve_order_by_swaps(nodes: &mut [PackNode], edges: &[PackEdge]) -> bool {
    let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);
    let mut moved = false;
    for layer in 0..=max_layer {
        let mut idx: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.layer == layer)
            .map(|(i, _)| i)
            .collect();
        if idx.len() < 2 {
            continue;
        }
        idx.sort_by(|&a, &b| {
            nodes[a]
                .cross
                .partial_cmp(&nodes[b].cross)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| node_tie(nodes, a).cmp(&node_tie(nodes, b)))
        });
        let mut improved = true;
        while improved {
            improved = false;
            for k in 0..idx.len() - 1 {
                if try_swap_if_better(nodes, edges, idx[k], idx[k + 1]) {
                    idx.swap(k, k + 1);
                    improved = true;
                    moved = true;
                }
            }
            if idx.len() <= 10 {
                for a in 0..idx.len() {
                    for b in (a + 2)..idx.len() {
                        if try_swap_if_better(nodes, edges, idx[a], idx[b]) {
                            idx.swap(a, b);
                            improved = true;
                            moved = true;
                        }
                    }
                }
            }
        }
    }
    moved
}

fn try_swap_if_better(
    nodes: &mut [PackNode],
    edges: &[PackEdge],
    i: usize,
    j: usize,
) -> bool {
    let before = order_cost(nodes, edges);
    let ci = nodes[i].cross;
    nodes[i].cross = nodes[j].cross;
    nodes[j].cross = ci;
    let after = order_cost(nodes, edges);
    if after < before {
        true
    } else {
        let ci = nodes[i].cross;
        nodes[i].cross = nodes[j].cross;
        nodes[j].cross = ci;
        false
    }
}

fn order_cost(nodes: &[PackNode], edges: &[PackEdge]) -> (usize, i32, i64) {
    // Prefer real→real chord crossings over unit-dummy segments so long
    // spanning edges (Morphine→L3) are scored against short ones.
    let xings = total_chord_crossings(nodes, edges);
    let bundle = soft_same_side_penalty(nodes, edges);
    let span = (total_rank_span(nodes, edges) * 1000.0).round() as i64;
    (xings, bundle, span)
}

fn total_rank_span(nodes: &[PackNode], edges: &[PackEdge]) -> f64 {
    let pos: HashMap<NodeKey, (f64, f64)> = nodes
        .iter()
        .map(|n| (n.key, (n.cross, n.layer as f64)))
        .collect();
    let mut span = 0.0_f64;
    for e in edges {
        let Some(&p) = pos.get(&e.src) else { continue; };
        let Some(&q) = pos.get(&e.tgt) else { continue; };
        span += (p.0 - q.0).hypot(p.1 - q.1);
    }
    span
}

fn soft_same_side_penalty(nodes: &[PackNode], edges: &[PackEdge]) -> i32 {
    let cross_of: HashMap<NodeKey, f64> = nodes.iter().map(|n| (n.key, n.cross)).collect();
    let layer_of: HashMap<NodeKey, usize> = nodes.iter().map(|n| (n.key, n.layer)).collect();
    let mut layer_max: HashMap<usize, f64> = HashMap::new();
    for n in nodes {
        let e = layer_max.entry(n.layer).or_insert(0.0);
        *e = e.max(n.cross);
    }
    let frac = |layer: usize, cross: f64| -> f64 {
        let m = layer_max.get(&layer).copied().unwrap_or(0.0).max(1e-9);
        cross / m
    };
    let mut penalty = 0_i32;
    for n in nodes.iter() {
        let mut by_layer: HashMap<usize, Vec<f64>> = HashMap::new();
        for e in edges {
            let other = if e.src == n.key {
                e.tgt
            } else if e.tgt == n.key {
                e.src
            } else {
                continue;
            };
            let Some(&ol) = layer_of.get(&other) else { continue; };
            if ol >= n.layer {
                continue;
            }
            if let Some(&c) = cross_of.get(&other) {
                by_layer.entry(ol).or_default().push(frac(ol, c));
            }
        }
        let self_f = frac(n.layer, n.cross);
        for nbrs in by_layer.values() {
            if nbrs.len() < 2 {
                continue;
            }
            let left = nbrs.iter().filter(|&&c| c < self_f - 1e-9).count();
            let right = nbrs.iter().filter(|&&c| c > self_f + 1e-9).count();
            if left == 0 || right == 0 {
                penalty += (nbrs.len() as i32) - 1;
            }
        }
    }
    penalty
}

fn exhaustive_order_small_layers(nodes: &mut [PackNode], edges: &[PackEdge]) {
    // Always search small layers for best `order_cost` (crossings, then
    // bundle, then path span). Skipping when crossings were already zero left
    // span-suboptimal ranks that lengthen multi-hop arrows.
    const MAX_N: usize = 8;
    let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);
    for layer in 0..=max_layer {
        let mut idx: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.layer == layer)
            .map(|(i, _)| i)
            .collect();
        if idx.len() < 2 || idx.len() > MAX_N {
            continue;
        }
        idx.sort_by(|&a, &b| {
            nodes[a]
                .cross
                .partial_cmp(&nodes[b].cross)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| node_tie(nodes, a).cmp(&node_tie(nodes, b)))
        });
        let n = idx.len();
        let mut best_perm: Vec<usize> = (0..n).collect();
        let mut best_cost = {
            for (rank, &i) in idx.iter().enumerate() {
                nodes[i].cross = rank as f64;
            }
            order_cost(nodes, edges)
        };
        let mut perm: Vec<usize> = (0..n).collect();
        let mut c = vec![0usize; n];
        let mut i = 0;
        while i < n {
            if c[i] < i {
                if i % 2 == 0 {
                    perm.swap(0, i);
                } else {
                    perm.swap(c[i], i);
                }
                for (rank, &p) in perm.iter().enumerate() {
                    nodes[idx[p]].cross = rank as f64;
                }
                let cost = order_cost(nodes, edges);
                if cost < best_cost {
                    best_cost = cost;
                    best_perm = perm.clone();
                }
                c[i] += 1;
                i = 0;
            } else {
                c[i] = 0;
                i += 1;
            }
        }
        for (rank, &p) in best_perm.iter().enumerate() {
            nodes[idx[p]].cross = rank as f64;
        }
    }
}

fn layer_indices_in_order(nodes: &[PackNode], layer: usize) -> Vec<usize> {
    let mut idx: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.layer == layer)
        .map(|(i, _)| i)
        .collect();
    idx.sort_by(|&a, &b| {
        nodes[a]
            .ord
            .cmp(&nodes[b].ord)
            .then_with(|| node_tie(nodes, a).cmp(&node_tie(nodes, b)))
    });
    idx
}

fn node_tie(nodes: &[PackNode], i: usize) -> (u64, String) {
    match nodes[i].key {
        NodeKey::Real(ri) => (ri as u64, format!("r{ri}")),
        NodeKey::Dummy(d) => (u64::MAX / 2 + d as u64, format!("d{d}")),
    }
}

fn barycenter_reorder(
    nodes: &mut [PackNode],
    edges: &[PackEdge],
    layer: usize,
    from_prev: bool,
) -> bool {
    let mut layer_idx: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.layer == layer)
        .map(|(i, _)| i)
        .collect();
    if layer_idx.len() < 2 {
        return false;
    }
    let adj_layer = if from_prev {
        layer.checked_sub(1)
    } else {
        Some(layer + 1)
    };
    let Some(adj_layer) = adj_layer else {
        return false;
    };
    let mut adj_pos: HashMap<NodeKey, f64> = HashMap::new();
    let mut adj_nodes: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.layer == adj_layer)
        .map(|(i, _)| i)
        .collect();
    adj_nodes.sort_by(|&a, &b| {
        nodes[a]
            .cross
            .partial_cmp(&nodes[b].cross)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| node_tie(nodes, a).cmp(&node_tie(nodes, b)))
    });
    for (rank, &i) in adj_nodes.iter().enumerate() {
        adj_pos.insert(nodes[i].key, rank as f64);
    }

    let mut bary: Vec<(f64, i64, usize)> = Vec::with_capacity(layer_idx.len());
    let mut any_connected = false;
    for &i in &layer_idx {
        let key = nodes[i].key;
        let mut sum = 0.0;
        let mut cnt = 0.0;
        for e in edges {
            let (mine, other) = if e.src == key {
                (true, e.tgt)
            } else if e.tgt == key {
                (true, e.src)
            } else {
                (false, e.src)
            };
            if !mine {
                continue;
            }
            if let Some(&p) = adj_pos.get(&other) {
                sum += p;
                cnt += 1.0;
            }
        }
        let bc = if cnt > 0.0 {
            any_connected = true;
            sum / cnt
        } else {
            nodes[i].cross
        };
        let stable = (nodes[i].cross * 1000.0).round() as i64;
        bary.push((bc, stable, i));
    }
    if !any_connected {
        return false;
    }
    let first_bc = bary[0].0;
    if bary.iter().all(|b| (b.0 - first_bc).abs() < 1e-9) {
        return false;
    }
    layer_idx.sort_by(|&a, &b| {
        nodes[a]
            .cross
            .partial_cmp(&nodes[b].cross)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let before: Vec<usize> = layer_idx.clone();
    bary.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    for (rank, &(_, _, i)) in bary.iter().enumerate() {
        nodes[i].cross = rank as f64;
    }
    let after: Vec<usize> = bary.iter().map(|t| t.2).collect();
    before != after
}

fn assign_coordinates(
    nodes: &mut [PackNode],
    edges: &[PackEdge],
    layout: &LayoutOpts,
    metrics: &[NodeMetrics],
    budgets: &LayeredLayoutConfig,
) {
    let dir = layout.direction_or_default();
    let node_gap = layout.node_spacing_or_default();
    let layer_gap = layout.layer_spacing_or_default();
    let align = layout.align_or_default();
    let vertical_flow = matches!(dir, LayoutDirection::Up | LayoutDirection::Down);

    let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);

    // Cross-axis pack by box size (down/up: X widths; left/right: Y heights).
    for layer in 0..=max_layer {
        pack_layer_cross(nodes, layer, node_gap, vertical_flow);
    }

    if vertical_flow {
        // Down/up: straighten on box centers, then share a horizontal mark line
        // (baseline / common-core) within each layer — not mark-on-X packing.
        for _ in 0..budgets.align_passes {
            let mut moved = false;
            for layer in 0..=max_layer {
                if median_pull_marks(nodes, edges, layer, true) {
                    moved = true;
                }
            }
            for layer in (0..=max_layer).rev() {
                if median_pull_marks(nodes, edges, layer, false) {
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        for layer in 0..=max_layer {
            resolve_cross_overlaps(nodes, layer, node_gap, true);
        }
        assign_flow_by_marks(nodes, metrics, align, layer_gap);
    } else {
        // Left/right: layer fronts from widths; mark-align on Y across layers.
        let mut layer_thickness = vec![0.0_f64; max_layer + 1];
        for n in nodes.iter() {
            layer_thickness[n.layer] = layer_thickness[n.layer].max(n.w);
        }
        let mut layer_origin = vec![0.0_f64; max_layer + 1];
        for l in 1..=max_layer {
            layer_origin[l] = layer_origin[l - 1] + layer_thickness[l - 1] + layer_gap;
        }
        for n in nodes.iter_mut() {
            n.flow = layer_origin[n.layer];
        }

        for n in nodes.iter_mut() {
            if let Some(ri) = n.real {
                let (ml, _) = mark_cross_local(&metrics[ri], align);
                let top = n.cross - n.h * 0.5;
                n.cross = top + ml; // store mark
            }
        }
        for _ in 0..budgets.align_passes {
            let mut moved = false;
            for layer in 0..=max_layer {
                if median_pull_marks(nodes, edges, layer, true) {
                    moved = true;
                }
            }
            for layer in (0..=max_layer).rev() {
                if median_pull_marks(nodes, edges, layer, false) {
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        for layer in 0..=max_layer {
            apply_cross_marks_as_tops(nodes, layer, node_gap, align, metrics);
        }
    }

    // Mirror flow for left/up.
    if matches!(dir, LayoutDirection::Left | LayoutDirection::Up) {
        let max_flow = nodes
            .iter()
            .map(|n| {
                let thick = if vertical_flow { n.h } else { n.w };
                n.flow + thick
            })
            .fold(0.0_f64, f64::max);
        for n in nodes.iter_mut() {
            let thick = if vertical_flow { n.h } else { n.w };
            n.flow = max_flow - n.flow - thick;
        }
    }
}

fn pack_layer_cross(nodes: &mut [PackNode], layer: usize, node_gap: f64, vertical_flow: bool) {
    let idx = layer_indices_in_order(nodes, layer);
    let mut cursor = 0.0_f64;
    for &i in &idx {
        let span = if vertical_flow {
            nodes[i].w
        } else {
            nodes[i].h
        };
        nodes[i].cross = cursor + span * 0.5;
        cursor += span + node_gap;
    }
}

fn resolve_cross_overlaps(
    nodes: &mut [PackNode],
    layer: usize,
    node_gap: f64,
    vertical_flow: bool,
) {
    let idx = layer_indices_in_order(nodes, layer);
    let mut cursor = -node_gap;
    for &i in &idx {
        let span = if vertical_flow {
            nodes[i].w
        } else {
            nodes[i].h
        };
        let mut top = nodes[i].cross - span * 0.5;
        if top < cursor + node_gap {
            top = cursor + node_gap;
        }
        cursor = top + span;
        nodes[i].cross = top + span * 0.5;
    }
}

fn normalize_cross_origin(nodes: &mut [PackNode], vertical_flow: bool) {
    let min_top = nodes
        .iter()
        .filter(|n| n.real.is_some())
        .map(|n| {
            let span = if vertical_flow { n.w } else { n.h };
            n.cross - span * 0.5
        })
        .fold(f64::INFINITY, f64::min);
    if min_top.is_finite() && min_top < 0.0 {
        for n in nodes.iter_mut() {
            n.cross -= min_top;
        }
    }
}

/// Within each down/up layer, share one horizontal mark line (baseline / core).
fn assign_flow_by_marks(
    nodes: &mut [PackNode],
    metrics: &[NodeMetrics],
    align: LayerAlign,
    layer_gap: f64,
) {
    let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);
    let mut origin = 0.0_f64;
    for layer in 0..=max_layer {
        let idx: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.layer == layer)
            .map(|(i, _)| i)
            .collect();
        if idx.is_empty() {
            continue;
        }

        let mut mark_of: HashMap<usize, f64> = HashMap::new();
        for &i in &idx {
            let ml = if let Some(ri) = nodes[i].real {
                mark_flow_local(&metrics[ri], align).0
            } else {
                0.0
            };
            mark_of.insert(i, ml);
        }

        let max_ml = idx
            .iter()
            .map(|&i| mark_of[&i])
            .fold(0.0_f64, f64::max);
        let shared = origin + max_ml;
        for &i in &idx {
            nodes[i].flow = shared - mark_of[&i];
        }

        // Auto: unnamed cores share the mean named-baseline document line.
        if matches!(align, LayerAlign::Auto) {
            let mut named_doc = Vec::new();
            for &i in &idx {
                let Some(ri) = nodes[i].real else {
                    continue;
                };
                let (ml, is_text) = mark_flow_local(&metrics[ri], align);
                if is_text {
                    named_doc.push(nodes[i].flow + ml);
                }
            }
            if !named_doc.is_empty() {
                let mean_line = named_doc.iter().sum::<f64>() / named_doc.len() as f64;
                for &i in &idx {
                    let Some(ri) = nodes[i].real else {
                        continue;
                    };
                    let (_, is_text) = mark_flow_local(&metrics[ri], align);
                    if !is_text {
                        nodes[i].flow = mean_line - metrics[ri].centroid_local.1;
                    }
                }
            }
        }

        let mut bottom = origin;
        for &i in &idx {
            bottom = bottom.max(nodes[i].flow + nodes[i].h);
        }
        origin = bottom + layer_gap;
    }
}

fn median_pull_marks(
    nodes: &mut [PackNode],
    edges: &[PackEdge],
    layer: usize,
    from_prev: bool,
) -> bool {
    let adj_layer = if from_prev {
        layer.checked_sub(1)
    } else {
        Some(layer + 1)
    };
    let Some(adj_layer) = adj_layer else {
        return false;
    };
    let cross_of: HashMap<NodeKey, f64> = nodes
        .iter()
        .filter(|n| n.layer == adj_layer)
        .map(|n| (n.key, n.cross))
        .collect();
    let mut moved = false;
    for n in nodes.iter_mut().filter(|n| n.layer == layer) {
        let mut nbrs = Vec::new();
        for e in edges {
            let other = if e.src == n.key {
                e.tgt
            } else if e.tgt == n.key {
                e.src
            } else {
                continue;
            };
            if let Some(&c) = cross_of.get(&other) {
                nbrs.push(c);
            }
        }
        if nbrs.is_empty() {
            continue;
        }
        nbrs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let med = if nbrs.len() % 2 == 1 {
            nbrs[nbrs.len() / 2]
        } else {
            0.5 * (nbrs[nbrs.len() / 2 - 1] + nbrs[nbrs.len() / 2])
        };
        if (med - n.cross).abs() > 0.5 {
            n.cross = med;
            moved = true;
        }
    }
    moved
}

fn apply_cross_marks_as_tops(
    nodes: &mut [PackNode],
    layer: usize,
    node_gap: f64,
    align: LayerAlign,
    metrics: &[NodeMetrics],
) {
    let idx = layer_indices_in_order(nodes, layer);

    // Auto mixed: unnamed cores sit on the mean named-baseline line.
    if matches!(align, LayerAlign::Auto) {
        let mut named_centroids = Vec::new();
        for &i in &idx {
            let Some(ri) = nodes[i].real else {
                continue;
            };
            let (ml, is_text) = mark_cross_local(&metrics[ri], align);
            if is_text {
                named_centroids.push(nodes[i].cross - ml + metrics[ri].centroid_local.1);
            }
        }
        if !named_centroids.is_empty() {
            let mean_c = named_centroids.iter().sum::<f64>() / named_centroids.len() as f64;
            for &i in &idx {
                let Some(ri) = nodes[i].real else {
                    continue;
                };
                let (_, is_text) = mark_cross_local(&metrics[ri], align);
                if !is_text {
                    nodes[i].cross = mean_c;
                }
            }
        }
    }

    let mut tops: Vec<(usize, f64)> = Vec::new();
    for &i in &idx {
        let mark = nodes[i].cross;
        let ml = if let Some(ri) = nodes[i].real {
            mark_cross_local(&metrics[ri], align).0
        } else {
            0.0
        };
        tops.push((i, mark - ml));
    }
    let mut cursor = f64::NEG_INFINITY;
    for &mut (i, ref mut top) in tops.iter_mut() {
        let span = nodes[i].h;
        if cursor.is_finite() && *top < cursor + node_gap {
            *top = cursor + node_gap;
        }
        cursor = *top + span;
        nodes[i].cross = *top + span * 0.5;
    }
}

/// Local mark along Y for left/right packing (cross axis).
fn mark_cross_local(m: &NodeMetrics, align: LayerAlign) -> (f64, bool) {
    match align {
        LayerAlign::Top => (0.0, false),
        LayerAlign::Bottom => (m.h, false),
        LayerAlign::Middle => (m.h * 0.5, false),
        LayerAlign::Centroid => (m.centroid_local.1, false),
        LayerAlign::Baseline | LayerAlign::Auto => {
            if let Some(by) = m.baseline_local_y {
                (by, true)
            } else {
                (m.centroid_local.1, false)
            }
        }
    }
}

/// Local mark along Y for down/up layers (shared horizontal line).
fn mark_flow_local(m: &NodeMetrics, align: LayerAlign) -> (f64, bool) {
    mark_cross_local(m, align)
}



/// Last-line AABB separation for real tiles (order-preserving on cross, then flow).
fn separate_aabb_overlaps(nodes: &mut [PackNode], dir: LayoutDirection, gap: f64) {
    let vertical_flow = matches!(dir, LayoutDirection::Up | LayoutDirection::Down);
    let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);
    for layer in 0..=max_layer {
        resolve_cross_overlaps(nodes, layer, gap, vertical_flow);
    }
    // Push later layers down/right if they still collide with earlier ones.
    let mut reals: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.real.is_some())
        .map(|(i, _)| i)
        .collect();
    reals.sort_by(|&a, &b| {
        nodes[a]
            .layer
            .cmp(&nodes[b].layer)
            .then_with(|| nodes[a].ord.cmp(&nodes[b].ord))
    });
    for i in 0..reals.len() {
        for j in 0..i {
            let (ia, ib) = (reals[j], reals[i]);
            let (aw, ah) = (nodes[ia].w, nodes[ia].h);
            let (bw, bh) = (nodes[ib].w, nodes[ib].h);
            let (ax, ay) = if vertical_flow {
                (nodes[ia].cross - aw * 0.5, nodes[ia].flow)
            } else {
                (nodes[ia].flow, nodes[ia].cross - ah * 0.5)
            };
            let (bx, by) = if vertical_flow {
                (nodes[ib].cross - bw * 0.5, nodes[ib].flow)
            } else {
                (nodes[ib].flow, nodes[ib].cross - bh * 0.5)
            };
            let ox = (ax + aw).min(bx + bw) - ax.max(bx);
            let oy = (ay + ah).min(by + bh) - ay.max(by);
            if ox > gap && oy > gap {
                if vertical_flow {
                    nodes[ib].flow = ay + ah + gap;
                } else {
                    nodes[ib].flow = ax + aw + gap;
                }
            }
        }
    }
}

fn separate_position_overlaps(
    positions: &mut HashMap<String, Pt>,
    metrics: &[NodeMetrics],
    gap: f64,
) {
    let mut ids: Vec<String> = metrics.iter().map(|m| m.id.clone()).collect();
    ids.sort();
    for i in 0..ids.len() {
        for j in 0..i {
            let (ia, ib) = (&ids[j], &ids[i]);
            let Some(&(ax, ay)) = positions.get(ia.as_str()) else {
                continue;
            };
            let Some(&(bx, by)) = positions.get(ib.as_str()) else {
                continue;
            };
            let (aw, ah) = (metrics.iter().find(|m| m.id == *ia).map(|m| (m.w, m.h)).unwrap());
            let (bw, bh) = (metrics.iter().find(|m| m.id == *ib).map(|m| (m.w, m.h)).unwrap());
            let ox = (ax + aw).min(bx + bw) - ax.max(bx);
            let oy = (ay + ah).min(by + bh) - ay.max(by);
            if ox > gap && oy > gap {
                positions.insert(ib.clone(), (bx, ay + ah + gap));
            }
        }
    }
}

/// Pull reals toward mean of real parents/children (dummy chains contracted).
/// Pull dummies toward mean of their path's real endpoints. Each move is
/// clamped to slack against **real** same-layer neighbors (dummies ignored
/// for limits so gaps between reals are usable). Order is preserved.
fn pull_cross_toward_neighbors(
    nodes: &mut [PackNode],
    edges: &[PackEdge],
    gap: f64,
    vertical_flow: bool,
) -> bool {
    let paths = real_real_paths(nodes, edges);
    let mut real_sum: HashMap<usize, (f64, usize)> = HashMap::new();
    let mut dummy_target: HashMap<usize, (f64, usize)> = HashMap::new();
    for (src_i, tgt_i, dummies) in &paths {
        let sc = nodes[*src_i].cross;
        let tc = nodes[*tgt_i].cross;
        let e = real_sum.entry(*src_i).or_insert((0.0, 0));
        e.0 += tc;
        e.1 += 1;
        let e = real_sum.entry(*tgt_i).or_insert((0.0, 0));
        e.0 += sc;
        e.1 += 1;
        let corridor = 0.5 * (sc + tc);
        for &di in dummies {
            let e = dummy_target.entry(di).or_insert((0.0, 0));
            e.0 += corridor;
            e.1 += 1;
        }
    }

    let half = |n: &PackNode| -> f64 {
        if vertical_flow {
            n.w * 0.5
        } else {
            n.h * 0.5
        }
    };

    let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);
    let mut moved = false;
    for layer in 0..=max_layer {
        // Reals only — slack ignores dummies so a child can slide under its
        // parent when the gap is between real tiles.
        let reals: Vec<usize> = layer_indices_in_order(nodes, layer)
            .into_iter()
            .filter(|&i| nodes[i].real.is_some())
            .collect();
        for (k, &i) in reals.iter().enumerate() {
            let Some(&(sum, nmark)) = real_sum.get(&i) else {
                continue;
            };
            if nmark == 0 {
                continue;
            }
            let target = sum / nmark as f64;
            let my_half = half(&nodes[i]);
            let lo = if k > 0 {
                let j = reals[k - 1];
                nodes[j].cross + half(&nodes[j]) + gap + my_half
            } else {
                f64::NEG_INFINITY
            };
            let hi = if k + 1 < reals.len() {
                let j = reals[k + 1];
                nodes[j].cross - half(&nodes[j]) - gap - my_half
            } else {
                f64::INFINITY
            };
            if lo > hi {
                continue;
            }
            let new_cross = target.clamp(lo, hi);
            if (new_cross - nodes[i].cross).abs() > 0.5 {
                nodes[i].cross = new_cross;
                moved = true;
            }
        }

        // Dummies: toward corridor mean, clamped by full layer order (incl. reals).
        let idx = layer_indices_in_order(nodes, layer);
        for (k, &i) in idx.iter().enumerate() {
            if nodes[i].real.is_some() {
                continue;
            }
            let Some(&(sum, nmark)) = dummy_target.get(&i) else {
                continue;
            };
            if nmark == 0 {
                continue;
            }
            let target = sum / nmark as f64;
            let my_half = half(&nodes[i]);
            let lo = if k > 0 {
                let j = idx[k - 1];
                nodes[j].cross + half(&nodes[j]) + gap + my_half
            } else {
                f64::NEG_INFINITY
            };
            let hi = if k + 1 < idx.len() {
                let j = idx[k + 1];
                nodes[j].cross - half(&nodes[j]) - gap - my_half
            } else {
                f64::INFINITY
            };
            if lo > hi {
                continue;
            }
            let new_cross = target.clamp(lo, hi);
            if (new_cross - nodes[i].cross).abs() > 0.5 {
                nodes[i].cross = new_cross;
                moved = true;
            }
        }
    }
    moved
}

/// Each forward real→real path after contracting dummies: `(src, tgt, dummy idxs)`.
fn real_real_paths(
    nodes: &[PackNode],
    edges: &[PackEdge],
) -> Vec<(usize, usize, Vec<usize>)> {
    let idx_of: HashMap<NodeKey, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.key, i))
        .collect();
    let mut out_adj: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    for e in edges {
        out_adj.entry(e.src).or_default().push(e.tgt);
    }
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for (i, n) in nodes.iter().enumerate() {
        if n.real.is_none() {
            continue;
        }
        // DFS with path of dummy indices.
        let mut stack: Vec<(NodeKey, Vec<usize>)> = out_adj
            .get(&n.key)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|k| (k, Vec::new()))
            .collect();
        let mut visited = HashSet::new();
        visited.insert(n.key);
        while let Some((k, dummies)) = stack.pop() {
            if !visited.insert(k) {
                continue;
            }
            let Some(&j) = idx_of.get(&k) else {
                continue;
            };
            if nodes[j].real.is_some() {
                if nodes[j].layer > n.layer && seen.insert((i, j)) {
                    paths.push((i, j, dummies));
                }
                continue;
            }
            let mut next_dummies = dummies;
            next_dummies.push(j);
            if let Some(nbrs) = out_adj.get(&k) {
                for &nbr in nbrs {
                    stack.push((nbr, next_dummies.clone()));
                }
            }
        }
    }
    paths
}

/// Real→real endpoints of each edge path after contracting dummy chains.
fn real_real_edges(nodes: &[PackNode], edges: &[PackEdge]) -> Vec<(usize, usize)> {
    let idx_of: HashMap<NodeKey, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.key, i))
        .collect();
    let mut out_adj: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    for e in edges {
        out_adj.entry(e.src).or_default().push(e.tgt);
    }
    let mut pairs = Vec::new();
    let mut seen = HashSet::new();
    for (i, n) in nodes.iter().enumerate() {
        if n.real.is_none() {
            continue;
        }
        let mut stack: Vec<NodeKey> = out_adj.get(&n.key).cloned().unwrap_or_default();
        let mut visited = HashSet::new();
        visited.insert(n.key);
        while let Some(k) = stack.pop() {
            if !visited.insert(k) {
                continue;
            }
            let Some(&j) = idx_of.get(&k) else {
                continue;
            };
            if nodes[j].real.is_some() {
                if nodes[j].layer > n.layer && seen.insert((i, j)) {
                    pairs.push((i, j));
                }
                continue;
            }
            if let Some(nbrs) = out_adj.get(&k) {
                stack.extend(nbrs.iter().copied());
            }
        }
    }
    pairs
}

fn spoke_half_extent(n: &PackNode, vertical_flow: bool) -> f64 {
    if vertical_flow {
        n.w * 0.5
    } else {
        n.h * 0.5
    }
}

fn nudge_off_corridor(nodes: &mut [PackNode], i: usize, corridor: f64, clear: f64) {
    let d = nodes[i].cross - corridor;
    if d.abs() >= clear {
        return;
    }
    let dir = if d > 1e-9 {
        1.0
    } else if d < -1e-9 {
        -1.0
    } else if node_tie(nodes, i).0 % 2 == 0 {
        1.0
    } else {
        -1.0
    };
    nodes[i].cross = corridor + dir * clear;
}

/// Nudge intermediate-layer real tiles off the cross-axis corridor of long
/// parent→child edges so hub spokes need only short bows (or stay tips).
fn clear_long_edge_corridors(
    nodes: &mut [PackNode],
    edges: &[PackEdge],
    gap: f64,
    vertical_flow: bool,
) {
    let pairs = real_real_edges(nodes, edges);
    for &(si, ti) in &pairs {
        let ls = nodes[si].layer;
        let lt = nodes[ti].layer;
        if lt <= ls + 1 {
            continue;
        }
        let c0 = nodes[si].cross;
        let c1 = nodes[ti].cross;
        for i in 0..nodes.len() {
            if nodes[i].real.is_none() || i == si || i == ti {
                continue;
            }
            let l = nodes[i].layer;
            if l <= ls || l >= lt {
                continue;
            }
            let t = (l - ls) as f64 / (lt - ls) as f64;
            let corridor = c0 + (c1 - c0) * t;
            let clear = spoke_half_extent(&nodes[i], vertical_flow) + gap * 0.5;
            nudge_off_corridor(nodes, i, corridor, clear);
        }
    }
}


fn top_left_from_centers(n: &PackNode, dir: LayoutDirection) -> Pt {
    let vertical_flow = matches!(dir, LayoutDirection::Up | LayoutDirection::Down);
    if vertical_flow {
        // flow = y, cross = x
        (n.cross - n.w * 0.5, n.flow)
    } else {
        // flow = x, cross = y
        (n.flow, n.cross - n.h * 0.5)
    }
}

fn align_co_endpoint_mids(
    spec: &DepictSpec,
    positions: &mut HashMap<String, Pt>,
    metrics: &[NodeMetrics],
    layout: &LayoutOpts,
) {
    let size_of: HashMap<&str, (f64, f64)> = metrics
        .iter()
        .map(|m| (m.id.as_str(), (m.w, m.h)))
        .collect();
    let vertical_flow = matches!(
        layout.direction_or_default(),
        LayoutDirection::Up | LayoutDirection::Down
    );
    for edge in spec.edges() {
        if !edge.participates_in_layout() {
            continue;
        }
        let sources = edge.sources.as_slice();
        let targets = edge.targets.as_slice();
        if sources.len() < 2 && targets.len() < 2 {
            continue;
        }
        let Some(src_mid) = group_cross_mid(sources, positions, &size_of, vertical_flow) else {
            continue;
        };
        let Some(tgt_mid) = group_cross_mid(targets, positions, &size_of, vertical_flow) else {
            continue;
        };
        let delta = src_mid - tgt_mid;
        if delta.abs() < 0.5 {
            continue;
        }
        let move_targets = sources.len() >= 2;
        let ids = if move_targets { targets } else { sources };
        let signed = if move_targets { delta } else { -delta };
        for id in ids {
            if let Some(p) = positions.get_mut(id) {
                if vertical_flow {
                    p.0 += signed;
                } else {
                    p.1 += signed;
                }
            }
        }
    }
}

fn group_cross_mid(
    ids: &[String],
    pos: &HashMap<String, Pt>,
    size_of: &HashMap<&str, (f64, f64)>,
    vertical_flow: bool,
) -> Option<f64> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut any = false;
    for id in ids {
        let Some(&(x, y)) = pos.get(id) else {
            continue;
        };
        let Some(&(w, h)) = size_of.get(id.as_str()) else {
            continue;
        };
        if vertical_flow {
            lo = lo.min(x);
            hi = hi.max(x + w);
        } else {
            lo = lo.min(y);
            hi = hi.max(y + h);
        }
        any = true;
    }
    any.then_some(0.5 * (lo + hi))
}

/// Optional caption reach-back: shift name overlays so a named mol's chem
/// centroid can sit on a shared grid without moving CoordGen structure.
///
/// v1: no-op when baselines already justify; kept as the hook for gallery
/// `align-centroid` polish. Returns true if any overlay y changed.
/// Hook for a future caption-kit offset nudge under explicit centroid grids.
/// Metrics-only alignment covers Auto/Baseline tests in v1.
#[allow(dead_code)]
pub fn reachback_captions_for_centroid_grid(
    _paints: &mut [DocPaint],
    _positions: &HashMap<String, Pt>,
    _layout: &LayoutOpts,
) -> bool {
    false
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{DepictSpec, EdgeArrow, EdgeNode, EdgeNodeKind, MolNode, Node};

    fn scheme(children: Vec<Node>, layout: Option<LayoutOpts>) -> DepictSpec {
        DepictSpec::ReactionScheme {
            id: None,
            children,
            layout,
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
            svg: None,
        }
    }

    fn mol(id: &str, name: Option<&str>) -> Node {
        Node::Mol(MolNode {
            id: Some(id.into()),
            smiles: Some("CCO".into()),
            name: name.map(|s| crate::doc::MolText::Plain(s.into())),
            ..Default::default()
        })
    }

    fn edge(a: &str, b: &str) -> Node {
        soft_edge(a, b, true)
    }

    fn soft_edge(a: &str, b: &str, layout: bool) -> Node {
        Node::Edge(EdgeNode {
            type_: EdgeNodeKind::Edge,
            sources: crate::doc::MolIds::One(a.into()),
            targets: crate::doc::MolIds::One(b.into()),
            label: None,
            edge_routing: None,
            layout: if layout { None } else { Some(false) },
            role: None,
            arrow: EdgeArrow::Forward,
            color: None,
            stroke_width: None,
            dashed: !layout,
        })
    }

    fn paint_box(id: &str, w: f64, h: f64, baseline: Option<f64>, cy: f64) -> DocPaint {
        use crate::scene::{AtomIn, MoleculeIn, Scene, TextAnchor};
        let mut scene = Scene::empty(w, h);
        if let Some(by) = baseline {
            scene.overlays.push(Primitive::Text {
                x: w * 0.5,
                y: by,
                text: format!("name-{id}"),
                font_size: 12.0,
                fill: "#000".into(),
                anchor: TextAnchor::Middle,
                class: Some("mol-caption label".into()),
                rotate: None,
            });
        }
        DocPaint {
            id: id.into(),
            molecule: MoleculeIn {
                id: Some(id.into()),
                atoms: vec![AtomIn {
                    index: 0,
                    element: Some("C".to_string()),
                    z: Some(6),
                    x: w * 0.5,
                    y: cy,
                    label: None,
                    charge: 0,
                    radical: 0,
                    mapno: None,
                }],
                bonds: vec![],
                ..Default::default()
            },
            scene,
            frame_json: String::new(),
        }
    }

    #[test]
    fn order_invariant_under_permutation() {
        let sizes: Vec<(String, f64, f64)> = vec![
            ("A".to_string(), 40.0, 40.0),
            ("B".to_string(), 40.0, 40.0),
            ("C".to_string(), 40.0, 40.0),
        ];
        let paints: Vec<DocPaint> = sizes
            .iter()
            .map(|(id, w, h)| paint_box(id, *w, *h, None, 20.0))
            .collect();
        let spec_a = scheme(
            vec![
                mol("A", None),
                mol("B", None),
                mol("C", None),
                edge("A", "B"),
                edge("B", "C"),
            ],
            None,
        );
        let spec_b = scheme(
            vec![
                mol("C", None),
                edge("B", "C"),
                mol("B", None),
                edge("A", "B"),
                mol("A", None),
            ],
            None,
        );
        let layout = LayoutOpts::default();
        let pa = place_layered(&spec_a, &layout, &sizes, &paints).unwrap();
        let pb = place_layered(&spec_b, &layout, &sizes, &paints).unwrap();
        for id in ["A", "B", "C"] {
            let a = pa.positions[id];
            let b = pb.positions[id];
            assert!(
                (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6,
                "{id}: {a:?} vs {b:?}"
            );
        }
    }

    #[test]
    fn auto_align_named_shares_baseline() {
        let sizes: Vec<(String, f64, f64)> = vec![
            ("A".to_string(), 50.0, 80.0),
            ("B".to_string(), 50.0, 100.0),
        ];
        let paints = vec![
            paint_box("A", 50.0, 80.0, Some(70.0), 30.0),
            paint_box("B", 50.0, 100.0, Some(90.0), 40.0),
        ];
        let layout = LayoutOpts {
            align: Some(LayerAlign::Auto),
            ..Default::default()
        };
        let p = place_layered(
            &scheme(
                vec![mol("A", Some("ethanol")), mol("B", Some("big")), edge("A", "B")],
                Some(layout.clone()),
            ),
            &layout,
            &sizes,
            &paints,
        )
        .unwrap();
        let ya = p.positions["A"].1 + 70.0;
        let yb = p.positions["B"].1 + 90.0;
        assert!(
            (ya - yb).abs() < 1.0,
            "baselines should align: {ya} vs {yb} pos={:?}",
            p.positions
        );
    }

    #[test]
    fn layer_hint_forces_layer() {
        let sizes: Vec<(String, f64, f64)> = vec![
            ("A".to_string(), 40.0, 40.0),
            ("B".to_string(), 40.0, 40.0),
            ("C".to_string(), 40.0, 40.0),
        ];
        let paints: Vec<DocPaint> = sizes
            .iter()
            .map(|(id, w, h)| paint_box(id, *w, *h, None, 20.0))
            .collect();
        let mut b = match mol("B", None) {
            Node::Mol(m) => m,
            _ => unreachable!(),
        };
        b.layer = Some(0);
        let p = place_layered(
            &scheme(
                vec![
                    mol("A", None),
                    Node::Mol(b),
                    mol("C", None),
                    edge("A", "B"),
                    edge("B", "C"),
                ],
                None,
            ),
            &LayoutOpts::default(),
            &sizes,
            &paints,
        )
        .unwrap();
        assert_eq!(p.layers["B"], 0);
        assert!(p.layers["C"] >= 1);
    }

    #[test]
    fn soft_edge_promotes_shell_without_cycling_hard_dag() {
        // Hub→leaf hard; soft intermediate→leaf must deepen the leaf past the
        // intermediate. Soft hub←child feedback must not reverse the hub.
        let sizes: Vec<(String, f64, f64)> = ["H", "A", "I", "L"]
            .iter()
            .map(|id| (id.to_string(), 40.0, 40.0))
            .collect();
        let paints: Vec<DocPaint> = sizes
            .iter()
            .map(|(id, w, h)| paint_box(id, *w, *h, None, 20.0))
            .collect();
        let p = place_layered(
            &scheme(
                vec![
                    mol("H", None),
                    mol("A", None),
                    mol("I", None),
                    mol("L", None),
                    edge("H", "A"),
                    edge("H", "L"),
                    edge("A", "I"),
                    soft_edge("I", "L", false),
                    soft_edge("A", "H", false), // would cycle against H→A
                ],
                None,
            ),
            &LayoutOpts::default(),
            &sizes,
            &paints,
        )
        .unwrap();
        assert_eq!(p.layers["H"], 0);
        assert!(p.layers["I"] > p.layers["A"]);
        assert!(
            p.layers["L"] > p.layers["I"],
            "soft I→L must place L after I: {:?}",
            p.layers
        );
    }

    #[test]
    fn explicit_top_align_beats_auto() {
        let sizes: Vec<(String, f64, f64)> = vec![
            ("A".to_string(), 40.0, 60.0),
            ("B".to_string(), 40.0, 100.0),
        ];
        let paints = vec![
            paint_box("A", 40.0, 60.0, Some(50.0), 20.0),
            paint_box("B", 40.0, 100.0, Some(90.0), 40.0),
        ];
        let layout = LayoutOpts {
            align: Some(LayerAlign::Top),
            ..Default::default()
        };
        let p = place_layered(
            &scheme(
                vec![mol("A", Some("a")), mol("B", Some("b")), edge("A", "B")],
                Some(layout.clone()),
            ),
            &layout,
            &sizes,
            &paints,
        )
        .unwrap();
        assert!(
            (p.positions["A"].1 - p.positions["B"].1).abs() < 1.0,
            "tops should match across layers: {:?}",
            p.positions
        );
    }

    #[test]
    fn mixed_named_unnamed_auto() {
        let sizes: Vec<(String, f64, f64)> = vec![
            ("A".to_string(), 40.0, 80.0),
            ("B".to_string(), 40.0, 50.0),
        ];
        let paints = vec![
            paint_box("A", 40.0, 80.0, Some(70.0), 25.0),
            paint_box("B", 40.0, 50.0, None, 25.0),
        ];
        let layout = LayoutOpts {
            align: Some(LayerAlign::Auto),
            ..Default::default()
        };
        let p = place_layered(
            &scheme(
                vec![mol("A", Some("named")), mol("B", None), edge("A", "B")],
                Some(layout.clone()),
            ),
            &layout,
            &sizes,
            &paints,
        )
        .unwrap();
        let a_baseline = p.positions["A"].1 + 70.0;
        let b_centroid = p.positions["B"].1 + 25.0;
        assert!(
            (a_baseline - b_centroid).abs() < 2.0,
            "unnamed centroid mark on named baseline: {a_baseline} vs {b_centroid} {:?}",
            p.positions
        );
    }

    #[test]
    fn down_auto_named_shares_baseline_in_layer() {
        // Fan: two named products in one down-layer must share caption Y.
        let sizes: Vec<(String, f64, f64)> = vec![
            ("S".to_string(), 40.0, 40.0),
            ("A".to_string(), 50.0, 80.0),
            ("B".to_string(), 50.0, 110.0),
        ];
        let paints = vec![
            paint_box("S", 40.0, 40.0, Some(35.0), 20.0),
            paint_box("A", 50.0, 80.0, Some(70.0), 30.0),
            paint_box("B", 50.0, 110.0, Some(100.0), 40.0),
        ];
        let layout = LayoutOpts {
            direction: Some(LayoutDirection::Down),
            align: Some(LayerAlign::Auto),
            ..Default::default()
        };
        let p = place_layered(
            &scheme(
                vec![
                    mol("S", Some("src")),
                    mol("A", Some("ethanol")),
                    mol("B", Some("phenol")),
                    edge("S", "A"),
                    edge("S", "B"),
                ],
                Some(layout.clone()),
            ),
            &layout,
            &sizes,
            &paints,
        )
        .unwrap();
        assert_eq!(p.layers["A"], p.layers["B"]);
        let ya = p.positions["A"].1 + 70.0;
        let yb = p.positions["B"].1 + 100.0;
        assert!(
            (ya - yb).abs() < 1.0,
            "down-layer baselines should align: {ya} vs {yb} {:?}",
            p.positions
        );
    }

    #[test]
    fn down_centroid_align_in_layer() {
        let sizes: Vec<(String, f64, f64)> = vec![
            ("S".to_string(), 40.0, 40.0),
            ("A".to_string(), 50.0, 80.0),
            ("B".to_string(), 50.0, 110.0),
        ];
        let paints = vec![
            paint_box("S", 40.0, 40.0, None, 20.0),
            paint_box("A", 50.0, 80.0, None, 25.0),
            paint_box("B", 50.0, 110.0, None, 55.0),
        ];
        let layout = LayoutOpts {
            direction: Some(LayoutDirection::Down),
            align: Some(LayerAlign::Centroid),
            ..Default::default()
        };
        let p = place_layered(
            &scheme(
                vec![
                    mol("S", None),
                    mol("A", None),
                    mol("B", None),
                    edge("S", "A"),
                    edge("S", "B"),
                ],
                Some(layout.clone()),
            ),
            &layout,
            &sizes,
            &paints,
        )
        .unwrap();
        let ya = p.positions["A"].1 + 25.0;
        let yb = p.positions["B"].1 + 55.0;
        assert!(
            (ya - yb).abs() < 1.0,
            "down-layer centroids should align: {ya} vs {yb} {:?}",
            p.positions
        );
    }

    #[test]
    fn fan_hub_centers_over_products() {
        let fan: DepictSpec = serde_json::from_str(include_str!(
            "../../tests/fixtures/scheme/fan.json"
        ))
        .unwrap();
        // Synthetic sizes / paints — enough to exercise packing geometry.
        let mols = fan.mols();
        let sizes: Vec<(String, f64, f64)> = mols
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let id = m.id.clone().unwrap_or_else(|| format!("m{i}"));
                let h = if id == "src" { 160.0 } else { 80.0 };
                (id, 90.0, h)
            })
            .collect();
        let paints: Vec<DocPaint> = sizes
            .iter()
            .map(|(id, w, h)| {
                let cy = if id == "src" { 60.0 } else { 40.0 };
                let by = if id == "src" { Some(140.0) } else { None };
                paint_box(id, *w, *h, by, cy)
            })
            .collect();
        let layout = LayoutOpts::default();
        let p = place_layered(&fan, &layout, &sizes, &paints).unwrap();
        let src = p.positions["src"];
        let mut ys = Vec::new();
        let mut xs = Vec::new();
        for (id, &(x, y)) in &p.positions {
            if id == "src" {
                continue;
            }
            ys.push(y + 40.0); // approx centroid
            xs.push(x);
        }
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mid = 0.5 * (ys[0] + ys[ys.len() - 1]);
        let src_c = src.1 + 80.0;
        assert!(
            (src_c - mid).abs() < 30.0,
            "hub should sit near product column mid: hub={src_c} mid={mid} pos={:?}",
            p.positions
        );
        let xmin = xs.iter().cloned().fold(f64::INFINITY, f64::min);
        let xmax = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!(
            (xmax - xmin).abs() < 1.0,
            "same-layer products must share flow x: {xmin}..{xmax}"
        );
    }

    #[test]
    fn common_core_centroid_uses_align_tags() {
        use crate::scene::{AlignDebug, AlignDebugRole, AtomIn, MoleculeIn, Scene};
        let mut scene = Scene::empty(100.0, 80.0);
        let _ = &mut scene;
        let mut paint = DocPaint {
            id: "Q".into(),
            molecule: MoleculeIn {
                id: Some("Q".into()),
                atoms: vec![
                    AtomIn {
                        index: 0,
                        element: Some("C".into()),
                        z: Some(6),
                        x: 10.0,
                        y: 20.0,
                        label: None,
                        charge: 0,
                        radical: 0,
                        mapno: None,
                    },
                    AtomIn {
                        index: 1,
                        element: Some("C".into()),
                        z: Some(6),
                        x: 30.0,
                        y: 20.0,
                        label: None,
                        charge: 0,
                        radical: 0,
                        mapno: None,
                    },
                    AtomIn {
                        index: 2,
                        element: Some("O".into()),
                        z: Some(8),
                        x: 90.0,
                        y: 70.0,
                        label: None,
                        charge: 0,
                        radical: 0,
                        mapno: None,
                    },
                ],
                bonds: vec![],
                align_debug: Some(AlignDebug {
                    role: AlignDebugRole::Query,
                    peer_id: Some("T".into()),
                    tags: vec![(0, 0), (1, 1)],
                }),
                ..Default::default()
            },
            scene,
            frame_json: String::new(),
        };
        let c = local_chem_centroid(Some(&paint), 100.0, 80.0);
        // Core mean of atoms 0,1 at y=20 — not dragged by substituent at y=70.
        assert!(
            (c.1 - 20.0).abs() < 1e-6,
            "common-core y should be 20, got {:?}",
            c
        );
        paint.molecule.align_debug = None;
        let full = local_chem_centroid(Some(&paint), 100.0, 80.0);
        assert!(
            (full.1 - (20.0 + 20.0 + 70.0) / 3.0).abs() < 1e-6,
            "without tags, mean of all atoms: {:?}",
            full
        );
    }

    #[test]
    fn morphine_norcodeine_i0_leg_swap_cuts_abstract_crossings() {
        use crate::doc::depict_doc::test_scheme_paints;
        use crate::doc::spec::DepictSpec;
        const MORPHINE: &str =
            include_str!("../../tests/fixtures/xenobase/morphine-sulfate.json");
        let spec: DepictSpec = serde_json::from_str(MORPHINE).unwrap();
        let layout = match &spec {
            DepictSpec::ReactionScheme { layout, .. } => layout.clone().unwrap_or_default(),
            _ => LayoutOpts::default(),
        };
        let paints = test_scheme_paints(&spec).unwrap();
        let sizes: Vec<(String, f64, f64)> = paints
            .iter()
            .map(|p| (p.id.clone(), p.scene.width, p.scene.height))
            .collect();
        let budgets = &layout.engine().layered;
        let metrics = collect_metrics(&spec, &sizes, &paints);
        let id_of: HashMap<String, usize> = metrics
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id.clone(), i))
            .collect();
        let packing = packing_edges(&spec, &id_of);
        let (feedback, forward) = break_cycles(&packing, metrics.len(), budgets);
        let mut layer_of = assign_layers(&forward, &metrics, metrics.len());
        apply_layer_hints(&mut layer_of, &metrics);
        let hinted: HashSet<usize> = metrics
            .iter()
            .enumerate()
            .filter_map(|(i, m)| m.layer_hint.map(|_| i))
            .collect();
        promote_strict_layers(&mut layer_of, &forward, &hinted);
        let soft = soft_order_edges(&spec, &id_of);
        soft_promote_layers(&mut layer_of, &soft, &forward, &hinted);
        promote_strict_layers(&mut layer_of, &forward, &hinted);
        compact_layer_indices(&mut layer_of);
        let (mut nodes, mut edges) = insert_dummies(&metrics, &layer_of, &forward, &feedback);
        append_soft_edges_with_dummies(&mut nodes, &mut edges, &layer_of, &soft);
        order_layers(&mut nodes, &edges, budgets);

        let name_of = |key: NodeKey| -> String {
            match key {
                NodeKey::Real(ri) => metrics[ri].id.clone(),
                NodeKey::Dummy(d) => format!("d{d}"),
            }
        };
        let cross_of = |id: &str| -> f64 {
            nodes
                .iter()
                .find(|n| matches!(n.key, NodeKey::Real(ri) if metrics[ri].id == id))
                .map(|n| n.cross)
                .unwrap_or_else(|| panic!("missing {id}"))
        };

        let chords = total_chord_crossings(&nodes, &edges);
        for layer in 0..=nodes.iter().map(|n| n.layer).max().unwrap_or(0) {
            let mut idx: Vec<usize> = nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.layer == layer)
                .map(|(i, _)| i)
                .collect();
            idx.sort_by(|&a, &b| {
                nodes[a]
                    .cross
                    .partial_cmp(&nodes[b].cross)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let names: Vec<String> = idx.iter().map(|&i| name_of(nodes[i].key)).collect();
            eprintln!("L{layer}: {names:?}");
        }
        eprintln!("chord_crossings={chords}");

        // Pre-fix unit-segment scoring sat at 1 and refused the norcodeine↔i0
        // swap; chord scoring sees the true 3→1 win and must land on it.
        assert_eq!(
            chords, 1,
            "order_layers must reach 1 real→real chord crossing (Morphine)"
        );
        assert!(
            cross_of("i0") < cross_of("xbm:0001684"),
            "i0 must sit left of norcodeine after the improving leg swap"
        );
        assert!(
            cross_of("xbm:0001696") > cross_of("xbm:0005110"),
            "norcodeine's child must follow norcodeine to the right of i0's children"
        );
    }

}