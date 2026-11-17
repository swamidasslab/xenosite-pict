//! Lean hull-vis scheme router (replaces the old libavoid-style path).
//!
//! Shared hull visibility graph when needed; per arrow **incrementally** drop
//! endpoint owners' verts, append start/goal centroids, link centroids only to
//! foreign verts, then A*. Incremental query graphs must match from-scratch
//! builds (see tests). Clear tips with zero peer cost skip the graph entirely.
//! Two passes (peer-free + one Gauss–Seidel); tip-hug after lock.
//!
//! Steiner–Steiner edges clear **all** hulls (endpoints are not transparent for
//! those chords). Wybrow-style invisibility / promote-on-remove would change
//! that semantics — useful later for interactive obstacle edits, not for the
//! current skip model. We do take the paper's broad-phase idea: an obstacle
//! AABB grid speeds pair tests during shared-graph construction.

use std::collections::{HashMap, HashSet};

#[cfg(test)]
use std::collections::BTreeSet;

use pathfinding::directed::astar::astar;

use crate::arrows::{
    min_shaft_px, offset_polyline, reverse_source_inset, shorten_polyline_start,
};
use crate::engine_config::{EngineConfig, RouteConfig};
use crate::metrics::BOND_PX;
use crate::scene::Primitive;

use super::layout_constraints::{convex_hull_pts, edge_signed_distance, LayoutWorld, Shape};
use super::scheme_layout::SCHEME_INK_STANDOFF;
use super::{DepictSpec, DocPaint, EdgeNode};

type Pt = (f64, f64);

thread_local! {
    static ACTIVE_ROUTE: std::cell::RefCell<Option<RouteConfig>> =
        const { std::cell::RefCell::new(None) };
}

struct ActiveRouteGuard;
impl Drop for ActiveRouteGuard {
    fn drop(&mut self) {
        ACTIVE_ROUTE.with(|c| *c.borrow_mut() = None);
    }
}

fn route() -> RouteConfig {
    ACTIVE_ROUTE.with(|c| c.borrow().clone().unwrap_or_else(RouteConfig::pretty))
}

struct ArrowGroup {
    members: Vec<usize>,
    reversed: Vec<bool>,
}

/// Inflated full-ink hull: collision tests + Steiner ring (= `hull`).
#[derive(Clone)]
struct Obstacle {
    mol_id: String,
    hull: Vec<Pt>,
    aabb: (f64, f64, f64, f64),
}

/// Shared hull visibility graph (all mols). Per-arrow graphs strip endpoints.
struct VisGraph {
    pts: Vec<Pt>,
    /// Obstacle index for each vert (`obstacles[owner_idx[v]]`).
    owner_idx: Vec<usize>,
    adj: Vec<Vec<(usize, f64)>>,
}

/// Per-arrow search graph: foreign Steiners + start/goal centroids.
struct QueryGraph {
    pts: Vec<Pt>,
    adj: Vec<Vec<(usize, f64)>>,
    start_i: usize,
    goal_i: usize,
}

/// Uniform grid over obstacle AABBs — broad-phase for segment clears.
struct ObstacleGrid {
    cell: f64,
    /// Cell → obstacle indices whose AABB overlaps the cell.
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl ObstacleGrid {
    fn build(obstacles: &[Obstacle]) -> Self {
        let mut cell = 64.0_f64;
        for o in obstacles {
            let (x0, y0, x1, y1) = o.aabb;
            cell = cell.max(((x1 - x0).max(y1 - y0)) * 0.5).max(16.0);
        }
        let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, o) in obstacles.iter().enumerate() {
            let (x0, y0, x1, y1) = o.aabb;
            let ix0 = (x0 / cell).floor() as i32;
            let iy0 = (y0 / cell).floor() as i32;
            let ix1 = (x1 / cell).floor() as i32;
            let iy1 = (y1 / cell).floor() as i32;
            for ix in ix0..=ix1 {
                for iy in iy0..=iy1 {
                    cells.entry((ix, iy)).or_default().push(i);
                }
            }
        }
        Self { cell, cells }
    }

    /// Obstacles whose AABB may meet segment AABB (deduped).
    fn candidates(&self, a: Pt, b: Pt, buf: &mut Vec<usize>, seen: &mut Vec<u32>, stamp: u32) {
        buf.clear();
        let sx0 = a.0.min(b.0);
        let sy0 = a.1.min(b.1);
        let sx1 = a.0.max(b.0);
        let sy1 = a.1.max(b.1);
        let ix0 = (sx0 / self.cell).floor() as i32;
        let iy0 = (sy0 / self.cell).floor() as i32;
        let ix1 = (sx1 / self.cell).floor() as i32;
        let iy1 = (sy1 / self.cell).floor() as i32;
        for ix in ix0..=ix1 {
            for iy in iy0..=iy1 {
                let Some(list) = self.cells.get(&(ix, iy)) else {
                    continue;
                };
                for &oi in list {
                    if seen[oi] == stamp {
                        continue;
                    }
                    seen[oi] = stamp;
                    buf.push(oi);
                }
            }
        }
    }
}

pub struct RouteResult {
    pub paths: Vec<Option<Vec<Pt>>>,
    /// Diagnostic primitives when `debug` is on (hulls, chords, tips, paths).
    pub debug: Vec<Primitive>,
}

/// Colorblind-safe debug palette (Wong).
mod dbg {
    pub const HULL: &str = "#0072B2";
    pub const PATHWAY: &str = "#56B4E9";
    pub const CHORD: &str = "#CC79A7";
    pub const TIP: &str = "#E69F00";
    pub const CENTER: &str = "#009E73";
    pub const DIRTY: &str = "#D55E00";
    pub const DUMMY: &str = "#F0E442";
    /// Proper segment intersections the peer-cost detector reports.
    pub const CROSS: &str = "#D55E00";
    pub const CROSS_CORE: &str = "#000000";
}

