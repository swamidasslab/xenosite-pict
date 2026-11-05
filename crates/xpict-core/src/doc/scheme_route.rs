//! Libavoid-style scheme shaft routing: straight chord first, else a
//! visibility graph on inflated full-node-ink hulls with multi-penalty search.
//!
//! ELK places molecules only. Painted routes never reuse ELK edge sections.

use std::collections::{HashMap, HashSet};

use pathfinding::directed::dijkstra::dijkstra;

use crate::arrows::{
    min_shaft_px, offset_polyline, prune_close_controls, reverse_source_inset,
    shorten_polyline_start, CLOSE_CTRL_PX,
};
use crate::metrics::BOND_PX;

use super::layout_constraints::{convex_hull_pts, edge_signed_distance, LayoutWorld, Shape};
use super::scheme_layout::SCHEME_INK_STANDOFF;
use super::{DepictSpec, DocPaint, EdgeNode};

type Pt = (f64, f64);

/// Soft costs (libavoid-ish scales in document px).
const SEGMENT_PENALTY: f64 = 80.0;
const ANGLE_PENALTY: f64 = 120.0;
const SHARED_PATH_PENALTY: f64 = 110.0;
/// Soft cost per crossing with an already-routed shaft. High enough that two
/// crossings lose to a longer clean detour (libavoid ~200; we go higher).
const CROSSING_PENALTY: f64 = 450.0;
/// Inflate full-ink hulls by this before visibility (shapeBufferDistance).
const ROUTING_CLEARANCE: f64 = SCHEME_INK_STANDOFF + 0.5 * 1.6;
/// Parallel-track gap after shared corridors (idealNudgingDistance).
const NUDGE_PX: f64 = 8.0;
const COTERMINAL_LANE_PX: f64 = 8.0;
const COTERMINAL_PX: f64 = 8.0;
/// Tip must sit at least this far from another arrow's open shaft (no false branch).
const TIP_SHAFT_CLEAR: f64 = BOND_PX;

struct ArrowGroup {
    members: Vec<usize>,
    reversed: Vec<bool>,
}

/// One convex obstacle per mol (inflated full-node ink).
struct Obstacle {
    mol_id: String,
    /// Inflated hull vertices (CCW).
    hull: Vec<Pt>,
}

/// Committed route for soft share/cross penalties.
struct Committed {
    segs: Vec<(Pt, Pt)>,
}

pub struct RouteResult {
    pub paths: Vec<Option<Vec<Pt>>>,
}

