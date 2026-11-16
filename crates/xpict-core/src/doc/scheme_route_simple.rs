//! Lean scheme router — parallel to [`super::scheme_route`] (delete later).
//!
//! One hull visibility graph. Per arrow: strip start/end hull verts; centroids
//! link only to foreign hulls; A* minimizes length + crossings + shared length.
//! Several Gauss–Seidel passes on **centroid pathways**, worst-detour first
//! within each pass (tips posed only after pathways lock). Visibility verts
//! are the full inflated hull ring (no subsample chords through the interior).

use std::collections::{HashMap, HashSet};

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

struct VisGraph {
    pts: Vec<Pt>,
    /// Mol that owns each hull vertex (same length as `pts`).
    owner: Vec<String>,
    adj: Vec<Vec<(usize, f64)>>,
    /// Undirected hull edges `(min_i, max_i)` for crossing updates.
    edges: Vec<(usize, usize)>,
}

/// Extra path cost on hull–hull edges from already-routed shafts.
/// Updated with [`EdgeExtras::note_pathway`] after each commit — O(path × E).
#[derive(Default)]
struct EdgeExtras {
    /// Key = undirected `(min_i, max_i)` into [`VisGraph::pts`].
    on_edge: HashMap<(usize, usize), f64>,
}

impl EdgeExtras {
    fn get(&self, i: usize, j: usize) -> f64 {
        self.on_edge
            .get(&undirected_edge(i, j))
            .copied()
            .unwrap_or(0.0)
    }

    /// Accrue shared-segment costs for a committed **centroid pathway**.
    /// Crossings are scored live against pathway polylines during A*.
    fn note_pathway(&mut self, vis: &VisGraph, pathway: &[Pt]) {
        if pathway.len() < 2 || vis.edges.is_empty() {
            return;
        }
        let share_pen = route().shared_path_penalty;
        if share_pen <= 0.0 {
            return;
        }
        for w in pathway.windows(2) {
            let a = w[0];
            let b = w[1];
            let (ax0, ax1) = (a.0.min(b.0), a.0.max(b.0));
            let (ay0, ay1) = (a.1.min(b.1), a.1.max(b.1));
            for &(i, j) in &vis.edges {
                let p = vis.pts[i];
                let q = vis.pts[j];
                if p.0.max(q.0) < ax0 - 1.0
                    || p.0.min(q.0) > ax1 + 1.0
                    || p.1.max(q.1) < ay0 - 1.0
                    || p.1.min(q.1) > ay1 + 1.0
                {
                    continue;
                }
                let overlap = collinear_overlap_len(a, b, p, q);
                if overlap > 1.0 {
                    *self.on_edge.entry(undirected_edge(i, j)).or_insert(0.0) +=
                        share_pen * (overlap / BOND_PX);
                }
            }
        }
    }
}