pub fn route_scheme_edges(
    spec: &DepictSpec,
    positions: &HashMap<String, Pt>,
    paints: &[DocPaint],
    sizes: &[(String, f64, f64)],
    world: &LayoutWorld,
    mol_ids: &[String],
    engine: &EngineConfig,
    route_waypoints: &HashMap<(String, String), Vec<Pt>>,
    debug: bool,
) -> RouteResult {
    ACTIVE_ROUTE.with(|c| *c.borrow_mut() = Some(engine.route.clone()));
    let _route_guard = ActiveRouteGuard;
    let n_edges = spec.edges().len();
    let mut out = vec![None; n_edges];
    let mut debug_prims = Vec::new();
    if n_edges == 0 {
        return RouteResult {
            paths: out,
            debug: debug_prims,
        };
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

    let obstacles = build_obstacles(mol_ids, world, positions, &size_of, route().routing_clearance);
    // VisGraph / QueryGraphs are built lazily — Morphine is mostly tip chords, and
    // a clear tip with zero peer penalty is optimal (straight line + non-neg peers).
    let mut grid: Option<ObstacleGrid> = None;
    let mut vis: Option<VisGraph> = None;
    let mut query_graphs: Vec<Option<QueryGraph>> = Vec::new();
    if debug {
        for o in &obstacles {
            debug_prims.push(poly_closed(&o.hull, dbg::HULL, 2.5, 0.55, "debug-route-hull"));
            for &p in &o.hull {
                debug_prims.push(dot(p, 3.5, dbg::HULL, 0.65, "debug-route-hull-vert"));
            }
        }
        for id in mol_ids {
            if let Some(&c) = centroids.get(id) {
                debug_prims.push(dot(c, 4.5, dbg::CHORD, 0.85, "debug-route-centroid"));
            }
        }
    }

    let mut seeds: Vec<Vec<Pt>> = Vec::with_capacity(n_edges);
    for edge in spec.edges() {
        let (src, tgt) = edge_facing_src_tgt(edge, &centroids);
        let a = mean_centroid(edge.sources.as_slice(), &centroids)
            .unwrap_or_else(|| centroids.get(&src).copied().unwrap_or((0.0, 0.0)));
        let b = mean_centroid(edge.targets.as_slice(), &centroids)
            .unwrap_or_else(|| centroids.get(&tgt).copied().unwrap_or((0.0, 0.0)));
        seeds.push(vec![a, b]);
    }
    let edge_refs = spec.edges();
    let groups = coterminal_groups(&edge_refs, &seeds);

    let n_groups = groups.len();
    let mut group_centers: Vec<Vec<Pt>> = vec![Vec::new(); n_groups];
    let mut group_pathways: Vec<Vec<Pt>> = vec![Vec::new(); n_groups];
    let mut skips: Vec<HashSet<String>> = Vec::with_capacity(n_groups);
    let mut group_meta: Vec<(Pt, Pt, f64, f64, Vec<String>, Vec<String>)> =
        Vec::with_capacity(n_groups);

    for group in &groups {
        let rep = group.members[0];
        let edge = &spec.edges()[rep];
        let (src, tgt) = edge_facing_src_tgt(edge, &centroids);
        let src_c = mean_centroid(edge.sources.as_slice(), &centroids)
            .unwrap_or_else(|| centroids.get(&src).copied().unwrap_or((0.0, 0.0)));
        let tgt_c = mean_centroid(edge.targets.as_slice(), &centroids)
            .unwrap_or_else(|| centroids.get(&tgt).copied().unwrap_or((0.0, 0.0)));
        let min_shaft = min_shaft_px(edge.stroke_width);
        let clash = shaft_chem_clash(edge.stroke_width, group.members.len());
        let mut skip_owned: HashSet<String> = HashSet::new();
        for id in edge.sources.as_slice().iter().chain(edge.targets.as_slice()) {
            skip_owned.insert(id.clone());
        }
        group_meta.push((
            src_c,
            tgt_c,
            clash,
            min_shaft,
            edge.sources.as_slice().to_vec(),
            edge.targets.as_slice().to_vec(),
        ));
        skips.push(skip_owned);
    }
    query_graphs.resize_with(n_groups, || None);

    // Pass 0: peer-free tips/detours. Pass 1: one Gauss–Seidel against live
    // peers (tips-first by excess-over-tip). Extra passes rarely change Morphine.
    const ROUTE_PASSES: usize = 2;
    for pass in 0..ROUTE_PASSES {
        let mut order: Vec<usize> = (0..n_groups).collect();
        if pass > 0 {
            order.sort_by(|&a, &b| {
                let sa =
                    pathway_improve_score(&group_pathways[a], group_meta[a].0, group_meta[a].1);
                let sb =
                    pathway_improve_score(&group_pathways[b], group_meta[b].0, group_meta[b].1);
                sa.partial_cmp(&sb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.cmp(&b))
            });
        }
        for &gi in &order {
            let skip: HashSet<&str> = skips[gi].iter().map(String::as_str).collect();
            let (src_c, tgt_c, _, _, _, _) = &group_meta[gi];
            let committed: Vec<Vec<Pt>> = if pass == 0 {
                Vec::new()
            } else {
                group_pathways
                    .iter()
                    .enumerate()
                    .filter(|(j, c)| *j != gi && c.len() >= 2)
                    .map(|(_, c)| c.clone())
                    .collect()
            };
            // Clear tip with no peer penalty ⇒ optimal; skip VisGraph/A*.
            if points_visible(*src_c, *tgt_c, &obstacles, &skip) {
                let tip_peer =
                    segment_peer_cost(*src_c, *tgt_c, &committed, &obstacles, &skip);
                if tip_peer <= 1e-12 {
                    group_pathways[gi] = vec![*src_c, *tgt_c];
                    continue;
                }
            }
            if grid.is_none() {
                grid = Some(ObstacleGrid::build(&obstacles));
            }
            if vis.is_none() {
                vis = Some(build_visibility_graph(&obstacles, grid.as_ref().unwrap()));
            }
            if query_graphs[gi].is_none() {
                query_graphs[gi] = Some(build_query_graph_incremental(
                    vis.as_ref().unwrap(),
                    &obstacles,
                    grid.as_ref().unwrap(),
                    &skip,
                    *src_c,
                    *tgt_c,
                ));
            }
            group_pathways[gi] = visibility_route_cached(
                query_graphs[gi].as_ref().unwrap(),
                &obstacles,
                &skip,
                &committed,
            );
        }
        if debug {
            let n_tip = group_pathways.iter().filter(|p| p.len() == 2).count();
            eprintln!("route debug pass={pass}: {n_tip}/{} tip chords", group_pathways.len());
        }
    }

    // Pose tips only after every pathway is final.
    for (gi, _) in groups.iter().enumerate() {
        let (src_c, tgt_c, clash, min_shaft, src_ids, tgt_ids) = &group_meta[gi];
        group_centers[gi] = tip_hug_pathway(
            &group_pathways[gi],
            world,
            src_ids,
            tgt_ids,
            &centroids,
            *src_c,
            *tgt_c,
            *clash,
            *min_shaft,
        );
    }

    if debug {
        for (gi, group) in groups.iter().enumerate() {
            let (src_c, tgt_c, _, _, _, _) = group_meta[gi];
            let (src, tgt) = edge_facing_src_tgt(&spec.edges()[group.members[0]], &centroids);
            let pathway = &group_pathways[gi];
            let center = &group_centers[gi];
            debug_prims.push(poly_open(
                &[src_c, tgt_c],
                dbg::CHORD,
                2.5,
                0.55,
                Some("5 3"),
                "debug-route-chord",
            ));
            debug_prims.push(poly_open(
                pathway,
                dbg::PATHWAY,
                3.0,
                0.7,
                Some("4 3"),
                "debug-route-pathway",
            ));
            if let (Some(&a), Some(&b)) = (center.first(), center.last()) {
                debug_prims.push(dot(a, 5.5, dbg::TIP, 0.95, "debug-route-tip"));
                debug_prims.push(dot(b, 5.5, dbg::TIP, 0.95, "debug-route-tip"));
            }
            for &p in &lookup_waypoints(route_waypoints, &src, &tgt) {
                debug_prims.push(dot(p, 6.0, dbg::DUMMY, 0.95, "debug-route-dummy"));
            }
            let tip_ok = pathway.len() == 2;
            let color = if tip_ok { dbg::CENTER } else { dbg::DIRTY };
            let cls = if tip_ok {
                "debug-route-center tip"
            } else {
                "debug-route-center detour"
            };
            debug_prims.push(poly_open(center, color, 3.5, 0.85, None, cls));
        }
        // Intersections on centroid pathways (ignore hits inside either edge's
        // source/target hulls — same rule as peer-cost). Shared vis-graph
        // vertices count too (proper-cross misses endpoint meets).
        let mut hits = Vec::new();
        for i in 0..n_groups {
            if group_pathways[i].len() < 2 {
                continue;
            }
            for j in (i + 1)..n_groups {
                if group_pathways[j].len() < 2 {
                    continue;
                }
                let skip_ij: HashSet<&str> = skips[i]
                    .iter()
                    .chain(skips[j].iter())
                    .map(String::as_str)
                    .collect();
                for w in group_pathways[i].windows(2) {
                    for v in group_pathways[j].windows(2) {
                        if let Some(p) = segment_intersection_point(w[0], w[1], v[0], v[1]) {
                            if !point_in_skip_hull(p, &obstacles, &skip_ij) {
                                hits.push(p);
                            }
                        }
                    }
                }
                for &p in &group_pathways[i] {
                    if point_in_skip_hull(p, &obstacles, &skip_ij) {
                        continue;
                    }
                    if point_is_pathway_vertex(p, &group_pathways[j]) {
                        hits.push(p);
                    }
                }
            }
        }
        hits.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        });
        hits.dedup_by(|a, b| nearly_same_pt(*a, *b));
        for &p in &hits {
            push_cross_mark(&mut debug_prims, p);
        }
        let n_tip = group_pathways.iter().filter(|p| p.len() == 2).count();
        let n_detour = group_pathways.len().saturating_sub(n_tip);
        eprintln!(
            "route debug: {n_tip} tip chords, {n_detour} hull detours, {} pathway intersections (incl. shared verts)",
            hits.len()
        );
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
        let _ = &skips[gi];
    }

    RouteResult {
        paths: out,
        debug: debug_prims,
    }
}