/// Route every scheme edge. `world` must already have `scheme/mols/{id}` ink
/// leaves installed (chem + captions). ELK `edge_paths` are ignored.
pub fn route_scheme_edges(
    spec: &DepictSpec,
    positions: &HashMap<String, Pt>,
    paints: &[DocPaint],
    sizes: &[(String, f64, f64)],
    world: &LayoutWorld,
    mol_ids: &[String],
) -> RouteResult {
    let n_edges = spec.edges().len();
    let mut out = vec![None; n_edges];
    if n_edges == 0 {
        return RouteResult { paths: out };
    }

    let size_of: HashMap<&str, (f64, f64)> = sizes
        .iter()
        .map(|(id, w, h)| (id.as_str(), (*w, *h)))
        .collect();
    let paint_of: HashMap<&str, &DocPaint> = paints.iter().map(|p| (p.id.as_str(), p)).collect();

    let centroids: HashMap<String, Pt> = mol_ids
        .iter()
        .map(|id| {
            (
                id.clone(),
                mol_centroid(&paint_of, positions, &size_of, id),
            )
        })
        .collect();

    let obstacles = build_obstacles(mol_ids, world, ROUTING_CLEARANCE);

    // Seed centroid chords for coterminal grouping (ELK paths discarded).
    let mut seeds: Vec<Vec<Pt>> = Vec::with_capacity(n_edges);
    for edge in spec.edges() {
        let (src, tgt) = edge_src_tgt(edge);
        let a = centroids.get(&src).copied().unwrap_or((0.0, 0.0));
        let b = centroids.get(&tgt).copied().unwrap_or((0.0, 0.0));
        seeds.push(vec![a, b]);
    }
    let groups = coterminal_groups(&spec.edges(), &seeds);

    // Visibility graph once per scheme (shared obstacle hull verts).
    let vis = build_visibility_graph(&obstacles);

    // ---- Pass 1: draft every group as a direct tip–tip chord (or a
    // mol-clearing detour with *empty* committed set). Intersection counts on
    // this draft tell the second pass which arrows should go the long way.
    let mut drafts: Vec<Vec<Pt>> = Vec::with_capacity(groups.len());
    let mut group_ends: Vec<(String, String)> = Vec::with_capacity(groups.len());
    let mut tips: Vec<(Pt, Pt)> = Vec::with_capacity(groups.len());
    let mut skips: Vec<HashSet<String>> = Vec::with_capacity(groups.len());
    let mut clashes: Vec<f64> = Vec::with_capacity(groups.len());

    for group in &groups {
        let rep = group.members[0];
        let edge = &spec.edges()[rep];
        let (src, tgt) = edge_src_tgt(edge);
        let src_c = centroids.get(&src).copied().unwrap_or((0.0, 0.0));
        let tgt_c = centroids.get(&tgt).copied().unwrap_or((0.0, 0.0));
        let min_shaft = min_shaft_px(edge.stroke_width);
        let clash = shaft_chem_clash(edge.stroke_width, group.members.len());
        let start = tip_pose(world, &src, src_c, tgt_c, clash, min_shaft);
        let goal = tip_pose(world, &tgt, tgt_c, src_c, clash, min_shaft);
        let skip_owned: HashSet<String> = [src.clone(), tgt.clone()].into_iter().collect();

        // Draft prefers the geometric direct line even when it hits ink — that
        // is what reveals crossings. Real clearance comes in pass 2.
        let draft = prune_close_controls(&[start, goal], CLOSE_CTRL_PX);
        tips.push((start, goal));
        skips.push(skip_owned);
        clashes.push(clash);
        group_ends.push((src, tgt));
        drafts.push(if draft.len() >= 2 {
            draft
        } else {
            vec![start, goal]
        });
    }

    let draft_cross = draft_crossing_counts(&drafts);

    // ---- Pass 2: lock clean straights first, then re-route high-crossing /
    // mol-blocked arrows against the locked set (so a long detour beats two
    // intersections with already-fixed corridors).
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by_key(|&i| {
        let straight_ok = drafts[i].len() == 2
            && {
                let skip: HashSet<&str> = skips[i].iter().map(String::as_str).collect();
                let (s, g) = tips[i];
                segment_clears_obstacles(s, g, &obstacles, &skip)
                    && chord_clears_ink(world, s, g, mol_ids, &skip, clashes[i])
            };
        // Clean straights first (0), then few crossings, then many.
        let tier = if straight_ok && draft_cross[i] == 0 {
            0
        } else if straight_ok {
            1
        } else {
            2
        };
        (tier, std::cmp::Reverse(draft_cross[i]), i)
    });

    let mut group_centers: Vec<Vec<Pt>> = vec![Vec::new(); groups.len()];
    let mut committed: Vec<Committed> = Vec::new();

    for &gi in &order {
        let (src, tgt) = &group_ends[gi];
        let (start, goal) = tips[gi];
        let skip: HashSet<&str> = skips[gi].iter().map(String::as_str).collect();
        let clash = clashes[gi];

        let keep_draft = drafts[gi].len() == 2
            && segment_clears_obstacles(start, goal, &obstacles, &skip)
            && chord_clears_ink(world, start, goal, mol_ids, &skip, clash)
            && count_crossings_with(&drafts[gi], &committed) == 0
            && !tip_hits_committed_shaft(start, &committed)
            && !tip_hits_committed_shaft(goal, &committed);

        let center = if keep_draft {
            drafts[gi].clone()
        } else {
            // Against locked corridors: high crossing cost → long way around.
            route_one_centerline(
                start,
                goal,
                src,
                tgt,
                &skip,
                world,
                mol_ids,
                clash,
                &obstacles,
                &vis,
                &committed,
            )
        };

        let center = if center.len() >= 2 {
            center
        } else {
            vec![start, goal]
        };
        committed.push(Committed {
            segs: center.windows(2).map(|w| (w[0], w[1])).collect(),
        });
        group_centers[gi] = center;
    }

    // Pull tips off foreign shafts so an arrival does not look like a branch.
    clear_tips_from_foreign_shafts(&mut group_centers, &group_ends, &centroids);
    for (gi, center) in group_centers.iter_mut().enumerate() {
        let skip: HashSet<&str> = skips[gi].iter().map(String::as_str).collect();
        *center = string_pull(center, &obstacles, &skip);
    }

    nudge_shared_corridors(&mut group_centers);

    let edge_list = spec.edges();
    for (gi, group) in groups.iter().enumerate() {
        let center = &group_centers[gi];
        let offs = lane_offsets(group.members.len());
        let has_reverse = group.reversed.iter().any(|&r| r);
        let inset = if has_reverse && group.members.len() > 1 {
            let stroke = group
                .members
                .iter()
                .filter_map(|&i| edge_list.get(i).and_then(|e| e.stroke_width))
                .fold(0.0_f64, f64::max);
            let stroke = if stroke > 0.0 { Some(stroke) } else { None };
            reverse_source_inset(stroke)
        } else {
            0.0
        };
        for (lane, &i) in group.members.iter().enumerate() {
            let mut pts = offset_polyline(center, offs[lane]);
            if group.reversed[lane] {
                pts.reverse();
            }
            pts = prune_close_controls(&pts, CLOSE_CTRL_PX);
            if inset > 0.0 {
                let min_keep = min_shaft_px(edge_list.get(i).and_then(|e| e.stroke_width));
                let len: f64 = pts
                    .windows(2)
                    .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
                    .sum();
                let max_inset = (len - min_keep).max(0.0);
                pts = shorten_polyline_start(&pts, inset.min(max_inset));
            }
            if pts.len() >= 2 {
                out[i] = Some(pts);
            }
        }
    }

    RouteResult { paths: out }
}

/// How many other draft polylines each draft properly crosses.
fn draft_crossing_counts(drafts: &[Vec<Pt>]) -> Vec<usize> {
    let n = drafts.len();
    let mut counts = vec![0usize; n];
    for i in 0..n {
        for j in (i + 1)..n {
            let c = count_poly_crossings(&drafts[i], &drafts[j]);
            if c > 0 {
                counts[i] += c;
                counts[j] += c;
            }
        }
    }
    counts
}

fn count_poly_crossings(a: &[Pt], b: &[Pt]) -> usize {
    let mut n = 0;
    for w in a.windows(2) {
        for z in b.windows(2) {
            if segments_properly_cross(w[0], w[1], z[0], z[1]) {
                n += 1;
            }
        }
    }
    n
}

fn count_crossings_with(pts: &[Pt], committed: &[Committed]) -> usize {
    let mut n = 0;
    for w in pts.windows(2) {
        for c in committed {
            for &(p, q) in &c.segs {
                if segments_properly_cross(w[0], w[1], p, q) {
                    n += 1;
                }
            }
        }
    }
    n
}

fn tip_hits_committed_shaft(tip: Pt, committed: &[Committed]) -> bool {
    for c in committed {
        for &(a, b) in &c.segs {
            let (d, t) = point_seg_dist(tip, a, b);
            if (0.12..=0.88).contains(&t) && d < TIP_SHAFT_CLEAR {
                return true;
            }
        }
    }
    false
}