fn undirected_edge(i: usize, j: usize) -> (usize, usize) {
    if i < j {
        (i, j)
    } else {
        (j, i)
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
    let vis = build_visibility_graph(&obstacles);
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

    // Pass 0: every group with no peer costs (independent tips/detours).
    // Later passes: Gauss–Seidel, smallest excess-over-tip first (tips before
    // wild walks) so short paths re-lock against live peers, then detours adapt.
    const ROUTE_PASSES: usize = 4;
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
            let mut edge_extras = EdgeExtras::default();
            for c in &committed {
                edge_extras.note_pathway(&vis, c);
            }
            let (src_c, tgt_c, _, _, _, _) = &group_meta[gi];
            let skip: HashSet<&str> = skips[gi].iter().map(String::as_str).collect();
            group_pathways[gi] = visibility_route(
                *src_c,
                *tgt_c,
                &vis,
                &obstacles,
                &skip,
                &edge_extras,
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
        // source/target hulls — same rule as peer-cost).
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
            }
        }
        for &p in &hits {
            push_cross_mark(&mut debug_prims, p);
        }
        let n_tip = group_pathways.iter().filter(|p| p.len() == 2).count();
        let n_detour = group_pathways.len().saturating_sub(n_tip);
        eprintln!(
            "route debug: {n_tip} tip chords, {n_detour} hull detours, {} proper pathway intersections",
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

/// A* on the shared hull graph with start/end owners stripped. Centroids link
/// only to foreign hull verts. Hull–hull edges pay cached share extras;
/// every candidate segment pays live crossing/share vs **pathway** `committed`.
fn visibility_route(
    start: Pt,
    goal: Pt,
    vis: &VisGraph,
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
    edge_extras: &EdgeExtras,
    committed: &[Vec<Pt>],
) -> Vec<Pt> {
    // Active Steiner set = hull verts whose owner is not an endpoint.
    let mut local_of_global = vec![None; vis.pts.len()];
    let mut global_of_local: Vec<usize> = Vec::new();
    for (g, own) in vis.owner.iter().enumerate() {
        if skip.contains(own.as_str()) {
            continue;
        }
        local_of_global[g] = Some(global_of_local.len());
        global_of_local.push(g);
    }
    let n_local = global_of_local.len();

    let start_links = link_terminal_filtered(start, vis, &local_of_global, obstacles, skip);
    let goal_links = link_terminal_filtered(goal, vis, &local_of_global, obstacles, skip);
    // Temporary centroid↔centroid edge when the two hubs can see each other.
    let direct = points_visible(start, goal, obstacles, skip).then(|| dist(start, goal));

    let mut to_start = vec![None; n_local];
    let mut to_goal = vec![None; n_local];
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
            _ => vis.pts[global_of_local[i - 2]],
        }
    };
    // Hull–hull: share from frozen extras; cross live vs pathed peers.
    // Terminal/direct: both cross + share live (not graph edges).
    let path = astar(
        &0usize,
        |&u| {
            let mut nbrs: Vec<(usize, f64)> = Vec::new();
            match u {
                0 => {
                    for &(j, len) in &start_links {
                        let pv = vis.pts[global_of_local[j]];
                        nbrs.push((
                            j + 2,
                            len + segment_peer_cost(start, pv, committed, obstacles, skip),
                        ));
                    }
                    if let Some(len) = direct {
                        nbrs.push((
                            1,
                            len + segment_peer_cost(start, goal, committed, obstacles, skip),
                        ));
                    }
                }
                1 => {
                    for &(j, len) in &goal_links {
                        let pv = vis.pts[global_of_local[j]];
                        nbrs.push((
                            j + 2,
                            len + segment_peer_cost(goal, pv, committed, obstacles, skip),
                        ));
                    }
                    if let Some(len) = direct {
                        nbrs.push((
                            0,
                            len + segment_peer_cost(start, goal, committed, obstacles, skip),
                        ));
                    }
                }
                _ => {
                    let g = global_of_local[u - 2];
                    for &(gj, len) in &vis.adj[g] {
                        if let Some(lj) = local_of_global[gj] {
                            nbrs.push((
                                lj + 2,
                                len + edge_extras.get(g, gj)
                                    + segment_cross_cost(
                                        vis.pts[g],
                                        vis.pts[gj],
                                        committed,
                                        obstacles,
                                        skip,
                                    ),
                            ));
                        }
                    }
                    if let Some(len) = to_start[u - 2] {
                        nbrs.push((
                            0,
                            len + segment_peer_cost(node_pt(u), start, committed, obstacles, skip),
                        ));
                    }
                    if let Some(len) = to_goal[u - 2] {
                        nbrs.push((
                            1,
                            len + segment_peer_cost(node_pt(u), goal, committed, obstacles, skip),
                        ));
                    }
                }
            }
            nbrs.into_iter()
                .map(|(v, cost)| (v, ordered_float(cost)))
                .collect::<Vec<_>>()
        },
        |&u| ordered_float(dist(node_pt(u), goal)),
        |&g| g == 1,
    );
    let astar_pts: Vec<Pt> = match path {
        Some((idxs, _)) => idxs.into_iter().map(node_pt).collect(),
        None => vec![start, goal],
    };
    // Prefer visible centroid↔centroid when its cost beats (or ties) A*.
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

/// All visible foreign hull verts for a centroid terminal (local Steiner indices).
fn link_terminal_filtered(
    hub: Pt,
    vis: &VisGraph,
    local_of_global: &[Option<usize>],
    obstacles: &[Obstacle],
    skip: &HashSet<&str>,
) -> Vec<(usize, f64)> {
    let mut out = Vec::new();
    for (g, &p) in vis.pts.iter().enumerate() {
        let Some(local) = local_of_global[g] else {
            continue;
        };
        if points_visible(hub, p, obstacles, skip) {
            out.push((local, dist(hub, p)));
        }
    }
    out
}

/// Hull visibility between two points, ignoring `skip` owners.
fn points_visible(a: Pt, b: Pt, obstacles: &[Obstacle], skip: &HashSet<&str>) -> bool {
    segment_clears_obstacles(a, b, obstacles, skip)
}

fn build_visibility_graph(obstacles: &[Obstacle]) -> VisGraph {
    let mut pts = Vec::new();
    let mut owner: Vec<String> = Vec::new();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for o in obstacles {
        let start = pts.len();
        for &p in &o.hull {
            pts.push(p);
            owner.push(o.mol_id.clone());
        }
        ranges.push((start, pts.len()));
    }
    let n = pts.len();
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    let mut edges: Vec<(usize, usize)> = Vec::new();
    let mut linked = vec![HashSet::<usize>::new(); n];
    let mut add = |i: usize, j: usize, len: f64| {
        if i == j || len < 1e-6 || !linked[i].insert(j) {
            return;
        }
        linked[j].insert(i);
        adj[i].push((j, len));
        adj[j].push((i, len));
        edges.push(undirected_edge(i, j));
    };

    // Hull boundary rings always — along the silhouette, never through it.
    for &(lo, hi) in &ranges {
        let m = hi - lo;
        if m < 2 {
            continue;
        }
        for k in 0..m {
            let i = lo + k;
            let j = lo + (k + 1) % m;
            add(i, j, dist(pts[i], pts[j]));
        }
    }

    // Full inter-hull visibility (no k-nearest cap).
    let empty = HashSet::new();
    for i in 0..n {
        let oi = owner[i].as_str();
        for j in (i + 1)..n {
            if owner[j].as_str() == oi {
                continue;
            }
            let len = dist(pts[i], pts[j]);
            if segment_clears_obstacles(pts[i], pts[j], obstacles, &empty) {
                add(i, j, len);
            }
        }
    }
    VisGraph {
        pts,
        owner,
        adj,
        edges,
    }
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
        let vis = build_visibility_graph(&obstacles);
        let skip: HashSet<&str> = ["A", "B"].into_iter().collect();
        let extras = EdgeExtras::default();
        let path = visibility_route(
            (5.0, 5.0),
            (45.0, 5.0),
            &vis,
            &obstacles,
            &skip,
            &extras,
            &[],
        );
        assert_eq!(
            path.len(),
            2,
            "clear centroid chord must be the temp direct edge: {path:?}"
        );
    }
}