fn lookup_waypoints(
    map: &HashMap<(String, String), Vec<Pt>>,
    src: &str,
    tgt: &str,
) -> Vec<Pt> {
    if let Some(w) = map.get(&(src.to_string(), tgt.to_string())) {
        return w.clone();
    }
    if let Some(w) = map.get(&(tgt.to_string(), src.to_string())) {
        let mut r = w.clone();
        r.reverse();
        return r;
    }
    Vec::new()
}

/// Build query graph then A* (tests / one-shot). Prefer [`visibility_route_cached`].
#[cfg(test)]
fn visibility_route(
    start: Pt,
    goal: Pt,
    vis: &VisGraph,
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
    committed: &[Vec<Pt>],
) -> Vec<Pt> {
    let grid = ObstacleGrid::build(obstacles);
    let g = build_query_graph_incremental(vis, obstacles, &grid, skip, start, goal);
    visibility_route_cached(&g, obstacles, skip, committed)
}

/// A* on a prebuilt query graph. Tip short-circuits when there are no peers.
fn visibility_route_cached(
    g: &QueryGraph,
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
    committed: &[Vec<Pt>],
) -> Vec<Pt> {
    let start = g.pts[g.start_i];
    let goal = g.pts[g.goal_i];
    // No peer costs ⇒ any clear centroid tip dominates every detour on length.
    if committed.is_empty() && points_visible(start, goal, obstacles, skip) {
        return vec![start, goal];
    }
    astar_on_query(g, obstacles, skip, committed)
}

fn astar_on_query(
    g: &QueryGraph,
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
    committed: &[Vec<Pt>],
) -> Vec<Pt> {
    let start = g.pts[g.start_i];
    let goal = g.pts[g.goal_i];
    let path = astar(
        &g.start_i,
        |&u| {
            g.adj[u]
                .iter()
                .map(|&(v, len)| {
                    let cost =
                        len + segment_peer_cost(g.pts[u], g.pts[v], committed, obstacles, skip);
                    (v, ordered_float(cost))
                })
                .collect::<Vec<_>>()
        },
        |&u| ordered_float(dist(g.pts[u], goal)),
        |&u| u == g.goal_i,
    );
    let astar_pts: Vec<Pt> = match path {
        Some((idxs, _)) => idxs.into_iter().map(|i| g.pts[i]).collect(),
        None => vec![start, goal],
    };
    if points_visible(start, goal, obstacles, skip) {
        let tip = vec![start, goal];
        let tip_cost = pathway_cost(&tip, committed, obstacles, skip);
        let astar_cost = pathway_cost(&astar_pts, committed, obstacles, skip);
        if tip_cost <= astar_cost {
            return tip;
        }
    }
    astar_pts
}

fn add_undirected(
    i: usize,
    j: usize,
    len: f64,
    linked: &mut [HashSet<usize>],
    adj: &mut [Vec<(usize, f64)>],
) {
    if i == j || len < 1e-6 || !linked[i].insert(j) {
        return;
    }
    linked[j].insert(i);
    adj[i].push((j, len));
    adj[j].push((i, len));
}

/// Shared graph over **all** hull verts (rings + full inter-hull visibility).
fn build_visibility_graph(obstacles: &[Obstacle], grid: &ObstacleGrid) -> VisGraph {
    let mut pts = Vec::new();
    let mut owner_idx: Vec<usize> = Vec::new();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (oi, o) in obstacles.iter().enumerate() {
        let lo = pts.len();
        for &p in &o.hull {
            pts.push(p);
            owner_idx.push(oi);
        }
        ranges.push((lo, pts.len()));
    }
    let n = pts.len();
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    let mut linked = vec![HashSet::<usize>::new(); n];

    for &(lo, hi) in &ranges {
        let m = hi - lo;
        if m < 2 {
            continue;
        }
        for k in 0..m {
            let i = lo + k;
            let j = lo + (k + 1) % m;
            add_undirected(i, j, dist(pts[i], pts[j]), &mut linked, &mut adj);
        }
    }

    let mut cand = Vec::new();
    let mut seen = vec![0u32; obstacles.len()];
    let mut stamp = 1u32;
    // Inter-hull pairs only (same-owner chords stay on the ring).
    for a in 0..obstacles.len() {
        let (alo, ahi) = ranges[a];
        for b in (a + 1)..obstacles.len() {
            let (blo, bhi) = ranges[b];
            for i in alo..ahi {
                for j in blo..bhi {
                    let len = dist(pts[i], pts[j]);
                    if len < 1e-6 {
                        continue;
                    }
                    stamp = stamp.wrapping_add(1);
                    if stamp == 0 {
                        seen.fill(0);
                        stamp = 1;
                    }
                    if segment_clears_obstacles_grid(
                        pts[i],
                        pts[j],
                        obstacles,
                        &grid,
                        &mut cand,
                        &mut seen,
                        stamp,
                    ) {
                        add_undirected(i, j, len, &mut linked, &mut adj);
                    }
                }
            }
        }
    }
    VisGraph {
        pts,
        owner_idx,
        adj,
    }
}

