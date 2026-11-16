//! Libavoid-style scheme shaft routing: straight chord first, else a
//! visibility graph on inflated full-node-ink hulls with multi-penalty search.
//!
//! Layered placer (or non-layered ELK) positions molecules only. Painted
//! routes never reuse layout edge sections.
//!
//! Intersection scoring uses **centroid→centroid pathways**. Tip hug (ink
//! pose along the final approach) is a separate step and must not “clear”
//! crossings by pulling ends back toward a centroid.

use std::collections::{HashMap, HashSet};

use pathfinding::directed::astar::astar;
#[cfg(test)]
use pathfinding::directed::dijkstra::dijkstra;

use crate::arrows::{
    min_shaft_px, offset_polyline, prune_close_controls, reverse_source_inset,
    shorten_polyline_start, CLOSE_CTRL_PX,
};
use crate::engine_config::{EngineConfig, RouteConfig};
use crate::metrics::BOND_PX;

use super::layout_constraints::{convex_hull_pts, edge_signed_distance, LayoutWorld, Shape};
use super::scheme_layout::SCHEME_INK_STANDOFF;
use super::{DepictSpec, DocPaint, EdgeNode};

type Pt = (f64, f64);

thread_local! {
    /// Set for the duration of [`route_scheme_edges`] so helpers read the
    /// active [`RouteConfig`] without threading it through every signature.
    static ACTIVE_ROUTE: std::cell::RefCell<Option<RouteConfig>> =
        const { std::cell::RefCell::new(None) };
    /// Test-only patch for [`RouteConfig::crossing_penalty`] (tip↔detour sweep).
    #[cfg(test)]
    static CROSSING_PENALTY_OVERRIDE: std::cell::RefCell<Option<f64>> =
        const { std::cell::RefCell::new(None) };
}

struct ActiveRouteGuard;
impl Drop for ActiveRouteGuard {
    fn drop(&mut self) {
        ACTIVE_ROUTE.with(|c| *c.borrow_mut() = None);
    }
}

fn route() -> RouteConfig {
    ACTIVE_ROUTE.with(|c| {
        let mut r = c.borrow().clone().unwrap_or_else(RouteConfig::pretty);
        #[cfg(test)]
        CROSSING_PENALTY_OVERRIDE.with(|o| {
            if let Some(p) = *o.borrow() {
                r.crossing_penalty = p;
            }
        });
        r
    })
}

/// Run `f` with [`RouteConfig::crossing_penalty`] forced to `penalty`.
#[cfg(test)]
pub(crate) fn with_crossing_penalty<R>(penalty: f64, f: impl FnOnce() -> R) -> R {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            CROSSING_PENALTY_OVERRIDE.with(|o| *o.borrow_mut() = None);
        }
    }
    CROSSING_PENALTY_OVERRIDE.with(|o| *o.borrow_mut() = Some(penalty));
    let _g = Guard;
    f()
}
struct ArrowGroup {
    members: Vec<usize>,
    reversed: Vec<bool>,
}

/// One convex obstacle per mol (inflated full-node ink).
#[derive(Clone)]
struct Obstacle {
    mol_id: String,
    /// Blocking hull (CCW): inflated ink ∪ mol tile AABB.
    hull: Vec<Pt>,
    /// Inflated ink-only hull for tip pose — tips hug chem/captions, not the
    /// ELK tile edge (AABB hugging collapsed short shafts between neighbors).
    tip_hull: Vec<Pt>,
    /// Cached AABB of `hull` for fast segment reject.
    aabb: (f64, f64, f64, f64),
    /// Unpadded placement box for PackMetrics / viewport shaft checks.
    /// Equals `aabb` when the hull was not routing-padded.
    placement_aabb: (f64, f64, f64, f64),
}

fn obstacle_with_hull(mol_id: String, hull: Vec<Pt>) -> Obstacle {
    let aabb = hull_aabb(&hull);
    let tip_hull = hull.clone();
    Obstacle {
        mol_id,
        hull,
        tip_hull,
        aabb,
        placement_aabb: aabb,
    }
}

fn obstacle_with_hulls(mol_id: String, hull: Vec<Pt>, tip_hull: Vec<Pt>) -> Obstacle {
    let aabb = hull_aabb(&hull);
    Obstacle {
        mol_id,
        hull,
        tip_hull,
        aabb,
        placement_aabb: aabb,
    }
}

