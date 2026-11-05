//! Libavoid-style scheme shaft routing: straight chord first, else a
//! visibility graph on inflated full-node-ink hulls with multi-penalty search.
//!
//! ELK places molecules only. Painted routes never reuse ELK edge sections.
//!
//! Intersection scoring uses **centroid→centroid pathways**. Tip hug (ink
//! pose along the final approach) is a separate step and must not “clear”
//! crossings by pulling ends back toward a centroid.

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
/// Soft cost for collinear overlap with a locked pathway — must dominate a
/// short shared run so two arrows do not paint as one thick shaft.
const SHARED_PATH_PENALTY: f64 = 400.0;
/// Soft cost per crossing with an already-routed shaft. Must dominate a long
/// clean detour (length + bends); otherwise Dijkstra hugs the near silhouette
/// and "crosses" locked shafts instead of taking the far free-space side.
const CROSSING_PENALTY: f64 = 2_500.0;
/// Extra margin outside each hull AABB for free-space Steiner corners so
/// over-top / around-box corridors exist without hugging ink.
const BOX_CORNER_MARGIN: f64 = BOND_PX;
/// Padding outside the union of all obstacles for a scheme-wide Steiner
/// frame — the corridor that goes over-top / around the whole net.
const FRAME_MARGIN: f64 = BOND_PX * 4.0;
/// After the sequential pass, re-route still-crossing shafts a few times so
/// crossings that only appear once both ends exist get priced.
const REOPT_PASSES: usize = 3;
/// Inflate full-ink hulls by this before visibility (shapeBufferDistance).
const ROUTING_CLEARANCE: f64 = SCHEME_INK_STANDOFF + 0.5 * 1.6;
/// Parallel-track gap after shared corridors (idealNudgingDistance).
const NUDGE_PX: f64 = 8.0;
const COTERMINAL_LANE_PX: f64 = 8.0;
const COTERMINAL_PX: f64 = 8.0;
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

    // ---- Pass 1: draft every group as a *centroid→centroid* chord.
    // Intersections are measured on these pathways — never on shortened tips.
    // Tip hug is a separate step after pathways are locked.
    let mut drafts: Vec<Vec<Pt>> = Vec::with_capacity(groups.len());
    let mut group_ends: Vec<(String, String)> = Vec::with_capacity(groups.len());
    let mut group_centroids: Vec<(Pt, Pt)> = Vec::with_capacity(groups.len());
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
        // Free-space terminals for the geometric router (outside inflated hulls).
        // Crossing scores still use the centroid pathway, not these tips.
        let start = tip_outside_hull(
            &obstacles,
            &src,
            tip_pose(world, &src, src_c, tgt_c, clash, min_shaft),
            src_c,
            tgt_c,
        );
        let goal = tip_outside_hull(
            &obstacles,
            &tgt,
            tip_pose(world, &tgt, tgt_c, src_c, clash, min_shaft),
            tgt_c,
            src_c,
        );
        let skip_owned: HashSet<String> = [src.clone(), tgt.clone()].into_iter().collect();

        tips.push((start, goal));
        skips.push(skip_owned);
        clashes.push(clash);
        group_ends.push((src, tgt));
        group_centroids.push((src_c, tgt_c));
        // Draft chord is always centroid→centroid — even when it pierces ink.
        drafts.push(vec![src_c, tgt_c]);
    }

    let draft_cross = draft_crossing_counts(&drafts);

    // ---- Pass 2: lock clean pathways first, then re-route high-crossing /
    // mol-blocked arrows against the locked *centroid pathways*.
    let mut order: Vec<usize> = (0..groups.len()).collect();
    let empty_skip: HashSet<&str> = HashSet::new();
    order.sort_by_key(|&i| {
        let (s, g) = tips[i];
        let ink_skip: HashSet<&str> = skips[i].iter().map(String::as_str).collect();
        let tip_clear = segment_clears_obstacles(s, g, &obstacles, &empty_skip)
            && chord_clears_ink(world, s, g, mol_ids, &ink_skip, clashes[i]);
        // Clean tip chord + zero centroid-pathway crossings first. Within dirty
        // tiers, few-crossing shafts lock first so multi-cross arrows route last.
        let tier = if tip_clear && draft_cross[i] == 0 {
            0
        } else if tip_clear {
            1
        } else {
            2
        };
        (tier, draft_cross[i], i)
    });

    let mut group_centers: Vec<Vec<Pt>> = vec![Vec::new(); groups.len()];
    let mut committed: Vec<Committed> = Vec::new();

    for &gi in &order {
        let (src, tgt) = &group_ends[gi];
        let (start, goal) = tips[gi];
        let (src_c, tgt_c) = group_centroids[gi];
        let skip: HashSet<&str> = skips[gi].iter().map(String::as_str).collect();
        let clash = clashes[gi];

        // Lock a straight tip chord only when the centroid pathway has no
        // crossings — tip shortening must not hide an X.
        let keep_draft = draft_cross[gi] == 0
            && segment_clears_obstacles(start, goal, &obstacles, &empty_skip)
            && chord_clears_ink(world, start, goal, mol_ids, &skip, clash)
            && path_crosses_committed(&[src_c, tgt_c], &committed) == 0;

        let center = if keep_draft {
            vec![start, goal]
        } else {
            // Against locked centroid pathways: high crossing cost → detour.
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
        committed.push(committed_from_pathway(&center, src_c, tgt_c));
        group_centers[gi] = center;
    }

    // Sequential commit only prices crossings against *already* routed shafts.
    // Re-optimize still-crossing pathways against the full peer set a few times.
    reoptimize_crossing_routes(
        &mut group_centers,
        &group_ends,
        &group_centroids,
        &tips,
        &skips,
        &clashes,
        world,
        mol_ids,
        &obstacles,
        &vis,
    );

    // Final string-pull must not reopen a locked pathway X.
    for gi in 0..group_centers.len() {
        let (src_c, tgt_c) = group_centroids[gi];
        let others: Vec<Committed> = group_centers
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != gi)
            .map(|(j, c)| {
                let (sc, tc) = group_centroids[j];
                committed_from_pathway(c, sc, tc)
            })
            .collect();
        let before = group_centers[gi].clone();
        let pulled = string_pull(&before, &obstacles, &empty_skip, &others);
        let before_x = path_crosses_committed(
            &extend_to_centroids(&before, src_c, tgt_c),
            &others,
        );
        let after_x =
            path_crosses_committed(&extend_to_centroids(&pulled, src_c, tgt_c), &others);
        if path_clears_obstacles(&pulled, &obstacles, &empty_skip) && after_x <= before_x {
            group_centers[gi] = pulled;
        }
    }

    // ---- Tip layout (separate from pathway routing): hug ink along the
    // final approach rays. Does not participate in intersection scoring.
    resnap_tips_to_approach_rays(
        &mut group_centers,
        &group_ends,
        &centroids,
        world,
        &obstacles,
        &clashes,
        spec,
        &groups,
    );

    nudge_shared_corridors(&mut group_centers, &group_centroids);

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