/// From-scratch reference: omit endpoint hulls, add centroids, link to foreign.
#[cfg(test)]
fn build_query_graph_scratch(
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
    start: Pt,
    goal: Pt,
) -> QueryGraph {
    let mut pts: Vec<Pt> = Vec::new();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for o in obstacles {
        if skip.contains(o.mol_id.as_str()) {
            continue;
        }
        let lo = pts.len();
        pts.extend_from_slice(&o.hull);
        ranges.push((lo, pts.len()));
    }
    let n_stein = pts.len();
    let start_i = n_stein;
    let goal_i = n_stein + 1;
    pts.push(start);
    pts.push(goal);
    let n = pts.len();
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    let mut linked = vec![HashSet::<usize>::new(); n];

    for &(lo, hi) in &ranges {
        let m = hi - lo;
        if m < 2 {
            continue;
        }
        for k in 0..m {
            let i = lo + k;
            let j = lo + (k + 1) % m;
            add_undirected(i, j, dist(pts[i], pts[j]), &mut linked, &mut adj);
        }
    }

    let empty = HashSet::new();
    for i in 0..n_stein {
        for j in (i + 1)..n_stein {
            let same_ring = ranges
                .iter()
                .any(|&(lo, hi)| i >= lo && i < hi && j >= lo && j < hi);
            if same_ring {
                continue;
            }
            let len = dist(pts[i], pts[j]);
            if segment_clears_obstacles(pts[i], pts[j], obstacles, &empty) {
                add_undirected(i, j, len, &mut linked, &mut adj);
            }
        }
    }

    let mut skip_flag = vec![false; obstacles.len()];
    for (i, o) in obstacles.iter().enumerate() {
        if skip.contains(o.mol_id.as_str()) {
            skip_flag[i] = true;
        }
    }
    let grid = ObstacleGrid::build(obstacles);
    link_centroids_into_query(
        &mut adj,
        &mut linked,
        &pts,
        n_stein,
        start_i,
        goal_i,
        start,
        goal,
        obstacles,
        &skip_flag,
        &grid,
    );
    QueryGraph {
        pts,
        adj,
        start_i,
        goal_i,
    }
}

/// Incremental: strip endpoint-owner verts from shared [`VisGraph`], append
/// centroids, link centroids only to surviving foreign verts.
fn build_query_graph_incremental(
    vis: &VisGraph,
    obstacles: &[Obstacle],
    grid: &ObstacleGrid,
    skip: &HashSet<&str>,
    start: Pt,
    goal: Pt,
) -> QueryGraph {
    let mut skip_flag = vec![false; obstacles.len()];
    for (i, o) in obstacles.iter().enumerate() {
        if skip.contains(o.mol_id.as_str()) {
            skip_flag[i] = true;
        }
    }
    let mut local_of_global = vec![None; vis.pts.len()];
    let mut global_of_local: Vec<usize> = Vec::new();
    for (g, &oi) in vis.owner_idx.iter().enumerate() {
        if skip_flag[oi] {
            continue;
        }
        local_of_global[g] = Some(global_of_local.len());
        global_of_local.push(g);
    }
    let n_stein = global_of_local.len();
    let start_i = n_stein;
    let goal_i = n_stein + 1;
    let mut pts: Vec<Pt> = global_of_local.iter().map(|&g| vis.pts[g]).collect();
    pts.push(start);
    pts.push(goal);
    let n = pts.len();
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];

    // Remap surviving shared edges (VisGraph already deduped undirected).
    for (li, &gi) in global_of_local.iter().enumerate() {
        for &(gj, len) in &vis.adj[gi] {
            let Some(lj) = local_of_global[gj] else {
                continue;
            };
            if lj > li {
                adj[li].push((lj, len));
                adj[lj].push((li, len));
            }
        }
    }

    let mut linked = vec![HashSet::<usize>::new(); n];
    for (i, nbrs) in adj.iter().enumerate() {
        for &(j, _) in nbrs {
            linked[i].insert(j);
        }
    }
    link_centroids_into_query(
        &mut adj,
        &mut linked,
        &pts,
        n_stein,
        start_i,
        goal_i,
        start,
        goal,
        obstacles,
        &skip_flag,
        grid,
    );
    QueryGraph {
        pts,
        adj,
        start_i,
        goal_i,
    }
}

fn link_centroids_into_query(
    adj: &mut [Vec<(usize, f64)>],
    linked: &mut [HashSet<usize>],
    pts: &[Pt],
    n_stein: usize,
    start_i: usize,
    goal_i: usize,
    start: Pt,
    goal: Pt,
    obstacles: &[Obstacle],
    skip_flag: &[bool],
    grid: &ObstacleGrid,
) {
    let mut cand = Vec::new();
    let mut seen = vec![0u32; obstacles.len()];
    let mut stamp = 1u32;
    let visible = |a: Pt, b: Pt,
                       cand: &mut Vec<usize>,
                       seen: &mut Vec<u32>,
                       stamp: &mut u32|
     -> bool {
        *stamp = stamp.wrapping_add(1);
        if *stamp == 0 {
            seen.fill(0);
            *stamp = 1;
        }
        segment_clears_obstacles_grid_skip(a, b, obstacles, skip_flag, grid, cand, seen, *stamp)
    };
    for i in 0..n_stein {
        if visible(start, pts[i], &mut cand, &mut seen, &mut stamp) {
            add_undirected(start_i, i, dist(start, pts[i]), linked, adj);
        }
        if visible(goal, pts[i], &mut cand, &mut seen, &mut stamp) {
            add_undirected(goal_i, i, dist(goal, pts[i]), linked, adj);
        }
    }
    if visible(start, goal, &mut cand, &mut seen, &mut stamp) {
        add_undirected(start_i, goal_i, dist(start, goal), linked, adj);
    }
}

/// Undirected geometric edge set for equality checks (quantized).
#[cfg(test)]
fn query_graph_edge_set(g: &QueryGraph) -> BTreeSet<((i64, i64), (i64, i64), i64)> {
    let q = |p: Pt| ((p.0 * 1000.0).round() as i64, (p.1 * 1000.0).round() as i64);
    let mut out = BTreeSet::new();
    for (i, nbrs) in g.adj.iter().enumerate() {
        for &(j, len) in nbrs {
            if j <= i {
                continue;
            }
            let a = q(g.pts[i]);
            let b = q(g.pts[j]);
            let (a, b) = if a <= b { (a, b) } else { (b, a) };
            out.insert((a, b, (len * 1000.0).round() as i64));
        }
    }
    out
}