/// Straight chord first; else visibility + string-pull, preferring a simple
/// single bow when the visibility path is windy for little gain.
fn route_one_centerline(
    start: Pt,
    goal: Pt,
    src: &str,
    tgt: &str,
    skip: &HashSet<&str>,
    world: &LayoutWorld,
    mol_ids: &[String],
    clash: f64,
    obstacles: &[Obstacle],
    vis: &VisGraph,
    committed: &[Committed],
) -> Vec<Pt> {
    if segment_clears_obstacles(start, goal, obstacles, skip)
        && chord_clears_ink(world, start, goal, mol_ids, skip, clash)
    {
        return prune_close_controls(&[start, goal], CLOSE_CTRL_PX);
    }

    let vis_path = visibility_route(start, goal, src, tgt, vis, obstacles, committed);
    let pulled = string_pull(&vis_path, obstacles, skip);
    // If string-pull opened a near-straight corridor, keep it.
    if pulled.len() == 2 {
        return pulled;
    }

    let bow = fallback_bow(start, goal, obstacles, skip);
    prefer_simpler_path(&pulled, &bow)
}

/// Remove intermediate verts when the chord still clears obstacles (classic
/// string-pull / funnel). Stops hull-hugging when a straighter cut exists.
fn string_pull(pts: &[Pt], obstacles: &[Obstacle], skip: &HashSet<&str>) -> Vec<Pt> {
    if pts.len() < 3 {
        return prune_close_controls(pts, CLOSE_CTRL_PX);
    }
    let mut out = vec![pts[0]];
    let mut i = 0;
    while i < pts.len() - 1 {
        let mut j = pts.len() - 1;
        while j > i + 1 {
            if segment_clears_obstacles(pts[i], pts[j], obstacles, skip) {
                break;
            }
            j -= 1;
        }
        out.push(pts[j]);
        i = j;
    }
    prune_close_controls(&out, CLOSE_CTRL_PX)
}

/// Prefer fewer elbows when length is comparable (within ~25%).
fn prefer_simpler_path(a: &[Pt], b: &[Pt]) -> Vec<Pt> {
    let la = polyline_len(a);
    let lb = polyline_len(b);
    if a.len() < 2 {
        return b.to_vec();
    }
    if b.len() < 2 {
        return a.to_vec();
    }
    let bends_a = a.len().saturating_sub(2);
    let bends_b = b.len().saturating_sub(2);
    // Same bend count → shorter. Fewer bends wins if not much longer.
    if bends_b < bends_a && lb <= la * 1.25 + BOND_PX {
        return b.to_vec();
    }
    if bends_a < bends_b && la <= lb * 1.25 + BOND_PX {
        return a.to_vec();
    }
    if la <= lb {
        a.to_vec()
    } else {
        b.to_vec()
    }
}

fn polyline_len(pts: &[Pt]) -> f64 {
    pts.windows(2)
        .map(|w| dist(w[0], w[1]))
        .sum()
}

/// If a tip sits on another arrow's open shaft it reads as a branch. Pull that
/// tip toward its mol centroid (along the end chord) until clear.
fn clear_tips_from_foreign_shafts(
    centers: &mut [Vec<Pt>],
    ends: &[(String, String)],
    centroids: &HashMap<String, Pt>,
) {
    let n = centers.len();
    if n < 2 {
        return;
    }
    let segs: Vec<Vec<(Pt, Pt)>> = centers
        .iter()
        .map(|c| c.windows(2).map(|w| (w[0], w[1])).collect())
        .collect();

    for i in 0..n {
        if centers[i].len() < 2 {
            continue;
        }
        let last = centers[i].len() - 1;
        let (ref src, ref tgt) = ends[i];
        for &(end, mol_id) in &[(0usize, src.as_str()), (last, tgt.as_str())] {
            let tip = centers[i][end];
            let Some((d, _)) = nearest_foreign_shaft(tip, i, &segs) else {
                continue;
            };
            if d >= TIP_SHAFT_CLEAR {
                continue;
            }
            let mol_c = centroids.get(mol_id).copied().unwrap_or(tip);
            let toward = if end == 0 {
                centers[i][1]
            } else {
                centers[i][last - 1]
            };
            let pull = (TIP_SHAFT_CLEAR - d + 2.0).min(BOND_PX * 2.0);
            let dx = mol_c.0 - tip.0;
            let dy = mol_c.1 - tip.1;
            let l = dx.hypot(dy).max(1e-9);
            let mut new_tip = (tip.0 + dx / l * pull, tip.1 + dy / l * pull);
            // Keep tip on the centroid→toward ray when possible.
            let rdx = toward.0 - mol_c.0;
            let rdy = toward.1 - mol_c.1;
            let rl = rdx.hypot(rdy).max(1e-9);
            let t_along =
                ((new_tip.0 - mol_c.0) * rdx + (new_tip.1 - mol_c.1) * rdy) / (rl * rl);
            if t_along > 0.05 {
                new_tip = (mol_c.0 + rdx * t_along, mol_c.1 + rdy * t_along);
            }
            centers[i][end] = new_tip;
        }
        centers[i] = prune_close_controls(&centers[i], CLOSE_CTRL_PX);
    }
}

fn nearest_foreign_shaft(
    tip: Pt,
    self_i: usize,
    segs: &[Vec<(Pt, Pt)>],
) -> Option<(f64, (Pt, Pt))> {
    let mut best: Option<(f64, (Pt, Pt))> = None;
    for (j, js) in segs.iter().enumerate() {
        if j == self_i {
            continue;
        }
        for &(a, b) in js {
            let (d, t) = point_seg_dist(tip, a, b);
            // Ignore near the foreign shaft's own terminals (shared mol tips).
            if !(0.12..=0.88).contains(&t) {
                continue;
            }
            if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                best = Some((d, (a, b)));
            }
        }
    }
    best
}