fn obstacle_tile(
    mol_id: String,
    padded_hull: Vec<Pt>,
    tip_hull: Vec<Pt>,
    placement: (f64, f64, f64, f64),
) -> Obstacle {
    let aabb = hull_aabb(&padded_hull);
    Obstacle {
        mol_id,
        hull: padded_hull,
        tip_hull,
        aabb,
        placement_aabb: placement,
    }
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
    engine: &EngineConfig,
) -> RouteResult {
    ACTIVE_ROUTE.with(|c| *c.borrow_mut() = Some(engine.route.clone()));
    let _route_guard = ActiveRouteGuard;
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

    let (obstacles, tile_obstacles) = build_obstacles(
        mol_ids,
        world,
        positions,
        &size_of,
        route().routing_clearance,
    );

    // Seed centroid chords for coterminal grouping (ELK paths discarded).
    let mut seeds: Vec<Vec<Pt>> = Vec::with_capacity(n_edges);
    for edge in spec.edges() {
        let (src, tgt) = edge_facing_src_tgt(edge, &centroids);
        let a = mean_centroid(edge.sources.as_slice(), &centroids)
            .unwrap_or_else(|| centroids.get(&src).copied().unwrap_or((0.0, 0.0)));
        let b = mean_centroid(edge.targets.as_slice(), &centroids)
            .unwrap_or_else(|| centroids.get(&tgt).copied().unwrap_or((0.0, 0.0)));
        seeds.push(vec![a, b]);
    }
    let groups = coterminal_groups(&spec.edges(), &seeds);

    // Visibility graph once per scheme on inflated chem convex hulls.
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
        let (src, tgt) = edge_facing_src_tgt(edge, &centroids);
        // Multi-endpoint: pathway + tips use the *group* centroid so the shaft
        // leaves the middle of A+B (not only the nearer mol's latitude).
        let src_c = mean_centroid(edge.sources.as_slice(), &centroids)
            .unwrap_or_else(|| centroids.get(&src).copied().unwrap_or((0.0, 0.0)));
        let tgt_c = mean_centroid(edge.targets.as_slice(), &centroids)
            .unwrap_or_else(|| centroids.get(&tgt).copied().unwrap_or((0.0, 0.0)));
        let min_shaft = min_shaft_px(edge.stroke_width);
        let clash = shaft_chem_clash(edge.stroke_width, group.members.len());
        // Free-space terminals for the geometric router (outside inflated hulls).
        // Crossing scores still use the centroid pathway, not these tips.
        let start = tip_outside_hull(
            &obstacles,
            &src,
            tip_pose_group(
                world,
                edge.sources.as_slice(),
                &centroids,
                src_c,
                tgt_c,
                clash,
                min_shaft,
            ),
            src_c,
            tgt_c,
        );
        let goal = tip_outside_hull(
            &obstacles,
            &tgt,
            tip_pose_group(
                world,
                edge.targets.as_slice(),
                &centroids,
                tgt_c,
                src_c,
                clash,
                min_shaft,
            ),
            tgt_c,
            src_c,
        );
        // Skip every co-endpoint mol so sibling sources/targets are not treated
        // as obstacles for the facing-tip chord (A+B→C tips on B→C).
        let mut skip_owned: HashSet<String> = HashSet::new();
        for id in edge.sources.as_slice().iter().chain(edge.targets.as_slice()) {
            skip_owned.insert(id.clone());
        }
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
    order.sort_by_key(|&i| {
        let (s, g) = tips[i];
        // Tile obstacles reject chords that skim foreign viewport whitespace;
        // ink obstacles alone miss those. Endpoint tiles are skipped.
        let skip_i: HashSet<&str> = skips[i].iter().map(String::as_str).collect();
        let tip_clear = segment_clears_obstacles(s, g, &tile_obstacles, &skip_i);
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

        // Lock a straight tip when the *tip* clears foreign ink + place boxes
        // and does not cross already-committed tip shafts. Do **not** require
        // zero centroid-pathway draft crossings — that forced Morphine's
        // sky-blue into a free-space length search and over-bent a place-clear
        // tip. Post-placement routing is geometric, not a cost minimizer.
        let tip_ink_ok = segment_clears_obstacles(start, goal, &obstacles, &skip);
        let tip_place_ok = path_clears_tile_aabbs(&[start, goal], &tile_obstacles, &skip);
        let tip_peer_ok = path_crosses_committed(&[start, goal], &committed) == 0;
        let keep_draft = tip_ink_ok && tip_place_ok && tip_peer_ok;
        let center = if keep_draft {
            vec![start, goal]
        } else {
            route_one_centerline(
                start,
                goal,
                src_c,
                tgt_c,
                src,
                tgt,
                &skip,
                world,
                mol_ids,
                clash,
                &obstacles,
                &tile_obstacles,
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
        &tile_obstacles,
        &vis,
    );

    // Final string-pull / collinear collapse must not *add* pathway crossings
    // or re-collapse to a tip chord that skims foreign viewport tiles.
    simplify_routed_centers(
        &mut group_centers,
        &group_centroids,
        &obstacles,
        &tile_obstacles,
        &skips,
    );

    nudge_shared_corridors(&mut group_centers, &group_centroids);
    // Nudge can reintroduce tiny elbows — pull again with the same rules.
    simplify_routed_centers(
        &mut group_centers,
        &group_centroids,
        &obstacles,
        &tile_obstacles,
        &skips,
    );

    // Tip layout last: hug ink on rays aimed at the *final* group centroids
    // (post mol-nudge + corridor offset). Earlier tip snaps would miss after
    // shared-corridor offsets slid end chords off the centroid line.
    resnap_tips_to_approach_rays(
        &mut group_centers,
        &group_ends,
        &group_centroids,
        &centroids,
        world,
        &obstacles,
        &tile_obstacles,
        &skips,
        &clashes,
        spec,
        &groups,
    );
    // Tip resnap moves endpoints — drop elbows that are now ink-redundant.
    simplify_ink_elbows(
        &mut group_centers,
        &group_centroids,
        &obstacles,
        &tile_obstacles,
        &skips,
    );
    // Resnap can drag a previously place-clear bow so a segment clips a
    // sibling box (Morphine sky through 8306). Repair geometrically: tip if
    // clear, else a tight hull/first-escape bow — no cost minimization.
    for gi in 0..group_centers.len() {
        if group_centers[gi].len() < 2 {
            continue;
        }
        let a = group_centers[gi][0];
        let b = *group_centers[gi].last().unwrap();
        let skip: HashSet<&str> = skips[gi].iter().map(String::as_str).collect();
        let tip = [a, b];
        let tip_ok = segment_clears_obstacles(a, b, &obstacles, &skip)
            && path_clears_tile_aabbs(&tip, &tile_obstacles, &skip);
        if tip_ok {
            group_centers[gi] = vec![a, b];
            continue;
        }
        if path_clears_tile_aabbs(&group_centers[gi], &tile_obstacles, &skip)
            && path_clears_obstacles(&group_centers[gi], &obstacles, &skip)
        {
            continue;
        }
        group_centers[gi] = fallback_bow(a, b, &obstacles, &tile_obstacles, &skip, &[]);
    }

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

/// Pathway crossing count for a free-space walk, including centroid stubs.
fn pathway_crosses_committed(
    path: &[Pt],
    src_c: Pt,
    tgt_c: Pt,
    committed: &[Committed],
) -> usize {
    path_crosses_committed(&extend_to_centroids(path, src_c, tgt_c), committed)
}

/// Straight tip when ink- and place-clear; else convex-hull visibility + pull.
/// Elbows come from visible chem-hull ports (not tile-AABB visibility).
fn route_one_centerline(
    start: Pt,
    goal: Pt,
    src_c: Pt,
    tgt_c: Pt,
    src: &str,
    tgt: &str,
    skip: &HashSet<&str>,
    world: &LayoutWorld,
    mol_ids: &[String],
    clash: f64,
    obstacles: &[Obstacle],
    tile_obstacles: &[Obstacle],
    vis: &VisGraph,
    committed: &[Committed],
) -> Vec<Pt> {
    let _ = (world, mol_ids, clash); // tip ink already posed; hulls pad structure
    // Straight tip when ink-clean and clear of foreign placement boxes.
    // Padded hull skims alone must not reject (network fan); deep AABB clips must.
    let ink_clear = segment_clears_obstacles(start, goal, obstacles, skip);
    let place_clear = path_clears_tile_aabbs(&[start, goal], tile_obstacles, skip);
    let crosses_locked = pathway_crosses_committed(&[start, goal], src_c, tgt_c, committed) > 0;
    // Pass-2 must actually re-route against the locked set. An ink-clear tip
    // chord that still crosses on the centroid pathway is not done.
    if ink_clear && place_clear && !crosses_locked {
        return prune_close_controls(&[start, goal], CLOSE_CTRL_PX);
    }

    let place_ok = |p: &[Pt]| path_clears_tile_aabbs(p, tile_obstacles, skip);
    let finish = |candidate: Vec<Pt>| {
        let mut out = prefer_tip_unless_detour_improves(
            start,
            goal,
            src_c,
            tgt_c,
            &candidate,
            obstacles,
            tile_obstacles,
            skip,
            committed,
        );
        // Drop elbows that exist only for tile pad — ink is the hard constraint,
        // but do not re-collapse onto a tip that cuts a foreign placement box.
        if out.len() >= 3 && path_clears_obstacles(&out, obstacles, skip) {
            let pulled =
                drop_skippable_controls(&out, obstacles, skip, committed, src_c, tgt_c);
            if path_clears_tile_aabbs(&pulled, tile_obstacles, skip)
                || !path_clears_tile_aabbs(&out, tile_obstacles, skip)
            {
                out = pulled;
            }
        }
        out
    };

    // Geometric bow first (chem ports / tight escape). Visibility A* can hitch
    // the scheme-frame highway and over-bend a place-dirty tip — that is a
    // post-placement cost walk the layout should not need.
    let bow = fallback_bow(start, goal, obstacles, tile_obstacles, skip, committed);
    let bow_ok = bow.len() >= 3
        && path_clears_obstacles(&bow, obstacles, skip)
        && place_ok(&bow)
        && !detour_backs_up_past_start(start, goal, &bow);
    if bow_ok {
        return finish(bow);
    }

    let vis_path = visibility_route(
        start,
        goal,
        src_c,
        tgt_c,
        src,
        tgt,
        skip,
        vis,
        obstacles,
        tile_obstacles,
        committed,
    );
    // String-pull on ink hulls; prefer_tip decides tip vs detour.
    let pulled =
        drop_skippable_controls(&vis_path, obstacles, skip, committed, src_c, tgt_c);
    let pulled_ok = path_clears_obstacles(&pulled, obstacles, skip)
        && path_clears_obstacles(&pulled, tile_obstacles, skip)
        && place_ok(&pulled);
    let pulled_x = pathway_crosses_committed(&pulled, src_c, tgt_c, committed);
    if pulled.len() == 2 && pulled_ok && pulled_x == 0 {
        return finish(pulled);
    }

    // Convex-hull visibility walk — still reject if it clips a placement box.
    let vis_ok = path_clears_obstacles(&vis_path, obstacles, skip)
        && path_clears_obstacles(&vis_path, tile_obstacles, skip)
        && place_ok(&vis_path);
    if pulled_ok && pulled_x == 0 && pulled.len() >= 2 {
        return finish(pulled);
    }
    if vis_ok && vis_path.len() >= 3 {
        return finish(prune_close_controls(&vis_path, CLOSE_CTRL_PX));
    }
    // Fall through with the (possibly weak) bow already computed.
    let bow_ink = path_clears_obstacles(&bow, obstacles, skip);
    let pulled_ink = path_clears_obstacles(&pulled, obstacles, skip);
    let bow_x = pathway_crosses_committed(&bow, src_c, tgt_c, committed);
    // Ink beats crossings: never pick an ink-hitting walk over an ink-clear one.
    let bow_tile = path_clears_obstacles(&bow, tile_obstacles, skip) && place_ok(&bow);
    let pulled_tile = path_clears_obstacles(&pulled, tile_obstacles, skip) && place_ok(&pulled);
    if bow_ink && !pulled_ink && bow.len() >= 2 {
        return finish(bow);
    }
    if pulled_ink && !bow_ink && pulled.len() >= 2 {
        return finish(pulled);
    }
    if bow_ink && pulled_ink {
        if bow_x < pulled_x && bow.len() >= 2 && (bow_tile || bow.len() >= 3) {
            return finish(bow);
        }
        if pulled_x < bow_x && pulled.len() >= 2 && pulled_tile {
            return finish(pulled);
        }
    }
    let candidate = match (pulled_ok, bow_ok_tile(bow_tile, &bow)) {
        (true, true) => prefer_simpler_path(&pulled, &bow),
        (true, false) => pulled,
        (false, true) => bow,
        (false, false) => {
            let vis = prune_close_controls(&vis_path, CLOSE_CTRL_PX);
            if vis.len() >= 3 && place_ok(&vis) {
                vis
            } else if bow.len() >= 3 {
                bow
            } else if pulled.len() >= 3 {
                pulled
            } else {
                let forced =
                    fallback_bow(start, goal, obstacles, tile_obstacles, skip, committed);
                if forced.len() >= 3 {
                    forced
                } else {
                    vec![start, goal]
                }
            }
        }
    };
    finish(candidate)
}

fn bow_ok_tile(tile_ok: bool, bow: &[Pt]) -> bool {
    tile_ok || bow.len() >= 3
}

/// Prefer the tip unless the detour *strictly improves* structure / cost.
///
/// Tip/ink policy: an ink-hitting tip loses to an ink-clear detour; both
/// ink-clean → prefer tip unless the tip cuts a foreign *placement* AABB
/// (deep sibling clip) and the detour clears that box without a hub wrap.
/// Padded tile hull skims alone must not force a wrap (network fan).
fn prefer_tip_unless_detour_improves(
    start: Pt,
    goal: Pt,
    src_c: Pt,
    tgt_c: Pt,
    detour: &[Pt],
    ink_obstacles: &[Obstacle],
    tile_obstacles: &[Obstacle],
    skip: &HashSet<&str>,
    committed: &[Committed],
) -> Vec<Pt> {
    if detour.len() < 3 {
        return detour.to_vec();
    }
    let tip = vec![start, goal];
    let tip_ink_ok = path_clears_obstacles(&tip, ink_obstacles, skip);
    let det_ink_ok = path_clears_obstacles(detour, ink_obstacles, skip);
    let tip_place_ok = path_clears_tile_aabbs(&tip, tile_obstacles, skip);
    let tip_tile_ok = path_clears_obstacles(&tip, tile_obstacles, skip);
    let det_tile_ok = path_clears_obstacles(detour, tile_obstacles, skip);
    match (tip_ink_ok, det_ink_ok) {
        (false, true) => {
            // Ink-clear detour wins unless it is a page-scale hub wrap.
            if detour_backs_up_past_start(start, goal, detour) {
                tip
            } else {
                detour.to_vec()
            }
        }
        (true, false) => {
            // Tip ink-clean but cuts a sibling placement box: keep the detour
            // when it is not a hub wrap (Morphine sky-blue through 8306).
            if !tip_place_ok && !detour_backs_up_past_start(start, goal, detour) {
                detour.to_vec()
            } else {
                tip
            }
        }
        (false, false) => tip, // both dirty — tip is simpler than a wild wrap
        (true, true) => {
            // Deep placement-box clip beats tip length. Never re-collapse to a
            // tip that punches a sibling viewport — Morphine sky-blue through
            // 8306 was ink-clean and still wrong.
            if !tip_place_ok {
                return detour.to_vec();
            }
            if detour_backs_up_past_start(start, goal, detour) {
                return tip;
            }
            // Both ink-clean: padded tile hull alone must not force a wrap.
            if tip_tile_ok && !det_tile_ok {
                return tip;
            }
            let tip_x = pathway_crosses_committed(&tip, src_c, tgt_c, committed);
            let det_x = pathway_crosses_committed(detour, src_c, tgt_c, committed);
            let tip_cost = polyline_len(&tip) + tip_x as f64 * route().crossing_penalty;
            let det_cost = polyline_len(detour) + det_x as f64 * route().crossing_penalty;
            if det_cost < tip_cost {
                detour.to_vec()
            } else {
                tip
            }
        }
    }
}

/// True when a detour reverses along the start→goal chord or wanders far
/// outside the start→goal box (page-scale hub wraps). Lateral peeks above or
/// beside the start to skirt a sibling tile are *not* backs-ups — axis-aligned
/// "above start" checks were rejecting Morphine side bows and collapsing them
/// onto tip-through-tile chords.
fn detour_backs_up_past_start(start: Pt, goal: Pt, detour: &[Pt]) -> bool {
    let dx = goal.0 - start.0;
    let dy = goal.1 - start.1;
    let chord = dist(start, goal).max(1.0);
    let ux = dx / chord;
    let uy = dy / chord;
    let back_pad = 40.0;
    for &p in detour {
        let along = (p.0 - start.0) * ux + (p.1 - start.1) * uy;
        if along < -back_pad {
            return true;
        }
    }
    let min_x = start.0.min(goal.0);
    let max_x = start.0.max(goal.0);
    let min_y = start.1.min(goal.1);
    let max_y = start.1.max(goal.1);
    let pad = chord * 0.25 + 80.0;
    for &p in detour {
        if p.0 < min_x - pad || p.0 > max_x + pad || p.1 < min_y - pad || p.1 > max_y + pad {
            return true;
        }
    }
    false
}

/// Strict interior AABB samples against **placement** boxes — matches
/// PackMetrics / review viewport checks more closely than convex-hull
/// membership (which can miss shallow clips). Used to reject tip chords that
/// cut through a sibling mol box even when padded-hull / ink checks pass.
fn path_clears_tile_aabbs(pts: &[Pt], tiles: &[Obstacle], skip: &HashSet<&str>) -> bool {
    if pts.len() < 2 {
        return true;
    }
    let mut len = 0.0;
    for w in pts.windows(2) {
        len += dist(w[0], w[1]);
    }
    if len < 8.0 {
        return true;
    }
    let skip_ends = (len * 0.08).max(4.0).min(18.0);
    let inset = 4.0;
    let mut along = 0.0;
    for w in pts.windows(2) {
        let seg = dist(w[0], w[1]);
        let n = ((seg / 4.0).ceil() as usize).max(1);
        for i in 0..=n {
            let t = i as f64 / n as f64;
            let q = (w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t);
            let s = along + seg * t;
            if s < skip_ends || s > len - skip_ends {
                continue;
            }
            for o in tiles {
                if skip.contains(o.mol_id.as_str()) {
                    continue;
                }
                let (x0, y0, x1, y1) = o.placement_aabb;
                if x1 - x0 < inset * 2.0 || y1 - y0 < inset * 2.0 {
                    continue;
                }
                if q.0 > x0 + inset && q.0 < x1 - inset && q.1 > y0 + inset && q.1 < y1 - inset
                {
                    return false;
                }
            }
        }
        along += seg;
    }
    true
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

/// True when interior control `b` can be dropped: neighbor chord `a→c` clears
/// obstacles and does not add pathway crossings vs `a→b→c`.
fn control_skippable(
    a: Pt,
    b: Pt,
    c: Pt,
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
    committed: &[Committed],
    src_c: Pt,
    tgt_c: Pt,
) -> bool {
    if !segment_clears_obstacles(a, c, obstacles, skip) {
        return false;
    }
    let sub_x = pathway_crosses_committed(&[a, b, c], src_c, tgt_c, committed);
    let chord_x = pathway_crosses_committed(&[a, c], src_c, tgt_c, committed);
    chord_x <= sub_x
}

/// Drop each interior control that is safe to skip (clear chord, no extra X).
/// Cheaper than funnel string-pull: one clearance test per candidate elbow.
fn drop_skippable_controls(
    pts: &[Pt],
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
    committed: &[Committed],
    src_c: Pt,
    tgt_c: Pt,
) -> Vec<Pt> {
    let mut pts = prune_close_controls(pts, CLOSE_CTRL_PX);
    if pts.len() < 3 {
        return pts;
    }
    // Repeat until stable so chains of redundant elbows collapse.
    loop {
        let mut out = vec![pts[0]];
        let mut dropped = false;
        let mut i = 1;
        while i + 1 < pts.len() {
            let a = *out.last().unwrap();
            let b = pts[i];
            let c = pts[i + 1];
            if control_skippable(a, b, c, obstacles, skip, committed, src_c, tgt_c) {
                dropped = true;
                i += 1;
                continue;
            }
            out.push(b);
            i += 1;
        }
        out.push(*pts.last().unwrap());
        pts = prune_close_controls(&out, CLOSE_CTRL_PX);
        if !dropped || pts.len() < 3 {
            break;
        }
    }
    collapse_collinear(&pts, obstacles, skip, committed)
}

/// Remove intermediate verts when the chord still clears obstacles (classic
/// string-pull / funnel). Prefer [`drop_skippable_controls`] for elbow cleanup.
///
/// Crossing rank uses the **centroid pathway** (`src_c`/`tgt_c` stubs): a tip
/// pose chord can miss an X that the mol→mol pathway still makes, and must not
/// collapse a priced detour back onto that tip.
fn string_pull(
    pts: &[Pt],
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
    committed: &[Committed],
    src_c: Pt,
    tgt_c: Pt,
) -> Vec<Pt> {
    // Funnel search is O(n²) pathway rebuilds; skip-tests are enough in practice.
    drop_skippable_controls(pts, obstacles, skip, committed, src_c, tgt_c)
}

/// Drop intermediate verts that are nearly collinear when the chord still
/// clears hulls and does not add committed crossings.
fn collapse_collinear(
    pts: &[Pt],
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
    committed: &[Committed],
) -> Vec<Pt> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    // Cosine of turn: keep only real elbows (~> ~14°). Tighter thresholds left
    // inflate-pad micro-bends that Catmull turns into visible kinks.
    const COLLINEAR_DOT: f64 = 0.97;
    let mut out = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let a = *out.last().unwrap();
        let b = pts[i];
        let c = pts[i + 1];
        let (ux, uy) = unit((b.0 - a.0, b.1 - a.1));
        let (vx, vy) = unit((c.0 - b.0, c.1 - b.1));
        let collinear = ux * vx + uy * vy >= COLLINEAR_DOT;
        let short_stub = dist(a, b) < BOND_PX || dist(b, c) < BOND_PX;
        if collinear || short_stub {
            let sub_x = path_crosses_committed(&[a, b, c], committed);
            let chord_x = path_crosses_committed(&[a, c], committed);
            if segment_clears_obstacles(a, c, obstacles, skip) && chord_x <= sub_x {
                continue; // drop b
            }
        }
        out.push(b);
    }
    out.push(*pts.last().unwrap());
    prune_close_controls(&out, CLOSE_CTRL_PX)
}

/// Crossing- and (deflated-)obstacle-safe Ramer–Douglas–Peucker. `pad_obstacles`
/// should be routing hulls shrunk by `eps` so inflate-pad stair-steps collapse
/// while chords through real blockers stay rejected.
fn cosmetic_douglas_peucker(
    pts: &[Pt],
    committed: &[Committed],
    pad_obstacles: &[Obstacle],
    skip: &HashSet<&str>,
    eps: f64,
) -> Vec<Pt> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    fn rec(
        pts: &[Pt],
        committed: &[Committed],
        pad_obstacles: &[Obstacle],
        skip: &HashSet<&str>,
        eps: f64,
        out: &mut Vec<Pt>,
    ) {
        if pts.len() < 3 {
            if let Some(&p) = pts.last() {
                if out.last().copied() != Some(p) {
                    out.push(p);
                }
            }
            return;
        }
        let a = pts[0];
        let c = pts[pts.len() - 1];
        let mut best_i = 0usize;
        let mut best_d = 0.0;
        for (i, &b) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
            let d = point_line_dist(b, a, c);
            if d > best_d {
                best_d = d;
                best_i = i;
            }
        }
        let sub_x = path_crosses_committed(pts, committed);
        let chord_x = path_crosses_committed(&[a, c], committed);
        if best_d <= eps
            && chord_x <= sub_x
            && segment_clears_obstacles(a, c, pad_obstacles, skip)
        {
            if out.last().copied() != Some(c) {
                out.push(c);
            }
            return;
        }
        if best_i == 0 {
            for &p in pts.iter().skip(1) {
                if out.last().copied() != Some(p) {
                    out.push(p);
                }
            }
            return;
        }
        rec(&pts[..=best_i], committed, pad_obstacles, skip, eps, out);
        rec(&pts[best_i..], committed, pad_obstacles, skip, eps, out);
    }
    let mut out = vec![pts[0]];
    rec(pts, committed, pad_obstacles, skip, eps, &mut out);
    prune_close_controls(&out, CLOSE_CTRL_PX)
}

fn pad_tolerant_obstacles(obstacles: &[Obstacle], shrink: f64) -> Vec<Obstacle> {
    obstacles
        .iter()
        .map(|o| {
            let mut out = obstacle_with_hulls(
                o.mol_id.clone(),
                inflate_convex(&o.hull, -shrink),
                inflate_convex(&o.tip_hull, -shrink),
            );
            out.placement_aabb = o.placement_aabb;
            out
        })
        .collect()
}

fn point_line_dist(p: Pt, a: Pt, b: Pt) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let len = vx.hypot(vy);
    if len < 1e-9 {
        return dist(p, a);
    }
    let t = ((p.0 - a.0) * vx + (p.1 - a.1) * vy) / (len * len);
    let t = t.clamp(0.0, 1.0);
    let q = (a.0 + vx * t, a.1 + vy * t);
    dist(p, q)
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

/// Drop intermediate controls that are unnecessary for **ink** clearance
/// (and do not add pathway crossings). Tile-only elbows are fair game.
fn simplify_ink_elbows(
    centers: &mut [Vec<Pt>],
    group_centroids: &[(Pt, Pt)],
    ink_obstacles: &[Obstacle],
    tile_obstacles: &[Obstacle],
    skips: &[HashSet<String>],
) {
    for gi in 0..centers.len() {
        if centers[gi].len() < 3 {
            continue;
        }
        let skip: HashSet<&str> = skips
            .get(gi)
            .map(|s| s.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let (src_c, tgt_c) = group_centroids[gi];
        let others: Vec<Committed> = centers
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != gi)
            .map(|(j, c)| {
                let (sc, tc) = group_centroids[j];
                committed_from_pathway(c, sc, tc)
            })
            .collect();
        if !path_clears_obstacles(&centers[gi], ink_obstacles, &skip) {
            continue;
        }
        let before_x = pathway_crosses_committed(&centers[gi], src_c, tgt_c, &others);
        let before_tile = path_clears_obstacles(&centers[gi], tile_obstacles, &skip);
        let before_place = path_clears_tile_aabbs(&centers[gi], tile_obstacles, &skip);
        let pulled =
            drop_skippable_controls(&centers[gi], ink_obstacles, &skip, &others, src_c, tgt_c);
        if pulled.len() < 2 || !path_clears_obstacles(&pulled, ink_obstacles, &skip) {
            continue;
        }
        let after_x = pathway_crosses_committed(&pulled, src_c, tgt_c, &others);
        let after_tile = path_clears_obstacles(&pulled, tile_obstacles, &skip);
        let after_place = path_clears_tile_aabbs(&pulled, tile_obstacles, &skip);
        // Ink-only pull must not turn a place/tile-clear bow into a tip-through-box.
        if after_x <= before_x
            && pulled.len() <= centers[gi].len()
            && (after_tile || !before_tile)
            && (after_place || !before_place)
        {
            centers[gi] = pulled;
        }
    }
}

/// String-pull + collinear collapse + pad-tolerant Douglas–Peucker for every
/// centerline; accept only when pathway crossings do not increase.
fn simplify_routed_centers(
    centers: &mut [Vec<Pt>],
    group_centroids: &[(Pt, Pt)],
    obstacles: &[Obstacle],
    tile_obstacles: &[Obstacle],
    skips: &[HashSet<String>],
) {
    let tile_pad = pad_tolerant_obstacles(tile_obstacles, route().cosmetic_simplify_eps);
    for gi in 0..centers.len() {
        let skip: HashSet<&str> = skips
            .get(gi)
            .map(|s| s.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let (src_c, tgt_c) = group_centroids[gi];
        let others: Vec<Committed> = centers
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != gi)
            .map(|(j, c)| {
                let (sc, tc) = group_centroids[j];
                committed_from_pathway(c, sc, tc)
            })
            .collect();
        let before = centers[gi].clone();
        let before_x =
            path_crosses_committed(&extend_to_centroids(&before, src_c, tgt_c), &others);
        // Skip-tests drop tile-only elbows; tile pass may not reintroduce ink hits.
        let ink_pulled =
            drop_skippable_controls(&before, obstacles, &skip, &others, src_c, tgt_c);
        let tile_pulled =
            drop_skippable_controls(&ink_pulled, tile_obstacles, &skip, &others, src_c, tgt_c);
        let mut pulled = if path_clears_obstacles(&tile_pulled, obstacles, &skip) {
            tile_pulled
        } else {
            ink_pulled
        };
        pulled = cosmetic_douglas_peucker(
            &pulled,
            &others,
            &tile_pad,
            &skip,
            route().cosmetic_simplify_eps,
        );
        // Second pass against deflated hulls: drop leftover ~10° elbows that
        // only existed to walk the inflate pad.
        pulled = collapse_collinear(&pulled, &tile_pad, &skip, &others);
        pulled = drop_skippable_controls(&pulled, &tile_pad, &skip, &others, src_c, tgt_c);
        // Prefer a tip–tip chord or single bow when tile-clear and no extra X.
        if pulled.len() > 2 {
            let a = pulled[0];
            let b = *pulled.last().unwrap();
            if segment_clears_obstacles(a, b, tile_obstacles, &skip) {
                let chord = vec![a, b];
                let cx = path_crosses_committed(
                    &extend_to_centroids(&chord, src_c, tgt_c),
                    &others,
                );
                if cx <= before_x {
                    pulled = chord;
                }
            }
            if pulled.len() > 2 {
                let bow = fallback_bow(a, b, tile_obstacles, tile_obstacles, &skip, &others);
                if bow.len() < pulled.len() && path_clears_obstacles(&bow, tile_obstacles, &skip)
                {
                    let bx = path_crosses_committed(
                        &extend_to_centroids(&bow, src_c, tgt_c),
                        &others,
                    );
                    if bx <= before_x {
                        pulled = bow;
                    }
                }
            }
        }
        // Ink is hard. Full foreign tiles are also hard for *accepting* a
        // simplify — pad-tolerant pulls may drop inflate-only elbows, but must
        // not reintroduce a shaft through a sibling viewport box.
        if !path_clears_obstacles(&pulled, obstacles, &skip) {
            continue;
        }
        let after_x =
            path_crosses_committed(&extend_to_centroids(&pulled, src_c, tgt_c), &others);
        let fewer_bends = pulled.len() < before.len();
        let tile_ok = path_clears_obstacles(&pulled, tile_obstacles, &skip);
        let before_tile = path_clears_obstacles(&before, tile_obstacles, &skip);
        let pad_ok = path_clears_obstacles(&pulled, &tile_pad, &skip);
        if !tile_ok && before_tile {
            continue;
        }
        if after_x < before_x
            || (after_x == before_x && tile_ok && !before_tile)
            || (after_x == before_x && fewer_bends && tile_ok)
            || (after_x == before_x && pad_ok && before_tile && pulled.len() <= before.len())
        {
            centers[gi] = pulled;
        }
    }
}

/// Pairwise pathway crossing counts. `pair[i * n + j]` (i < j) stores how many
/// proper crossings pathway i makes with j; `degree[i]` is the row sum.
struct CrossingIndex {
    n: usize,
    /// Upper triangle packed? Keep dense n×n for simple updates (symmetric).
    pair: Vec<usize>,
    degree: Vec<usize>,
}

impl CrossingIndex {
    fn new(centers: &[Vec<Pt>], group_centroids: &[(Pt, Pt)]) -> Self {
        let n = centers.len();
        let mut pair = vec![0usize; n * n];
        let mut degree = vec![0usize; n];
        for i in 0..n {
            let (sc_i, tc_i) = group_centroids[i];
            let a = extend_to_centroids(&centers[i], sc_i, tc_i);
            for j in (i + 1)..n {
                let (sc_j, tc_j) = group_centroids[j];
                let b = extend_to_centroids(&centers[j], sc_j, tc_j);
                let c = count_poly_crossings(&a, &b);
                if c == 0 {
                    continue;
                }
                pair[i * n + j] = c;
                pair[j * n + i] = c;
                degree[i] += c;
                degree[j] += c;
            }
        }
        Self { n, pair, degree }
    }

    /// After `centers[gi]` changed, recompute only pairs involving `gi`.
    fn refresh_arrow(
        &mut self,
        gi: usize,
        centers: &[Vec<Pt>],
        group_centroids: &[(Pt, Pt)],
    ) {
        let n = self.n;
        let (sc_i, tc_i) = group_centroids[gi];
        let a = extend_to_centroids(&centers[gi], sc_i, tc_i);
        // Drop old contributions involving gi.
        for j in 0..n {
            if j == gi {
                continue;
            }
            let old = self.pair[gi * n + j];
            if old > 0 {
                self.degree[gi] -= old;
                self.degree[j] -= old;
                self.pair[gi * n + j] = 0;
                self.pair[j * n + gi] = 0;
            }
        }
        // Add new contributions.
        for j in 0..n {
            if j == gi {
                continue;
            }
            let (sc_j, tc_j) = group_centroids[j];
            let b = extend_to_centroids(&centers[j], sc_j, tc_j);
            let c = count_poly_crossings(&a, &b);
            if c == 0 {
                continue;
            }
            self.pair[gi * n + j] = c;
            self.pair[j * n + gi] = c;
            self.degree[gi] += c;
            self.degree[j] += c;
        }
    }
}

/// Re-route crossing arrows **one at a time**: skip zero-cross, prefer most
/// intersections then longest length, commit immediately, and only refresh
/// pairwise counts that involve the changed arrow.
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
    tile_obstacles: &[Obstacle],
    vis: &VisGraph,
) {
    let n = centers.len();
    if n < 2 {
        return;
    }
    let mut index = CrossingIndex::new(centers, group_centroids);
    let mut lengths: Vec<f64> = (0..n)
        .map(|i| {
            let (sc, tc) = group_centroids[i];
            polyline_len(&extend_to_centroids(&centers[i], sc, tc))
        })
        .collect();
    let max_steps = n
        .saturating_mul(route().reopt_passes)
        .max(route().reopt_passes)
        .min(route().reopt_max_steps);
    let mut stuck: HashSet<usize> = HashSet::new();
    for _ in 0..max_steps {
        // Skip zero-cross; among the rest: most intersections, then longest.
        let mut worst: Option<(usize, f64, usize)> = None; // (cross, len, gi)
        for i in 0..n {
            if stuck.contains(&i) || index.degree[i] == 0 {
                continue;
            }
            let c = index.degree[i];
            let len = lengths[i];
            let replace = match worst {
                None => true,
                Some((wc, wlen, wi)) => {
                    c > wc || (c == wc && (len > wlen || (len == wlen && i < wi)))
                }
            };
            if replace {
                worst = Some((c, len, i));
            }
        }
        let Some((before_x, _, gi)) = worst else {
            break;
        };
        let before = centers[gi].clone();
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
        let (src_c, tgt_c) = group_centroids[gi];
        let (start, goal) = tips[gi];
        let skip: HashSet<&str> = skips[gi].iter().map(String::as_str).collect();
        let clash = clashes[gi];
        let candidate = route_one_centerline(
            start,
            goal,
            src_c,
            tgt_c,
            src,
            tgt,
            &skip,
            world,
            mol_ids,
            clash,
            obstacles,
            tile_obstacles,
            vis,
            &peers,
        );
        if candidate.len() < 2 {
            stuck.insert(gi);
            continue;
        }
        centers[gi] = candidate;
        index.refresh_arrow(gi, centers, group_centroids);
        let after_x = index.degree[gi];
        let clears = path_clears_obstacles(&centers[gi], tile_obstacles, &skip);
        let before_clears = path_clears_obstacles(&before, tile_obstacles, &skip);
        let new_len = polyline_len(&extend_to_centroids(&centers[gi], src_c, tgt_c));
        // Lexicographic: fewer pathway crossings first, then clearance, then length.
        let better = after_x < before_x
            || (after_x == before_x && clears && !before_clears)
            || (after_x == before_x
                && clears
                && before_clears
                && new_len + 1.0 < lengths[gi]);
        if better {
            lengths[gi] = new_len;
            // Fixed in place — peers' degrees already refreshed via index.
            stuck.clear();
        } else {
            centers[gi] = before;
            index.refresh_arrow(gi, centers, group_centroids);
            stuck.insert(gi);
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

/// Place start/end tips on rays that aim at the final group centroids.
///
/// Tips always pose along `src_c → tgt_c` (and reverse), even when the pathway
/// bows — aiming at the first elbow left Morphine fan tips ~40px off-chord.
/// Ink clearance for the tip move still validates against the next/prev elbow.
fn resnap_tips_to_approach_rays(
    centers: &mut [Vec<Pt>],
    ends: &[(String, String)],
    group_centroids: &[(Pt, Pt)],
    centroids: &HashMap<String, Pt>,
    world: &LayoutWorld,
    obstacles: &[Obstacle],
    tile_obstacles: &[Obstacle],
    skips: &[HashSet<String>],
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
        let (src_c, tgt_c) = group_centroids
            .get(gi)
            .copied()
            .unwrap_or((centers[gi][0], centers[gi][centers[gi].len() - 1]));
        let clash = clashes.get(gi).copied().unwrap_or(SCHEME_INK_STANDOFF);
        let edge = groups
            .get(gi)
            .and_then(|g| edges.get(g.members[0]));
        let min_shaft = edge
            .map(|e| min_shaft_px(e.stroke_width))
            .unwrap_or_else(|| min_shaft_px(None));
        let src_ids = edge
            .map(|e| e.sources.as_slice())
            .unwrap_or(std::slice::from_ref(src));
        let tgt_ids = edge
            .map(|e| e.targets.as_slice())
            .unwrap_or(std::slice::from_ref(tgt));
        let next = centers[gi][1];
        let prev = centers[gi][centers[gi].len() - 2];
        let toward_start = tgt_c;
        let toward_goal = src_c;
        let start = tip_outside_hull(
            obstacles,
            src,
            tip_pose_group(
                world,
                src_ids,
                centroids,
                src_c,
                toward_start,
                clash,
                min_shaft,
            ),
            src_c,
            toward_start,
        );
        let goal = tip_outside_hull(
            obstacles,
            tgt,
            tip_pose_group(
                world,
                tgt_ids,
                centroids,
                tgt_c,
                toward_goal,
                clash,
                min_shaft,
            ),
            tgt_c,
            toward_goal,
        );
        // Tip layout only cares about hull clearance — pathway crossings are
        // already decided and must not be "fixed" by pulling tips back.
        let empty = HashSet::new();
        let skip: HashSet<&str> = skips
            .get(gi)
            .map(|s| s.iter().map(String::as_str).collect())
            .unwrap_or_default();
        // Ink-clear check still uses the next/prev pathway control so a bowed
        // first segment is validated, not the far centroid tip.
        let clear_next = if centers[gi].len() == 2 { goal } else { next };
        let clear_prev = if centers[gi].len() == 2 { start } else { prev };
        // Tip hug may lengthen a chord through a sibling placement box — refuse
        // endpoint moves that introduce a deep foreign-tile clip (2-pt or bow).
        let place_ok = |pts: &[Pt]| path_clears_tile_aabbs(pts, tile_obstacles, &skip);
        let before = centers[gi].clone();
        let try_start = {
            let mut cand = before.clone();
            cand[0] = start;
            cand
        };
        if segment_clears_obstacles(start, clear_next, obstacles, &empty)
            && (place_ok(&try_start) || !place_ok(&before))
        {
            centers[gi][0] = start;
        }
        let try_goal = {
            let mut cand = centers[gi].clone();
            let last = cand.len() - 1;
            cand[last] = goal;
            cand
        };
        if segment_clears_obstacles(clear_prev, goal, obstacles, &empty)
            && (place_ok(&try_goal) || !place_ok(&centers[gi]))
        {
            let last = centers[gi].len() - 1;
            centers[gi][last] = goal;
        }
        centers[gi] = prune_close_controls(&centers[gi], CLOSE_CTRL_PX);
        // prune_close_controls can drop a visible-port elbow and leave a tip
        // through a sibling box — restore a port bow when that happens.
        if centers[gi].len() == 2 && !place_ok(&centers[gi]) {
            let a = centers[gi][0];
            let b = centers[gi][1];
            // Hull ports from chem obstacles; place AABBs only gate acceptance.
            let bow = fallback_bow(a, b, obstacles, tile_obstacles, &skip, &[]);
            if bow.len() >= 3 && place_ok(&bow) {
                centers[gi] = bow;
            } else if before.len() >= 3 && place_ok(&before) {
                centers[gi] = before;
            }
        }
    }
}

/// Returns `(ink_obstacles, tile_obstacles)`.
///
/// - **Ink** obstacles drive the visibility graph / bows (tight).
/// - **Tile** obstacles (= inflated ink ∪ padded placement AABB) only gate
///   straight tip chords so Morphine-style fans cannot skim foreign
///   viewport whitespace. Using tiles for visibility forced page-edge loops.
fn build_obstacles(
    mol_ids: &[String],
    world: &LayoutWorld,
    positions: &HashMap<String, Pt>,
    size_of: &HashMap<&str, (f64, f64)>,
    inflate: f64,
) -> (Vec<Obstacle>, Vec<Obstacle>) {
    let mut ink_out = Vec::with_capacity(mol_ids.len());
    let mut tile_out = Vec::with_capacity(mol_ids.len());
    for id in mol_ids {
        let under = format!("scheme/mols/{id}");
        let ink_pts = collect_ink_points(world, &under);
        let tile = match (positions.get(id.as_str()), size_of.get(id.as_str())) {
            (Some(&(x, y)), Some(&(w, h))) => Some((x, y, x + w, y + h)),
            _ => world.get(&under).map(|n| {
                let a = n.world_shape().aabb();
                (a.x0, a.y0, a.x1, a.y1)
            }),
        };
        // Vis / ink obstacle = simplified chem convex hull + slight standoff.
        // Do not dense-sample ink curves — few ports, elbows stay readable.
        let tip_hull = if ink_pts.len() >= 3 {
            simplified_standoff_hull(&ink_pts, inflate, route().max_hull_verts)
        } else if let Some((x0, y0, x1, y1)) = tile {
            simplified_standoff_hull(
                &[(x0, y0), (x1, y0), (x1, y1), (x0, y1)],
                inflate,
                route().max_hull_verts,
            )
        } else {
            continue;
        };
        if tip_hull.len() < 3 {
            continue;
        }
        ink_out.push(obstacle_with_hulls(id.clone(), tip_hull.clone(), tip_hull.clone()));

        // Tile gate uses the placement AABB (plus pad). `convex_hull_pts` makes
        // the ring CCW — a hand-wound TL→TR→BR→BL quad is clockwise in y-down
        // space and made `point_strictly_in_convex` treat every interior sample
        // as outside (Morphine tips through sibling tiles looked "clear").
        if let Some((x0, y0, x1, y1)) = tile {
            let p = inflate.max(0.0);
            let tile_hull = convex_hull_pts(&[
                (x0 - p, y0 - p),
                (x1 + p, y0 - p),
                (x1 + p, y1 + p),
                (x0 - p, y1 + p),
            ]);
            if tile_hull.len() >= 3 {
                tile_out.push(obstacle_tile(
                    id.clone(),
                    tile_hull,
                    tip_hull,
                    (x0, y0, x1, y1),
                ));
            }
        } else if tip_hull.len() >= 3 {
            let place = hull_aabb(&tip_hull);
            tile_out.push(obstacle_tile(
                id.clone(),
                tip_hull.clone(),
                tip_hull,
                place,
            ));
        }
    }
    (ink_out, tile_out)
}

/// Coarse subset of a convex ring — vis ports, not dense curve sampling.
fn subsample_convex_ring(hull: &[Pt], max_verts: usize) -> Vec<Pt> {
    let n = hull.len();
    if n <= max_verts || max_verts < 3 {
        return hull.to_vec();
    }
    let mut out = Vec::with_capacity(max_verts);
    for i in 0..max_verts {
        let idx = (i * n) / max_verts;
        out.push(hull[idx]);
    }
    out
}

/// Chem ink → convex hull → few verts → slight outward standoff.
/// This ring is both the ink obstacle and the visibility-graph ports.
fn simplified_standoff_hull(ink_pts: &[Pt], standoff: f64, max_verts: usize) -> Vec<Pt> {
    let raw = convex_hull_pts(ink_pts);
    if raw.len() < 3 {
        return raw;
    }
    let simple = subsample_convex_ring(&raw, max_verts.max(3));
    inflate_convex(&simple, standoff)
}

/// Push chem-hull verts radially from the place-box center until each clears
/// the padded AABB. Elbows track the chem silhouette, not box corners.
fn project_hull_outside_place(chem: &[Pt], place: (f64, f64, f64, f64), clearance: f64) -> Vec<Pt> {
    let (x0, y0, x1, y1) = place;
    if x1 - x0 < 4.0 || y1 - y0 < 4.0 || chem.is_empty() {
        return Vec::new();
    }
    let cx = 0.5 * (x0 + x1);
    let cy = 0.5 * (y0 + y1);
    let outside = |q: Pt| {
        q.0 < x0 - clearance
            || q.0 > x1 + clearance
            || q.1 < y0 - clearance
            || q.1 > y1 + clearance
    };
    let mut out = Vec::with_capacity(chem.len());
    for &p in chem {
        if outside(p) {
            out.push(p);
            continue;
        }
        let mut dx = p.0 - cx;
        let mut dy = p.1 - cy;
        let len = dx.hypot(dy);
        if len < 1e-9 {
            // Degenerate: push toward the nearest place edge.
            let to_l = p.0 - x0;
            let to_r = x1 - p.0;
            let to_t = p.1 - y0;
            let to_b = y1 - p.1;
            let m = to_l.min(to_r).min(to_t).min(to_b);
            if (m - to_l).abs() < 1e-9 {
                dx = -1.0;
                dy = 0.0;
            } else if (m - to_r).abs() < 1e-9 {
                dx = 1.0;
                dy = 0.0;
            } else if (m - to_t).abs() < 1e-9 {
                dx = 0.0;
                dy = -1.0;
            } else {
                dx = 0.0;
                dy = 1.0;
            }
        } else {
            dx /= len;
            dy /= len;
        }
        let mut q = p;
        let mut advanced = false;
        for step in 1..=128 {
            q = (p.0 + dx * step as f64, p.1 + dy * step as f64);
            if outside(q) {
                out.push(q);
                advanced = true;
                break;
            }
        }
        if !advanced {
            // Fallback: land on the padded place boundary along this ray.
            out.push((
                p.0 + dx * ((x1 - x0) + (y1 - y0) + clearance),
                p.1 + dy * ((x1 - x0) + (y1 - y0) + clearance),
            ));
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

/// Visibility graph on **simplified chem-hull ports** (slight standoff):
/// nodes are the coarse hull verts already stored on each ink obstacle;
/// edges are boundary walks plus clear LOS between foreign hulls.
/// No tile-AABB Steiners — elbows belong on the hull.
fn build_visibility_graph(obstacles: &[Obstacle]) -> VisGraph {
    let mut pts = Vec::new();
    let mut owner = Vec::new();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for o in obstacles {
        let start = pts.len();
        // Ink obstacles already carry simplified+standoff tip_hull.
        let ring = if o.tip_hull.len() >= 3 {
            &o.tip_hull
        } else {
            &o.hull
        };
        // Ring is already coarse; keep as-is (no second denser resample).
        for &p in ring {
            pts.push(p);
            owner.push(Some(o.mol_id.clone()));
        }
        ranges.push((start, pts.len()));
    }
    // Scheme frame — free-space highway linked only to nearest hull ports.
    let frame_range = if pts.is_empty() {
        None
    } else {
        let (gx0, gy0, gx1, gy1) = hull_aabb(&pts);
        let m = route().frame_margin;
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

    // 1) Hull boundary edges (walk around each chem convex).
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

    let empty_skip = HashSet::new();
    // 2) Scheme frame cycle (inflated cost so local hull bows win).
    if let Some((flo, _fhi)) = frame_range {
        for k in 0..4 {
            let i = flo + k;
            let j = flo + (k + 1) % 4;
            if segment_clears_obstacles(pts[i], pts[j], obstacles, &empty_skip) {
                add(
                    &mut adj,
                    &mut linked,
                    i,
                    j,
                    frame_edge_cost(dist(pts[i], pts[j])),
                );
            }
        }
    }

    // 3) Frame ↔ nearest hull ports; frame diagonals.
    let frame: Vec<usize> = owner
        .iter()
        .enumerate()
        .filter_map(|(i, o)| o.is_none().then_some(i))
        .collect();
    for &fi in &frame {
        for &(hlo, hhi) in &ranges {
            let mut cands: Vec<(f64, usize)> = (hlo..hhi)
                .map(|j| (dist(pts[fi], pts[j]), j))
                .collect();
            cands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let mut kept = 0usize;
            for (len, j) in cands {
                if kept >= route().frame_hull_near {
                    break;
                }
                // Strict-interior hit test: silhouette ports clear; through-hull
                // links to the back face do not (do not skip the port owner).
                if segment_clears_obstacles(pts[fi], pts[j], obstacles, &empty_skip) {
                    add(&mut adj, &mut linked, fi, j, frame_edge_cost(len));
                    kept += 1;
                }
            }
        }
        for &fj in &frame {
            if fj <= fi {
                continue;
            }
            let len = dist(pts[fi], pts[fj]);
            if segment_clears_obstacles(pts[fi], pts[fj], obstacles, &empty_skip) {
                add(&mut adj, &mut linked, fi, fj, frame_edge_cost(len));
            }
        }
    }

    // 4) Inter-hull visibility: clear line-of-sight between foreign hull ports.
    // Keep both owners in the hit test so only exterior-visible pairs link.
    for i in 0..n {
        if owner[i].is_none() {
            continue;
        }
        let oi = owner[i].as_deref();
        let mut cands: Vec<(f64, usize)> = Vec::new();
        for j in 0..n {
            if j == i || owner[j].is_none() {
                continue;
            }
            if oi.is_some() && owner[j].as_deref() == oi {
                continue;
            }
            cands.push((dist(pts[i], pts[j]), j));
        }
        cands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut kept = 0usize;
        for (len, j) in cands {
            if kept >= route().inter_vis_near {
                break;
            }
            if segment_clears_obstacles(pts[i], pts[j], obstacles, &empty_skip) {
                add(&mut adj, &mut linked, i, j, len);
                kept += 1;
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
    if n < 3 || dist.abs() < 1e-12 {
        return hull.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = hull[(i + n - 1) % n];
        let cur = hull[i];
        let next = hull[(i + 1) % n];
        let e1 = (cur.0 - prev.0, cur.1 - prev.1);
        let e2 = (next.0 - cur.0, next.1 - cur.1);
        // CCW walk: outward is right of the edge (ey, -ex). Negative `dist`
        // shrinks (pad-tolerant simplify / cosmetic DP).
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
    // Use ink tip hull — not the blocking AABB∪ink hull.
    let hull = &o.tip_hull;
    if !point_in_convex(tip, hull) {
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
    if !point_in_convex(at(hi), hull) {
        for _ in 0..32 {
            let mid = 0.5 * (lo + hi);
            if point_in_convex(at(mid), hull) {
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
        if !point_in_convex(p, hull) {
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
        if segment_hits_obstacle(a, b, o) {
            return false;
        }
    }
    true
}

fn segment_hits_obstacle(a: Pt, b: Pt, o: &Obstacle) -> bool {
    segment_hits_convex_aabb(a, b, &o.hull, o.aabb)
}

/// Open segment hits the **strict interior** of a convex CCW polygon.
/// Boundary edges (hull walks) must not count as hits — closed `point_in`
/// treated on-edge midpoints as inside and blocked every detour.
fn segment_hits_convex(a: Pt, b: Pt, hull: &[Pt]) -> bool {
    segment_hits_convex_aabb(a, b, hull, hull_aabb(hull))
}

fn segment_hits_convex_aabb(a: Pt, b: Pt, hull: &[Pt], aabb: (f64, f64, f64, f64)) -> bool {
    if hull.len() < 3 {
        return false;
    }
    let (x0, y0, x1, y1) = aabb;
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
    for i in 1..16 {
        let t = i as f64 / 16.0;
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
    src_c: Pt,
    tgt_c: Pt,
    _src: &str,
    _tgt: &str,
    skip: &HashSet<&str>,
    vis: &VisGraph,
    obstacles: &[Obstacle],
    tile_obstacles: &[Obstacle],
    committed: &[Committed],
) -> Vec<Pt> {
    // Nodes: start=0, goal=1, then shared hull verts at 2.. — do **not** clone
    // the shared adj each call; index into `vis.adj` with a +2 shift.
    let start_links = link_terminal_sparse(start, &vis.pts, &vis.owner, obstacles, skip);
    let goal_links = link_terminal_sparse(goal, &vis.pts, &vis.owner, obstacles, skip);
    // Direct tip when ink-clear. Tile whitespace is soft here — forced
    // tile-avoid A* produced orthogonal wraps on otherwise-clear spokes.
    let _ = tile_obstacles;
    let direct = segment_clears_obstacles(start, goal, obstacles, skip).then(|| dist(start, goal));    // Reverse maps: vis-node → link length to start/goal.
    let mut to_start = vec![None; vis.pts.len()];
    let mut to_goal = vec![None; vis.pts.len()];
    for &(j, len) in &start_links {
        to_start[j] = Some(len);
    }
    for &(j, len) in &goal_links {
        to_goal[j] = Some(len);
    }

    let node_pt = |i: usize| -> Pt {
        match i {
            0 => start,
            1 => goal,
            _ => vis.pts[i - 2],
        }
    };
    // A*: edge costs are ≥ Euclidean length, so dist-to-goal is admissible.
    let path = astar(
        &(0usize, 0usize),
        |&(u, prev)| {
            let mut nbrs: Vec<(usize, f64)> = Vec::new();
            match u {
                0 => {
                    for &(j, len) in &start_links {
                        nbrs.push((j + 2, len));
                    }
                    if let Some(len) = direct {
                        nbrs.push((1, len));
                    }
                }
                1 => {
                    for &(j, len) in &goal_links {
                        nbrs.push((j + 2, len));
                    }
                    if let Some(len) = direct {
                        nbrs.push((0, len));
                    }
                }
                _ => {
                    let vi = u - 2;
                    for &(j, len) in &vis.adj[vi] {
                        nbrs.push((j + 2, len));
                    }
                    if let Some(len) = to_start[vi] {
                        nbrs.push((0, len));
                    }
                    if let Some(len) = to_goal[vi] {
                        nbrs.push((1, len));
                    }
                }
            }
            nbrs.into_iter()
                .filter_map(|(v, len)| {
                    if v == prev && u != 0 {
                        return None;
                    }
                    let pu = node_pt(u);
                    let pv = node_pt(v);
                    let pp = if prev == 0 {
                        start
                    } else if prev == 1 {
                        goal
                    } else if prev >= 2 {
                        vis.pts[prev - 2]
                    } else {
                        pu
                    };
                    let bend = if u == 0 || u == prev {
                        0.0
                    } else {
                        angle_penalty(pp, pu, pv) + route().segment_penalty * 0.15
                    };
                    let share = shared_penalty(pu, pv, committed);
                    let cross = if u == 0 && v == 1 {
                        crossing_penalty(src_c, tgt_c, committed)
                    } else if u == 0 {
                        crossing_penalty(src_c, pv, committed)
                    } else if v == 1 {
                        crossing_penalty(pu, tgt_c, committed)
                    } else {
                        crossing_penalty(pu, pv, committed)
                    };
                    let cost = len + bend + share + cross;
                    Some(((v, u), ordered_float(cost)))
                })
                .collect::<Vec<_>>()
        },
        |&(u, _)| ordered_float(dist(node_pt(u), goal)),
        |&(g, _)| g == 1,
    );

    let Some((states, _)) = path else {
        return fallback_bow(start, goal, obstacles, tile_obstacles, skip, committed);
    };
    let mut idxs = Vec::with_capacity(states.len());
    for (node, _) in states {
        if idxs.last().copied() != Some(node) {
            idxs.push(node);
        }
    }
    let mut pts: Vec<Pt> = idxs.into_iter().map(node_pt).collect();
    // Skip redundant controls against ink first, then tiles (keep ink-clear).
    let ink_pulled = drop_skippable_controls(&pts, obstacles, skip, committed, src_c, tgt_c);
    let tile_pulled =
        drop_skippable_controls(&ink_pulled, tile_obstacles, skip, committed, src_c, tgt_c);
    pts = if path_clears_obstacles(&tile_pulled, obstacles, skip) {
        tile_pulled
    } else {
        ink_pulled
    };
    if pts.len() < 2 {
        return fallback_bow(start, goal, obstacles, tile_obstacles, skip, committed);
    }
    pts
}

/// Inflate Euclidean length for scheme-frame highway edges so A* prefers
/// local bows when they exist.
fn frame_edge_cost(len: f64) -> f64 {
    len * 2.5 + BOND_PX * 20.0
}

/// Link a tip to all clear frame (unowned) ports plus the nearest clear hull /
/// box verts — not every visibility node (that was O(V) clears per tip).
fn link_terminal_sparse(
    hub: Pt,
    pts: &[Pt],
    owner: &[Option<String>],
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
) -> Vec<(usize, f64)> {
    let mut out = Vec::new();
    let mut near: Vec<(f64, usize)> = Vec::new();
    for (j, &p) in pts.iter().enumerate() {
        let len = dist(hub, p);
        if len < 1e-6 {
            continue;
        }
        if owner[j].is_none() {
            if segment_clears_obstacles(hub, p, obstacles, skip) {
                out.push((j, frame_edge_cost(len)));
            }
        } else {
            near.push((len, j));
        }
    }
    near.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut linked = 0usize;
    for (len, j) in near {
        if linked >= route().terminal_link_near {
            break;
        }
        // Do not skip the port owner: strict-interior clears allow silhouette
        // landings; skipping would accept through-hull links to the back face.
        if segment_clears_obstacles(hub, pts[j], obstacles, skip) {
            out.push((j, len));
            linked += 1;
        }
    }
    out
}

fn fallback_bow(
    start: Pt,
    goal: Pt,
    hull_obstacles: &[Obstacle],
    place_tiles: &[Obstacle],
    skip: &HashSet<&str>,
    committed: &[Committed],
) -> Vec<Pt> {
    let dx = goal.0 - start.0;
    let dy = goal.1 - start.1;
    let (px, py) = unit((-dy, dx));
    let chord = dist(start, goal).max(1.0);
    let inv = 1.0 / chord;
    // Rank by (wrap, tip-chord tightness) only — not crossings/length. Scoring
    // a field of ports by cost preferred page-scale bows over local skirts.
    let _ = committed;
    let mut best: Option<(f64, f64, Vec<Pt>)> = None; // wrap_pen, tightness, pts
    let consider = |best: &mut Option<(f64, f64, Vec<Pt>)>, pts: Vec<Pt>| {
        if !place_tiles.is_empty() && !path_clears_tile_aabbs(&pts, place_tiles, skip) {
            return;
        }
        let wrap_pen = if detour_backs_up_past_start(start, goal, &pts) {
            1.0e6
        } else {
            0.0
        };
        let mut tight = 0.0_f64;
        for q in pts.iter().skip(1).take(pts.len().saturating_sub(2)) {
            let off = ((q.0 - start.0) * -dy + (q.1 - start.1) * dx).abs() * inv;
            tight = tight.max(off);
        }
        let better = match best {
            None => true,
            Some((bw, bt, _)) => wrap_pen < *bw || (wrap_pen == *bw && tight < *bt),
        };
        if better {
            *best = Some((wrap_pen, tight, pts));
        }
    };

    // 1) Seed elbows at visible ports on simplified chem hulls (+ standoff).
    // Never seed place-AABB corners — those look like box routing. When a tip
    // punches a place box, project that mol's chem-hull verts radially outward
    // until they clear the place AABB + clearance (silhouette-shaped ports).
    let try_ring = |best: &mut Option<(f64, f64, Vec<Pt>)>, ring: &[Pt]| {
        for &bow in ring {
            if !segment_clears_obstacles(start, bow, hull_obstacles, skip)
                || !segment_clears_obstacles(bow, goal, hull_obstacles, skip)
            {
                continue;
            }
            let pts = prune_close_controls(&[start, bow, goal], CLOSE_CTRL_PX);
            if pts.len() < 3 {
                continue;
            }
            consider(best, pts);
        }
    };
    for o in hull_obstacles {
        if skip.contains(o.mol_id.as_str()) {
            continue;
        }
        let ring = if o.tip_hull.len() >= 3 {
            &o.tip_hull
        } else {
            &o.hull
        };
        try_ring(&mut best, ring);
    }
    let p = route().routing_clearance;
    for o in place_tiles {
        if skip.contains(o.mol_id.as_str()) {
            continue;
        }
        if path_clears_tile_aabbs(&[start, goal], std::slice::from_ref(o), skip) {
            continue;
        }
        let chem = if o.tip_hull.len() >= 3 {
            &o.tip_hull
        } else if o.hull.len() >= 3 {
            &o.hull
        } else {
            continue;
        };
        let ports = project_hull_outside_place(chem, o.placement_aabb, p);
        if !ports.is_empty() {
            try_ring(&mut best, &ports);
        }
        // Local place-face skirts (midpoint + short two-elbow runs). Needed when
        // a single face midpoint re-enters the box on the way to the tip.
        let (x0, y0, x1, y1) = o.placement_aabb;
        if x1 - x0 >= 4.0 && y1 - y0 >= 4.0 {
            let cx = 0.5 * (x0 + x1);
            let cy = 0.5 * (y0 + y1);
            try_ring(
                &mut best,
                &[
                    (cx, y0 - p),
                    (cx, y1 + p),
                    (x0 - p, cy),
                    (x1 + p, cy),
                ],
            );
            let try_path = |best: &mut Option<(f64, f64, Vec<Pt>)>, pts: &[Pt]| {
                if pts.len() < 3 {
                    return;
                }
                if detour_backs_up_past_start(start, goal, pts) {
                    return;
                }
                for w in pts.windows(2) {
                    if !segment_clears_obstacles(w[0], w[1], hull_obstacles, skip) {
                        return;
                    }
                }
                let pts = prune_close_controls(pts, CLOSE_CTRL_PX);
                if pts.len() >= 3 {
                    consider(best, pts);
                }
            };
            try_path(
                &mut best,
                &[start, (x0 - p, y0 - p), (x1 + p, y0 - p), goal],
            );
            try_path(
                &mut best,
                &[start, (x0 - p, y1 + p), (x1 + p, y1 + p), goal],
            );
            try_path(
                &mut best,
                &[start, (x0 - p, y0 - p), (x0 - p, y1 + p), goal],
            );
            try_path(
                &mut best,
                &[start, (x1 + p, y0 - p), (x1 + p, y1 + p), goal],
            );
        }
    }
    if let Some((w, _, pts)) = best.as_ref() {
        if *w == 0.0 {
            return pts.clone();
        }
    }
    if let Some((_, _, pts)) = best {
        return pts;
    }

    // Last resort: first mid-chord perpendicular escape with a hard tightness
    // cap — refuse page-scale offsets (Morphine sky frame highway).
    let step = BOND_PX;
    let max_tight = (chord * 0.12).clamp(24.0, 72.0);
    let k_max = (max_tight / step).ceil() as usize;
    let mid = (start.0 + 0.5 * dx, start.1 + 0.5 * dy);
    for sign in [1.0, -1.0] {
        for k in 1..=k_max {
            let off = k as f64 * step * sign;
            if off.abs() > max_tight {
                break;
            }
            let bow = (mid.0 + px * off, mid.1 + py * off);
            if !segment_clears_obstacles(start, bow, hull_obstacles, skip)
                || !segment_clears_obstacles(bow, goal, hull_obstacles, skip)
            {
                continue;
            }
            let pts = prune_close_controls(&[start, bow, goal], CLOSE_CTRL_PX);
            if pts.len() < 3 || detour_backs_up_past_start(start, goal, &pts) {
                continue;
            }
            if !place_tiles.is_empty() && !path_clears_tile_aabbs(&pts, place_tiles, skip) {
                continue;
            }
            return pts;
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
    n * route().shared_path_penalty
}

fn crossing_penalty(a: Pt, b: Pt, committed: &[Committed]) -> f64 {
    let ax0 = a.0.min(b.0);
    let ay0 = a.1.min(b.1);
    let ax1 = a.0.max(b.0);
    let ay1 = a.1.max(b.1);
    let mut n = 0.0;
    for c in committed {
        for &(p, q) in &c.segs {
            let px0 = p.0.min(q.0);
            let py0 = p.1.min(q.1);
            let px1 = p.0.max(q.0);
            let py1 = p.1.max(q.1);
            if ax1 < px0 || ax0 > px1 || ay1 < py0 || ay0 > py1 {
                continue;
            }
            if segments_properly_cross(a, b, p, q) {
                n += 1.0;
            }
        }
    }
    n * route().crossing_penalty
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
    turn * route().angle_penalty
}

/// Lateral corridor offset that keeps endpoints fixed so tip resnap can still
/// aim heads at the final group centroids.
fn offset_polyline_interior(pts: &[Pt], dist: f64) -> Vec<Pt> {
    if pts.len() <= 2 || dist.abs() < 1e-9 {
        return pts.to_vec();
    }
    let mut out = offset_polyline(pts, dist);
    out[0] = pts[0];
    let last = pts.len() - 1;
    out[last] = pts[last];
    out
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
            // Keep endpoints planted — tip resnap aims them at final centroids.
            // Laterally shifting tip chords would leave heads parallel-but-off.
            let mut cand = offset_polyline_interior(&b, route().nudge_px);
            // Flip if offset moved toward a.
            let mid_a = polyline_mid(&a);
            let mid_b = polyline_mid(&cand);
            let mid_b0 = polyline_mid(&b);
            let d0 = dist(mid_a, mid_b0);
            let d1 = dist(mid_a, mid_b);
            if d1 + 0.5 < d0 {
                cand = offset_polyline_interior(&b, -route().nudge_px);
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
        .map(|i| (i as f64 - mid) * route().coterminal_lane_px)
        .collect()
}

fn group_half_width(n: usize) -> f64 {
    if n <= 1 {
        0.0
    } else {
        (n as f64 - 1.0) * 0.5 * route().coterminal_lane_px
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

fn mean_centroid(ids: &[String], centroids: &HashMap<String, Pt>) -> Option<Pt> {
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut n = 0.0;
    for id in ids {
        if let Some(&(x, y)) = centroids.get(id) {
            sx += x;
            sy += y;
            n += 1.0;
        }
    }
    (n >= 1.0).then_some((sx / n, sy / n))
}

/// Source/target mols that face each other across a (possibly multi-endpoint) edge.
///
/// Author lists may order sources/targets arbitrarily. A shaft from the *first*
/// source to the *first* target pierces co-endpoint siblings (A+B→C drew A→C
/// through B). Pick each side's mol closest to the opposite group's centroid so
/// tip_outside_hull knows which inflated hull to clear.
fn edge_facing_src_tgt(edge: &EdgeNode, centroids: &HashMap<String, Pt>) -> (String, String) {
    let sources = edge.sources.as_slice();
    let targets = edge.targets.as_slice();
    if sources.is_empty() || targets.is_empty() {
        return (String::new(), String::new());
    }
    if sources.len() == 1 && targets.len() == 1 {
        return (sources[0].clone(), targets[0].clone());
    }
    let closest = |ids: &[String], to: Pt| -> String {
        ids.iter()
            .min_by(|a, b| {
                let da = centroids
                    .get(a.as_str())
                    .map(|&c| dist(c, to))
                    .unwrap_or(f64::INFINITY);
                let db = centroids
                    .get(b.as_str())
                    .map(|&c| dist(c, to))
                    .unwrap_or(f64::INFINITY);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
            .unwrap_or_default()
    };
    let src_g = mean_centroid(sources, centroids).unwrap_or((0.0, 0.0));
    let tgt_g = mean_centroid(targets, centroids).unwrap_or((0.0, 0.0));
    (closest(sources, tgt_g), closest(targets, src_g))
}

/// Far side of a multi-mol endpoint group along `centroid → toward`.
///
/// Stacked co-endpoints put the group centroid in the `+` gap, so a walk that
/// requires ink occupancy never leaves the corridor. Pose each member from its
/// own chem centroid, then keep the tip with the largest projection onto the
/// group ray (and snap onto that ray so the shaft stays centered).
fn tip_pose_group(
    world: &LayoutWorld,
    mols: &[String],
    centroids: &HashMap<String, Pt>,
    centroid: Pt,
    toward: Pt,
    clash: f64,
    min_shaft: f64,
) -> Pt {
    if mols.len() <= 1 {
        let id = mols.first().map(String::as_str).unwrap_or("");
        let mol_c = centroids.get(id).copied().unwrap_or(centroid);
        return tip_pose(world, id, mol_c, toward, clash, min_shaft);
    }
    let dx = toward.0 - centroid.0;
    let dy = toward.1 - centroid.1;
    let span = dx.hypot(dy).max(min_shaft);
    let l = dx.hypot(dy).max(1e-9);
    let ux = dx / l;
    let uy = dy / l;
    let at = |t: f64| (centroid.0 + ux * t, centroid.1 + uy * t);
    let mut best_t = clash.min(span);
    for id in mols {
        let mol_c = centroids.get(id).copied().unwrap_or(centroid);
        let tip = tip_pose(world, id, mol_c, toward, clash, min_shaft);
        let t = (tip.0 - centroid.0) * ux + (tip.1 - centroid.1) * uy;
        if t > best_t {
            best_t = t;
        }
    }
    at(best_t.clamp(clash.min(span), span.max(clash)))
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
    if fwd <= 2.0 * route().coterminal_px {
        Some(false)
    } else if rev <= 2.0 * route().coterminal_px {
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
        let pulled = string_pull(&windy, &empty, &skip, &[], windy[0], *windy.last().unwrap());
        assert_eq!(
            pulled.len(),
            2,
            "open space must collapse to a straight chord: {pulled:?}"
        );
    }

    #[test]
    fn cosmetic_dp_collapses_inflate_pad_stairstep() {
        // Near-straight 4-point hug (tiny lateral jog) must become a chord
        // when it does not add crossings — the case Catmull was kinking.
        let committed: Vec<Committed> = Vec::new();
        let skip = HashSet::new();
        let stairstep = vec![
            (0.0, 0.0),
            (40.0, 50.0),
            (42.0, 55.0),
            (80.0, 120.0),
        ];
        let pulled = cosmetic_douglas_peucker(
            &stairstep,
            &committed,
            &[],
            &skip,
            route().cosmetic_simplify_eps,
        );
        assert_eq!(pulled.len(), 2, "pad stairstep must collapse: {pulled:?}");
    }

    #[test]
    fn cosmetic_dp_keeps_real_blocker_detour() {
        // Use routing-scale inflate so deflating by ε still leaves a blocker.
        let square = vec![(40.0, 40.0), (60.0, 40.0), (60.0, 60.0), (40.0, 60.0)];
        let obstacles = vec![obstacle_with_hull("blocker".into(), inflate_convex(&square, route().routing_clearance))];
        let pad = pad_tolerant_obstacles(&obstacles, route().cosmetic_simplify_eps);
        let skip = HashSet::new();
        let detour = vec![
            (50.0, 10.0),
            (38.0, 38.0),
            (38.0, 62.0),
            (50.0, 90.0),
        ];
        let pulled = cosmetic_douglas_peucker(
            &detour,
            &[],
            &pad,
            &skip,
            route().cosmetic_simplify_eps,
        );
        assert!(
            pulled.len() >= 3,
            "must not punch the blocker: {pulled:?}"
        );
        // Straight centroid chord is blocked; cosmetic DP must not invent it.
        assert!(
            segment_hits_convex((50.0, 10.0), (50.0, 90.0), &obstacles[0].hull),
            "sanity: chord still hits routing hull"
        );
    }

    #[test]
    fn skip_control_drops_steiner_kinks_without_adding_crossings() {
        // Path already crosses a locked shaft once; Steiner elbows must drop
        // when the neighbor chord does not add crossings (may even clear the X).
        let empty: Vec<Obstacle> = Vec::new();
        let skip = HashSet::new();
        let committed = vec![Committed {
            segs: vec![((40.0, 50.0), (60.0, 50.0))],
        }];
        // Down the left, zigzag, cross mid, zigzag, down the right.
        let kinked = vec![
            (50.0, 0.0),
            (30.0, 20.0),
            (50.0, 40.0),
            (50.0, 60.0),
            (70.0, 80.0),
            (50.0, 100.0),
        ];
        assert_eq!(path_crosses_committed(&kinked, &committed), 1);
        let pulled = drop_skippable_controls(
            &kinked,
            &empty,
            &skip,
            &committed,
            kinked[0],
            *kinked.last().unwrap(),
        );
        assert!(
            path_crosses_committed(&pulled, &committed) <= 1,
            "must not add crossings: {pulled:?}"
        );
        assert!(
            pulled.len() < kinked.len(),
            "Steiner kinks should collapse: {pulled:?}"
        );
        assert!(
            pulled.len() <= 3,
            "expected a near-straight walk: {pulled:?}"
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
        let pulled = string_pull(
            &detour,
            &empty,
            &skip,
            &committed,
            detour[0],
            *detour.last().unwrap(),
        );
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
        // route().crossing_penalty must force the clear side.
        let square = vec![(40.0, 40.0), (60.0, 40.0), (60.0, 60.0), (40.0, 60.0)];
        let obstacles = vec![obstacle_with_hull("blocker".into(), inflate_convex(&square, 2.0))];
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
            (50.0, 10.0),
            (50.0, 90.0),
            "a",
            "b",
            &skip,
            &world,
            &mol_ids,
            2.0,
            &obstacles,
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
        let obstacles = vec![obstacle_with_hull("blocker".into(), inflate_convex(&square, 2.0))];
        let vis = build_visibility_graph(&obstacles);
        let skip = HashSet::new();
        let world = LayoutWorld::new("scheme");
        let mol_ids: Vec<String> = Vec::new();
        let path = route_one_centerline(
            (50.0, 10.0),
            (50.0, 90.0),
            (50.0, 10.0),
            (50.0, 90.0),
            "a",
            "b",
            &skip,
            &world,
            &mol_ids,
            2.0,
            &obstacles,
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
        let obstacles = vec![obstacle_with_hull("blocker".into(), hull.clone())];
        let vis = build_visibility_graph(&obstacles);
        // Boundary walk + inter-obstacle (n/a) → each vert degree ≥ 2.
        assert!(
            vis.adj.iter().all(|n| !n.is_empty()),
            "blocker hull verts must be linked on the boundary"
        );
        let empty = HashSet::new();
        let path = visibility_route(
            (50.0, 10.0),
            (50.0, 90.0),
            (50.0, 10.0),
            (50.0, 90.0),
            "a",
            "b",
            &empty,
            &vis,
            &obstacles,
            &obstacles,
            &[],
        );
        assert!(
            path.len() >= 3,
            "must detour around blocker, got {path:?}"
        );
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
            (20.0, 0.0),
            (20.0, 100.0),
            "a",
            "b",
            &skip,
            &world,
            &mol_ids,
            2.0,
            &obstacles,
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

    #[test]
    fn clear_wrap_without_crossing_gain_collapses_to_tip() {
        // Detour that does not reduce crossings must not stick — tip wins.
        let start = (50.0, 400.0);
        let goal = (200.0, 80.0);
        let wrap = vec![
            start,
            (10.0, 320.0),
            (0.0, 200.0),
            (40.0, 120.0),
            goal,
        ];
        let empty: Vec<Obstacle> = Vec::new();
        let skip = HashSet::new();
        let out = prefer_tip_unless_detour_improves(
            start, goal, start, goal, &wrap, &empty, &empty, &skip, &[],
        );
        assert_eq!(
            out,
            vec![start, goal],
            "non-improving wrap must collapse to tip: {out:?}"
        );
    }

    #[test]
    fn mild_bow_without_crossing_gain_collapses_to_tip() {
        // Morphine bottom-orange failure: ratio ~1.06 bow that does not beat tip.
        let start = (1600.0, 660.0);
        let goal = (1500.0, 790.0);
        let bow = vec![start, (1590.0, 720.0), (1550.0, 760.0), goal];
        let empty: Vec<Obstacle> = Vec::new();
        let skip = HashSet::new();
        let out = prefer_tip_unless_detour_improves(
            start, goal, start, goal, &bow, &empty, &empty, &skip, &[],
        );
        assert_eq!(out, vec![start, goal], "mild non-improving bow → tip: {out:?}");
    }

    #[test]
    fn box_hull_stitch_does_not_cut_through_owner_ink() {
        // Same-owner box→hull links must not chord the silhouette interior.
        let square = vec![(40.0, 40.0), (60.0, 40.0), (60.0, 60.0), (40.0, 60.0)];
        let hull = inflate_convex(&square, 4.0);
        let obstacles = vec![obstacle_with_hull("mol".into(), hull)];
        let vis = build_visibility_graph(&obstacles);
        let owned: Vec<usize> = vis
            .owner
            .iter()
            .enumerate()
            .filter_map(|(i, o)| o.as_deref().filter(|&id| id == "mol").map(|_| i))
            .collect();
        for &i in &owned {
            for &(j, _) in &vis.adj[i] {
                if !owned.contains(&j) {
                    continue;
                }
                assert!(
                    !segment_hits_convex(vis.pts[i], vis.pts[j], &obstacles[0].hull),
                    "same-owner visibility edge cuts ink: {:?}→{:?}",
                    vis.pts[i],
                    vis.pts[j]
                );
            }
        }
    }

    #[test]
    fn skip_control_drops_redundant_first_elbow() {
        // Elbow only needed if we refuse tile-skimming; ink chord start→p2 is clear.
        let start = (0.0, 0.0);
        let elbow = (20.0, 15.0);
        let mid = (60.0, 40.0);
        let goal = (100.0, 0.0);
        let path = vec![start, elbow, mid, goal];
        let empty: Vec<Obstacle> = Vec::new();
        let skip = HashSet::new();
        let pulled = drop_skippable_controls(&path, &empty, &skip, &[], start, goal);
        assert!(
            pulled.len() <= 3,
            "redundant first elbow must collapse: {pulled:?}"
        );
        assert!(
            pulled.iter().all(|p| *p != elbow),
            "elbow must be gone: {pulled:?}"
        );
    }

    #[test]
    fn raising_crossing_penalty_switches_tip_to_bottom_two_bend() {
        // Tip chord crosses one locked peer; a 2-bend below clears. Cost is
        // length + crossings×penalty — soft penalty keeps the tip, hard
        // penalty buys the bottom bow.
        let start = (0.0, 0.0);
        let goal = (100.0, 0.0);
        let bottom = vec![start, (0.0, 40.0), (100.0, 40.0), goal]; // len 180
        let committed = vec![Committed {
            segs: vec![((50.0, -20.0), (50.0, 20.0))], // crosses tip once
        }];
        let empty: Vec<Obstacle> = Vec::new();
        let skip = HashSet::new();
        assert_eq!(
            pathway_crosses_committed(&[start, goal], start, goal, &committed),
            1
        );
        assert_eq!(
            pathway_crosses_committed(&bottom, start, goal, &committed),
            0
        );
        // tip_cost = 100 + P; bottom_cost = 180. Switch when P > 80.
        let low = with_crossing_penalty(40.0, || {
            prefer_tip_unless_detour_improves(
                start, goal, start, goal, &bottom, &empty, &empty, &skip, &committed,
            )
        });
        assert_eq!(low, vec![start, goal], "soft penalty keeps tip: {low:?}");
        let high = with_crossing_penalty(200.0, || {
            prefer_tip_unless_detour_improves(
                start, goal, start, goal, &bottom, &empty, &empty, &skip, &committed,
            )
        });
        assert_eq!(
            high, bottom,
            "hard penalty buys the bottom 2-bend: {high:?}"
        );
    }

    #[test]
    fn ink_hitting_wrap_collapses_to_tip_even_if_shorter_crossings() {
        // Both tip and wrap punch ink → tip (do not keep a wild dashed wrap).
        let start = (0.0, 0.0);
        let goal = (200.0, 0.0);
        let wrap = vec![start, (100.0, 40.0), (100.0, -40.0), goal];
        let hull = vec![(80.0, -20.0), (120.0, -20.0), (120.0, 20.0), (80.0, 20.0)];
        let ink = vec![obstacle_with_hull("mol".into(), hull.clone())];
        let tiles = vec![obstacle_with_hull("mol".into(), inflate_convex(&hull, 8.0))];
        let skip = HashSet::new();
        assert!(!path_clears_obstacles(&wrap, &ink, &skip));
        assert!(!path_clears_obstacles(&[start, goal], &ink, &skip));
        let out = prefer_tip_unless_detour_improves(
            start, goal, start, goal, &wrap, &ink, &tiles, &skip, &[],
        );
        assert_eq!(out, vec![start, goal], "dirty wrap must not stick: {out:?}");
    }

    #[test]
    fn morphine_fan_chord_hits_padded_tile() {
        // Morphine→M3S tip chord skims Morphine N-oxide viewport top.
        let a = (1000.44, 211.73);
        let b = (1267.95, 463.13);
        let pad = route().routing_clearance;
        let padded = convex_hull_pts(&[
            (1064.0 - pad, 401.53 - pad),
            (1266.56 + pad, 401.53 - pad),
            (1266.56 + pad, 644.75 + pad),
            (1064.0 - pad, 644.75 + pad),
        ]);
        assert!(
            segment_hits_convex(a, b, &padded),
            "padded N-oxide tile must block the Morphine→M3S chord"
        );
        let obstacles = vec![obstacle_with_hull("xbm:0004960".into(), padded)];
        let skip: HashSet<&str> = ["xbm:0004961", "xbm:0007817"].into_iter().collect();
        assert!(
            !segment_clears_obstacles(a, b, &obstacles, &skip),
            "tip chord must not clear foreign N-oxide tile"
        );
    }

    #[test]
    fn morphine_live_tip_through_4955_is_blocked_by_aabb_tile() {
        // Captured from refine output: Morphine→right child tip through 4955.
        let a = (839.9866590982359, 165.08809379790006);
        let b = (1242.552695276199, 385.40565241063285);
        let tile = convex_hull_pts(&[
            (987.5, 334.5),
            (987.5 + 208.5, 334.5),
            (987.5 + 208.5, 334.5 + 214.7),
            (987.5, 334.5 + 214.7),
        ]);
        assert!(
            segment_hits_convex(a, b, &tile),
            "live Morphine tip must hit 4955 AABB"
        );
        let obstacles = vec![obstacle_with_hull("xbm:0004955".into(), tile)];
        let skip: HashSet<&str> = ["xbm:0004961", "xbm:0004960"].into_iter().collect();
        assert!(
            !segment_clears_obstacles(a, b, &obstacles, &skip),
            "tip must not clear 4955 tile obstacle"
        );
        let bow = fallback_bow(a, b, &obstacles, &obstacles, &skip, &[]);
        assert!(
            bow.len() >= 3,
            "fallback_bow must skirt 4955, got {bow:?}"
        );
        assert!(
            path_clears_obstacles(&bow, &obstacles, &skip),
            "bow must clear 4955: {bow:?}"
        );
        assert!(
            !detour_backs_up_past_start(a, b, &bow),
            "side bow must not count as backs-up: {bow:?}"
        );
    }

    /// Irregular chem silhouette inside a place box (not a rectangle).
    fn chem_hex_in_place(place: (f64, f64, f64, f64)) -> Vec<Pt> {
        let (x0, y0, x1, y1) = place;
        let cx = 0.5 * (x0 + x1);
        let cy = 0.5 * (y0 + y1);
        let rx = (x1 - x0) * 0.28;
        let ry = (y1 - y0) * 0.32;
        simplified_standoff_hull(
            &[
                (cx, cy - ry),
                (cx + rx * 0.9, cy - ry * 0.4),
                (cx + rx * 0.55, cy + ry * 0.85),
                (cx - rx * 0.2, cy + ry),
                (cx - rx, cy + ry * 0.15),
                (cx - rx * 0.65, cy - ry * 0.55),
            ],
            4.0,
            8,
        )
    }

    fn place_pad_ring(place: (f64, f64, f64, f64), pad: f64) -> Vec<Pt> {
        let (x0, y0, x1, y1) = place;
        convex_hull_pts(&[
            (x0 - pad, y0 - pad),
            (x1 + pad, y0 - pad),
            (x1 + pad, y1 + pad),
            (x0 - pad, y1 + pad),
        ])
    }

    fn elbow_near_aabb_corner(ctrl: Pt, place: (f64, f64, f64, f64), pad: f64) -> bool {
        let (x0, y0, x1, y1) = place;
        let corners = [
            (x0 - pad, y0 - pad),
            (x1 + pad, y0 - pad),
            (x1 + pad, y1 + pad),
            (x0 - pad, y1 + pad),
        ];
        corners
            .iter()
            .any(|&c| (ctrl.0 - c.0).hypot(ctrl.1 - c.1) < 6.0)
    }

    #[test]
    fn morphine_sky_tip_through_8306_prefers_bow() {
        let a = (844.06, 139.03);
        let b = (1782.20, 400.47);
        let place = (1548.55, 317.65, 1548.55 + 218.6, 317.65 + 229.2);
        let chem = chem_hex_in_place(place);
        let pad = place_pad_ring(place, 8.0);
        let ink = obstacle_with_hulls("xbm:0008306".into(), chem.clone(), chem.clone());
        let tile = obstacle_tile("xbm:0008306".into(), pad, chem, place);
        let skip: HashSet<&str> = ["xbm:0004961", "xbm:0006520"].into_iter().collect();
        assert!(!path_clears_tile_aabbs(&[a, b], &[tile.clone()], &skip));
        // Production: ink = chem hull, place = AABB gate (not AABB ports).
        let bow = fallback_bow(a, b, &[ink], &[tile.clone()], &skip, &[]);
        assert!(bow.len() >= 3, "bow: {bow:?}");
        assert!(
            !detour_backs_up_past_start(a, b, &bow),
            "visible-port bow must not hub-wrap: {bow:?}"
        );
        let ctrl = bow[1];
        assert!(
            ctrl.0 > 1400.0 && ctrl.1 < 600.0,
            "bow elbow should stay by 8306 (not a page wrap), got {ctrl:?}"
        );
        assert!(
            !elbow_near_aabb_corner(ctrl, place, 8.0),
            "elbow must not sit on place-AABB corner: {ctrl:?}"
        );
        assert!(
            path_clears_tile_aabbs(&bow, &[tile.clone()], &skip),
            "bow must clear place: {bow:?}"
        );
        let out = prefer_tip_unless_detour_improves(
            a, b, a, b, &bow, &[], &[tile], &skip, &[],
        );
        assert!(out.len() >= 3, "prefer_tip keep bow: {out:?}");
    }

    #[test]
    fn morphine_pink_tip_through_4955_prefers_bow() {
        let a = (839.99, 165.09);
        let b = (1218.87, 372.44);
        let place = (987.5, 334.5, 987.5 + 208.5, 334.5 + 214.7);
        let chem = chem_hex_in_place(place);
        let pad = place_pad_ring(place, 8.0);
        let ink = obstacle_with_hulls("xbm:0004955".into(), chem.clone(), chem.clone());
        let tile = obstacle_tile("xbm:0004955".into(), pad, chem, place);
        let skip: HashSet<&str> = ["xbm:0004961", "xbm:0004960"].into_iter().collect();
        assert!(
            !path_clears_tile_aabbs(&[a, b], &[tile.clone()], &skip),
            "placement AABB must flag pink tip"
        );
        let bow = fallback_bow(a, b, &[ink], &[tile.clone()], &skip, &[]);
        assert!(bow.len() >= 3, "bow: {bow:?}");
        assert!(
            path_clears_tile_aabbs(&bow, &[tile.clone()], &skip),
            "bow must clear place: {bow:?}"
        );
        assert!(
            !detour_backs_up_past_start(a, b, &bow),
            "side bow must not count as hub wrap: {bow:?}"
        );
        assert!(
            !elbow_near_aabb_corner(bow[1], place, 8.0),
            "elbow must not sit on place-AABB corner: {:?}",
            bow[1]
        );
        let out = prefer_tip_unless_detour_improves(
            a, b, a, b, &bow, &[], &[tile], &skip, &[],
        );
        assert!(
            out.len() >= 3,
            "prefer_tip must keep bow when tip cuts place: {out:?}"
        );
    }

    #[test]
    fn aabb_sampler_flags_1684_tip() {
        // Dashed i0→5110 tip cuts norcodeine (1684) placement box.
        let a = (723.66, 807.84);
        let b = (447.09, 1007.08);
        let place = (439.83, 641.43, 439.83 + 210.62, 641.43 + 239.78);
        let chem = chem_hex_in_place(place);
        let pad = place_pad_ring(place, 8.0);
        let ink = obstacle_with_hulls("xbm:0001684".into(), chem.clone(), chem.clone());
        let tile = obstacle_tile("xbm:0001684".into(), pad, chem, place);
        let skip: HashSet<&str> = ["i0", "xbm:0005110"].into_iter().collect();
        assert!(
            !path_clears_tile_aabbs(&[a, b], &[tile.clone()], &skip),
            "1684 tip must be flagged by AABB sampler"
        );
        let bow = fallback_bow(a, b, &[ink], &[tile.clone()], &skip, &[]);
        assert!(bow.len() >= 3, "visible-port bow: {bow:?}");
        assert!(
            !detour_backs_up_past_start(a, b, &bow),
            "1684 bow must not hub-wrap: {bow:?}"
        );
        assert!(
            !elbow_near_aabb_corner(bow[1], place, 8.0),
            "elbow must not sit on place-AABB corner: {:?}",
            bow[1]
        );
        let out = prefer_tip_unless_detour_improves(
            a, b, a, b, &bow, &[], &[tile], &skip, &[],
        );
        assert!(out.len() >= 3, "prefer_tip keep bow: {out:?}");
    }

    #[test]
    fn morphine_sky_tip_should_be_nearly_straight() {
        use crate::doc::depict;
        use crate::doc::spec::DepictSpec;
        use crate::scene::Primitive;
        const MORPHINE: &str =
            include_str!("../../tests/fixtures/xenobase/morphine-sulfate.json");
        let spec: DepictSpec = serde_json::from_str(MORPHINE).unwrap();
        let scene = depict(&spec).unwrap().remove(0).scene;
        let mut sky: Option<Vec<(f64, f64)>> = None;
        for p in &scene.overlays {
            let Primitive::Path {
                d,
                stroke,
                class,
                ..
            } = p
            else {
                continue;
            };
            if stroke.as_deref() != Some("#56b4e9") {
                continue;
            }
            let c = class.as_deref().unwrap_or("");
            if !c.contains("edge") || c.contains("head") {
                continue;
            }
            // parse M/L numbers
            let mut nums = Vec::new();
            let mut cur = String::new();
            for ch in d.chars() {
                if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E' {
                    cur.push(ch);
                } else if !cur.is_empty() {
                    if let Ok(v) = cur.parse::<f64>() {
                        nums.push(v);
                    }
                    cur.clear();
                }
            }
            if !cur.is_empty() {
                if let Ok(v) = cur.parse::<f64>() {
                    nums.push(v);
                }
            }
            let pts: Vec<(f64, f64)> = nums.chunks(2).filter(|c| c.len()==2).map(|c| (c[0], c[1])).collect();
            sky = Some(pts);
        }
        let pts = sky.expect("sky shaft");
        let a = pts[0];
        let b = *pts.last().unwrap();
        let chord = ((b.0 - a.0).hypot(b.1 - a.1)).max(1.0);
        let mut len = 0.0;
        for w in pts.windows(2) {
            len += (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
        }
        let dx = b.0 - a.0;
        let dy = b.1 - a.1;
        let inv = 1.0 / chord;
        let px = -dy * inv;
        let py = dx * inv;
        let mut max_off = 0.0_f64;
        for q in &pts {
            max_off = max_off.max(((q.0 - a.0) * px + (q.1 - a.1) * py).abs());
        }
        eprintln!(
            "sky n={} detour={:.3} max_off={:.1} pts={:?}",
            pts.len(),
            len / chord,
            max_off,
            pts
        );
        assert!(
            max_off < 80.0 && len / chord < 1.08,
            "sky bow too large: n={} max_off={max_off:.1} detour={:.3} (no frame-highway minimize)",
            pts.len(),
            len / chord
        );
    }

}