#[cfg(test)]
fn assert_query_graphs_identical(scratch: &QueryGraph, incr: &QueryGraph, label: &str) {
    assert_eq!(
        scratch.pts.len(),
        incr.pts.len(),
        "{label}: vertex count scratch={} incr={}",
        scratch.pts.len(),
        incr.pts.len()
    );
    for (i, (&a, &b)) in scratch.pts.iter().zip(incr.pts.iter()).enumerate() {
        assert!(
            nearly_same_pt(a, b),
            "{label}: pt[{i}] scratch={a:?} incr={b:?}"
        );
    }
    assert_eq!(
        scratch.start_i, incr.start_i,
        "{label}: start_i"
    );
    assert_eq!(scratch.goal_i, incr.goal_i, "{label}: goal_i");
    let es = query_graph_edge_set(scratch);
    let ei = query_graph_edge_set(incr);
    assert_eq!(
        es, ei,
        "{label}: edge sets differ (scratch {} edges, incr {} edges)",
        es.len(),
        ei.len()
    );
}

fn pathway_cost(
    pts: &[Pt],
    committed: &[Vec<Pt>],
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
) -> f64 {
    pts.windows(2)
        .map(|w| dist(w[0], w[1]) + segment_peer_cost(w[0], w[1], committed, obstacles, skip))
        .sum()
}

fn pathway_geom_len(pts: &[Pt]) -> f64 {
    pts.windows(2)
        .map(|w| dist(w[0], w[1]))
        .sum()
}

/// Excess length over the centroid chord — higher ⇒ re-route sooner.
fn pathway_improve_score(pathway: &[Pt], src: Pt, tgt: Pt) -> f64 {
    let tip = dist(src, tgt);
    if pathway.len() < 2 {
        return tip;
    }
    pathway_geom_len(pathway) - tip
}

fn nearly_same_pt(a: Pt, b: Pt) -> bool {
    dist(a, b) < 1e-6
}

/// True when `p` is a vertex of `pathway` (Steiner / centroid sample).
fn point_is_pathway_vertex(p: Pt, pathway: &[Pt]) -> bool {
    pathway.iter().any(|&q| nearly_same_pt(p, q))
}

fn segment_cross_cost(
    u: Pt,
    v: Pt,
    committed: &[Vec<Pt>],
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
) -> f64 {
    let pen = route().crossing_penalty;
    if pen <= 0.0 {
        return 0.0;
    }
    let mut c = 0.0;
    for other in committed {
        for w in other.windows(2) {
            if let Some(p) = segment_intersection_point(u, v, w[0], w[1]) {
                // Fan spokes meet inside source/target hulls — not a route crossing.
                if point_in_skip_hull(p, obstacles, skip) {
                    continue;
                }
                c += pen;
            }
        }
        // Sharing a hull Steiner (or any pathway vertex) is also a conflict —
        // proper-segment tests miss endpoint meets at a common vis-graph node.
        for &end in &[u, v] {
            if point_in_skip_hull(end, obstacles, skip) {
                continue;
            }
            if point_is_pathway_vertex(end, other) {
                c += pen;
            }
        }
    }
    c
}

/// Crossing + shared-length cost vs already-routed **centroid pathways**.
fn segment_peer_cost(
    u: Pt,
    v: Pt,
    committed: &[Vec<Pt>],
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
) -> f64 {
    let share_pen = route().shared_path_penalty;
    let mut c = segment_cross_cost(u, v, committed, obstacles, skip);
    if share_pen <= 0.0 {
        return c;
    }
    for other in committed {
        for w in other.windows(2) {
            let overlap = collinear_overlap_len(u, v, w[0], w[1]);
            if overlap > 1.0 {
                c += share_pen * (overlap / BOND_PX);
            }
        }
    }
    c
}

fn point_in_skip_hull(p: Pt, obstacles: &[Obstacle], skip: &HashSet<&str>) -> bool {
    for o in obstacles {
        if skip.contains(o.mol_id.as_str()) && point_in_convex(p, &o.hull) {
            return true;
        }
    }
    false
}

/// Hull visibility between two points, ignoring `skip` owners.
fn points_visible(a: Pt, b: Pt, obstacles: &[Obstacle], skip: &HashSet<&str>) -> bool {
    segment_clears_obstacles(a, b, obstacles, skip)
}

/// Length of collinear overlap between segments AB and PQ (0 if not collinear).
fn collinear_overlap_len(a: Pt, b: Pt, p: Pt, q: Pt) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let ab = dx.hypot(dy);
    if ab < 1e-9 {
        return 0.0;
    }
    let (ux, uy) = (dx / ab, dy / ab);
    // Lateral distance of P and Q from AB line.
    let lat = |r: Pt| ((r.0 - a.0) * uy - (r.1 - a.1) * ux).abs();
    let thresh = route().coterminal_lane_px * 0.5;
    if lat(p) > thresh || lat(q) > thresh {
        return 0.0;
    }
    // Project onto AB parameter [0, ab].
    let t = |r: Pt| ((r.0 - a.0) * ux + (r.1 - a.1) * uy).clamp(0.0, ab);
    let (t0, t1) = {
        let tp = t(p);
        let tq = t(q);
        (tp.min(tq), tp.max(tq))
    };
    (t1.min(ab) - t0.max(0.0)).max(0.0)
}

/// Paint-only: replace pathway ends with ink-standoff tips along approach rays.
fn tip_hug_pathway(
    pathway: &[Pt],
    world: &LayoutWorld,
    src_ids: &[String],
    tgt_ids: &[String],
    centroids: &HashMap<String, Pt>,
    src_c: Pt,
    tgt_c: Pt,
    clash: f64,
    min_shaft: f64,
) -> Vec<Pt> {
    if pathway.len() < 2 {
        return pathway.to_vec();
    }
    let toward_start = pathway[1];
    let toward_goal = pathway[pathway.len() - 2];
    let start = tip_pose_group(world, src_ids, centroids, src_c, toward_start, clash, min_shaft);
    let goal = tip_pose_group(world, tgt_ids, centroids, tgt_c, toward_goal, clash, min_shaft);
    let mut out = pathway.to_vec();
    out[0] = start;
    let last = out.len() - 1;
    out[last] = goal;
    out
}

/// Intersection of AB and CD when [`segments_properly_cross`] is true.
fn segment_intersection_point(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<Pt> {
    if !segments_properly_cross(a, b, c, d) {
        return None;
    }
    let dx1 = b.0 - a.0;
    let dy1 = b.1 - a.1;
    let dx2 = d.0 - c.0;
    let dy2 = d.1 - c.1;
    let den = dx1 * dy2 - dy1 * dx2;
    if den.abs() < 1e-18 {
        return None;
    }
    let t = ((c.0 - a.0) * dy2 - (c.1 - a.1) * dx2) / den;
    Some((a.0 + t * dx1, a.1 + t * dy1))
}

fn push_cross_mark(out: &mut Vec<Primitive>, p: Pt) {
    let s = 10.0;
    out.push(dot(p, 7.0, dbg::CROSS, 0.95, "debug-route-cross"));
    out.push(poly_open(
        &[(p.0 - s, p.1 - s), (p.0 + s, p.1 + s)],
        dbg::CROSS_CORE,
        4.0,
        1.0,
        None,
        "debug-route-cross-x",
    ));
    out.push(poly_open(
        &[(p.0 - s, p.1 + s), (p.0 + s, p.1 - s)],
        dbg::CROSS_CORE,
        4.0,
        1.0,
        None,
        "debug-route-cross-x",
    ));
}

fn build_obstacles(
    mol_ids: &[String],
    world: &LayoutWorld,
    positions: &HashMap<String, Pt>,
    size_of: &HashMap<&str, (f64, f64)>,
    inflate: f64,
) -> Vec<Obstacle> {
    let mut out = Vec::with_capacity(mol_ids.len());
    for id in mol_ids {
        let under = format!("scheme/mols/{id}");
        let ink_pts = collect_ink_points(world, &under);
        let ink_pts = if ink_pts.len() >= 3 {
            ink_pts
        } else if let (Some(&(x, y)), Some(&(w, h))) =
            (positions.get(id.as_str()), size_of.get(id.as_str()))
        {
            vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
        } else {
            continue;
        };
        let raw = convex_hull_pts(&ink_pts);
        if raw.len() < 3 {
            continue;
        }
        let hull = inflate_convex(&raw, inflate);
        if hull.len() < 3 {
            continue;
        }
        let aabb = hull_aabb(&hull);
        out.push(Obstacle {
            mol_id: id.clone(),
            hull,
            aabb,
        });
    }
    out
}

fn lane_offsets(n: usize) -> Vec<f64> {
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![0.0];
    }
    let step = route().coterminal_lane_px;
    let mid = (n as f64 - 1.0) * 0.5;
    (0..n).map(|i| (i as f64 - mid) * step).collect()
}