fn point_seg_dist(p: Pt, a: Pt, b: Pt) -> (f64, f64) {
    let vx = b.0 - a.0;
    let vy = b.1 - a.1;
    let l2 = vx * vx + vy * vy;
    if l2 < 1e-12 {
        return (dist(p, a), 0.0);
    }
    let t = (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / l2).clamp(0.0, 1.0);
    let q = (a.0 + t * vx, a.1 + t * vy);
    (dist(p, q), t)
}

fn build_obstacles(mol_ids: &[String], world: &LayoutWorld, inflate: f64) -> Vec<Obstacle> {
    let mut out = Vec::with_capacity(mol_ids.len());
    for id in mol_ids {
        let under = format!("scheme/mols/{id}");
        let pts = collect_ink_points(world, &under);
        let hull = if pts.len() >= 3 {
            convex_hull_pts(&pts)
        } else if let Some(a) = world.get(&under).map(|n| n.world_shape().aabb()) {
            vec![
                (a.x0, a.y0),
                (a.x1, a.y0),
                (a.x1, a.y1),
                (a.x0, a.y1),
            ]
        } else {
            continue;
        };
        let hull = if hull.len() >= 3 {
            simplify_hull(&inflate_convex(&hull, inflate), 12)
        } else {
            hull
        };
        if hull.len() >= 3 {
            out.push(Obstacle {
                mol_id: id.clone(),
                hull,
            });
        }
    }
    out
}

/// Keep at most `max_verts` by uniform stride (convex hull stays convex).
fn simplify_hull(hull: &[Pt], max_verts: usize) -> Vec<Pt> {
    if hull.len() <= max_verts {
        return hull.to_vec();
    }
    let n = hull.len();
    let mut out = Vec::with_capacity(max_verts);
    for i in 0..max_verts {
        let idx = (i * n) / max_verts;
        out.push(hull[idx]);
    }
    out
}

struct VisGraph {
    pts: Vec<Pt>,
    owner: Vec<Option<String>>,
    /// Adjacency among `pts` only (length cost).
    adj: Vec<Vec<(usize, f64)>>,
}

fn build_visibility_graph(obstacles: &[Obstacle]) -> VisGraph {
    let mut pts = Vec::new();
    let mut owner = Vec::new();
    // Ranges of verts per obstacle for boundary edges.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for o in obstacles {
        let start = pts.len();
        for &p in &o.hull {
            pts.push(p);
            owner.push(Some(o.mol_id.clone()));
        }
        ranges.push((start, pts.len()));
    }
    let n = pts.len();
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    let mut linked = vec![HashSet::<usize>::new(); n];
    let add = |adj: &mut Vec<Vec<(usize, f64)>>,
               linked: &mut [HashSet<usize>],
               i: usize,
               j: usize,
               len: f64| {
        if i == j || len < 1e-6 || !linked[i].insert(j) {
            return;
        }
        linked[j].insert(i);
        adj[i].push((j, len));
        adj[j].push((i, len));
    };

    // 1) Inflated-hull boundary edges (walk around each obstacle).
    for (ri, &(lo, hi)) in ranges.iter().enumerate() {
        let m = hi - lo;
        if m < 2 {
            continue;
        }
        let skip: HashSet<&str> = [obstacles[ri].mol_id.as_str()].into_iter().collect();
        for k in 0..m {
            let i = lo + k;
            let j = lo + (k + 1) % m;
            if segment_clears_obstacles(pts[i], pts[j], obstacles, &skip) {
                add(&mut adj, &mut linked, i, j, dist(pts[i], pts[j]));
            }
        }
    }

    // 2) Sparse long-range: each vert → up to K nearest verts on *other* obstacles.
    const K_NEAR: usize = 8;
    for i in 0..n {
        let oi = owner[i].as_deref();
        let mut cand: Vec<(f64, usize)> = (0..n)
            .filter(|&j| j != i && owner[j].as_deref() != oi)
            .map(|j| (dist(pts[i], pts[j]), j))
            .collect();
        cand.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut got = 0;
        for &(len, j) in cand.iter().take(24) {
            let mut skip: HashSet<&str> = HashSet::new();
            if let Some(a) = oi {
                skip.insert(a);
            }
            if let Some(b) = owner[j].as_deref() {
                skip.insert(b);
            }
            if segment_clears_obstacles(pts[i], pts[j], obstacles, &skip) {
                add(&mut adj, &mut linked, i, j, len);
                got += 1;
                if got >= K_NEAR {
                    break;
                }
            }
        }
    }
    VisGraph { pts, owner, adj }
}

fn collect_ink_points(world: &LayoutWorld, under: &str) -> Vec<Pt> {
    let mut pts = Vec::new();
    let mut stack = vec![under.to_string()];
    while let Some(id) = stack.pop() {
        let Some(n) = world.get(&id) else {
            continue;
        };
        if !n.children.is_empty() {
            stack.extend(n.children.iter().cloned());
            continue;
        }
        match n.world_shape() {
            Shape::Point { x, y } => {
                if n.object_type != super::layout_constraints::ObjectType::Group {
                    pts.push((x, y));
                }
            }
            Shape::Aabb(a) => {
                if a.width() > 1e-9 || a.height() > 1e-9 {
                    pts.push((a.x0, a.y0));
                    pts.push((a.x1, a.y0));
                    pts.push((a.x1, a.y1));
                    pts.push((a.x0, a.y1));
                }
            }
            Shape::Polyline(p) | Shape::PointSet(p) => pts.extend(p),
            Shape::Circle { cx, cy, r } => {
                let k = 8;
                for i in 0..k {
                    let t = std::f64::consts::TAU * (i as f64) / (k as f64);
                    pts.push((cx + r * t.cos(), cy + r * t.sin()));
                }
            }
            _ => {}
        }
    }
    pts
}