/// Prepend/append centroids so crossing tests see the full mol→mol pathway,
/// not a tip-shortened shaft that can hide an X by pulling back.
fn extend_to_centroids(path: &[Pt], src_c: Pt, tgt_c: Pt) -> Vec<Pt> {
    if path.is_empty() {
        return vec![src_c, tgt_c];
    }
    let mut out = Vec::with_capacity(path.len() + 2);
    if dist(path[0], src_c) > 1e-6 {
        out.push(src_c);
    }
    out.extend_from_slice(path);
    if dist(*path.last().unwrap(), tgt_c) > 1e-6 {
        out.push(tgt_c);
    }
    out
}

fn committed_from_pathway(path: &[Pt], src_c: Pt, tgt_c: Pt) -> Committed {
    let pts = extend_to_centroids(path, src_c, tgt_c);
    Committed {
        segs: pts.windows(2).map(|w| (w[0], w[1])).collect(),
    }
}

/// Straight chord first when it also clears already-locked pathways; else
/// visibility + string-pull. Never replace a clean detour with a bow that
/// reintroduces crossings. Tip-on-shaft is *not* a routing concern.
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
    let empty: HashSet<&str> = HashSet::new();
    let mol_clear = segment_clears_obstacles(start, goal, obstacles, &empty)
        && chord_clears_ink(world, start, goal, mol_ids, skip, clash);
    let crosses_locked = path_crosses_committed(&[start, goal], committed) > 0;
    // Pass-2 must actually re-route against the locked set. A mol-clear chord
    // that still crosses a locked pathway is not done.
    if mol_clear && !crosses_locked {
        return prune_close_controls(&[start, goal], CLOSE_CTRL_PX);
    }

    let vis_path = visibility_route(start, goal, src, tgt, vis, obstacles, committed);
    // String-pull must not cut through endpoint hulls or reopen locked crossings.
    let pulled = string_pull(&vis_path, obstacles, &empty, committed);
    let pulled_ok = path_clears_obstacles(&pulled, obstacles, &empty);
    if pulled.len() == 2
        && pulled_ok
        && path_crosses_committed(&pulled, committed) == 0
    {
        return pulled;
    }

    let bow = fallback_bow(start, goal, obstacles, &empty, committed);
    let bow_ok = path_clears_obstacles(&bow, obstacles, &empty);
    // A hull-clear visibility detour always beats a punch-through "simpler"
    // chord (prefer_simpler used to discard 4-point walks for 2-point pierces).
    match (pulled_ok, bow_ok) {
        (true, false) => return pulled,
        (false, true) => return bow,
        (true, true) => {
            let pulled_x = path_crosses_committed(&pulled, committed);
            let bow_x = path_crosses_committed(&bow, committed);
            if bow_x < pulled_x {
                return bow;
            }
            if bow_x > pulled_x {
                return pulled;
            }
            return prefer_simpler_path(&pulled, &bow);
        }
        (false, false) => {
            // Last resort: keep the longer non-trivial path if either has bends.
            if pulled.len() >= 3 {
                return pulled;
            }
            if bow.len() >= 3 {
                return bow;
            }
            pulled
        }
    }
}