fn shaft_chem_clash(stroke_width: Option<f64>, bundle_n: usize) -> f64 {
    let half = if bundle_n <= 1 {
        0.0
    } else {
        (bundle_n as f64 - 1.0) * 0.5 * route().coterminal_lane_px
    };
    SCHEME_INK_STANDOFF + half + stroke_width.unwrap_or(1.6) * 0.5
}

/// All painted ink under `under`: bonds, atoms, regions, captions, labels.
/// Parent tile AABBs are never sampled (nodes with children are only walked).
/// Leaf ink AABBs (glyph / region / caption boxes) **are** included — skipping
/// them left GlcA labels and mol names out of routing hulls.
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
        if n.object_type == super::layout_constraints::ObjectType::Group {
            if let Shape::PointSet(p) = n.world_shape() {
                pts.extend(p);
            }
            continue;
        }
        match n.world_shape() {
            Shape::Point { x, y } => pts.push((x, y)),
            Shape::Line { x0, y0, x1, y1 } => {
                pts.push((x0, y0));
                pts.push((x1, y1));
            }
            Shape::Polyline(p) | Shape::PointSet(p) => pts.extend(p),
            Shape::Circle { cx, cy, r } => {
                let k = 8;
                for i in 0..k {
                    let t = std::f64::consts::TAU * (i as f64) / (k as f64);
                    pts.push((cx + r * t.cos(), cy + r * t.sin()));
                }
            }
            Shape::Aabb(a) => {
                // Skip empty placeholder boxes; keep label/region/caption ink.
                if a.width() > 1e-9 || a.height() > 1e-9 {
                    pts.push((a.x0, a.y0));
                    pts.push((a.x1, a.y0));
                    pts.push((a.x1, a.y1));
                    pts.push((a.x0, a.y1));
                }
            }
        }
    }
    pts
}

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

fn segment_clears_obstacles(a: Pt, b: Pt, obstacles: &[Obstacle], skip: &HashSet<&str>) -> bool {
    for o in obstacles {
        if skip.contains(o.mol_id.as_str()) {
            continue;
        }
        if segment_hits_convex_aabb(a, b, &o.hull, o.aabb) {
            return false;
        }
    }
    true
}

/// Like [`segment_clears_obstacles`] with empty skip, using an AABB grid broad-phase.
fn segment_clears_obstacles_grid(
    a: Pt,
    b: Pt,
    obstacles: &[Obstacle],
    grid: &ObstacleGrid,
    cand: &mut Vec<usize>,
    seen: &mut Vec<u32>,
    stamp: u32,
) -> bool {
    segment_clears_obstacles_grid_skip(a, b, obstacles, &[], grid, cand, seen, stamp)
}

/// Grid broad-phase clears; `skip_flag[i]` makes obstacle `i` transparent.
/// Empty `skip_flag` means nothing is skipped.
fn segment_clears_obstacles_grid_skip(
    a: Pt,
    b: Pt,
    obstacles: &[Obstacle],
    skip_flag: &[bool],
    grid: &ObstacleGrid,
    cand: &mut Vec<usize>,
    seen: &mut Vec<u32>,
    stamp: u32,
) -> bool {
    grid.candidates(a, b, cand, seen, stamp);
    for &oi in cand.iter() {
        if skip_flag.get(oi).copied().unwrap_or(false) {
            continue;
        }
        let o = &obstacles[oi];
        if segment_hits_convex_aabb(a, b, &o.hull, o.aabb) {
            return false;
        }
    }
    true
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
    for i in 0..n {
        for j in (i + 1)..n {
            if seeds_coterminal(&seed_pts[i], &seed_pts[j]).is_some() {
                unite(&mut parent, i, j);
            }
        }
    }
    let mut buckets: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        buckets.entry(find(&mut parent, i)).or_default().push(i);
    }
    let mut groups = Vec::new();
    let mut keys: Vec<_> = buckets.keys().copied().collect();
    keys.sort_unstable();
    for k in keys {
        let mut members = buckets.remove(&k).unwrap_or_default();
        members.sort_unstable();
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
    let (x, y) = positions.get(id).copied().unwrap_or((0.0, 0.0));
    if let Some(paint) = paints.get(id) {
        return paint.center_in_document((x, y));
    }
    let (w, h) = sizes.get(id).copied().unwrap_or((0.0, 0.0));
    (x + w * 0.5, y + h * 0.5)
}

fn poly_open(
    pts: &[Pt],
    stroke: &str,
    width: f64,
    opacity: f64,
    dash: Option<&str>,
    class: &str,
) -> Primitive {
    let mut d = String::new();
    for (i, &(x, y)) in pts.iter().enumerate() {
        if i == 0 {
            d.push_str(&format!("M {:.2} {:.2}", x, y));
        } else {
            d.push_str(&format!(" L {:.2} {:.2}", x, y));
        }
    }
    Primitive::Path {
        d,
        stroke: Some(stroke.into()),
        fill: Some("none".into()),
        stroke_width: width,
        opacity,
        stroke_dasharray: dash.map(str::to_string),
        stroke_linecap: Some("round".into()),
        class: Some(class.into()),
        data_text: None,
    }
}

fn poly_closed(pts: &[Pt], stroke: &str, width: f64, opacity: f64, class: &str) -> Primitive {
    let mut d = String::new();
    for (i, &(x, y)) in pts.iter().enumerate() {
        if i == 0 {
            d.push_str(&format!("M {:.2} {:.2}", x, y));
        } else {
            d.push_str(&format!(" L {:.2} {:.2}", x, y));
        }
    }
    d.push_str(" Z");
    Primitive::Path {
        d,
        stroke: Some(stroke.into()),
        fill: Some("none".into()),
        stroke_width: width,
        opacity,
        stroke_dasharray: Some("4 2".into()),
        stroke_linecap: Some("butt".into()),
        class: Some(class.into()),
        data_text: None,
    }
}