/// Outward parallel offset of a CCW convex polygon by `dist`.
fn inflate_convex(hull: &[Pt], dist: f64) -> Vec<Pt> {
    let n = hull.len();
    if n < 3 || dist <= 0.0 {
        return hull.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = hull[(i + n - 1) % n];
        let cur = hull[i];
        let next = hull[(i + 1) % n];
        let e1 = (cur.0 - prev.0, cur.1 - prev.1);
        let e2 = (next.0 - cur.0, next.1 - cur.1);
        // CCW walk: outward is right of the edge (ey, -ex).
        let n1 = unit((e1.1, -e1.0));
        let n2 = unit((e2.1, -e2.0));
        let bx = n1.0 + n2.0;
        let by = n1.1 + n2.1;
        let bl = (bx * bx + by * by).sqrt();
        let (ux, uy) = if bl > 1e-9 {
            (bx / bl, by / bl)
        } else {
            n1
        };
        let cos = (ux * n1.0 + uy * n1.1).abs().max(0.2);
        let s = dist / cos;
        out.push((cur.0 + ux * s, cur.1 + uy * s));
    }
    out
}

fn tip_pose(
    world: &LayoutWorld,
    mol: &str,
    centroid: Pt,
    toward: Pt,
    clash: f64,
    min_shaft: f64,
) -> Pt {
    let chem = format!("scheme/mols/{mol}");
    let dx = toward.0 - centroid.0;
    let dy = toward.1 - centroid.1;
    let span = dx.hypot(dy).max(min_shaft);
    let l = dx.hypot(dy).max(1e-9);
    let ux = dx / l;
    let uy = dy / l;
    let at = |t: f64| (centroid.0 + ux * t, centroid.1 + uy * t);
    let cap = span.max(clash);
    if world.get(&chem).is_none() {
        return at(clash.min(cap));
    }
    let n_samp = ((cap / 4.0).ceil() as usize).clamp(16, 160);
    let mut last_in = 0.0;
    for i in 0..=n_samp {
        let t = cap * (i as f64) / (n_samp as f64);
        if min_ink_signed(world, &chem, at(t)) < clash {
            last_in = t;
        }
    }
    let mut lo = last_in;
    let mut hi = cap;
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if min_ink_signed(world, &chem, at(mid)) >= clash {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    at(hi)
}

fn min_ink_signed(world: &LayoutWorld, under: &str, p: Pt) -> f64 {
    let pt = Shape::Point { x: p.0, y: p.1 };
    let mut best = f64::INFINITY;
    let mut stack = vec![under.to_string()];
    while let Some(id) = stack.pop() {
        let Some(n) = world.get(&id) else {
            continue;
        };
        if !n.children.is_empty() {
            stack.extend(n.children.iter().cloned());
            continue;
        }
        let s = n.world_shape();
        if n.object_type == super::layout_constraints::ObjectType::Group
            && matches!(s, Shape::Point { .. })
        {
            continue;
        }
        let skip = match &s {
            Shape::Aabb(a) => a.width() <= 1e-9 && a.height() <= 1e-9,
            Shape::Polyline(pts) | Shape::PointSet(pts) => pts.len() < 2,
            _ => false,
        };
        if skip {
            continue;
        }
        best = best.min(edge_signed_distance(&pt, &s));
    }
    if let Some(d) = world.ink_signed(under, p.0, p.1) {
        best = best.min(d);
    }
    best
}

fn chord_clears_ink(
    world: &LayoutWorld,
    a: Pt,
    b: Pt,
    mol_ids: &[String],
    skip: &HashSet<&str>,
    clash: f64,
) -> bool {
    let n = 24usize;
    for i in 1..n {
        let t = i as f64 / n as f64;
        let p = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        for id in mol_ids {
            if skip.contains(id.as_str()) {
                continue;
            }
            if min_ink_signed(world, &format!("scheme/mols/{id}"), p) < clash - 0.5 {
                return false;
            }
        }
    }
    true
}

fn segment_clears_obstacles(
    a: Pt,
    b: Pt,
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
) -> bool {
    for o in obstacles {
        if skip.contains(o.mol_id.as_str()) {
            continue;
        }
        if segment_hits_convex(a, b, &o.hull) {
            return false;
        }
    }
    true
}

/// Open segment hits the interior of a convex CCW polygon.
fn segment_hits_convex(a: Pt, b: Pt, hull: &[Pt]) -> bool {
    if hull.len() < 3 {
        return false;
    }
    // Fast AABB reject.
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for &(x, y) in hull {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let sx0 = a.0.min(b.0);
    let sy0 = a.1.min(b.1);
    let sx1 = a.0.max(b.0);
    let sy1 = a.1.max(b.1);
    if sx1 < x0 || sx0 > x1 || sy1 < y0 || sy0 > y1 {
        return false;
    }
    let mid = (0.5 * (a.0 + b.0), 0.5 * (a.1 + b.1));
    if point_in_convex(mid, hull) {
        return true;
    }
    let n = hull.len();
    for i in 0..n {
        let c = hull[i];
        let d = hull[(i + 1) % n];
        if segments_properly_cross(a, b, c, d) {
            return true;
        }
    }
    for i in 1..8 {
        let t = i as f64 / 8.0;
        let p = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        if point_in_convex(p, hull) {
            return true;
        }
    }
    false
}

fn point_in_convex(p: Pt, hull: &[Pt]) -> bool {
    let n = hull.len();
    if n < 3 {
        return false;
    }
    for i in 0..n {
        let a = hull[i];
        let b = hull[(i + 1) % n];
        let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
        if cross < -1e-6 {
            return false;
        }
    }
    true
}

fn segments_properly_cross(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    fn orient(p: Pt, q: Pt, r: Pt) -> f64 {
        (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
    }
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);
    (o1 * o2 < -1e-12) && (o3 * o4 < -1e-12)
}

fn visibility_route(
    start: Pt,
    goal: Pt,
    src: &str,
    tgt: &str,
    vis: &VisGraph,
    obstacles: &[Obstacle],
    committed: &[Committed],
) -> Vec<Pt> {
    // Nodes: start=0, goal=1, then shared hull verts at 2..
    let mut nodes: Vec<Pt> = vec![start, goal];
    nodes.extend_from_slice(&vis.pts);
    let n = nodes.len();
    let skip_end: HashSet<&str> = [src, tgt].into_iter().collect();

    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    // Copy shared visibility (shift indices by +2).
    for (i, nbrs) in vis.adj.iter().enumerate() {
        for &(j, len) in nbrs {
            adj[i + 2].push((j + 2, len));
        }
    }
    // Start / goal star links to nearby visible verts (and each other).
    link_terminal(&mut adj, &nodes, 0, obstacles, &skip_end, src, tgt, &vis.owner);
    link_terminal(&mut adj, &nodes, 1, obstacles, &skip_end, src, tgt, &vis.owner);
    if segment_clears_obstacles(start, goal, obstacles, &skip_end) {
        let len = dist(start, goal);
        adj[0].push((1, len));
        adj[1].push((0, len));
    }

    let path = dijkstra(
        &(0usize, 0usize),
        |&(u, prev)| {
            adj[u]
                .iter()
                .filter_map(|&(v, len)| {
                    if v == prev && u != 0 {
                        return None;
                    }
                    let bend = if u == 0 || u == prev {
                        0.0
                    } else {
                        angle_penalty(nodes[prev], nodes[u], nodes[v]) + SEGMENT_PENALTY * 0.15
                    };
                    let share = shared_penalty(nodes[u], nodes[v], committed);
                    let cross = crossing_penalty(nodes[u], nodes[v], committed);
                    // Landing a new tip on an open shaft looks like a branch.
                    let tip_hit = if v == 1 || u == 0 {
                        tip_on_shaft_penalty(nodes[v], committed)
                    } else {
                        0.0
                    };
                    let cost = len + bend + share + cross + tip_hit;
                    Some(((v, u), ordered_float(cost)))
                })
                .collect::<Vec<_>>()
        },
        |&(g, _)| g == 1,
    );

    let Some((states, _)) = path else {
        return fallback_bow(start, goal, obstacles, &skip_end);
    };
    let mut idxs = Vec::with_capacity(states.len());
    for (node, _) in states {
        if idxs.last().copied() != Some(node) {
            idxs.push(node);
        }
    }
    let mut pts: Vec<Pt> = idxs.into_iter().map(|i| nodes[i]).collect();
    pts = string_pull(&pts, obstacles, &skip_end);
    if pts.len() < 2 {
        return fallback_bow(start, goal, obstacles, &skip_end);
    }
    pts
}

fn tip_on_shaft_penalty(p: Pt, committed: &[Committed]) -> f64 {
    let mut pen = 0.0;
    for c in committed {
        for &(a, b) in &c.segs {
            let (d, t) = point_seg_dist(p, a, b);
            if (0.12..=0.88).contains(&t) && d < TIP_SHAFT_CLEAR {
                pen += 300.0 * (1.0 - d / TIP_SHAFT_CLEAR);
            }
        }
    }
    pen
}

fn link_terminal(
    adj: &mut [Vec<(usize, f64)>],
    nodes: &[Pt],
    hub: usize,
    obstacles: &[Obstacle],
    skip_end: &HashSet<&str>,
    src: &str,
    tgt: &str,
    owner: &[Option<String>],
) {
    let mut cand: Vec<(f64, usize)> = (2..nodes.len())
        .map(|j| (dist(nodes[hub], nodes[j]), j))
        .collect();
    cand.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut linked = 0;
    for (probe, &(len, j)) in cand.iter().enumerate() {
        if probe > 64 {
            break;
        }
        let mut skip = skip_end.clone();
        if let Some(Some(o)) = owner.get(j - 2) {
            if o == src || o == tgt {
                skip.insert(o.as_str());
            }
        }
        if segment_clears_obstacles(nodes[hub], nodes[j], obstacles, &skip) {
            adj[hub].push((j, len));
            adj[j].push((hub, len));
            linked += 1;
            if linked >= 12 {
                break;
            }
        }
    }
}

fn fallback_bow(start: Pt, goal: Pt, obstacles: &[Obstacle], skip: &HashSet<&str>) -> Vec<Pt> {
    let mid = (0.5 * (start.0 + goal.0), 0.5 * (start.1 + goal.1));
    let dx = goal.0 - start.0;
    let dy = goal.1 - start.1;
    let (px, py) = unit((-dy, dx));
    for sign in [1.0, -1.0] {
        for k in 1..8 {
            let off = (k as f64) * BOND_PX * 2.0 * sign;
            let bow = (mid.0 + px * off, mid.1 + py * off);
            if segment_clears_obstacles(start, bow, obstacles, skip)
                && segment_clears_obstacles(bow, goal, obstacles, skip)
            {
                return prune_close_controls(&[start, bow, goal], CLOSE_CTRL_PX);
            }
        }
    }
    vec![start, goal]
}

fn shared_penalty(a: Pt, b: Pt, committed: &[Committed]) -> f64 {
    let mut n = 0.0;
    for c in committed {
        for &(p, q) in &c.segs {
            if segments_collinear_overlap(a, b, p, q) {
                n += 1.0;
            }
        }
    }
    n * SHARED_PATH_PENALTY
}

fn crossing_penalty(a: Pt, b: Pt, committed: &[Committed]) -> f64 {
    let mut n = 0.0;
    for c in committed {
        for &(p, q) in &c.segs {
            if segments_properly_cross(a, b, p, q) {
                n += 1.0;
            }
        }
    }
    n * CROSSING_PENALTY
}

fn segments_collinear_overlap(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let ab = dist(a, b);
    if ab < 1e-6 {
        return false;
    }
    let ux = (b.0 - a.0) / ab;
    let uy = (b.1 - a.1) / ab;
    // Both c,d near the infinite line through ab.
    let dc = ((c.0 - a.0) * (-uy) + (c.1 - a.1) * ux).abs();
    let dd = ((d.0 - a.0) * (-uy) + (d.1 - a.1) * ux).abs();
    if dc > 3.0 || dd > 3.0 {
        return false;
    }
    // Project onto ab parameter [0,1] and look for overlap.
    let proj = |p: Pt| ((p.0 - a.0) * ux + (p.1 - a.1) * uy) / ab;
    let mut t0 = proj(c);
    let mut t1 = proj(d);
    if t0 > t1 {
        std::mem::swap(&mut t0, &mut t1);
    }
    let lo = t0.max(0.0);
    let hi = t1.min(1.0);
    hi - lo > 0.15
}

fn angle_penalty(a: Pt, b: Pt, c: Pt) -> f64 {
    let v1 = unit((b.0 - a.0, b.1 - a.1));
    let v2 = unit((c.0 - b.0, c.1 - b.1));
    let dot = (v1.0 * v2.0 + v1.1 * v2.1).clamp(-1.0, 1.0);
    let turn = 1.0 - dot; // 0 = straight, 2 = reverse
    turn * ANGLE_PENALTY
}

/// Separate remaining shared / near-collinear corridor runs into parallel tracks.
fn nudge_shared_corridors(centers: &mut [Vec<Pt>]) {
    let n = centers.len();
    if n < 2 {
        return;
    }
    // Pairwise: if two centerlines share a long collinear run, offset later one.
    for i in 0..n {
        for j in (i + 1)..n {
            let a = centers[i].clone();
            let b = centers[j].clone();
            if a.len() < 2 || b.len() < 2 {
                continue;
            }
            // Compare full polylines via average midpoint proximity + direction.
            if !polylines_share_corridor(&a, &b) {
                continue;
            }
            let (px, py) = corridor_normal(&a);
            centers[j] = offset_polyline(&b, NUDGE_PX);
            // Flip if offset moved toward a.
            let mid_a = polyline_mid(&a);
            let mid_b = polyline_mid(&centers[j]);
            let mid_b0 = polyline_mid(&b);
            let d0 = dist(mid_a, mid_b0);
            let d1 = dist(mid_a, mid_b);
            if d1 + 0.5 < d0 {
                centers[j] = offset_polyline(&b, -NUDGE_PX);
            }
            let _ = (px, py);
        }
    }
}

fn polylines_share_corridor(a: &[Pt], b: &[Pt]) -> bool {
    let mut hits = 0;
    for w in a.windows(2) {
        for z in b.windows(2) {
            if segments_collinear_overlap(w[0], w[1], z[0], z[1]) {
                hits += 1;
            }
        }
    }
    hits >= 1
}

fn corridor_normal(pts: &[Pt]) -> (f64, f64) {
    if pts.len() < 2 {
        return (0.0, 1.0);
    }
    let a = pts[0];
    let b = pts[pts.len() - 1];
    unit((-(b.1 - a.1), b.0 - a.0))
}

fn polyline_mid(pts: &[Pt]) -> Pt {
    let mid = pts.len() / 2;
    pts[mid]
}

fn lane_offsets(n: usize) -> Vec<f64> {
    if n <= 1 {
        return vec![0.0];
    }
    let mid = (n - 1) as f64 / 2.0;
    (0..n)
        .map(|i| (i as f64 - mid) * COTERMINAL_LANE_PX)
        .collect()
}

fn group_half_width(n: usize) -> f64 {
    if n <= 1 {
        0.0
    } else {
        (n as f64 - 1.0) * 0.5 * COTERMINAL_LANE_PX
    }
}

fn shaft_chem_clash(stroke_width: Option<f64>, bundle_n: usize) -> f64 {
    SCHEME_INK_STANDOFF + group_half_width(bundle_n) + stroke_width.unwrap_or(1.6) * 0.5
}

fn edge_src_tgt(edge: &EdgeNode) -> (String, String) {
    let src = edge.sources.as_slice().first().cloned().unwrap_or_default();
    let tgt = edge.targets.as_slice().first().cloned().unwrap_or_default();
    (src, tgt)
}

fn unordered_mol_pair(src: &str, tgt: &str) -> (String, String) {
    if src <= tgt {
        (src.to_string(), tgt.to_string())
    } else {
        (tgt.to_string(), src.to_string())
    }
}

fn dist(a: Pt, b: Pt) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn unit(v: Pt) -> Pt {
    let l = v.0.hypot(v.1).max(1e-9);
    (v.0 / l, v.1 / l)
}

fn ordered_float(x: f64) -> i64 {
    // pathfinding needs Ord costs; use millipixels.
    (x * 1000.0).round() as i64
}

fn path_ends(pts: &[Pt]) -> Option<(Pt, Pt)> {
    if pts.len() < 2 {
        return None;
    }
    Some((pts[0], pts[pts.len() - 1]))
}

fn seeds_coterminal(a: &[Pt], b: &[Pt]) -> Option<bool> {
    let (a0, a1) = path_ends(a)?;
    let (b0, b1) = path_ends(b)?;
    let fwd = dist(a0, b0) + dist(a1, b1);
    let rev = dist(a0, b1) + dist(a1, b0);
    if fwd <= 2.0 * COTERMINAL_PX {
        Some(false)
    } else if rev <= 2.0 * COTERMINAL_PX {
        Some(true)
    } else {
        None
    }
}

fn coterminal_groups(edges: &[&EdgeNode], seed_pts: &[Vec<Pt>]) -> Vec<ArrowGroup> {
    let n = edges.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    fn unite(parent: &mut [usize], i: usize, j: usize) {
        let a = find(parent, i);
        let b = find(parent, j);
        if a != b {
            parent[a] = b;
        }
    }
    // Union by unordered mol pair first.
    let mut pair_rep: HashMap<(String, String), usize> = HashMap::new();
    for (i, e) in edges.iter().enumerate() {
        let (s, t) = edge_src_tgt(e);
        if s.is_empty() || t.is_empty() {
            continue;
        }
        let key = unordered_mol_pair(&s, &t);
        if let Some(&j) = pair_rep.get(&key) {
            unite(&mut parent, i, j);
        } else {
            pair_rep.insert(key, i);
        }
    }
    // Also merge by geometric seed terminals.
    for i in 0..n {
        for j in (i + 1)..n {
            if seeds_coterminal(&seed_pts[i], &seed_pts[j]).is_some() {
                unite(&mut parent, i, j);
            }
        }
    }
    let mut buckets: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        buckets.entry(r).or_default().push(i);
    }
    let mut groups = Vec::new();
    let mut keys: Vec<_> = buckets.keys().copied().collect();
    keys.sort_unstable();
    for k in keys {
        let mut members = buckets.remove(&k).unwrap_or_default();
        members.sort_unstable();
        // Prefer first layout-participating edge as representative.
        if let Some(pos) = members
            .iter()
            .position(|&i| edges[i].participates_in_layout())
        {
            members.swap(0, pos);
        }
        let rep = members[0];
        let mut reversed = vec![false; members.len()];
        for (lane, &i) in members.iter().enumerate() {
            if lane == 0 {
                continue;
            }
            reversed[lane] = seeds_coterminal(&seed_pts[rep], &seed_pts[i]).unwrap_or(false);
            // Direction from source/target ids when seeds are just centroids.
            let (rs, rt) = edge_src_tgt(edges[rep]);
            let (s, t) = edge_src_tgt(edges[i]);
            if s == rt && t == rs {
                reversed[lane] = true;
            } else if s == rs && t == rt {
                reversed[lane] = false;
            }
        }
        groups.push(ArrowGroup { members, reversed });
    }
    groups
}

fn mol_centroid(
    paints: &HashMap<&str, &DocPaint>,
    positions: &HashMap<String, Pt>,
    sizes: &HashMap<&str, (f64, f64)>,
    id: &str,
) -> Pt {
    if let Some(paint) = paints.get(id) {
        if !paint.molecule.atoms.is_empty() {
            let (x, y) = positions.get(id).copied().unwrap_or((0.0, 0.0));
            let n = paint.molecule.atoms.len() as f64;
            let sx: f64 = paint.molecule.atoms.iter().map(|a| a.x).sum();
            let sy: f64 = paint.molecule.atoms.iter().map(|a| a.y).sum();
            return super::scheme_layout::chem_to_document(paint, (x, y), sx / n, sy / n);
        }
    }
    let (x, y) = positions.get(id).copied().unwrap_or((0.0, 0.0));
    let (w, h) = sizes.get(id).copied().unwrap_or((0.0, 0.0));
    (x + w * 0.5, y + h * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inflate_convex_grows_square() {
        let sq = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let out = inflate_convex(&sq, 2.0);
        assert_eq!(out.len(), 4);
        // Vertices move outward; bbox should grow.
        let xs: Vec<f64> = out.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = out.iter().map(|p| p.1).collect();
        assert!(xs.iter().cloned().fold(f64::INFINITY, f64::min) < -1.0);
        assert!(xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max) > 11.0);
        assert!(ys.iter().cloned().fold(f64::INFINITY, f64::min) < -1.0);
        assert!(ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max) > 11.0);
    }

    #[test]
    fn straight_chord_clears_empty() {
        let hull = inflate_convex(
            &[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)],
            2.0,
        );
        assert!(!segment_hits_convex((-10.0, 40.0), (30.0, 40.0), &hull));
        assert!(segment_hits_convex((5.0, 10.0), (15.0, 10.0), &hull));
    }

    #[test]
    fn pathfinding_dijkstra_on_toy_graph() {
        let result = dijkstra(
            &0usize,
            |&n| match n {
                0 => vec![(1, 1i64), (2, 4)],
                1 => vec![(2, 1), (3, 5)],
                2 => vec![(3, 1)],
                _ => vec![],
            },
            |&n| n == 3,
        );
        let (path, cost) = result.expect("path");
        assert_eq!(path, vec![0, 1, 2, 3]);
        assert_eq!(cost, 3);
    }

    #[test]
    fn string_pull_drops_hull_hug_when_chord_clears() {
        let empty: Vec<Obstacle> = Vec::new();
        let skip = HashSet::new();
        let windy = vec![
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 10.0),
            (20.0, 10.0),
            (20.0, 0.0),
            (30.0, 0.0),
        ];
        let pulled = string_pull(&windy, &empty, &skip);
        assert_eq!(
            pulled.len(),
            2,
            "open space must collapse to a straight chord: {pulled:?}"
        );
    }

    #[test]
    fn draft_crossing_counts_see_an_x() {
        let a = vec![(0.0, 0.0), (10.0, 10.0)];
        let b = vec![(0.0, 10.0), (10.0, 0.0)];
        let c = vec![(20.0, 0.0), (30.0, 0.0)];
        let counts = draft_crossing_counts(&[a, b, c]);
        assert_eq!(counts, vec![1, 1, 0]);
    }

    #[test]
    fn prefer_simpler_takes_single_bow_over_windy() {
        let windy = vec![
            (0.0, 0.0),
            (5.0, 20.0),
            (10.0, 5.0),
            (15.0, 20.0),
            (20.0, 0.0),
        ];
        let bow = vec![(0.0, 0.0), (10.0, 12.0), (20.0, 0.0)];
        let got = prefer_simpler_path(&windy, &bow);
        assert_eq!(got.len(), 3, "single bow should win: {got:?}");
    }
}