fn path_clears_obstacles(pts: &[Pt], obstacles: &[Obstacle], skip: &HashSet<&str>) -> bool {
    if pts.len() < 2 {
        return false;
    }
    pts.windows(2)
        .all(|w| segment_clears_obstacles(w[0], w[1], obstacles, skip))
}

fn path_crosses_committed(pts: &[Pt], committed: &[Committed]) -> usize {
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

/// Remove intermediate verts when the chord still clears obstacles (classic
/// string-pull / funnel). Stops hull-hugging when a straighter cut exists.
/// Rejects shortcuts that properly cross a committed shaft — otherwise a
/// priced zero-crossing walk collapses back into an X.
fn string_pull(
    pts: &[Pt],
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
    committed: &[Committed],
) -> Vec<Pt> {
    if pts.len() < 3 {
        return prune_close_controls(pts, CLOSE_CTRL_PX);
    }
    let mut out = vec![pts[0]];
    let mut i = 0;
    while i < pts.len() - 1 {
        let mut j = pts.len() - 1;
        while j > i + 1 {
            if segment_clears_obstacles(pts[i], pts[j], obstacles, skip)
                && path_crosses_committed(&[pts[i], pts[j]], committed) == 0
            {
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

/// Re-route groups whose *centroid pathways* still properly cross a peer.
fn reoptimize_crossing_routes(
    centers: &mut [Vec<Pt>],
    ends: &[(String, String)],
    group_centroids: &[(Pt, Pt)],
    tips: &[(Pt, Pt)],
    skips: &[HashSet<String>],
    clashes: &[f64],
    world: &LayoutWorld,
    mol_ids: &[String],
    obstacles: &[Obstacle],
    vis: &VisGraph,
) {
    let n = centers.len();
    if n < 2 {
        return;
    }
    // Skip arrows whose last attempt did not improve until a peer does —
    // dense nets otherwise re-Dijkstra the same irreducible X's every pass.
    let mut stuck: HashSet<usize> = HashSet::new();
    for _ in 0..REOPT_PASSES {
        let mut crossers: Vec<(usize, usize)> = (0..n)
            .map(|i| (count_pathway_crossings_with_peers(i, centers, group_centroids), i))
            .filter(|(c, i)| *c > 0 && !stuck.contains(i))
            .collect();
        if crossers.is_empty() {
            break;
        }
        // Worst offenders first.
        crossers.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut improved = false;
        for &(_, gi) in &crossers {
            let before = centers[gi].clone();
            let before_x = count_pathway_crossings_with_peers(gi, centers, group_centroids);
            if before_x == 0 {
                continue;
            }
            let peers: Vec<Committed> = centers
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != gi)
                .map(|(j, c)| {
                    let (sc, tc) = group_centroids[j];
                    committed_from_pathway(c, sc, tc)
                })
                .collect();
            let (src, tgt) = &ends[gi];
            let (start, goal) = tips[gi];
            let skip: HashSet<&str> = skips[gi].iter().map(String::as_str).collect();
            let clash = clashes[gi];
            let candidate = route_one_centerline(
                start,
                goal,
                src,
                tgt,
                &skip,
                world,
                mol_ids,
                clash,
                obstacles,
                vis,
                &peers,
            );
            if candidate.len() < 2 {
                stuck.insert(gi);
                continue;
            }
            // Temporarily install to score peer *pathway* crossings.
            centers[gi] = candidate;
            let after_x = count_pathway_crossings_with_peers(gi, centers, group_centroids);
            let empty = HashSet::new();
            let clears = path_clears_obstacles(&centers[gi], obstacles, &empty);
            let before_clears = path_clears_obstacles(&before, obstacles, &empty);
            // Never accept more crossings; otherwise prefer fewer, then
            // newly-clear, then shorter at equal clearance.
            let better = after_x < before_x
                || (after_x == before_x
                    && clears
                    && !before_clears)
                || (after_x == before_x
                    && clears
                    && before_clears
                    && polyline_len(&centers[gi]) + 1.0 < polyline_len(&before));
            if better {
                improved = true;
                stuck.clear();
            } else {
                centers[gi] = before;
                stuck.insert(gi);
            }
        }
        if !improved {
            break;
        }
    }
}

fn count_pathway_crossings_with_peers(
    i: usize,
    centers: &[Vec<Pt>],
    group_centroids: &[(Pt, Pt)],
) -> usize {
    let (sc_i, tc_i) = group_centroids[i];
    let a = extend_to_centroids(&centers[i], sc_i, tc_i);
    let mut n = 0;
    for (j, other) in centers.iter().enumerate() {
        if j == i {
            continue;
        }
        let (sc_j, tc_j) = group_centroids[j];
        let b = extend_to_centroids(other, sc_j, tc_j);
        n += count_poly_crossings(&a, &b);
    }
    n
}

/// Place start/end tips on the rays `src_centroid → path[1]` and
/// `tgt_centroid → path[n-2]` so end chords aim at the centroids after a bow.
fn resnap_tips_to_approach_rays(
    centers: &mut [Vec<Pt>],
    ends: &[(String, String)],
    centroids: &HashMap<String, Pt>,
    world: &LayoutWorld,
    obstacles: &[Obstacle],
    clashes: &[f64],
    spec: &DepictSpec,
    groups: &[ArrowGroup],
) {
    let edges = spec.edges();
    for gi in 0..centers.len() {
        if centers[gi].len() < 2 {
            continue;
        }
        let (ref src, ref tgt) = ends[gi];
        let src_c = centroids.get(src).copied().unwrap_or(centers[gi][0]);
        let last_i = centers[gi].len() - 1;
        let tgt_c = centroids
            .get(tgt)
            .copied()
            .unwrap_or(centers[gi][last_i]);
        let clash = clashes.get(gi).copied().unwrap_or(SCHEME_INK_STANDOFF);
        let min_shaft = groups
            .get(gi)
            .and_then(|g| edges.get(g.members[0]))
            .map(|e| min_shaft_px(e.stroke_width))
            .unwrap_or_else(|| min_shaft_px(None));
        let next = centers[gi][1];
        let prev = centers[gi][centers[gi].len() - 2];
        let start = tip_outside_hull(
            obstacles,
            src,
            tip_pose(world, src, src_c, next, clash, min_shaft),
            src_c,
            next,
        );
        let goal = tip_outside_hull(
            obstacles,
            tgt,
            tip_pose(world, tgt, tgt_c, prev, clash, min_shaft),
            tgt_c,
            prev,
        );
        // Tip layout only cares about hull clearance — pathway crossings are
        // already decided and must not be "fixed" by pulling tips back.
        let empty = HashSet::new();
        if segment_clears_obstacles(start, next, obstacles, &empty) {
            centers[gi][0] = start;
        }
        if segment_clears_obstacles(prev, goal, obstacles, &empty) {
            let last = centers[gi].len() - 1;
            centers[gi][last] = goal;
        }
        centers[gi] = prune_close_controls(&centers[gi], CLOSE_CTRL_PX);
    }
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
        // Keep the full inflated hull. Stride-simplifying verts shrinks a
        // convex polygon inward and lets shafts clip molecule ink.
        let hull = if hull.len() >= 3 {
            inflate_convex(&hull, inflate)
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
    // Expanded AABB corner Steiner points (free-space "over the top" ports).
    let mut box_ranges: Vec<(usize, usize)> = Vec::new();
    for o in obstacles {
        let start = pts.len();
        for &p in &o.hull {
            pts.push(p);
            owner.push(Some(o.mol_id.clone()));
        }
        ranges.push((start, pts.len()));
        let (x0, y0, x1, y1) = hull_aabb(&o.hull);
        let m = BOX_CORNER_MARGIN;
        let corners = [
            (x0 - m, y0 - m),
            (x1 + m, y0 - m),
            (x1 + m, y1 + m),
            (x0 - m, y1 + m),
        ];
        let b0 = pts.len();
        for c in corners {
            pts.push(c);
            owner.push(Some(o.mol_id.clone()));
        }
        box_ranges.push((b0, pts.len()));
    }
    // Scheme-wide frame so a route can travel over-top / around the net in
    // free space instead of only hopping per-mol AABB corners.
    let frame_range = if pts.is_empty() {
        None
    } else {
        let (gx0, gy0, gx1, gy1) = hull_aabb(&pts);
        let m = FRAME_MARGIN;
        let corners = [
            (gx0 - m, gy0 - m),
            (gx1 + m, gy0 - m),
            (gx1 + m, gy1 + m),
            (gx0 - m, gy1 + m),
        ];
        let f0 = pts.len();
        for c in corners {
            pts.push(c);
            owner.push(None);
        }
        Some((f0, pts.len()))
    };
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

    // 1) Inflated-hull boundary edges (walk around each obstacle). The open
    // segment lies on the boundary of that obstacle, so skip only its owner.
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

    // 1b) Outer AABB-corner cycles — free-space ring around each mol so a
    // long route can go over the top / down the far side without hugging ink.
    for (ri, &(lo, hi)) in box_ranges.iter().enumerate() {
        let skip: HashSet<&str> = [obstacles[ri].mol_id.as_str()].into_iter().collect();
        for k in 0..4 {
            let i = lo + k;
            let j = lo + (k + 1) % 4;
            if segment_clears_obstacles(pts[i], pts[j], obstacles, &skip) {
                add(&mut adj, &mut linked, i, j, dist(pts[i], pts[j]));
            }
        }
        // Also stitch each box corner to nearby hull verts of the same mol.
        for i in lo..hi {
            for j in ranges[ri].0..ranges[ri].1 {
                if segment_clears_obstacles(pts[i], pts[j], obstacles, &skip) {
                    add(&mut adj, &mut linked, i, j, dist(pts[i], pts[j]));
                }
            }
        }
    }

    // 1c) Scheme frame cycle (unowned Steiners — not same-owner skipped below).
    let empty_skip = HashSet::new();
    if let Some((flo, _fhi)) = frame_range {
        for k in 0..4 {
            let i = flo + k;
            let j = flo + (k + 1) % 4;
            if segment_clears_obstacles(pts[i], pts[j], obstacles, &empty_skip) {
                add(&mut adj, &mut linked, i, j, dist(pts[i], pts[j]));
            }
        }
    }

    // 2) Full inter-obstacle visibility: every clear segment between verts on
    // *different* hulls/boxes (frame verts are unowned and link to all clear).
    // Same-owner non-ring chords still skip (cut ink).
    for i in 0..n {
        let oi = owner[i].as_deref();
        for j in (i + 1)..n {
            if oi.is_some() && owner[j].as_deref() == oi {
                continue;
            }
            if segment_clears_obstacles(pts[i], pts[j], obstacles, &empty_skip) {
                add(&mut adj, &mut linked, i, j, dist(pts[i], pts[j]));
            }
        }
    }
    VisGraph { pts, owner, adj }
}

fn hull_aabb(hull: &[Pt]) -> (f64, f64, f64, f64) {
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
    (x0, y0, x1, y1)
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

/// Push an ink tip out along the centroid→toward ray until it clears that
/// mol's inflated convex hull (open segment visibility must not skip it).
fn tip_outside_hull(
    obstacles: &[Obstacle],
    mol: &str,
    tip: Pt,
    centroid: Pt,
    toward: Pt,
) -> Pt {
    let Some(o) = obstacles.iter().find(|o| o.mol_id == mol) else {
        return tip;
    };
    if !point_in_convex(tip, &o.hull) {
        return tip;
    }
    let dx = toward.0 - centroid.0;
    let dy = toward.1 - centroid.1;
    let span = dx.hypot(dy).max(BOND_PX * 8.0);
    let l = dx.hypot(dy).max(1e-9);
    let ux = dx / l;
    let uy = dy / l;
    // Walk from tip outward; binary-search the first exterior point.
    let mut lo = ((tip.0 - centroid.0) * ux + (tip.1 - centroid.1) * uy).max(0.0);
    let mut hi = span.max(lo + BOND_PX);
    let at = |t: f64| (centroid.0 + ux * t, centroid.1 + uy * t);
    if !point_in_convex(at(hi), &o.hull) {
        for _ in 0..32 {
            let mid = 0.5 * (lo + hi);
            if point_in_convex(at(mid), &o.hull) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        return at(hi);
    }
    // Ray still inside at span — step further.
    for k in 1..16 {
        let t = span + (k as f64) * BOND_PX * 2.0;
        let p = at(t);
        if !point_in_convex(p, &o.hull) {
            return p;
        }
    }
    tip
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

/// Open segment hits the **strict interior** of a convex CCW polygon.
/// Boundary edges (hull walks) must not count as hits — closed `point_in`
/// treated on-edge midpoints as inside and blocked every detour.
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
    if point_strictly_in_convex(mid, hull) {
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
        if point_strictly_in_convex(p, hull) {
            return true;
        }
    }
    false
}

/// Closed membership (boundary counts as inside) — for tip push-out.
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

/// Open interior only — on-boundary points are outside.
fn point_strictly_in_convex(p: Pt, hull: &[Pt]) -> bool {
    let n = hull.len();
    if n < 3 {
        return false;
    }
    for i in 0..n {
        let a = hull[i];
        let b = hull[(i + 1) % n];
        let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
        if cross <= 1e-6 {
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
    _src: &str,
    _tgt: &str,
    vis: &VisGraph,
    obstacles: &[Obstacle],
    committed: &[Committed],
) -> Vec<Pt> {
    // Nodes: start=0, goal=1, then shared hull verts at 2..
    let mut nodes: Vec<Pt> = vec![start, goal];
    nodes.extend_from_slice(&vis.pts);
    let n = nodes.len();

    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    // Copy shared visibility (shift indices by +2).
    for (i, nbrs) in vis.adj.iter().enumerate() {
        for &(j, len) in nbrs {
            adj[i + 2].push((j + 2, len));
        }
    }
    // Start / goal → every hull vert whose open segment clears *all* inflated
    // hulls (including src/tgt). Skipping endpoints let chords tunnel through
    // sugar moieties and beat real left/right detours on cost.
    link_terminal(&mut adj, &nodes, 0, obstacles, &vis.owner);
    link_terminal(&mut adj, &nodes, 1, obstacles, &vis.owner);
    let empty_skip = HashSet::new();
    if segment_clears_obstacles(start, goal, obstacles, &empty_skip) {
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
                    // Tip-on-shaft is tip-layout, not pathway cost.
                    let cost = len + bend + share + cross;
                    Some(((v, u), ordered_float(cost)))
                })
                .collect::<Vec<_>>()
        },
        |&(g, _)| g == 1,
    );

    let Some((states, _)) = path else {
        return fallback_bow(start, goal, obstacles, &empty_skip, committed);
    };
    let mut idxs = Vec::with_capacity(states.len());
    for (node, _) in states {
        if idxs.last().copied() != Some(node) {
            idxs.push(node);
        }
    }
    let mut pts: Vec<Pt> = idxs.into_iter().map(|i| nodes[i]).collect();
    pts = string_pull(&pts, obstacles, &empty_skip, committed);
    if pts.len() < 2 {
        return fallback_bow(start, goal, obstacles, &empty_skip, committed);
    }
    pts
}

fn link_terminal(
    adj: &mut [Vec<(usize, f64)>],
    nodes: &[Pt],
    hub: usize,
    obstacles: &[Obstacle],
    _owner: &[Option<String>],
) {
    let empty_skip = HashSet::new();
    for j in 2..nodes.len() {
        let len = dist(nodes[hub], nodes[j]);
        if len < 1e-6 {
            continue;
        }
        if segment_clears_obstacles(nodes[hub], nodes[j], obstacles, &empty_skip) {
            adj[hub].push((j, len));
            adj[j].push((hub, len));
        }
    }
}

fn fallback_bow(
    start: Pt,
    goal: Pt,
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
    committed: &[Committed],
) -> Vec<Pt> {
    let mid = (0.5 * (start.0 + goal.0), 0.5 * (start.1 + goal.1));
    let dx = goal.0 - start.0;
    let dy = goal.1 - start.1;
    let (px, py) = unit((-dy, dx));
    let mut best: Option<(usize, f64, Vec<Pt>)> = None;
    for sign in [1.0, -1.0] {
        for k in 1..24 {
            let off = (k as f64) * BOND_PX * 2.0 * sign;
            let bow = (mid.0 + px * off, mid.1 + py * off);
            if !segment_clears_obstacles(start, bow, obstacles, skip)
                || !segment_clears_obstacles(bow, goal, obstacles, skip)
            {
                continue;
            }
            let pts = prune_close_controls(&[start, bow, goal], CLOSE_CTRL_PX);
            let x = path_crosses_committed(&pts, committed);
            let len = polyline_len(&pts);
            let better = match &best {
                None => true,
                Some((bx, bl, _)) => x < *bx || (x == *bx && len < *bl),
            };
            if better {
                best = Some((x, len, pts));
                if x == 0 {
                    break;
                }
            }
        }
        if best.as_ref().is_some_and(|(x, _, _)| *x == 0) {
            break;
        }
    }
    if let Some((_, _, pts)) = best {
        return pts;
    }
    // Last resort: still prefer a long side-step over punching ink. Walk the
    // start→goal AABB corners if a single bow cannot clear.
    let span = dist(start, goal).max(BOND_PX * 4.0);
    for sign in [1.0, -1.0] {
        let a = (start.0 + px * span * sign, start.1 + py * span * sign);
        let b = (goal.0 + px * span * sign, goal.1 + py * span * sign);
        if segment_clears_obstacles(start, a, obstacles, skip)
            && segment_clears_obstacles(a, b, obstacles, skip)
            && segment_clears_obstacles(b, goal, obstacles, skip)
        {
            return prune_close_controls(&[start, a, b, goal], CLOSE_CTRL_PX);
        }
    }
    // Only punch the chord when every detour failed — caller may still reject.
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
fn nudge_shared_corridors(centers: &mut [Vec<Pt>], group_centroids: &[(Pt, Pt)]) {
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
            let before_x = count_pathway_crossings_with_peers(j, centers, group_centroids);
            let (px, py) = corridor_normal(&a);
            let mut cand = offset_polyline(&b, NUDGE_PX);
            // Flip if offset moved toward a.
            let mid_a = polyline_mid(&a);
            let mid_b = polyline_mid(&cand);
            let mid_b0 = polyline_mid(&b);
            let d0 = dist(mid_a, mid_b0);
            let d1 = dist(mid_a, mid_b);
            if d1 + 0.5 < d0 {
                cand = offset_polyline(&b, -NUDGE_PX);
            }
            let _ = (px, py);
            // Never reopen a pathway X just to parallel-track shared corridors.
            centers[j] = cand;
            if count_pathway_crossings_with_peers(j, centers, group_centroids) > before_x {
                centers[j] = b;
            }
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
        let pulled = string_pull(&windy, &empty, &skip, &[]);
        assert_eq!(
            pulled.len(),
            2,
            "open space must collapse to a straight chord: {pulled:?}"
        );
    }

    #[test]
    fn string_pull_keeps_detour_that_avoids_locked_shaft() {
        let empty: Vec<Obstacle> = Vec::new();
        let skip = HashSet::new();
        // Vertical start→goal would cross the locked horizontal; the U-detour
        // goes around. Pull must not collapse back to the piercing chord.
        let detour = vec![(50.0, 0.0), (0.0, 0.0), (0.0, 100.0), (50.0, 100.0)];
        let committed = vec![Committed {
            segs: vec![((0.0, 50.0), (100.0, 50.0))],
        }];
        assert_eq!(
            path_crosses_committed(&[(50.0, 0.0), (50.0, 100.0)], &committed),
            1,
            "sanity: direct chord crosses"
        );
        let pulled = string_pull(&detour, &empty, &skip, &committed);
        assert_eq!(
            path_crosses_committed(&pulled, &committed),
            0,
            "pull must not collapse into the locked shaft: {pulled:?}"
        );
        assert!(
            pulled.len() >= 3,
            "must keep the detour verts: {pulled:?}"
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


    #[test]
    fn tip_shortening_does_not_hide_centroid_pathway_cross() {
        // Short painted shafts miss each other; centroid pathways still cross.
        let src_a = (0.0, 0.0);
        let tgt_a = (100.0, 100.0);
        let src_b = (0.0, 100.0);
        let tgt_b = (100.0, 0.0);
        let short_a = vec![(5.0, 5.0), (30.0, 30.0)];
        let short_b = vec![(5.0, 95.0), (30.0, 70.0)];
        assert_eq!(
            count_poly_crossings(&short_a, &short_b),
            0,
            "short tips must not cross"
        );
        let path_a = extend_to_centroids(&short_a, src_a, tgt_a);
        let path_b = extend_to_centroids(&short_b, src_b, tgt_b);
        assert!(
            count_poly_crossings(&path_a, &path_b) > 0,
            "centroid pathways must still count the X: {path_a:?} vs {path_b:?}"
        );
    }

    #[test]
    fn reoptimize_untangles_sequential_blind_cross() {
        // Simulate pass-2 order blindness: A routed first as a straight, B
        // later as a crossing straight (no peers priced for A). Reopt must
        // see both shafts and pull one clear.
        let empty_obs: Vec<Obstacle> = Vec::new();
        let vis = build_visibility_graph(&empty_obs);
        let world = LayoutWorld::new("scheme");
        let mol_ids: Vec<String> = Vec::new();
        let mut centers = vec![
            vec![(0.0, 0.0), (100.0, 100.0)],
            vec![(0.0, 100.0), (100.0, 0.0)],
        ];
        let ends = vec![("a".into(), "b".into()), ("c".into(), "d".into())];
        let group_centroids = [
            (centers[0][0], centers[0][1]),
            (centers[1][0], centers[1][1]),
        ];
        assert_eq!(
            count_pathway_crossings_with_peers(0, &centers, &group_centroids),
            1
        );
        let tips = group_centroids;
        let skips = [
            HashSet::from(["a".into(), "b".into()]),
            HashSet::from(["c".into(), "d".into()]),
        ];
        let clashes = [2.0_f64, 2.0];
        reoptimize_crossing_routes(
            &mut centers,
            &ends,
            &group_centroids,
            &tips,
            &skips,
            &clashes,
            &world,
            &mol_ids,
            &empty_obs,
            &vis,
        );
        assert_eq!(
            count_pathway_crossings_with_peers(0, &centers, &group_centroids),
            0,
            "reopt must clear the X once both shafts exist: {centers:?}"
        );
        assert_eq!(
            count_pathway_crossings_with_peers(1, &centers, &group_centroids),
            0
        );
    }

    #[test]
    fn higher_crossing_cost_takes_far_side_not_near_cross() {
        // Blocker in the middle; a locked shaft on the LEFT. The short walk
        // around the left crosses it; the far (right) walk does not. Raising
        // CROSSING_PENALTY must force the clear side.
        let square = vec![(40.0, 40.0), (60.0, 40.0), (60.0, 60.0), (40.0, 60.0)];
        let obstacles = vec![Obstacle {
            mol_id: "blocker".into(),
            hull: inflate_convex(&square, 2.0),
        }];
        let vis = build_visibility_graph(&obstacles);
        // Locked shaft cuts the *left* silhouette corridor (near side).
        let committed = vec![Committed {
            segs: vec![((10.0, 50.0), (45.0, 50.0))],
        }];
        let skip = HashSet::new();
        let world = LayoutWorld::new("scheme");
        let mol_ids: Vec<String> = Vec::new();
        let path = route_one_centerline(
            (50.0, 10.0),
            (50.0, 90.0),
            "a",
            "b",
            &skip,
            &world,
            &mol_ids,
            2.0,
            &obstacles,
            &vis,
            &committed,
        );
        assert!(
            path.len() >= 3,
            "must detour around the blocker: {path:?}"
        );
        assert_eq!(
            path_crosses_committed(&path, &committed),
            0,
            "high crossing cost must prefer the clear (right) side: {path:?}"
        );
        let max_x = path.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        assert!(
            max_x > 60.0,
            "expected a right-side walk (over the far box), got {path:?}"
        );
    }

    #[test]
    fn route_one_keeps_visibility_detour_over_punch() {
        // Visibility finds a walk; fallback_bow's last resort is the piercing
        // chord. Must not prefer that "simpler" 2-point path.
        let square = vec![(40.0, 40.0), (60.0, 40.0), (60.0, 60.0), (40.0, 60.0)];
        let obstacles = vec![Obstacle {
            mol_id: "blocker".into(),
            hull: inflate_convex(&square, 2.0),
        }];
        let vis = build_visibility_graph(&obstacles);
        let skip = HashSet::new();
        let world = LayoutWorld::new("scheme");
        let mol_ids: Vec<String> = Vec::new();
        let path = route_one_centerline(
            (50.0, 10.0),
            (50.0, 90.0),
            "a",
            "b",
            &skip,
            &world,
            &mol_ids,
            2.0,
            &obstacles,
            &vis,
            &[],
        );
        assert!(
            path.len() >= 3,
            "must keep the visibility walk, not punch: {path:?}"
        );
        assert!(
            path_clears_obstacles(&path, &obstacles, &skip),
            "kept path must clear the blocker: {path:?}"
        );
    }

    #[test]
    fn visibility_finds_left_detour_around_blocker() {
        // Start above a square, goal below — straight hits the hull; full
        // visibility must walk around (left or right), never through.
        let square = vec![(40.0, 40.0), (60.0, 40.0), (60.0, 60.0), (40.0, 60.0)];
        let hull = inflate_convex(&square, 2.0);
        assert!(segment_hits_convex((50.0, 10.0), (50.0, 90.0), &hull));
        let obstacles = vec![Obstacle {
            mol_id: "blocker".into(),
            hull: hull.clone(),
        }];
        let vis = build_visibility_graph(&obstacles);
        // Boundary walk + inter-obstacle (n/a) → each vert degree ≥ 2.
        assert!(
            vis.adj.iter().all(|n| !n.is_empty()),
            "blocker hull verts must be linked on the boundary"
        );
        let path = visibility_route(
            (50.0, 10.0),
            (50.0, 90.0),
            "a",
            "b",
            &vis,
            &obstacles,
            &[],
        );
        assert!(
            path.len() >= 3,
            "must detour around blocker, got {path:?}"
        );
        let empty = HashSet::new();
        for w in path.windows(2) {
            assert!(
                segment_clears_obstacles(w[0], w[1], &obstacles, &empty),
                "detour segment {:?}→{:?} still hits the hull",
                w[0],
                w[1]
            );
        }
        // Prefer the geometrically longer side is fine; just not the chord.
        assert!(
            polyline_len(&path) > dist((50.0, 10.0), (50.0, 90.0)) + 1.0,
            "detour should be longer than the blocked chord"
        );
    }

    #[test]
    fn route_one_detours_when_straight_crosses_locked() {
        // Locked horizontal shaft across the middle; start/goal are clear of
        // mols but the chord crosses the locked edge → must not stay straight.
        let obstacles: Vec<Obstacle> = Vec::new();
        let vis = build_visibility_graph(&obstacles);
        let committed = vec![Committed {
            segs: vec![((0.0, 50.0), (100.0, 50.0))],
        }];
        let skip = HashSet::new();
        let world = LayoutWorld::new("scheme");
        let mol_ids: Vec<String> = Vec::new();
        let path = route_one_centerline(
            (20.0, 0.0),
            (20.0, 100.0),
            "a",
            "b",
            &skip,
            &world,
            &mol_ids,
            2.0,
            &obstacles,
            &vis,
            &committed,
        );
        assert!(
            path.len() >= 3,
            "expected a bow around the locked shaft, got {path:?}"
        );
        assert_eq!(
            path_crosses_committed(&path, &committed),
            0,
            "detour must not cross the locked shaft: {path:?}"
        );
    }
}