fn dot(p: Pt, r: f64, fill: &str, opacity: f64, class: &str) -> Primitive {
    Primitive::Circle {
        cx: p.0,
        cy: p.1,
        r,
        fill: Some(fill.into()),
        stroke: None,
        stroke_width: 0.0,
        opacity,
        class: Some(class.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(id: &str, x0: f64, y0: f64, s: f64) -> Obstacle {
        let hull = vec![(x0, y0), (x0 + s, y0), (x0 + s, y0 + s), (x0, y0 + s)];
        Obstacle {
            mol_id: id.into(),
            aabb: hull_aabb(&hull),
            hull,
        }
    }

    fn rect(id: &str, x0: f64, y0: f64, w: f64, h: f64) -> Obstacle {
        let hull = vec![(x0, y0), (x0 + w, y0), (x0 + w, y0 + h), (x0, y0 + h)];
        Obstacle {
            mol_id: id.into(),
            aabb: hull_aabb(&hull),
            hull,
        }
    }

    fn pentagon(id: &str, cx: f64, cy: f64, r: f64) -> Obstacle {
        let mut hull = Vec::with_capacity(5);
        for k in 0..5 {
            let a = std::f64::consts::TAU * (k as f64) / 5.0 - std::f64::consts::FRAC_PI_2;
            hull.push((cx + r * a.cos(), cy + r * a.sin()));
        }
        Obstacle {
            mol_id: id.into(),
            aabb: hull_aabb(&hull),
            hull,
        }
    }

    fn assert_incr_eq_scratch(
        obstacles: &[Obstacle],
        skip: &HashSet<&str>,
        start: Pt,
        goal: Pt,
        label: &str,
    ) {
        let grid = ObstacleGrid::build(obstacles);
        let vis = build_visibility_graph(obstacles, &grid);
        let scratch = build_query_graph_scratch(obstacles, skip, start, goal);
        let incr = build_query_graph_incremental(&vis, obstacles, &grid, skip, start, goal);
        assert_query_graphs_identical(&scratch, &incr, label);
    }

    fn assert_routes_match(
        obstacles: &[Obstacle],
        skip: &HashSet<&str>,
        start: Pt,
        goal: Pt,
        committed: &[Vec<Pt>],
        label: &str,
    ) {
        let grid = ObstacleGrid::build(obstacles);
        let vis = build_visibility_graph(obstacles, &grid);
        let scratch = build_query_graph_scratch(obstacles, skip, start, goal);
        let incr = build_query_graph_incremental(&vis, obstacles, &grid, skip, start, goal);
        assert_query_graphs_identical(&scratch, &incr, label);
        let a = astar_on_query(&scratch, obstacles, skip, committed);
        let b = visibility_route(start, goal, &vis, obstacles, skip, committed);
        assert_eq!(a.len(), b.len(), "{label}: route length {a:?} vs {b:?}");
        for (i, (&p, &q)) in a.iter().zip(b.iter()).enumerate() {
            assert!(
                nearly_same_pt(p, q),
                "{label}: route[{i}] {p:?} vs {q:?}"
            );
        }
    }

    #[test]
    fn points_visible_blocks_foreign_hull_even_if_endpoint_buried() {
        // A and B side by side; C overlaps both centroids via inflate.
        let a = square("A", 0.0, 0.0, 10.0);
        let b = square("B", 20.0, 0.0, 10.0);
        let c = square("C", 5.0, -2.0, 20.0); // covers (5,5) and (25,5)
        let obstacles = vec![a, b, c];
        let skip: HashSet<&str> = ["A", "B"].into_iter().collect();
        let start = (5.0, 5.0);
        let goal = (25.0, 5.0);
        assert!(
            point_in_convex(start, &obstacles[2].hull),
            "sanity: start buried in C"
        );
        assert!(
            !points_visible(start, goal, &obstacles, &skip),
            "buried-in-neighbor must block — that chord cuts C's chem"
        );
        // Owners only: clear gap, no foreign hull.
        let ab = vec![obstacles[0].clone(), obstacles[1].clone()];
        assert!(
            points_visible(start, goal, &ab, &skip),
            "owner-only skip must allow the clear centroid chord"
        );
        // Blocker between A and B with endpoints outside it.
        let wall = square("W", 12.0, 0.0, 6.0); // x 12..18, y 0..6 — pierces y=5 chord
        let with_wall = vec![obstacles[0].clone(), obstacles[1].clone(), wall];
        assert!(
            !points_visible(start, goal, &with_wall, &skip),
            "true piercer must still block"
        );
    }

    #[test]
    fn visibility_route_takes_visible_centroid_chord() {
        let a = square("A", 0.0, 0.0, 10.0);
        let b = square("B", 40.0, 0.0, 10.0);
        let obstacles = vec![a, b];
        let skip: HashSet<&str> = ["A", "B"].into_iter().collect();
        let grid = ObstacleGrid::build(&obstacles);
        let vis = build_visibility_graph(&obstacles, &grid);
        let path = visibility_route((5.0, 5.0), (45.0, 5.0), &vis, &obstacles, &skip, &[]);
        assert_eq!(
            path.len(),
            2,
            "clear centroid chord must be the temp direct edge: {path:?}"
        );
    }

    #[test]
    fn query_graph_omits_endpoint_hulls_and_adds_centroids() {
        let a = square("A", 0.0, 0.0, 10.0);
        let b = square("B", 40.0, 0.0, 10.0);
        let c = square("C", 20.0, 20.0, 10.0);
        let obstacles = vec![a, b, c];
        let skip: HashSet<&str> = ["A", "B"].into_iter().collect();
        let start = (5.0, 5.0);
        let goal = (45.0, 5.0);
        let g = build_query_graph_scratch(&obstacles, &skip, start, goal);
        assert_eq!(g.pts[g.start_i], start);
        assert_eq!(g.pts[g.goal_i], goal);
        // Only C's 4 hull verts + 2 centroids.
        assert_eq!(g.pts.len(), 6, "endpoint hulls must be absent: {:?}", g.pts);
        assert!(
            !g.adj[g.start_i].is_empty() || points_visible(start, goal, &obstacles, &skip),
            "start must link to foreign steiners or direct goal"
        );
        // No edge from start to a point inside A's hull ring (A was omitted).
        for &(j, _) in &g.adj[g.start_i] {
            if j == g.goal_i {
                continue;
            }
            assert!(
                !point_in_convex(g.pts[j], &obstacles[0].hull),
                "start must not link to own-hull verts"
            );
        }
        assert_incr_eq_scratch(&obstacles, &skip, start, goal, "omit-endpoints");
    }

    #[test]
    fn incremental_matches_scratch_clear_pair() {
        let obstacles = vec![
            square("A", 0.0, 0.0, 10.0),
            square("B", 40.0, 0.0, 10.0),
        ];
        let skip: HashSet<&str> = ["A", "B"].into_iter().collect();
        assert_routes_match(
            &obstacles,
            &skip,
            (5.0, 5.0),
            (45.0, 5.0),
            &[],
            "clear-pair",
        );
    }

    #[test]
    fn incremental_matches_scratch_with_blocker() {
        let obstacles = vec![
            square("A", 0.0, 0.0, 10.0),
            square("B", 40.0, 0.0, 10.0),
            square("W", 18.0, -5.0, 10.0),
        ];
        let skip: HashSet<&str> = ["A", "B"].into_iter().collect();
        assert_routes_match(
            &obstacles,
            &skip,
            (5.0, 5.0),
            (45.0, 5.0),
            &[],
            "blocker-W",
        );
    }

    #[test]
    fn incremental_matches_scratch_skip_first_middle_last() {
        let obstacles = vec![
            square("A", 0.0, 0.0, 8.0),
            square("B", 20.0, 0.0, 8.0),
            square("C", 40.0, 0.0, 8.0),
            square("D", 10.0, 20.0, 8.0),
            square("E", 30.0, 20.0, 8.0),
        ];
        let start = (4.0, 4.0);
        let goal = (44.0, 4.0);
        for (src, tgt) in [("A", "C"), ("B", "E"), ("D", "A"), ("E", "B"), ("C", "D")] {
            let skip: HashSet<&str> = [src, tgt].into_iter().collect();
            assert_incr_eq_scratch(
                &obstacles,
                &skip,
                start,
                goal,
                &format!("skip-{src}-{tgt}"),
            );
        }
    }

    #[test]
    fn incremental_matches_scratch_all_pairs_grid() {
        // 3×3 grid of squares — every endpoint pair must strip identically.
        let mut obstacles = Vec::new();
        for row in 0..3 {
            for col in 0..3 {
                let id = format!("m{row}{col}");
                obstacles.push(square(
                    &id,
                    col as f64 * 30.0,
                    row as f64 * 30.0,
                    12.0,
                ));
            }
        }
        let grid = ObstacleGrid::build(&obstacles);
        let vis = build_visibility_graph(&obstacles, &grid);
        for i in 0..obstacles.len() {
            for j in 0..obstacles.len() {
                if i == j {
                    continue;
                }
                let src = obstacles[i].mol_id.as_str();
                let tgt = obstacles[j].mol_id.as_str();
                let skip: HashSet<&str> = [src, tgt].into_iter().collect();
                let start = (
                    obstacles[i].hull[0].0 + 6.0,
                    obstacles[i].hull[0].1 + 6.0,
                );
                let goal = (
                    obstacles[j].hull[0].0 + 6.0,
                    obstacles[j].hull[0].1 + 6.0,
                );
                let scratch = build_query_graph_scratch(&obstacles, &skip, start, goal);
                let incr = build_query_graph_incremental(
                    &vis, &obstacles, &grid, &skip, start, goal,
                );
                assert_query_graphs_identical(
                    &scratch,
                    &incr,
                    &format!("grid-{src}->{tgt}"),
                );
            }
        }
    }

    #[test]
    fn incremental_matches_scratch_mixed_shapes() {
        let obstacles = vec![
            square("sq", 0.0, 0.0, 10.0),
            rect("wide", 40.0, -5.0, 20.0, 8.0),
            pentagon("p", 25.0, 30.0, 8.0),
            rect("tall", 60.0, 20.0, 6.0, 24.0),
        ];
        let cases = [
            ("sq", "wide", (5.0, 5.0), (50.0, -1.0)),
            ("sq", "p", (5.0, 5.0), (25.0, 30.0)),
            ("wide", "tall", (50.0, -1.0), (63.0, 32.0)),
            ("p", "tall", (25.0, 30.0), (63.0, 32.0)),
        ];
        for (src, tgt, start, goal) in cases {
            let skip: HashSet<&str> = [src, tgt].into_iter().collect();
            assert_routes_match(
                &obstacles,
                &skip,
                start,
                goal,
                &[],
                &format!("mixed-{src}-{tgt}"),
            );
        }
    }

    #[test]
    fn incremental_matches_scratch_no_foreign_left() {
        // Strip both mols — query is just the two centroids (+ optional chord).
        let obstacles = vec![
            square("A", 0.0, 0.0, 10.0),
            square("B", 40.0, 0.0, 10.0),
        ];
        let skip: HashSet<&str> = ["A", "B"].into_iter().collect();
        let start = (5.0, 5.0);
        let goal = (45.0, 5.0);
        let scratch = build_query_graph_scratch(&obstacles, &skip, start, goal);
        assert_eq!(scratch.pts.len(), 2);
        assert_incr_eq_scratch(&obstacles, &skip, start, goal, "no-foreign");
    }

    #[test]
    fn incremental_matches_scratch_dense_wall() {
        // Corridor of blockers between A and B — many foreign verts.
        let mut obstacles = vec![
            square("A", 0.0, 20.0, 10.0),
            square("B", 100.0, 20.0, 10.0),
        ];
        for i in 0..6 {
            obstacles.push(square(
                &format!("w{i}"),
                15.0 + i as f64 * 12.0,
                10.0 + (i % 2) as f64 * 15.0,
                10.0,
            ));
        }
        let skip: HashSet<&str> = ["A", "B"].into_iter().collect();
        assert_routes_match(
            &obstacles,
            &skip,
            (5.0, 25.0),
            (105.0, 25.0),
            &[],
            "dense-wall",
        );
    }

    #[test]
    fn incremental_matches_scratch_with_committed_peers() {
        let obstacles = vec![
            square("A", 0.0, 0.0, 10.0),
            square("B", 60.0, 0.0, 10.0),
            square("C", 25.0, 25.0, 10.0),
        ];
        let skip: HashSet<&str> = ["A", "B"].into_iter().collect();
        let committed = vec![vec![(5.0, 40.0), (30.0, 30.0), (65.0, 40.0)]];
        assert_routes_match(
            &obstacles,
            &skip,
            (5.0, 5.0),
            (65.0, 5.0),
            &committed,
            "committed-peers",
        );
    }

    #[test]
    fn shared_hull_vertex_counts_as_crossing() {
        let _guard = ActiveRouteGuard;
        ACTIVE_ROUTE.with(|c| *c.borrow_mut() = Some(RouteConfig::pretty()));
        let pen = route().crossing_penalty;
        assert!(pen > 0.0);
        // Foreign Steiner Q on a committed pathway; candidate segment ends at Q.
        // Proper-cross is false (endpoint meet) — must still pay.
        let q = (50.0, 50.0);
        let committed = vec![vec![(0.0, 50.0), q, (100.0, 50.0)]];
        let obstacles: Vec<Obstacle> = Vec::new();
        let skip: HashSet<&str> = HashSet::new();
        let cost = segment_cross_cost((50.0, 0.0), q, &committed, &obstacles, &skip);
        assert!(
            cost >= pen - 1e-9,
            "shared vis vertex must cost a crossing, got {cost} (pen={pen})"
        );
        let clear = segment_cross_cost(
            (50.0, 0.0),
            (50.0, 10.0),
            &committed,
            &obstacles,
            &skip,
        );
        assert!(
            clear < 1e-9,
            "segment that misses the shared vertex must be free, got {clear}"
        );
    }
}
