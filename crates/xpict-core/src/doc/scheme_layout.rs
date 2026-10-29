//! ELK seed + constraint refine for reaction-scheme edges and labels.
//!
//! Molecules stay fixed. ELK endpoints/routing points and edge labels (xy + rot)
//! are the DOF. Catmull–Rom is how those points are stroked, not extra vertices.

use std::collections::HashMap;

use crate::arrows::{
    edge_anchors, min_shaft_px, offset_polyline, prune_close_controls, ANCHOR_GAP, CLOSE_CTRL_PX,
};
use crate::metrics::BOND_PX;
use crate::scene::{LayerName, MoleculeIn, Primitive, TextAnchor};

use super::layout_constraints::{
    edge_signed_distance, push_clear_leaves, push_clear_leaves_where, shapes_properly_cross, solve,
    Constraint, HardConstraint, LayoutNode, LayoutWorld, ObjectType, Shape, SolveOpts,
    LAGRANGIAN_CLEAR, LAGRANGIAN_TETHER,
};
use super::layout_items::Aabb;
use super::{DepictSpec, DocPaint, EdgeNode, LayoutOpts};

/// Gap from painted mol ink (chem **and** captions) to the shaft tip.
///
/// Half a bond: close enough to read as “from this mol”, far enough not to
/// sit on heteroatoms. Extra space between mols is shaft — the solver hugs
/// this pose, so shifting molecules lengthens the arrow.
pub const SCHEME_INK_STANDOFF: f64 = 0.5 * BOND_PX;
/// Minimum gap between nearly-parallel overlapping shafts.
pub const SCHEME_EDGE_CLEARANCE: f64 = BOND_PX;
/// Center-to-center gap between coterminal lanes. Unrelated parallel shafts
/// still use [`SCHEME_EDGE_CLEARANCE`]; paired A→B / B→A should sit like a
/// compact double arrow, not a bond-length apart.
const COTERMINAL_LANE_PX: f64 = 8.0;
/// Default scheme-arrow stroke (matches [`crate::arrows`] house width).
const SHAFT_STROKE_PX: f64 = 1.6;
const LABEL_FONT_PX: f64 = 11.0;
const LABEL_SHAFT_PAD_PX: f64 = 4.0;

/// Air between the glyph disk and the painted shaft (half stroke + pad).
fn shaft_air(stroke_width: Option<f64>) -> f64 {
    stroke_width.unwrap_or(SHAFT_STROKE_PX) * 0.5 + LABEL_SHAFT_PAD_PX
}

/// Center-to-centerline gap the solver should leave (air + half glyph).
fn label_shaft_clash(stroke_width: Option<f64>) -> f64 {
    shaft_air(stroke_width) + LABEL_FONT_PX * 0.5
}

/// MaxDist ceiling: clash plus one bond so CLEAR can push off a fat shaft.
fn label_shaft_tether(stroke_width: Option<f64>) -> f64 {
    label_shaft_clash(stroke_width) + BOND_PX
}
const POSE_STRENGTH: f64 = 0.35;
/// Keep the painted shaft on a ray through the mol centroid (perp²).
const RAY_TO_CENTROID: f64 = 4.0;
/// Hold the endpoint at the ink-standoff pose on that ray.
const ENDPOINT_POSE: f64 = 1.5;
/// Endpoints within this gap count as the same geometric terminal.
const COTERMINAL_PX: f64 = 8.0;

/// Coterminal arrows (same unordered mol pair, or shared endpoints even if
/// reversed) share one solved centerline, then paint as parallel lanes.
struct ArrowGroup {
    /// Edge indices; `members[0]` is the representative installed in the solver.
    members: Vec<usize>,
    /// True when that member's painted direction is opposite the centerline.
    reversed: Vec<bool>,
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

fn pt_dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn path_ends(pts: &[(f64, f64)]) -> Option<((f64, f64), (f64, f64))> {
    if pts.len() < 2 {
        return None;
    }
    Some((pts[0], pts[pts.len() - 1]))
}

/// `Some(reversed)` when `a` and `b` share terminals within [`COTERMINAL_PX`].
fn seeds_coterminal(a: &[(f64, f64)], b: &[(f64, f64)]) -> Option<bool> {
    let (a0, a1) = path_ends(a)?;
    let (b0, b1) = path_ends(b)?;
    let fwd = pt_dist(a0, b0) + pt_dist(a1, b1);
    let rev = pt_dist(a0, b1) + pt_dist(a1, b0);
    if fwd <= 2.0 * COTERMINAL_PX {
        Some(false)
    } else if rev <= 2.0 * COTERMINAL_PX {
        Some(true)
    } else {
        None
    }
}

fn member_reversed(
    member_seed: &[(f64, f64)],
    rep_seed: &[(f64, f64)],
    member_src: &str,
    member_tgt: &str,
    rep_src: &str,
    rep_tgt: &str,
) -> bool {
    // Mol pair is the source of truth: A→B vs B→A with identical seeds still reverse.
    if member_src == rep_tgt && member_tgt == rep_src && member_src != member_tgt {
        return true;
    }
    if member_src == rep_src && member_tgt == rep_tgt {
        return false;
    }
    if let Some(rev) = seeds_coterminal(member_seed, rep_seed) {
        return rev;
    }
    if let (Some((m0, m1)), Some((c0, c1))) = (path_ends(member_seed), path_ends(rep_seed)) {
        let fwd = pt_dist(m0, c0) + pt_dist(m1, c1);
        let rev = pt_dist(m0, c1) + pt_dist(m1, c0);
        if (rev - fwd).abs() > 1.0 {
            return rev < fwd;
        }
    }
    false
}

fn find_group(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        let p = parent[i];
        parent[i] = parent[p];
        i = p;
    }
    i
}

/// Partition edges that share an unordered mol pair or the same terminals.
fn coterminal_groups(edges: &[&EdgeNode], seed_pts: &[Vec<(f64, f64)>]) -> Vec<ArrowGroup> {
    let n = edges.len();
    if n == 0 {
        return Vec::new();
    }
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        let (si, ti) = edge_src_tgt(edges[i]);
        for j in (i + 1)..n {
            let (sj, tj) = edge_src_tgt(edges[j]);
            let same_pair = unordered_mol_pair(&si, &ti) == unordered_mol_pair(&sj, &tj)
                && !(si.is_empty() && ti.is_empty());
            let same_ends = seeds_coterminal(&seed_pts[i], &seed_pts[j]).is_some();
            if same_pair || same_ends {
                let a = find_group(&mut parent, i);
                let b = find_group(&mut parent, j);
                if a != b {
                    parent[b] = a;
                }
            }
        }
    }
    let mut buckets: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut order = Vec::new();
    for i in 0..n {
        let r = find_group(&mut parent, i);
        if !buckets.contains_key(&r) {
            order.push(r);
        }
        buckets.entry(r).or_default().push(i);
    }
    order
        .into_iter()
        .map(|r| {
            let members = buckets.remove(&r).unwrap_or_default();
            let rep = members[0];
            let (rs, rt) = edge_src_tgt(edges[rep]);
            let reversed: Vec<bool> = members
                .iter()
                .map(|&i| {
                    if i == rep {
                        false
                    } else {
                        let (s, t) = edge_src_tgt(edges[i]);
                        member_reversed(&seed_pts[i], &seed_pts[rep], &s, &t, &rs, &rt)
                    }
                })
                .collect();
            ArrowGroup { members, reversed }
        })
        .collect()
}

fn group_centerline(group: &ArrowGroup, seed_pts: &[Vec<(f64, f64)>]) -> Vec<(f64, f64)> {
    let template = seed_pts[group.members[0]].clone();
    if group.members.len() == 1 || template.is_empty() {
        return template;
    }
    let n = template.len();
    let mut acc = template.clone();
    let mut count = 1.0;
    for (&i, &rev) in group.members.iter().zip(&group.reversed).skip(1) {
        let mut pts = seed_pts[i].clone();
        if rev {
            pts.reverse();
        }
        if pts.len() != n {
            continue;
        }
        // Skip a reversed copy of the same points: averaging those collapses
        // the shaft to its midpoint.
        if seeds_coterminal(&pts, &template) != Some(false) {
            continue;
        }
        for (j, p) in pts.iter().enumerate() {
            acc[j].0 += p.0;
            acc[j].1 += p.1;
        }
        count += 1.0;
    }
    for p in &mut acc {
        p.0 /= count;
        p.1 /= count;
    }
    acc
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

fn replace_edge_points(world: &mut LayoutWorld, eid: &str, pts: &[(f64, f64)]) {
    let children = world
        .get(eid)
        .map(|n| n.children.clone())
        .unwrap_or_default();
    let mut keep = Vec::new();
    for c in children {
        let drop = world
            .get(&c)
            .map(|n| n.object_type == ObjectType::EdgePoint)
            .unwrap_or(false);
        if drop {
            world.nodes.remove(&c);
        } else {
            keep.push(c);
        }
    }
    if let Some(n) = world.get_mut(eid) {
        n.children = keep;
    }
    for (k, &(x, y)) in pts.iter().enumerate() {
        world.insert(
            format!("{eid}/p{k}"),
            eid,
            ObjectType::EdgePoint,
            Shape::Point { x, y },
            true,
        );
    }
}

/// Refined shaft polylines and optional label poses `(x, y, rotate_deg)`.
pub struct RefinedEdges {
    pub edge_paths: Vec<Option<Vec<(f64, f64)>>>,
    pub edge_labels: Vec<Option<(f64, f64, f64)>>,
}

pub fn refine_scheme_edges(
    spec: &DepictSpec,
    layout: &LayoutOpts,
    positions: &HashMap<String, (f64, f64)>,
    edge_paths: &[Option<Vec<(f64, f64)>>],
    paints: &[DocPaint],
    sizes: &[(String, f64, f64)],
) -> RefinedEdges {
    let mut out_paths = edge_paths.to_vec();
    while out_paths.len() < spec.edges().len() {
        out_paths.push(None);
    }
    let mut out_labels = vec![None; spec.edges().len()];

    let mut world = LayoutWorld::rooted("scheme", ObjectType::Scheme);
    let size_of: HashMap<&str, (f64, f64)> = sizes
        .iter()
        .map(|(id, w, h)| (id.as_str(), (*w, *h)))
        .collect();
    let paint_of: HashMap<&str, &DocPaint> = paints.iter().map(|p| (p.id.as_str(), p)).collect();

    let mut mol_ids = Vec::new();
    for (id, w, h) in sizes {
        let (x, y) = positions.get(id.as_str()).copied().unwrap_or((0.0, 0.0));
        let (w, h) = (*w, *h);
        let mid = format!("scheme/mols/{id}");
        world.insert(
            &mid,
            "scheme",
            ObjectType::Mol,
            Shape::Aabb(Aabb::new(x, y, x + w, y + h)),
            true,
        );
        if let Some(paint) = paint_of.get(id.as_str()) {
            let (bonds, atoms) = chem_in_document(paint, (x, y));
            let chem = super::layout_constraints::install_chem_under(
                &mut world,
                &mid,
                &bonds,
                &atoms,
                &[],
                &[],
            );
            // Glyphs and captions are ink, not atom disks. Install them as
            // leaves so Outside can keep shafts off CHO letters and names.
            install_ink_leaves(&mut world, &chem, scene_ink_shapes(paint, (x, y)));
        }
        let (cx, cy) = mol_centroid(&paint_of, positions, &size_of, id);
        world.insert(
            format!("{mid}/centroid"),
            &mid,
            ObjectType::Group,
            Shape::Point { x: cx, y: cy },
            true,
        );
        mol_ids.push(id.clone());
    }

    let mut cs = Vec::new();
    let mut hard = Vec::new();

    for (i, a) in mol_ids.iter().enumerate() {
        for b in mol_ids.iter().skip(i + 1) {
            hard.push(HardConstraint::Outside {
                a: format!("scheme/mols/{a}"),
                b: format!("scheme/mols/{b}"),
                clash_dist: layout.node_spacing_or_default(),
                lagrangian: LAGRANGIAN_CLEAR,
            });
        }
    }

    let edges_root = world.insert(
        "scheme/edges",
        "scheme",
        ObjectType::Group,
        Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
        true,
    );

    let mut seed_pts: Vec<Vec<(f64, f64)>> = Vec::with_capacity(spec.edges().len());
    for (i, edge) in spec.edges().iter().enumerate() {
        let src = edge
            .sources
            .as_slice()
            .first()
            .map(String::as_str)
            .unwrap_or("");
        let tgt = edge
            .targets
            .as_slice()
            .first()
            .map(String::as_str)
            .unwrap_or("");
        let src_box = mol_box(positions, &size_of, src);
        let tgt_box = mol_box(positions, &size_of, tgt);
        let pts = edge_paths
            .get(i)
            .and_then(|p| p.as_ref())
            .filter(|p| p.len() >= 2)
            .cloned()
            .unwrap_or_else(|| {
                let (a, b) = edge_anchors(src_box, tgt_box, ANCHOR_GAP);
                vec![a, b]
            });
        seed_pts.push(pts);
    }

    let groups = coterminal_groups(&spec.edges(), &seed_pts);
    let n_edges = spec.edges().len();
    let mut is_follower = vec![false; n_edges];
    let mut group_of = vec![0usize; n_edges];
    let mut centerlines = seed_pts.clone();
    for (gi, group) in groups.iter().enumerate() {
        for &i in &group.members {
            group_of[i] = gi;
        }
        if group.members.len() > 1 {
            centerlines[group.members[0]] = group_centerline(group, &seed_pts);
            for &i in group.members.iter().skip(1) {
                is_follower[i] = true;
            }
        }
    }

    for (i, edge) in spec.edges().iter().enumerate() {
        let eid = format!("scheme/edges/e{i}");
        world.insert(
            &eid,
            &edges_root,
            ObjectType::Edge,
            Shape::Polyline(vec![]),
            true,
        );
        if is_follower[i] {
            continue;
        }
        let src = edge
            .sources
            .as_slice()
            .first()
            .map(String::as_str)
            .unwrap_or("");
        let tgt = edge
            .targets
            .as_slice()
            .first()
            .map(String::as_str)
            .unwrap_or("");
        let pts = &centerlines[i];
        // Tip pose is ink + pad along the centroid ray. The solver holds that
        // with PreferPose, RayThrough, Outside vs chem, and Outside vs the
        // other tip (min shaft). ELK length is not a cap — leftover corridor
        // is shaft, so moving the mols lengthens the arrow.
        let src_c = mol_centroid(&paint_of, positions, &size_of, src);
        let tgt_c = mol_centroid(&paint_of, positions, &size_of, tgt);
        let src_chem = format!("scheme/mols/{src}/chem");
        let tgt_chem = format!("scheme/mols/{tgt}/chem");
        let min_shaft = min_shaft_px(edge.stroke_width);
        let (pose0, pose1) = if pts.len() >= 2 {
            shaft_endpoint_poses(&world, &src_chem, &tgt_chem, src_c, tgt_c, min_shaft)
        } else {
            (src_c, tgt_c)
        };
        // A 2-point chord that hugs both tips can still cut a ring or CHO.
        // Hinges at ¼ and ¾ keep each last chord on a centroid ray (so a
        // reversed coterminal lane still aims); the mid bows off chem.
        // Consecutive-point Outside used to force 8px gaps on a ~26px
        // corridor and the Catmull became a squiggle — do not add those.
        let bundle_n = groups[group_of[i]].members.len();
        let shaft_clash = SCHEME_INK_STANDOFF + group_half_width(bundle_n);
        let injected =
            pts.len() == 2 && !chord_clears_chem(&world, pose0, pose1, &mol_ids, shaft_clash);
        let ctrl: Vec<(f64, f64)> = if injected {
            vec![
                pose0,
                lerp2(pose0, pose1, 0.25),
                bow_seed(&world, pose0, pose1, &mol_ids, shaft_clash),
                lerp2(pose0, pose1, 0.75),
                pose1,
            ]
        } else {
            pts.clone()
        };
        let last = ctrl.len().saturating_sub(1);
        for (k, &(x, y)) in ctrl.iter().enumerate() {
            let mut pose = if ctrl.len() >= 2 && k == 0 {
                pose0
            } else if ctrl.len() >= 2 && k == last {
                pose1
            } else {
                (x, y)
            };
            let mut on_centroid_ray = k == 0 || k == last;
            // Last painted chord is the arrow. Distinct neighbors only exist
            // on 4+ point routes; a 3-point mid is the bend DOF, not a head
            // hinge (those two `k` values are the same point).
            if !injected && last >= 3 && k == 1 {
                pose = behind_end_on_ray(src_c, pose0, (x, y));
                on_centroid_ray = true;
            }
            if !injected && last >= 3 && k == last - 1 {
                pose = behind_end_on_ray(tgt_c, pose1, (x, y));
                on_centroid_ray = true;
            }
            world.insert(
                format!("{eid}/p{k}"),
                &eid,
                ObjectType::EdgePoint,
                Shape::Point {
                    x: pose.0,
                    y: pose.1,
                },
                false,
            );
            // Injected hinges: RayThrough + Outside only. PreferPose on the
            // ¾-chord parks the last hinge on CHO.
            let hinge = injected && k != 0 && k != last && k != 2;
            if !hinge {
                cs.push(Constraint::PreferPose {
                    a: format!("{eid}/p{k}"),
                    x: pose.0,
                    y: pose.1,
                    strength: if on_centroid_ray {
                        ENDPOINT_POSE
                    } else {
                        POSE_STRENGTH
                    },
                });
            }
        }
        if ctrl.len() >= 2 {
            let src_centroid = format!("scheme/mols/{src}/centroid");
            let tgt_centroid = format!("scheme/mols/{tgt}/centroid");
            let neighbor0 = format!("{eid}/p1");
            let neighbor1 = format!("{eid}/p{}", last.saturating_sub(1));
            cs.push(Constraint::RayThrough {
                a: format!("{eid}/p0"),
                b: neighbor0,
                p: src_centroid,
                strength: RAY_TO_CENTROID,
            });
            cs.push(Constraint::RayThrough {
                a: format!("{eid}/p{last}"),
                b: neighbor1,
                p: tgt_centroid,
                strength: RAY_TO_CENTROID,
            });
            let p0 = format!("{eid}/p0");
            let p_end = format!("{eid}/p{last}");
            // Tips: 2px floor so a hug pose cannot sit inside an atom disk.
            // The ½-bond hug is PreferPose along the ray — isotropic Outside
            // at that gap shoves off the ray.
            push_clear_leaves(&world, &mut hard, &p0, &src_chem, 2.0);
            push_clear_leaves(&world, &mut hard, &p_end, &tgt_chem, 2.0);
            // Chord vs every mol's painted leaves (disks, bonds, glyphs,
            // captions). Endpoint-only Outside lets a centroid ray cut a ring
            // or CHO between the tips. Inflate by the bundle half-width so
            // offset lanes still clear after the centerline solve.
            for id in &mol_ids {
                push_clear_leaves(
                    &world,
                    &mut hard,
                    &eid,
                    &format!("scheme/mols/{id}/chem"),
                    shaft_clash,
                );
            }
            hard.push(HardConstraint::Outside {
                a: p0,
                b: p_end,
                clash_dist: min_shaft,
                lagrangian: LAGRANGIAN_CLEAR,
            });
            // Keep hinges from collapsing onto the bow (Catmull loop).
            // 8px was too big for a ~26px down-flow corridor (forced a
            // zigzag). A few px is "not a stub" without eating the shaft.
            if last >= 2 {
                for k in 0..last {
                    hard.push(HardConstraint::Outside {
                        a: format!("{eid}/p{k}"),
                        b: format!("{eid}/p{}", k + 1),
                        clash_dist: CLOSE_CTRL_PX * 2.0,
                        lagrangian: LAGRANGIAN_CLEAR,
                    });
                }
            }
        }
    }

    for i in 0..n_edges {
        if is_follower[i] {
            continue;
        }
        let a = format!("scheme/edges/e{i}");
        for j in (i + 1)..n_edges {
            if is_follower[j] {
                continue;
            }
            let b = format!("scheme/edges/e{j}");
            let clash = SCHEME_EDGE_CLEARANCE
                + group_half_width(groups[group_of[i]].members.len())
                + group_half_width(groups[group_of[j]].members.len());
            hard.push(HardConstraint::ParallelClear {
                a: a.clone(),
                b: b.clone(),
                clash_dist: clash,
                lagrangian: LAGRANGIAN_CLEAR,
            });
            let sa = Shape::Polyline(centerlines[i].clone());
            let sb = Shape::Polyline(centerlines[j].clone());
            if !shapes_properly_cross(&sa, &sb) {
                cs.push(Constraint::PolyCross {
                    a: a.clone(),
                    b,
                    strength: 0.8,
                });
            }
        }
    }

    let opts = SolveOpts {
        max_iters: 120,
        noise: 0.0,
        epsilon_px: 1.0,
        ..SolveOpts::default()
    };
    let _ = solve(&mut world, &cs, &hard, &opts);

    // One solved centerline per coterminal group; lanes are a constant-width
    // offset so ParallelClear cannot fling a coincident reverse arrow.
    let mut group_centers: Vec<Vec<(f64, f64)>> = Vec::with_capacity(groups.len());
    for group in &groups {
        let rep = group.members[0];
        let eid = format!("scheme/edges/e{rep}");
        let center = match world.eval_shape(&eid) {
            Some(Shape::Polyline(p)) if p.len() >= 2 => p,
            _ => centerlines[rep].clone(),
        };
        let center = prune_close_controls(&center, CLOSE_CTRL_PX);
        group_centers.push(center.clone());
        let offs = lane_offsets(group.members.len());
        for (lane, &i) in group.members.iter().enumerate() {
            let mut pts = offset_polyline(&center, offs[lane]);
            if group.reversed[lane] {
                pts.reverse();
            }
            pts = prune_close_controls(&pts, CLOSE_CTRL_PX);
            if pts.len() >= 2 {
                replace_edge_points(&mut world, &format!("scheme/edges/e{i}"), &pts);
                out_paths[i] = Some(pts);
            }
        }
    }

    // Pose labels against the shared centerline, not a reverse member's
    // painted direction — otherwise above/below collapse into one corridor.
    let mut pose_ids: Vec<String> = vec![String::new(); groups.len()];
    for (gi, group) in groups.iter().enumerate() {
        if group.members.len() <= 1 {
            pose_ids[gi] = format!("scheme/edges/e{}", group.members[0]);
            continue;
        }
        let cid = format!("scheme/edges/g{gi}/center");
        world.insert(
            &cid,
            &edges_root,
            ObjectType::Group,
            Shape::Polyline(group_centers[gi].clone()),
            true,
        );
        pose_ids[gi] = cid;
    }

    let mut label_cs = Vec::new();
    let mut label_hard = Vec::new();
    let mut label_ids = Vec::new();
    for (i, edge) in spec.edges().iter().enumerate() {
        let eid = format!("scheme/edges/e{i}");
        let live = match world.eval_shape(&eid) {
            Some(Shape::Polyline(p)) if p.len() >= 2 => p,
            _ => seed_pts[i].clone(),
        };
        let gi = group_of[i];
        let bundle = groups.get(gi).map(|g| g.members.len()).unwrap_or(1);
        let seed_path = group_centers
            .get(gi)
            .filter(|p| p.len() >= 2)
            .unwrap_or(&live);
        let Some((px, py)) = edge_label_seed(spec, edge, seed_path, group_half_width(bundle))
        else {
            continue;
        };
        // Circle at the glyph center, not the unrotated text AABB: a long
        // word's box swallows a Catmull bulge so Outside cannot push it off.
        let lid = format!("{eid}/label");
        world.insert(
            &lid,
            &eid,
            ObjectType::EdgeLabel,
            Shape::Circle {
                cx: px,
                cy: py,
                r: LABEL_FONT_PX * 0.5,
            },
            false,
        );
        world.set_rot(&lid, readable_deg(tangent_at(seed_path, px, py)));
        let pose = pose_ids
            .get(gi)
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| eid.clone());
        label_ids.push((i, lid, pose, edge.stroke_width, group_half_width(bundle)));
    }
    for (i, lid, pose, self_stroke, bundle_half) in &label_ids {
        label_cs.push(Constraint::Midpoint {
            a: lid.clone(),
            b: pose.clone(),
            strength: POSE_STRENGTH,
        });
        label_cs.push(Constraint::Parallel {
            a: lid.clone(),
            b: pose.clone(),
            strength: 0.8,
        });
        label_hard.push(HardConstraint::MaxDist {
            a: lid.clone(),
            b: pose.clone(),
            dist: label_shaft_tether(*self_stroke) + bundle_half,
            lagrangian: LAGRANGIAN_TETHER,
        });
        for j in 0..n_edges {
            let clash = if j == *i {
                shaft_air(*self_stroke)
            } else {
                shaft_air(spec.edges().get(j).and_then(|e| e.stroke_width))
            };
            label_hard.push(HardConstraint::Outside {
                a: lid.clone(),
                b: format!("scheme/edges/e{j}"),
                clash_dist: clash,
                lagrangian: LAGRANGIAN_CLEAR,
            });
        }
        // This scheme's kit: painted leaves are obstacles (chem, other labels).
        // Not a solver default. Skip control points (the shaft polyline is the
        // ink), centroids, and shade.
        push_clear_leaves_where(
            &world,
            &mut label_hard,
            lid,
            "scheme",
            LABEL_SHAFT_PAD_PX,
            |id, n| scheme_label_clears_leaf(lid, id, n),
        );
    }
    if !label_cs.is_empty() || !label_hard.is_empty() {
        let _ = solve(&mut world, &label_cs, &label_hard, &opts);
    }

    for i in 0..n_edges {
        let eid = format!("scheme/edges/e{i}");
        if let Some(Shape::Polyline(pts)) = world.eval_shape(&eid) {
            if pts.len() >= 2 {
                out_paths[i] = Some(pts);
            }
        }
        let lid = format!("{eid}/label");
        if let Some(n) = world.get(&lid) {
            let s = n.world_shape();
            let (x, y) = s.aabb().center();
            // Parallel is mod 180°; pick the right-side-up glyph after the solve.
            out_labels[i] = Some((x, y, flip_upside_down(n.rot)));
        }
    }

    RefinedEdges {
        edge_paths: out_paths,
        edge_labels: out_labels,
    }
}

fn mol_box(
    positions: &HashMap<String, (f64, f64)>,
    sizes: &HashMap<&str, (f64, f64)>,
    id: &str,
) -> (f64, f64, f64, f64) {
    let (x, y) = positions.get(id).copied().unwrap_or((0.0, 0.0));
    let (w, h) = sizes.get(id).copied().unwrap_or((0.0, 0.0));
    (x, y, w, h)
}

fn mol_centroid(
    paints: &HashMap<&str, &DocPaint>,
    positions: &HashMap<String, (f64, f64)>,
    sizes: &HashMap<&str, (f64, f64)>,
    id: &str,
) -> (f64, f64) {
    if let Some(paint) = paints.get(id) {
        if !paint.molecule.atoms.is_empty() {
            let (x, y) = positions.get(id).copied().unwrap_or((0.0, 0.0));
            let n = paint.molecule.atoms.len() as f64;
            let sx: f64 = paint.molecule.atoms.iter().map(|a| a.x).sum();
            let sy: f64 = paint.molecule.atoms.iter().map(|a| a.y).sum();
            return chem_to_document(paint, (x, y), sx / n, sy / n);
        }
    }
    let (x, y, w, h) = mol_box(positions, sizes, id);
    (x + w * 0.5, y + h * 0.5)
}

fn lerp2(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))
}

/// Place `prev` on the ray from `centroid` through `end`, on the far side of
/// `end`, so the chord `prev → end` points at the centroid.
fn behind_end_on_ray(centroid: (f64, f64), end: (f64, f64), prev: (f64, f64)) -> (f64, f64) {
    let vx = end.0 - centroid.0;
    let vy = end.1 - centroid.1;
    let len = vx.hypot(vy).max(1e-9);
    let ux = vx / len;
    let uy = vy / len;
    let along = (prev.0 - end.0) * ux + (prev.1 - end.1) * uy;
    let t = along.max(BOND_PX);
    (end.0 + ux * t, end.1 + uy * t)
}

/// Scheme labels clear painted leaves. Control points are not ink (the edge
/// polyline is). Centroids and shade are not obstacles.
fn scheme_label_clears_leaf(label: &str, id: &str, n: &LayoutNode) -> bool {
    if id == label {
        return false;
    }
    !matches!(
        n.object_type,
        ObjectType::EdgePoint | ObjectType::Shade | ObjectType::Group
    )
}

fn min_ink_signed(world: &LayoutWorld, under: &str, p: (f64, f64)) -> f64 {
    if let Some(d) = world.ink_signed(under, p.0, p.1) {
        return d;
    }
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
    best
}

/// Axis-aligned step from this centroid toward the other mol.
///
/// Walking the diagonal `from → other` aims the hug through a side CHO
/// when centroids are offset. Dominant axis keeps the last chord on the
/// centroid while the attach sits on the facing ring lip.
fn facing_point(from: (f64, f64), other: (f64, f64)) -> (f64, f64) {
    if (other.1 - from.1).abs() >= (other.0 - from.0).abs() {
        (from.0, other.1)
    } else {
        (other.0, from.1)
    }
}

/// Tips hug ink by [`SCHEME_INK_STANDOFF`] on the facing centroid ray. If
/// that would cross, sit on the ink lip so the shaft still fills the corridor.
fn shaft_endpoint_poses(
    world: &LayoutWorld,
    src_chem: &str,
    tgt_chem: &str,
    src_c: (f64, f64),
    tgt_c: (f64, f64),
    min_shaft: f64,
) -> ((f64, f64), (f64, f64)) {
    let src_toward = facing_point(src_c, tgt_c);
    let tgt_toward = facing_point(tgt_c, src_c);
    let span = (tgt_c.0 - src_c.0).hypot(tgt_c.1 - src_c.1);
    let ux = (tgt_c.0 - src_c.0) / span.max(1e-9);
    let uy = (tgt_c.1 - src_c.1) / span.max(1e-9);
    let along = |p: (f64, f64)| (p.0 - src_c.0) * ux + (p.1 - src_c.1) * uy;
    let at = |t: f64| (src_c.0 + ux * t, src_c.1 + uy * t);
    let p0 = ray_ink_pose(
        world,
        src_chem,
        src_c,
        src_toward,
        SCHEME_INK_STANDOFF,
        span,
    );
    let p1 = ray_ink_pose(
        world,
        tgt_chem,
        tgt_c,
        tgt_toward,
        SCHEME_INK_STANDOFF,
        span,
    );
    if along(p1) - along(p0) >= min_shaft {
        return (p0, p1);
    }
    let q0 = ray_ink_pose(world, src_chem, src_c, src_toward, 2.0, span);
    let q1 = ray_ink_pose(world, tgt_chem, tgt_c, tgt_toward, 2.0, span);
    if along(q1) - along(q0) >= min_shaft {
        return (q0, q1);
    }
    let mid = span * 0.5;
    let half = min_shaft * 0.5;
    (at(mid - half), at(mid + half))
}

/// Walk from `centroid` toward `toward` until chem signed distance ≥ `clash`.
fn ray_ink_pose(
    world: &LayoutWorld,
    chem: &str,
    centroid: (f64, f64),
    toward: (f64, f64),
    clash: f64,
    max_t: f64,
) -> (f64, f64) {
    let dx = toward.0 - centroid.0;
    let dy = toward.1 - centroid.1;
    let l = dx.hypot(dy).max(1e-9);
    let ux = dx / l;
    let uy = dy / l;
    let cap = max_t.max(clash);
    let at = |t: f64| (centroid.0 + ux * t, centroid.1 + uy * t);
    if world.get(chem).is_none() || !min_ink_signed(world, chem, at(0.0)).is_finite() {
        return at(clash.min(cap));
    }
    if min_ink_signed(world, chem, at(cap)) < clash {
        return at(cap);
    }
    let mut lo = 0.0;
    let mut hi = cap;
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if min_ink_signed(world, chem, at(mid)) >= clash {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    at(hi)
}

/// Document-space origin of unframed atom coords: ELK + mol group + crop
/// (viewBox origin) + scaled [`crate::depict::frame_offset`].
fn mol_frame_origin(paint: &DocPaint, elk: (f64, f64)) -> (f64, f64) {
    let (lx, ly) = paint
        .scene
        .viewports
        .first()
        .map(|vp| vp.local_origin())
        .unwrap_or((0.0, 0.0));
    let leftover = (elk.0 + lx - paint.scene.x, elk.1 + ly - paint.scene.y);
    let (fx, fy) = crate::depict::frame_offset(&paint.molecule);
    let s = paint.molecule.diagram_scale();
    (leftover.0 + s * fx, leftover.1 + s * fy)
}

fn chem_to_document(paint: &DocPaint, elk: (f64, f64), ax: f64, ay: f64) -> (f64, f64) {
    let (ox, oy) = mol_frame_origin(paint, elk);
    let s = paint.molecule.diagram_scale();
    (ox + s * ax, oy + s * ay)
}

fn chem_in_document(
    paint: &DocPaint,
    elk: (f64, f64),
) -> (Vec<(f64, f64, f64, f64)>, Vec<(usize, f64, f64)>) {
    let mol: &MoleculeIn = &paint.molecule;
    let mut by_idx: HashMap<i32, (f64, f64)> = HashMap::new();
    let mut atoms = Vec::with_capacity(mol.atoms.len());
    for a in &mol.atoms {
        let p = chem_to_document(paint, elk, a.x, a.y);
        by_idx.insert(a.index, p);
        atoms.push((a.index.max(0) as usize, p.0, p.1));
    }
    let mut bonds = Vec::with_capacity(mol.bonds.len());
    for b in &mol.bonds {
        let Some(&(x0, y0)) = by_idx.get(&b.begin) else {
            continue;
        };
        let Some(&(x1, y1)) = by_idx.get(&b.end) else {
            continue;
        };
        bonds.push((x0, y0, x1, y1));
    }
    (bonds, atoms)
}

/// Seed a bend off the chord, on the side with more chem air.
/// Off-chord PreferPose seed for the free mid (side with more chem air).
fn bow_seed(
    world: &LayoutWorld,
    p0: (f64, f64),
    p1: (f64, f64),
    mol_ids: &[String],
    clash: f64,
) -> (f64, f64) {
    let mid = ((p0.0 + p1.0) * 0.5, (p0.1 + p1.1) * 0.5);
    let vx = p1.0 - p0.0;
    let vy = p1.1 - p0.1;
    let len = vx.hypot(vy).max(1e-9);
    let nx = -vy / len;
    let ny = vx / len;
    // Cap the seed offset to the corridor so a ½-bond clash on a short
    // down-flow pair is a C, not a detour wider than the shaft.
    let off = clash.min(len * 0.35).max(COTERMINAL_LANE_PX);
    let left = (mid.0 + nx * off, mid.1 + ny * off);
    let right = (mid.0 - nx * off, mid.1 - ny * off);
    let d_left = mol_ids
        .iter()
        .map(|id| min_ink_signed(world, &format!("scheme/mols/{id}/chem"), left))
        .fold(f64::INFINITY, f64::min);
    let d_right = mol_ids
        .iter()
        .map(|id| min_ink_signed(world, &format!("scheme/mols/{id}/chem"), right))
        .fold(f64::INFINITY, f64::min);
    if d_left >= d_right {
        left
    } else {
        right
    }
}

/// True when interior samples of `p0→p1` already sit `clash` from chem ink.
///
/// Endpoints (`t` near 0/1) are the hug poses, so they are skipped. A CHO or
/// ring between the mols still sits on `t ∈ (⅛, ⅞)`.
fn chord_clears_chem(
    world: &LayoutWorld,
    p0: (f64, f64),
    p1: (f64, f64),
    mol_ids: &[String],
    clash: f64,
) -> bool {
    for k in 1..8 {
        let t = k as f64 / 8.0;
        let p = (p0.0 + t * (p1.0 - p0.0), p0.1 + t * (p1.1 - p0.1));
        for id in mol_ids {
            let d = min_ink_signed(world, &format!("scheme/mols/{id}/chem"), p);
            if d < clash - 0.5 {
                return false;
            }
        }
    }
    true
}

/// Raster + geometric leaves for painted glyphs / captions under `chem`.
fn install_ink_leaves(world: &mut LayoutWorld, chem: &str, stamps: Vec<Shape>) {
    world.add_ink(chem, stamps.clone());
    for (k, shape) in stamps.into_iter().enumerate() {
        world.insert(
            format!("{chem}/ink/{k}"),
            chem,
            ObjectType::AtomText,
            shape,
            true,
        );
    }
}

fn skip_ink_class(class: Option<&str>) -> bool {
    let Some(c) = class else {
        return false;
    };
    c.split_whitespace()
        .any(|t| t == "halo" || t.contains("debug-box") || t.contains("debug_box"))
}

fn path_is_filled(fill: Option<&str>) -> bool {
    matches!(fill, Some(s) if !s.is_empty() && !s.eq_ignore_ascii_case("none"))
}

pub(crate) fn prim_ink_shapes(p: &Primitive, ox: f64, oy: f64) -> Vec<Shape> {
    match p {
        Primitive::Circle {
            cx, cy, r, class, ..
        } => {
            if skip_ink_class(class.as_deref()) {
                return Vec::new();
            }
            vec![Shape::Circle {
                cx: cx + ox,
                cy: cy + oy,
                r: *r,
            }]
        }
        Primitive::Path { d, fill, class, .. } => {
            if skip_ink_class(class.as_deref()) {
                return Vec::new();
            }
            // Stroke-only paths (bonds) are already Line leaves under chem.
            // Stamping them as a filled AABB bloated diagonal bonds into slabs.
            if !path_is_filled(fill.as_deref()) {
                return Vec::new();
            }
            super::chrome_paint::path_aabb(d)
                .map(|a| Shape::Aabb(a.translated(ox, oy)))
                .into_iter()
                .collect()
        }
        Primitive::Text {
            x,
            y,
            text,
            font_size,
            anchor,
            class,
            ..
        } => {
            if skip_ink_class(class.as_deref()) {
                return Vec::new();
            }
            let w = text.chars().count() as f64 * *font_size * 0.55;
            let h = *font_size;
            let (x0, y0) = match anchor {
                TextAnchor::Start => (*x, *y - h * 0.8),
                TextAnchor::Middle => (*x - w * 0.5, *y - h * 0.5),
                TextAnchor::End => (*x - w, *y - h * 0.8),
            };
            vec![Shape::Aabb(Aabb::new(
                x0 + ox,
                y0 + oy,
                x0 + ox + w,
                y0 + oy + h,
            ))]
        }
    }
}

/// Labels, regions, captions, circles — everything painted except white halo.
fn scene_ink_shapes(paint: &DocPaint, elk: (f64, f64)) -> Vec<Shape> {
    let (lx, ly) = paint
        .scene
        .viewports
        .first()
        .map(|vp| vp.local_origin())
        .unwrap_or((0.0, 0.0));
    let (ox, oy) = (elk.0 + lx - paint.scene.x, elk.1 + ly - paint.scene.y);
    let mut out = Vec::new();
    for vp in &paint.scene.viewports {
        for layer in &vp.layers {
            if layer.name == LayerName::Halo {
                continue;
            }
            for p in &layer.primitives {
                out.extend(prim_ink_shapes(p, ox, oy));
            }
        }
    }
    for p in &paint.scene.overlays {
        out.extend(prim_ink_shapes(p, ox, oy));
    }
    out
}

fn edge_label_seed(
    spec: &DepictSpec,
    edge: &super::EdgeNode,
    pts: &[(f64, f64)],
    bundle_half: f64,
) -> Option<(f64, f64)> {
    let lab = edge.label.as_ref()?;
    let placements = lab.placements();
    let (id, pos) = placements.first()?;
    let text = match spec.node_by_id(id.trim()) {
        Some(super::Node::Text(t)) => t.text.clone(),
        _ => return None,
    };
    if text.trim().is_empty() {
        return None;
    }
    let painted = crate::catmull::sample_open_catmull(pts, 8);
    let (mx, my, _, _) = label_point(&painted);
    let (px, py) = label_offset_dir(&painted, *pos);
    let off = label_shaft_clash(edge.stroke_width) + bundle_half;
    Some((mx + px * off, my + py * off))
}

/// Unit perpendicular for `pos`. Above/below follow the page (screen-up), not
/// the painted direction, so a reverse B→A "below" is opposite A→B "above".
fn label_offset_dir(pts: &[(f64, f64)], pos: super::LabelPos) -> (f64, f64) {
    let (_, _, px, py) = label_point(pts);
    let p1 = (px, py);
    let p2 = (-px, -py);
    let up = |p: (f64, f64)| -p.1;
    let left = |p: (f64, f64)| -p.0;
    match pos {
        super::LabelPos::Above => {
            if up(p1) >= up(p2) {
                p1
            } else {
                p2
            }
        }
        super::LabelPos::Below => {
            if up(p1) >= up(p2) {
                p2
            } else {
                p1
            }
        }
        super::LabelPos::Left => {
            if left(p1) >= left(p2) {
                p1
            } else {
                p2
            }
        }
        super::LabelPos::Right => {
            if left(p1) >= left(p2) {
                p2
            } else {
                p1
            }
        }
    }
}

fn label_point(pts: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    if pts.len() < 2 {
        let (x, y) = pts.first().copied().unwrap_or((0.0, 0.0));
        return (x, y, 0.0, -1.0);
    }
    let mut best_i = 1;
    let mut best_len = -1.0;
    for i in 1..pts.len() {
        let l = (pts[i].0 - pts[i - 1].0).hypot(pts[i].1 - pts[i - 1].1);
        if l > best_len {
            best_len = l;
            best_i = i;
        }
    }
    let (x1, y1) = pts[best_i - 1];
    let (x2, y2) = pts[best_i];
    let l = (x2 - x1).hypot(y2 - y1).max(1e-9);
    let ux = (x2 - x1) / l;
    let uy = (y2 - y1) / l;
    ((x1 + x2) * 0.5, (y1 + y2) * 0.5, -uy, ux)
}

fn tangent_deg(pts: &[(f64, f64)]) -> f64 {
    if pts.len() < 2 {
        return 0.0;
    }
    let i = (pts.len() / 2).max(1);
    let (x0, y0) = pts[i - 1];
    let (x1, y1) = pts[i.min(pts.len() - 1)];
    (y1 - y0).atan2(x1 - x0).to_degrees()
}

/// Tangent of the painted shaft (Catmull samples) at the closest point to `(x, y)`.
fn tangent_at(pts: &[(f64, f64)], x: f64, y: f64) -> f64 {
    let painted = crate::catmull::sample_open_catmull(pts, 8);
    if painted.len() < 2 {
        return tangent_deg(pts);
    }
    let mut best_i = 1;
    let mut best_d = f64::INFINITY;
    for i in 1..painted.len() {
        let (x0, y0) = painted[i - 1];
        let (x1, y1) = painted[i];
        let vx = x1 - x0;
        let vy = y1 - y0;
        let l2 = vx * vx + vy * vy;
        let t = if l2 < 1e-12 {
            0.0
        } else {
            ((x - x0) * vx + (y - y0) * vy) / l2
        }
        .clamp(0.0, 1.0);
        let px = x0 + t * vx;
        let py = y0 + t * vy;
        let d = (x - px).hypot(y - py);
        if d < best_d {
            best_d = d;
            best_i = i;
        }
    }
    let (x0, y0) = painted[best_i - 1];
    let (x1, y1) = painted[best_i];
    (y1 - y0).atan2(x1 - x0).to_degrees()
}

/// Parallel to the edge, but never upside-down (`(-90, 90]` degrees).
fn readable_deg(deg: f64) -> f64 {
    flip_upside_down(deg)
}

/// After the solve: if the glyph is upside-down, rotate 180°.
fn flip_upside_down(deg: f64) -> f64 {
    let mut a = ((deg + 180.0) % 360.0 + 360.0) % 360.0 - 180.0;
    if a > 90.0 {
        a -= 180.0;
    } else if a < -90.0 {
        a += 180.0;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::layout_constraints::Shape;
    use crate::doc::layout_constraints::{
        hard_g, HardConstraint, LAGRANGIAN_CLEAR, LAGRANGIAN_TETHER,
    };
    use crate::doc::layout_items::ATOM_DISK_R_PX;

    #[test]
    fn ink_standoff_constant_is_half_bond() {
        assert!((SCHEME_INK_STANDOFF - 0.5 * BOND_PX).abs() < 1e-12);
    }

    #[test]
    fn framed_centroid_sits_inside_elk_box() {
        let a_spec: crate::doc::DepictSpec =
            serde_json::from_str(r#"{"type":"mol","id":"A","smiles":"CCO"}"#).unwrap();
        let a = crate::doc::render(&a_spec).unwrap().remove(0);
        let paint = crate::doc::DocPaint {
            id: a.id.clone(),
            molecule: a.molecule.clone(),
            scene: a.scene.clone(),
            frame_json: a.frame_molblock.clone(),
        };
        let w = a.scene.width;
        let h = a.scene.height;
        let paints = HashMap::from([("A", &paint)]);
        let positions = HashMap::from([("A".into(), (0.0, 0.0))]);
        let sizes = HashMap::from([("A", (w, h))]);
        let c = mol_centroid(&paints, &positions, &sizes, "A");
        assert!(
            c.0 >= -1.0 && c.0 <= w + 1.0 && c.1 >= -1.0 && c.1 <= h + 1.0,
            "framed centroid {c:?} must sit in the ELK box 0..{w} x 0..{h}"
        );
    }

    #[test]
    fn readable_deg_never_upside_down() {
        assert!((readable_deg(0.0)).abs() < 1e-9);
        assert!((readable_deg(180.0)).abs() < 1e-9);
        assert!((readable_deg(-176.0) - 4.0).abs() < 1e-9);
        assert!(readable_deg(135.0) < 0.0 && readable_deg(135.0) > -90.0);
        for d in [-200.0, -90.0, 0.0, 45.0, 90.0, 91.0, 180.0, 270.0] {
            let r = readable_deg(d);
            assert!((-90.0..=90.0).contains(&r), "{d} -> {r}");
        }
    }

    #[test]
    fn flip_upside_down_is_a_180_turn() {
        assert!((flip_upside_down(135.0) - (135.0 - 180.0)).abs() < 1e-9);
        assert!((flip_upside_down(-135.0) - (-135.0 + 180.0)).abs() < 1e-9);
        assert!((flip_upside_down(45.0) - 45.0).abs() < 1e-9);
        assert!((-90.0..=90.0).contains(&flip_upside_down(91.0)));
    }

    #[test]
    fn real_scheme_keeps_elk_direction() {
        let spec: crate::doc::DepictSpec = serde_json::from_str(
            r#"{
              "type": "reaction_scheme",
              "children": [
                {"type": "text", "id": "adh", "text": "ADH"},
                {"type": "mol", "id": "A", "smiles": "CCO"},
                {"type": "edge", "source": "A", "target": "B", "label": "adh", "arrow": "forward"},
                {"type": "mol", "id": "B", "smiles": "CC=O"}
              ]
            }"#,
        )
        .unwrap();
        let a = crate::doc::render(
            &serde_json::from_str(r#"{"type":"mol","id":"A","smiles":"CCO"}"#).unwrap(),
        )
        .unwrap()
        .remove(0);
        let b = crate::doc::render(
            &serde_json::from_str(r#"{"type":"mol","id":"B","smiles":"CC=O"}"#).unwrap(),
        )
        .unwrap()
        .remove(0);
        let paints = vec![
            crate::doc::DocPaint {
                id: a.id.clone(),
                molecule: a.molecule.clone(),
                scene: a.scene.clone(),
                frame_json: a.frame_molblock.clone(),
            },
            crate::doc::DocPaint {
                id: b.id.clone(),
                molecule: b.molecule.clone(),
                scene: b.scene.clone(),
                frame_json: b.frame_molblock.clone(),
            },
        ];
        let sizes: Vec<(String, f64, f64)> = paints
            .iter()
            .map(|p| (p.id.clone(), p.scene.width, p.scene.height))
            .collect();
        let gap = 80.0;
        let positions = HashMap::from([
            ("A".into(), (0.0, 0.0)),
            ("B".into(), (sizes[0].1 + gap, 0.0)),
        ]);
        let seed = vec![
            (sizes[0].1, sizes[0].2 * 0.5),
            (positions["B"].0, sizes[1].2 * 0.5),
        ];
        let refined = refine_scheme_edges(
            &spec,
            &LayoutOpts::default(),
            &positions,
            &[Some(seed.clone())],
            &paints,
            &sizes,
        );
        let pts = refined.edge_paths[0].clone().expect("path");
        let (x0, y0) = pts[0];
        let (x1, y1) = pts[pts.len() - 1];
        let sdx = seed[1].0 - seed[0].0;
        let sdy = seed[1].1 - seed[0].1;
        let dx = x1 - x0;
        let dy = y1 - y0;
        let paint_of: HashMap<&str, &DocPaint> =
            paints.iter().map(|p| (p.id.as_str(), p)).collect();
        let size_of: HashMap<&str, (f64, f64)> = sizes
            .iter()
            .map(|(id, w, h)| (id.as_str(), (*w, *h)))
            .collect();
        let ca = mol_centroid(&paint_of, &positions, &size_of, "A");
        let cb = mol_centroid(&paint_of, &positions, &size_of, "B");
        let va = &paints[0].scene.viewports[0];
        let vb = &paints[1].scene.viewports[0];
        let (fa, fb) = (
            crate::depict::frame_offset(&paints[0].molecule),
            crate::depict::frame_offset(&paints[1].molecule),
        );
        assert!(
            dx * sdx + dy * sdy > 0.0,
            "refined shaft reversed vs ELK: seed={seed:?} pts={pts:?} ca={ca:?} cb={cb:?} leftoverA=({},{}) oxA=({},{}) frameA={fa:?} leftoverB=({},{}) frameB={fb:?}",
            va.x, va.y, va.ox, va.oy, vb.x, vb.y
        );
        let da0 = paint_ink_signed(&paints[0], positions["A"], (x0, y0));
        let db1 = paint_ink_signed(&paints[1], positions["B"], (x1, y1));
        assert!(
            da0 + 4.0 >= SCHEME_INK_STANDOFF,
            "src should hug A's ink by ~½ bond: {da0} pts={pts:?} ca={ca:?}"
        );
        assert!(
            db1 + 4.0 >= SCHEME_INK_STANDOFF,
            "tgt should hug B's ink by ~½ bond: {db1} pts={pts:?} cb={cb:?}"
        );
        let shaft = (x1 - x0).hypot(y1 - y0);
        assert!(
            shaft > min_shaft_px(None),
            "shaft should span chem-to-chem, not a min_shaft stub: shaft={shaft} pts={pts:?}"
        );
        let line_a = point_to_line(x0, y0, x1, y1, ca.0, ca.1);
        let line_b = point_to_line(x0, y0, x1, y1, cb.0, cb.1);
        assert!(
            line_a < 8.0 && line_b < 8.0,
            "shaft ray should pass near both centroids: dA={line_a} dB={line_b} pts={pts:?} ca={ca:?} cb={cb:?}"
        );
        let (_, _, rot) = refined.edge_labels[0].expect("label");
        assert!(
            (-90.0..=90.0).contains(&rot),
            "label must not be upside down: {rot}"
        );
    }

    fn paint_ink_signed(paint: &DocPaint, elk: (f64, f64), p: (f64, f64)) -> f64 {
        let (bonds, atoms) = chem_in_document(paint, elk);
        let pt = Shape::Point { x: p.0, y: p.1 };
        let mut best = f64::INFINITY;
        for &(x0, y0, x1, y1) in &bonds {
            best = best.min(edge_signed_distance(&pt, &Shape::Line { x0, y0, x1, y1 }));
        }
        for &(_, x, y) in &atoms {
            best = best.min(edge_signed_distance(
                &pt,
                &Shape::Circle {
                    cx: x,
                    cy: y,
                    r: ATOM_DISK_R_PX,
                },
            ));
        }
        for s in scene_ink_shapes(paint, elk) {
            best = best.min(edge_signed_distance(&pt, &s));
        }
        best
    }

    #[test]
    fn ray_ink_pose_stops_at_ink_not_centroid() {
        let mut world = LayoutWorld::rooted("scheme", ObjectType::Scheme);
        world.insert(
            "mol",
            "scheme",
            ObjectType::Mol,
            Shape::Aabb(Aabb::new(0.0, 0.0, 200.0, 80.0)),
            true,
        );
        crate::doc::layout_constraints::install_chem_under(
            &mut world,
            "mol",
            &[(0.0, 40.0, 120.0, 40.0)],
            &[(0, 0.0, 40.0), (1, 120.0, 40.0)],
            &[],
            &[],
        );
        let centroid = (60.0, 40.0);
        let p = ray_ink_pose(&world, "mol/chem", centroid, (200.0, 40.0), 30.0, 180.0);
        let d = min_ink_signed(&world, "mol/chem", p);
        assert!(
            (d - 30.0).abs() < 4.0,
            "pose should sit 30px from the chain ink, not from the centroid: p={p:?} d={d}"
        );
        assert!(p.0 > 120.0, "must walk past the protruding atom: {p:?}");
    }

    fn refine_pair_at_gap(gap: f64) -> Vec<(f64, f64)> {
        let spec = scheme(vec![
            mol_node("A"),
            edge_node("A", "B", None),
            mol_node("B"),
        ]);
        let aw = 80.0;
        let positions = HashMap::from([("A".into(), (0.0, 0.0)), ("B".into(), (aw + gap, 0.0))]);
        let sizes = vec![("A".into(), aw, 80.0), ("B".into(), aw, 80.0)];
        let paints = vec![
            chem_paint("A", aw, 80.0, 40.0, 40.0),
            chem_paint("B", aw, 80.0, 40.0, 40.0),
        ];
        let seed = vec![(aw, 40.0), (aw + gap, 40.0)];
        refine_scheme_edges(
            &spec,
            &LayoutOpts::default(),
            &positions,
            &[Some(seed)],
            &paints,
            &sizes,
        )
        .edge_paths[0]
            .clone()
            .expect("path")
    }

    #[test]
    fn shaft_grows_when_mols_move_apart() {
        let tight = refine_pair_at_gap(80.0);
        let roomy = refine_pair_at_gap(200.0);
        let len = |pts: &[(f64, f64)]| {
            let a = pts[0];
            let b = pts[pts.len() - 1];
            (b.0 - a.0).hypot(b.1 - a.1)
        };
        let d_tight = len(&tight);
        let d_roomy = len(&roomy);
        assert!(
            d_roomy > d_tight + 80.0,
            "extra mol gap must become shaft, not air: tight={d_tight} roomy={d_roomy}"
        );
        assert!(
            (d_roomy - d_tight - 120.0).abs() < 24.0,
            "200−80 gap should show up on the shaft: tight={d_tight} roomy={d_roomy}"
        );
    }

    #[test]
    fn caption_on_the_ray_is_ink_but_leftover_is_shaft() {
        let spec = scheme(vec![
            mol_node("A"),
            edge_node("A", "B", None),
            mol_node("B"),
        ]);
        let layout = LayoutOpts::default();
        let positions = HashMap::from([("A".into(), (0.0, 0.0)), ("B".into(), (0.0, 280.0))]);
        let sizes = vec![("A".into(), 100.0, 100.0), ("B".into(), 100.0, 100.0)];
        let mut paints = vec![
            chem_paint("A", 100.0, 100.0, 50.0, 40.0),
            chem_paint("B", 100.0, 100.0, 50.0, 60.0),
        ];
        paints[0].scene.overlays.push(Primitive::Path {
            d: "M 20 80 L 80 80 L 80 100 L 20 100 Z".into(),
            stroke: None,
            fill: Some("#111".into()),
            stroke_width: 0.0,
            opacity: 1.0,
            stroke_dasharray: None,
            stroke_linecap: None,
            class: Some("mol-caption".into()),
            data_text: Some("phenol".into()),
        });
        let seed = vec![(50.0, 100.0), (50.0, 280.0)];
        let refined = refine_scheme_edges(
            &spec,
            &layout,
            &positions,
            &[Some(seed.clone())],
            &paints,
            &sizes,
        );
        let pts = refined.edge_paths[0].clone().expect("path");
        let (x0, y0) = pts[0];
        let (x1, y1) = pts[pts.len() - 1];
        let da = paint_ink_signed(&paints[0], positions["A"], (x0, y0));
        assert!(
            y0 > 100.0,
            "src end should sit past the caption, not on it: pts={pts:?}"
        );
        assert!(
            da + 2.0 >= SCHEME_INK_STANDOFF,
            "caption is ink: tip should hug it, not sit on the lip: da={da} pts={pts:?}"
        );
        assert!(
            (y1 - y0).abs() > 80.0,
            "leftover corridor is shaft: pts={pts:?}"
        );
        assert!(
            (x1 - x0).abs() < 12.0,
            "down-flow shaft should stay near vertical: pts={pts:?}"
        );
    }

    #[test]
    fn scene_ink_includes_heteroatom_glyphs() {
        let a = crate::doc::render(
            &serde_json::from_str(r#"{"type":"mol","id":"A","smiles":"CCO"}"#).unwrap(),
        )
        .unwrap()
        .remove(0);
        let paint = crate::doc::DocPaint {
            id: a.id.clone(),
            molecule: a.molecule.clone(),
            scene: a.scene.clone(),
            frame_json: a.frame_molblock.clone(),
        };
        let labels = paint
            .scene
            .viewports
            .first()
            .unwrap()
            .layers
            .iter()
            .find(|l| l.name == LayerName::Labels)
            .expect("labels");
        let has_oh = labels.primitives.iter().any(|p| {
            matches!(
                p,
                Primitive::Path {
                    data_text: Some(t),
                    ..
                } if t.contains('O')
            )
        });
        assert!(has_oh, "ethanol should paint an O/OH glyph");
        let shapes = scene_ink_shapes(&paint, (0.0, 0.0));
        assert!(
            shapes
                .iter()
                .any(|s| matches!(s, Shape::Aabb(a) if a.width() > 4.0 && a.height() > 4.0)),
            "glyph AABBs should be scheme-arrow ink: {shapes:?}"
        );
        let paints = HashMap::new();
        let positions = HashMap::from([("A".into(), (10.0, 20.0))]);
        let sizes = HashMap::from([("A", (40.0, 30.0))]);
        let c = mol_centroid(&paints, &positions, &sizes, "A");
        assert!(
            (c.0 - 30.0).abs() < 1e-9 && (c.1 - 35.0).abs() < 1e-9,
            "{c:?}"
        );
    }

    #[test]
    fn stroke_paths_are_not_filled_ink_stamps() {
        let stroke: Primitive =
            serde_json::from_str(r##"{"kind":"path","d":"M 0 0 L 100 80","stroke":"#000"}"##)
                .unwrap();
        assert!(
            prim_ink_shapes(&stroke, 0.0, 0.0).is_empty(),
            "bond strokes are Line leaves, not filled AABBs"
        );
        let filled: Primitive =
            serde_json::from_str(r##"{"kind":"path","d":"M 0 0 L 10 0 L 10 12 Z","fill":"#000"}"##)
                .unwrap();
        assert!(
            !prim_ink_shapes(&filled, 0.0, 0.0).is_empty(),
            "filled glyphs/wedges stay as ink AABBs"
        );
    }

    #[test]
    fn label_tether_is_hard_maxdist() {
        let h = HardConstraint::MaxDist {
            a: "e0/label".into(),
            b: "e0".into(),
            dist: label_shaft_tether(None),
            lagrangian: LAGRANGIAN_TETHER,
        };
        let mut w = LayoutWorld::rooted("scheme", ObjectType::Scheme);
        w.insert(
            "e0",
            "scheme",
            ObjectType::Edge,
            Shape::Polyline(vec![(0.0, 0.0), (40.0, 0.0)]),
            true,
        );
        w.insert(
            "e0/label",
            "e0",
            ObjectType::EdgeLabel,
            Shape::Aabb(Aabb::new(80.0, 40.0, 100.0, 52.0)),
            false,
        );
        assert!(hard_g(&w, &h) > 0.0);
    }

    #[test]
    fn parallel_clear_row_is_hard() {
        let h = HardConstraint::ParallelClear {
            a: "e0".into(),
            b: "e1".into(),
            clash_dist: SCHEME_EDGE_CLEARANCE,
            lagrangian: LAGRANGIAN_CLEAR,
        };
        let mut w = LayoutWorld::rooted("scheme", ObjectType::Scheme);
        w.insert(
            "e0",
            "scheme",
            ObjectType::Edge,
            Shape::Polyline(vec![(0.0, 0.0), (40.0, 0.0)]),
            true,
        );
        w.insert(
            "e1",
            "scheme",
            ObjectType::Edge,
            Shape::Polyline(vec![(0.0, 2.0), (40.0, 2.0)]),
            true,
        );
        assert!(hard_g(&w, &h) > 0.0);
    }

    #[test]
    fn lane_offsets_straddle_the_centerline() {
        assert_eq!(lane_offsets(1), vec![0.0]);
        let two = lane_offsets(2);
        assert!((two[1] - two[0] - COTERMINAL_LANE_PX).abs() < 1e-9);
        assert!((two[0] + two[1]).abs() < 1e-9);
        let three = lane_offsets(3);
        assert!((three[1]).abs() < 1e-9);
        assert!((three[2] - three[0] - 2.0 * COTERMINAL_LANE_PX).abs() < 1e-9);
    }

    #[test]
    fn reverse_member_follows_mol_pair_not_identical_seed() {
        let seed = vec![(0.0, 0.0), (40.0, 0.0)];
        assert!(member_reversed(&seed, &seed, "B", "A", "A", "B"));
        assert!(!member_reversed(&seed, &seed, "A", "B", "A", "B"));
        let rev = vec![(40.0, 0.0), (0.0, 0.0)];
        assert!(member_reversed(&rev, &seed, "B", "A", "A", "B"));
    }

    fn mol_node(id: &str) -> crate::doc::Node {
        crate::doc::Node::Mol(crate::doc::MolNode {
            id: Some(id.into()),
            smiles: Some("C".into()),
            ..Default::default()
        })
    }

    fn edge_node(src: &str, tgt: &str, label: Option<&str>) -> crate::doc::Node {
        crate::doc::Node::Edge(crate::doc::EdgeNode {
            type_: crate::doc::EdgeNodeKind::Edge,
            sources: crate::doc::MolIds::One(src.into()),
            targets: crate::doc::MolIds::One(tgt.into()),
            label: label.map(|id| crate::doc::Label::Id(id.into())),
            edge_routing: None,
            role: None,
            arrow: crate::doc::EdgeArrow::Forward,
            color: None,
            stroke_width: None,
            dashed: false,
        })
    }

    fn text_node(id: &str, text: &str) -> crate::doc::Node {
        crate::doc::Node::Text(crate::doc::TextNode {
            type_: crate::doc::TextNodeKind::Text,
            id: Some(id.into()),
            text: text.into(),
            color: None,
            scale: None,
            opts: None,
        })
    }

    fn edge_labeled(src: &str, tgt: &str, id: &str, pos: crate::doc::LabelPos) -> crate::doc::Node {
        crate::doc::Node::Edge(crate::doc::EdgeNode {
            type_: crate::doc::EdgeNodeKind::Edge,
            sources: crate::doc::MolIds::One(src.into()),
            targets: crate::doc::MolIds::One(tgt.into()),
            label: Some(crate::doc::Label::Placed(crate::doc::LabelPlacement {
                id: id.into(),
                pos: Some(pos),
            })),
            edge_routing: None,
            role: None,
            arrow: crate::doc::EdgeArrow::Forward,
            color: None,
            stroke_width: None,
            dashed: false,
        })
    }

    fn chem_paint(id: &str, w: f64, h: f64, ax: f64, ay: f64) -> DocPaint {
        use crate::scene::{AtomIn, Layer, LayerName, Scene, Viewport};
        DocPaint {
            id: id.into(),
            frame_json: String::new(),
            molecule: MoleculeIn {
                id: Some(id.into()),
                atoms: vec![AtomIn {
                    index: 0,
                    element: Some("C".into()),
                    z: Some(6),
                    x: ax,
                    y: ay,
                    label: None,
                    charge: 0,
                    mapno: None,
                }],
                bonds: vec![],
                ..Default::default()
            },
            scene: Scene {
                width: w,
                height: h,
                x: 0.0,
                y: 0.0,
                viewports: vec![Viewport {
                    id: Some(id.into()),
                    x: 0.0,
                    y: 0.0,
                    ox: 0.0,
                    oy: 0.0,
                    width: w,
                    height: h,
                    layers: vec![Layer {
                        name: LayerName::Bonds,
                        primitives: vec![],
                    }],
                }],
                overlays: vec![],
                halo: vec![],
            },
        }
    }

    fn scheme(children: Vec<crate::doc::Node>) -> DepictSpec {
        DepictSpec::ReactionScheme {
            id: None,
            children,
            layout: None,
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        }
    }

    #[test]
    fn refine_keeps_seed_crossing() {
        let spec = scheme(vec![
            mol_node("A"),
            mol_node("B"),
            mol_node("C"),
            mol_node("D"),
            edge_node("A", "B", None),
            edge_node("C", "D", None),
        ]);
        let layout = LayoutOpts::default();
        let positions = HashMap::from([
            ("A".into(), (0.0, 0.0)),
            ("B".into(), (200.0, 200.0)),
            ("C".into(), (0.0, 200.0)),
            ("D".into(), (200.0, 0.0)),
        ]);
        let sizes = vec![
            ("A".into(), 80.0, 80.0),
            ("B".into(), 80.0, 80.0),
            ("C".into(), 80.0, 80.0),
            ("D".into(), 80.0, 80.0),
        ];
        let paints = vec![
            chem_paint("A", 80.0, 80.0, 40.0, 40.0),
            chem_paint("B", 80.0, 80.0, 40.0, 40.0),
            chem_paint("C", 80.0, 80.0, 40.0, 40.0),
            chem_paint("D", 80.0, 80.0, 40.0, 40.0),
        ];
        let seed_a = vec![(40.0, 40.0), (240.0, 240.0)];
        let seed_b = vec![(40.0, 240.0), (240.0, 40.0)];
        assert!(shapes_properly_cross(
            &Shape::Polyline(seed_a.clone()),
            &Shape::Polyline(seed_b.clone())
        ));
        let refined = refine_scheme_edges(
            &spec,
            &layout,
            &positions,
            &[Some(seed_a), Some(seed_b)],
            &paints,
            &sizes,
        );
        let a = refined.edge_paths[0].clone().expect("e0");
        let b = refined.edge_paths[1].clone().expect("e1");
        assert!(
            shapes_properly_cross(&Shape::Polyline(a.clone()), &Shape::Polyline(b.clone())),
            "ELK X must remain after refine: {a:?} vs {b:?}"
        );
    }

    fn mean_nearest_sep(a: &[(f64, f64)], b: &[(f64, f64)]) -> f64 {
        let sa = crate::catmull::sample_open_catmull(a, 8);
        let sb = crate::catmull::sample_open_catmull(b, 8);
        let mut sum = 0.0;
        let mut n = 0.0;
        for &(x, y) in &sa {
            let mut best = f64::INFINITY;
            for w in sb.windows(2) {
                let (x0, y0) = w[0];
                let (x1, y1) = w[1];
                let vx = x1 - x0;
                let vy = y1 - y0;
                let l2 = vx * vx + vy * vy;
                let t = if l2 < 1e-12 {
                    0.0
                } else {
                    ((x - x0) * vx + (y - y0) * vy) / l2
                }
                .clamp(0.0, 1.0);
                let d = (x - (x0 + t * vx)).hypot(y - (y0 + t * vy));
                best = best.min(d);
            }
            if best.is_finite() {
                sum += best;
                n += 1.0;
            }
        }
        if n > 0.0 {
            sum / n
        } else {
            0.0
        }
    }

    fn sep_spread(a: &[(f64, f64)], b: &[(f64, f64)]) -> (f64, f64) {
        let sa = crate::catmull::sample_open_catmull(a, 8);
        let sb = crate::catmull::sample_open_catmull(b, 8);
        let mut lo = f64::INFINITY;
        let mut hi: f64 = 0.0;
        for &(x, y) in &sa {
            let mut best = f64::INFINITY;
            for w in sb.windows(2) {
                let (x0, y0) = w[0];
                let (x1, y1) = w[1];
                let vx = x1 - x0;
                let vy = y1 - y0;
                let l2 = vx * vx + vy * vy;
                let t = if l2 < 1e-12 {
                    0.0
                } else {
                    ((x - x0) * vx + (y - y0) * vy) / l2
                }
                .clamp(0.0, 1.0);
                best = best.min((x - (x0 + t * vx)).hypot(y - (y0 + t * vy)));
            }
            lo = lo.min(best);
            hi = hi.max(best);
        }
        (lo, hi)
    }

    #[test]
    fn coterminal_reverse_arrows_offset_not_flung() {
        let spec = scheme(vec![
            mol_node("A"),
            mol_node("B"),
            edge_node("A", "B", None),
            edge_node("B", "A", None),
        ]);
        let layout = LayoutOpts::default();
        let positions = HashMap::from([("A".into(), (0.0, 0.0)), ("B".into(), (240.0, 0.0))]);
        let sizes = vec![("A".into(), 80.0, 80.0), ("B".into(), 80.0, 80.0)];
        let paints = vec![
            chem_paint("A", 80.0, 80.0, 40.0, 40.0),
            chem_paint("B", 80.0, 80.0, 40.0, 40.0),
        ];
        // Same geometric terminals, even though the second edge is reversed.
        let seed = vec![(80.0, 40.0), (240.0, 40.0)];
        let refined = refine_scheme_edges(
            &spec,
            &layout,
            &positions,
            &[Some(seed.clone()), Some(seed.clone())],
            &paints,
            &sizes,
        );
        let a = refined.edge_paths[0].clone().expect("e0");
        let b = refined.edge_paths[1].clone().expect("e1");
        for p in a.iter().chain(&b) {
            assert!(
                (15.0..105.0).contains(&p.1) && (40.0..280.0).contains(&p.0),
                "coterminal arrow flung off the corridor: {p:?} a={a:?} b={b:?}"
            );
        }
        assert!(
            a[0].0 < a[a.len() - 1].0,
            "A→B must keep forward sense: {a:?}"
        );
        assert!(
            b[0].0 > b[b.len() - 1].0,
            "B→A must keep reverse sense: {b:?}"
        );
        let sep = mean_nearest_sep(&a, &b);
        assert!(
            (sep - COTERMINAL_LANE_PX).abs() < 6.0,
            "lanes should sit COTERMINAL_LANE_PX apart, got {sep} a={a:?} b={b:?}"
        );
        let (lo, hi) = sep_spread(&a, &b);
        assert!(
            hi - lo < 4.0,
            "offset must stay constant along the path: lo={lo} hi={hi} a={a:?} b={b:?}"
        );
    }

    #[test]
    fn two_forward_arrows_share_offset_lanes() {
        let spec = scheme(vec![
            mol_node("A"),
            mol_node("B"),
            edge_node("A", "B", None),
            edge_node("A", "B", None),
        ]);
        let layout = LayoutOpts::default();
        let positions = HashMap::from([("A".into(), (0.0, 0.0)), ("B".into(), (240.0, 0.0))]);
        let sizes = vec![("A".into(), 80.0, 80.0), ("B".into(), 80.0, 80.0)];
        let paints = vec![
            chem_paint("A", 80.0, 80.0, 40.0, 40.0),
            chem_paint("B", 80.0, 80.0, 40.0, 40.0),
        ];
        let seed = vec![(80.0, 40.0), (160.0, 80.0), (240.0, 40.0)];
        let refined = refine_scheme_edges(
            &spec,
            &layout,
            &positions,
            &[Some(seed.clone()), Some(seed)],
            &paints,
            &sizes,
        );
        let a = refined.edge_paths[0].clone().expect("e0");
        let b = refined.edge_paths[1].clone().expect("e1");
        for p in a.iter().chain(&b) {
            assert!(
                (-20.0..140.0).contains(&p.1) && (40.0..280.0).contains(&p.0),
                "parallel A→B flung: {p:?} a={a:?} b={b:?}"
            );
        }
        let sep = mean_nearest_sep(&a, &b);
        assert!(
            (sep - COTERMINAL_LANE_PX).abs() < 8.0,
            "curved lanes should stay COTERMINAL_LANE_PX apart, got {sep} a={a:?} b={b:?}"
        );
        let (lo, hi) = sep_spread(&a, &b);
        assert!(
            hi - lo < 8.0,
            "curved offset must stay even along the path: lo={lo} hi={hi} a={a:?} b={b:?}"
        );
    }

    fn real_paint(id: &str, smiles: &str, name: Option<&str>) -> DocPaint {
        let spec = match name {
            Some(n) => serde_json::json!({
                "type": "mol",
                "id": id,
                "smiles": smiles,
                "name": n,
            }),
            None => serde_json::json!({
                "type": "mol",
                "id": id,
                "smiles": smiles,
            }),
        };
        let a = crate::doc::render(&serde_json::from_value(spec).unwrap())
            .unwrap()
            .remove(0);
        DocPaint {
            id: a.id.clone(),
            molecule: a.molecule.clone(),
            scene: a.scene.clone(),
            frame_json: a.frame_molblock.clone(),
        }
    }

    /// Down-flow A→B / B→A through phenol + benzaldehyde (notebook example 5).
    /// Endpoint-only clash used to let the centroid chord cut CHO / the ring.
    #[test]
    fn coterminal_down_pair_shafts_clear_chem() {
        let paints = vec![
            real_paint("A", "c1ccccc1O", Some("phenol")),
            real_paint("B", "c1ccccc1C=O", Some("benzaldehyde")),
        ];
        let spec = scheme(vec![
            mol_node("A"),
            mol_node("B"),
            edge_node("A", "B", None),
            edge_node("B", "A", None),
        ]);
        let sizes: Vec<(String, f64, f64)> = paints
            .iter()
            .map(|p| (p.id.clone(), p.scene.width, p.scene.height))
            .collect();
        let max_w = sizes.iter().map(|s| s.1).fold(0.0_f64, f64::max);
        let gap = 48.0;
        let positions = HashMap::from([
            ("A".into(), ((max_w - sizes[0].1) * 0.5, 0.0)),
            ("B".into(), ((max_w - sizes[1].1) * 0.5, sizes[0].2 + gap)),
        ]);
        let a_pos = positions["A"];
        let b_pos = positions["B"];
        let seed = vec![
            (a_pos.0 + sizes[0].1 * 0.5, a_pos.1 + sizes[0].2),
            (b_pos.0 + sizes[1].1 * 0.5, b_pos.1),
        ];
        let refined = refine_scheme_edges(
            &spec,
            &LayoutOpts {
                direction: Some(crate::doc::LayoutDirection::Down),
                edge_routing: Some(crate::doc::EdgeRouting::Polyline),
                ..Default::default()
            },
            &positions,
            &[Some(seed.clone()), Some(seed)],
            &paints,
            &sizes,
        );
        let a = refined.edge_paths[0].clone().expect("e0");
        let b = refined.edge_paths[1].clone().expect("e1");
        let paint_of: HashMap<&str, &DocPaint> =
            paints.iter().map(|p| (p.id.as_str(), p)).collect();
        let size_of: HashMap<&str, (f64, f64)> = sizes
            .iter()
            .map(|(id, w, h)| (id.as_str(), (*w, *h)))
            .collect();
        let ca = mol_centroid(&paint_of, &positions, &size_of, "A");
        let cb = mol_centroid(&paint_of, &positions, &size_of, "B");
        for (i, pts, tgt) in [(0usize, &a, cb), (1, &b, ca)] {
            assert!(pts.len() >= 3, "cutting chord needs a mid DOF: {pts:?}");
            // Three verts inside 8px is the Catmull loop that painted as a rung.
            for w in pts.windows(3) {
                let d01 = (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1);
                let d12 = (w[1].0 - w[2].0).hypot(w[1].1 - w[2].1);
                let d02 = (w[0].0 - w[2].0).hypot(w[0].1 - w[2].1);
                assert!(
                    d01.max(d12).max(d02) >= 8.0,
                    "controls collapsed into a ladder rung: {pts:?}"
                );
            }
            let (tip, tan) = crate::catmull::open_catmull_end_frame(pts);
            let to_c = (tgt.0 - tip.0, tgt.1 - tip.1);
            let clen = to_c.0.hypot(to_c.1).max(1e-9);
            let aim = (tan.0 * to_c.0 + tan.1 * to_c.1) / clen;
            let perp = (to_c.0 * tan.1 - to_c.1 * tan.0).abs() / clen;
            assert!(
                aim > 0.0 && perp < 0.35,
                "edge {i} head must aim at the target centroid: perp={perp} tip={tip:?} tan={tan:?} tgt={tgt:?} pts={pts:?}"
            );
            let samples = crate::catmull::sample_open_catmull(pts, 8);
            for &(x, y) in &samples {
                for paint in &paints {
                    let elk = positions[&paint.id];
                    let d = paint_ink_signed(paint, elk, (x, y));
                    assert!(
                        d > 1.0,
                        "edge {i} sample ({x:.1},{y:.1}) hits {} ink: d={d:.1} pts={pts:?}",
                        paint.id
                    );
                }
            }
        }
        let sep = mean_nearest_sep(&a, &b);
        assert!(
            (sep - COTERMINAL_LANE_PX).abs() < 6.0,
            "lanes should stay parallel after clearing chem: sep={sep} a={a:?} b={b:?}"
        );
    }

    #[test]
    fn endpoint_inside_aabb_off_chem() {
        let spec = scheme(vec![
            mol_node("A"),
            edge_node("A", "B", None),
            mol_node("B"),
        ]);
        let layout = LayoutOpts::default();
        let positions = HashMap::from([("A".into(), (0.0, 0.0)), ("B".into(), (400.0, 0.0))]);
        let sizes = vec![("A".into(), 200.0, 200.0), ("B".into(), 200.0, 200.0)];
        let paints = vec![
            chem_paint("A", 200.0, 200.0, 100.0, 100.0),
            chem_paint("B", 200.0, 200.0, 100.0, 100.0),
        ];
        let seed = vec![(200.0, 100.0), (400.0, 100.0)];
        let refined = refine_scheme_edges(
            &spec,
            &layout,
            &positions,
            &[Some(seed.clone())],
            &paints,
            &sizes,
        );
        let pts = refined.edge_paths[0].clone().expect("path");
        assert!(pts.len() >= 2);
        let (x0, y0) = pts[0];
        let (x1, y1) = pts[pts.len() - 1];
        let paint_of: HashMap<&str, &DocPaint> =
            paints.iter().map(|p| (p.id.as_str(), p)).collect();
        let size_of: HashMap<&str, (f64, f64)> =
            HashMap::from([("A", (200.0, 200.0)), ("B", (200.0, 200.0))]);
        let ca = mol_centroid(&paint_of, &positions, &size_of, "A");
        let cb = mol_centroid(&paint_of, &positions, &size_of, "B");
        assert!(
            (x1 - x0) * (seed[1].0 - seed[0].0) + (y1 - y0) * (seed[1].1 - seed[0].1) > 0.0,
            "must keep ELK direction: {pts:?}"
        );
        assert!(
            x0 > ca.0 && x1 < cb.0,
            "ends should sit on the facing sides: ({x0},{y0})..({x1},{y1}) ca={ca:?} cb={cb:?}"
        );
        let da = paint_ink_signed(&paints[0], positions["A"], (x0, y0));
        let db = paint_ink_signed(&paints[1], positions["B"], (x1, y1));
        assert!(
            da + 4.0 >= SCHEME_INK_STANDOFF,
            "src should hug A's ink by ~½ bond, inside the empty AABB: {da} pts={pts:?} ca={ca:?}"
        );
        assert!(
            db + 4.0 >= SCHEME_INK_STANDOFF,
            "tgt should hug B's ink by ~½ bond, inside the empty AABB: {db} pts={pts:?} cb={cb:?}"
        );
        assert!(
            x0 < 200.0 && x1 > 400.0,
            "tips sit inside the oversized boxes, not on the ELK lips: pts={pts:?}"
        );
        let line_a = point_to_line(x0, y0, x1, y1, ca.0, ca.1);
        let line_b = point_to_line(x0, y0, x1, y1, cb.0, cb.1);
        assert!(
            line_a < 8.0 && line_b < 8.0,
            "shaft ray should pass near both centroids: dA={line_a} dB={line_b} pts={pts:?} ca={ca:?} cb={cb:?}"
        );
    }

    fn point_to_line(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
        let vx = x1 - x0;
        let vy = y1 - y0;
        let l = vx.hypot(vy).max(1e-9);
        ((px - x0) * vy - (py - y0) * vx).abs() / l
    }

    #[test]
    fn refined_shaft_emits_catmull_cubics() {
        let spec = scheme(vec![
            mol_node("A"),
            edge_node("A", "B", None),
            mol_node("B"),
        ]);
        let layout = LayoutOpts::default();
        let positions = HashMap::from([("A".into(), (0.0, 0.0)), ("B".into(), (240.0, 0.0))]);
        let sizes = vec![("A".into(), 80.0, 80.0), ("B".into(), 80.0, 80.0)];
        let paints = vec![
            chem_paint("A", 80.0, 80.0, 40.0, 40.0),
            chem_paint("B", 80.0, 80.0, 40.0, 40.0),
        ];
        let seed = vec![(80.0, 40.0), (160.0, 80.0), (240.0, 40.0)];
        let refined = refine_scheme_edges(
            &spec,
            &layout,
            &positions,
            &[Some(seed.clone())],
            &paints,
            &sizes,
        );
        let pts = refined.edge_paths[0].clone().expect("path");
        assert_eq!(
            pts.len(),
            seed.len(),
            "refine must keep ELK endpoints/elbows, not Catmull samples: {pts:?}"
        );
        let d = crate::catmull::centripetal_catmull_rom_open_path(&pts);
        assert!(d.contains('C'), "open CR shaft should be cubics: {d}");
        assert!(!d.contains('Z'), "{d}");
        let paint_of: HashMap<&str, &DocPaint> =
            paints.iter().map(|p| (p.id.as_str(), p)).collect();
        let size_of: HashMap<&str, (f64, f64)> = sizes
            .iter()
            .map(|(id, w, h)| (id.as_str(), (*w, *h)))
            .collect();
        let cb = mol_centroid(&paint_of, &positions, &size_of, "B");
        let (tip, tan) = crate::catmull::open_catmull_end_frame(&pts);
        let to_c = (cb.0 - tip.0, cb.1 - tip.1);
        let clen = to_c.0.hypot(to_c.1).max(1e-9);
        let aim = (tan.0 * to_c.0 + tan.1 * to_c.1) / clen;
        let perp = (to_c.0 * tan.1 - to_c.1 * tan.0).abs() / clen;
        assert!(
            aim > 0.0,
            "head must point toward the target centroid: tip={tip:?} tan={tan:?} cb={cb:?}"
        );
        assert!(
            perp < 0.2,
            "last chord must aim at the centroid: perp={perp} tip={tip:?} cb={cb:?} pts={pts:?}"
        );
    }

    #[test]
    fn two_point_head_aims_at_centroid_despite_side_port() {
        let spec = scheme(vec![
            mol_node("A"),
            edge_node("A", "B", None),
            mol_node("B"),
        ]);
        let layout = LayoutOpts::default();
        let positions = HashMap::from([("A".into(), (0.0, 0.0)), ("B".into(), (240.0, 0.0))]);
        let sizes = vec![("A".into(), 80.0, 80.0), ("B".into(), 80.0, 80.0)];
        let paints = vec![
            chem_paint("A", 80.0, 80.0, 40.0, 40.0),
            chem_paint("B", 80.0, 80.0, 40.0, 40.0),
        ];
        // ELK put B's port on the south side of the box, not on the centroid line.
        let seed = vec![(80.0, 40.0), (240.0, 80.0)];
        let refined =
            refine_scheme_edges(&spec, &layout, &positions, &[Some(seed)], &paints, &sizes);
        let pts = refined.edge_paths[0].clone().expect("path");
        let paint_of: HashMap<&str, &DocPaint> =
            paints.iter().map(|p| (p.id.as_str(), p)).collect();
        let size_of: HashMap<&str, (f64, f64)> = sizes
            .iter()
            .map(|(id, w, h)| (id.as_str(), (*w, *h)))
            .collect();
        let cb = mol_centroid(&paint_of, &positions, &size_of, "B");
        let (tip, tan) = crate::catmull::open_catmull_end_frame(&pts);
        let to_c = (cb.0 - tip.0, cb.1 - tip.1);
        let clen = to_c.0.hypot(to_c.1).max(1e-9);
        let aim = (tan.0 * to_c.0 + tan.1 * to_c.1) / clen;
        let perp = (to_c.0 * tan.1 - to_c.1 * tan.0).abs() / clen;
        assert!(
            aim > 0.0,
            "head must point toward the target centroid: tip={tip:?} tan={tan:?} cb={cb:?}"
        );
        assert!(
            perp < 0.2,
            "side ELK port must not aim the last chord off the centroid: perp={perp} tip={tip:?} cb={cb:?} pts={pts:?}"
        );
    }

    #[test]
    fn edge_label_clears_chem_leaves() {
        let spec = scheme(vec![
            mol_node("A"),
            mol_node("C"),
            crate::doc::Node::Text(crate::doc::TextNode {
                type_: crate::doc::TextNodeKind::Text,
                id: Some("lab".into()),
                text: "Glucuronidation".into(),
                color: None,
                scale: None,
                opts: None,
            }),
            edge_node("A", "B", Some("lab")),
            mol_node("B"),
        ]);
        let layout = LayoutOpts::default();
        let positions = HashMap::from([
            ("A".into(), (0.0, 0.0)),
            ("C".into(), (150.0, 0.0)),
            ("B".into(), (300.0, 0.0)),
        ]);
        let sizes = vec![
            ("A".into(), 80.0, 80.0),
            ("C".into(), 80.0, 80.0),
            ("B".into(), 80.0, 80.0),
        ];
        let paints = vec![
            chem_paint("A", 80.0, 80.0, 40.0, 40.0),
            chem_paint("C", 80.0, 80.0, 40.0, 40.0),
            chem_paint("B", 80.0, 80.0, 40.0, 40.0),
        ];
        let seed = vec![(80.0, 40.0), (300.0, 40.0)];
        let refined =
            refine_scheme_edges(&spec, &layout, &positions, &[Some(seed)], &paints, &sizes);
        let (lx, ly, _) = refined.edge_labels[0].expect("label");
        let d = paint_ink_signed(&paints[1], positions["C"], (lx, ly));
        let need = LABEL_FONT_PX * 0.5 + LABEL_SHAFT_PAD_PX;
        assert!(
            d + 1.0 >= need,
            "label must clear C's chem: d={d} need={need} at ({lx},{ly})"
        );
    }

    #[test]
    fn label_rotates_and_stays_off_shaft() {
        let spec = scheme(vec![
            mol_node("A"),
            crate::doc::Node::Text(crate::doc::TextNode {
                type_: crate::doc::TextNodeKind::Text,
                id: Some("lab".into()),
                text: "ox".into(),
                color: None,
                scale: None,
                opts: None,
            }),
            edge_node("A", "B", Some("lab")),
            mol_node("B"),
        ]);
        let layout = LayoutOpts::default();
        let positions = HashMap::from([("A".into(), (0.0, 0.0)), ("B".into(), (240.0, 0.0))]);
        let sizes = vec![("A".into(), 80.0, 80.0), ("B".into(), 80.0, 80.0)];
        let paints = vec![
            chem_paint("A", 80.0, 80.0, 40.0, 40.0),
            chem_paint("B", 80.0, 80.0, 40.0, 40.0),
        ];
        let seed = vec![(80.0, 40.0), (240.0, 40.0)];
        let refined =
            refine_scheme_edges(&spec, &layout, &positions, &[Some(seed)], &paints, &sizes);
        let pts = refined.edge_paths[0].clone().expect("path");
        let (lx, ly, rot) = refined.edge_labels[0].expect("label");
        let expected = flip_upside_down(tangent_at(&pts, lx, ly));
        assert!(
            (rot - expected).abs() < 15.0,
            "label rotate {rot} vs edge {expected}"
        );
        assert!(
            (-90.0..=90.0).contains(&rot),
            "label must not be upside down: {rot}"
        );
        let painted = crate::catmull::sample_open_catmull(&pts, 8);
        let (mx, my, _, _) = label_point(&painted);
        let d_mid = (lx - mx).hypot(ly - my);
        assert!(
            d_mid < label_shaft_tether(None) + 8.0,
            "label should stay near the shaft midpoint: d={d_mid} at ({lx},{ly}) mid=({mx},{my})"
        );
        let mut d_shaft = f64::INFINITY;
        for w in painted.windows(2) {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            let vx = x1 - x0;
            let vy = y1 - y0;
            let l2 = vx * vx + vy * vy;
            let t = if l2 < 1e-12 {
                0.0
            } else {
                ((lx - x0) * vx + (ly - y0) * vy) / l2
            }
            .clamp(0.0, 1.0);
            let px = x0 + t * vx;
            let py = y0 + t * vy;
            d_shaft = d_shaft.min((lx - px).hypot(ly - py));
        }
        let need = label_shaft_clash(None);
        assert!(
            d_shaft + 4.0 >= need,
            "label sits on shaft: d={d_shaft} need={need} at ({lx},{ly})"
        );
    }

    #[test]
    fn thick_curve_label_clears_catmull_shaft() {
        let spec = scheme(vec![
            mol_node("A"),
            crate::doc::Node::Text(crate::doc::TextNode {
                type_: crate::doc::TextNodeKind::Text,
                id: Some("lab".into()),
                text: "Glutathionation".into(),
                color: None,
                scale: None,
                opts: None,
            }),
            {
                let mut e = match edge_node("A", "B", Some("lab")) {
                    crate::doc::Node::Edge(e) => e,
                    other => panic!("{other:?}"),
                };
                e.stroke_width = Some(4.8);
                crate::doc::Node::Edge(e)
            },
            mol_node("B"),
        ]);
        let layout = LayoutOpts::default();
        let positions = HashMap::from([("A".into(), (0.0, 0.0)), ("B".into(), (0.0, 280.0))]);
        let sizes = vec![("A".into(), 80.0, 80.0), ("B".into(), 80.0, 80.0)];
        let paints = vec![
            chem_paint("A", 80.0, 80.0, 40.0, 40.0),
            chem_paint("B", 80.0, 80.0, 40.0, 40.0),
        ];
        let seed = vec![(40.0, 80.0), (120.0, 180.0), (40.0, 280.0)];
        let refined =
            refine_scheme_edges(&spec, &layout, &positions, &[Some(seed)], &paints, &sizes);
        let pts = refined.edge_paths[0].clone().expect("path");
        let (lx, ly, rot) = refined.edge_labels[0].expect("label");
        let painted = crate::catmull::sample_open_catmull(&pts, 8);
        let mut d_shaft = f64::INFINITY;
        let mut tang = 0.0;
        for w in painted.windows(2) {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            let vx = x1 - x0;
            let vy = y1 - y0;
            let l2 = vx * vx + vy * vy;
            let t = if l2 < 1e-12 {
                0.0
            } else {
                ((lx - x0) * vx + (ly - y0) * vy) / l2
            }
            .clamp(0.0, 1.0);
            let px = x0 + t * vx;
            let py = y0 + t * vy;
            let d = (lx - px).hypot(ly - py);
            if d < d_shaft {
                d_shaft = d;
                tang = (y1 - y0).atan2(x1 - x0).to_degrees();
            }
        }
        let need = label_shaft_clash(Some(4.8));
        assert!(
            d_shaft + 4.0 >= need,
            "solver left the label in the stroke: d={d_shaft} need={need} at ({lx},{ly})"
        );
        let expected = flip_upside_down(tang);
        // Parallel is mod 180: +84° and −90° are the same shaft.
        let mut d = (rot - expected).abs() % 180.0;
        if d > 90.0 {
            d = 180.0 - d;
        }
        assert!(
            d < 20.0,
            "label should follow the curve tangent: rot={rot} tangent={expected}"
        );
        assert!(
            (-90.0..=90.0).contains(&rot),
            "upside-down labels must flip 180° after the solve: {rot}"
        );
    }

    #[test]
    fn tight_pack_does_not_collapse_to_stub() {
        let spec = scheme(vec![
            mol_node("A"),
            {
                let mut e = match edge_node("A", "B", None) {
                    crate::doc::Node::Edge(e) => e,
                    other => panic!("{other:?}"),
                };
                e.stroke_width = Some(4.8);
                crate::doc::Node::Edge(e)
            },
            mol_node("B"),
        ]);
        let layout = LayoutOpts::default();
        let positions = HashMap::from([("A".into(), (0.0, 0.0)), ("B".into(), (0.0, 100.0))]);
        let sizes = vec![("A".into(), 80.0, 80.0), ("B".into(), 80.0, 80.0)];
        let paints = vec![
            chem_paint("A", 80.0, 80.0, 40.0, 40.0),
            chem_paint("B", 80.0, 80.0, 40.0, 40.0),
        ];
        let seed = vec![(40.0, 80.0), (40.0, 100.0)];
        let refined = refine_scheme_edges(
            &spec,
            &layout,
            &positions,
            &[Some(seed.clone())],
            &paints,
            &sizes,
        );
        let pts = refined.edge_paths[0].clone().expect("path");
        let (x0, y0) = pts[0];
        let (x1, y1) = pts[pts.len() - 1];
        let len = (x1 - x0).hypot(y1 - y0);
        assert!(
            len + 1.0 >= seed[1].1 - seed[0].1 - 4.0,
            "tight ELK shaft should not shrink to a 20px stub: len={len} seed={seed:?} pts={pts:?}"
        );
    }

    #[test]
    fn label_solve_cannot_move_shaft_endpoints() {
        let layout = LayoutOpts::default();
        let positions = HashMap::from([("A".into(), (0.0, 0.0)), ("B".into(), (0.0, 280.0))]);
        let sizes = vec![("A".into(), 80.0, 80.0), ("B".into(), 80.0, 80.0)];
        let paints = vec![
            chem_paint("A", 80.0, 80.0, 40.0, 40.0),
            chem_paint("B", 80.0, 80.0, 40.0, 40.0),
        ];
        let seed = vec![(40.0, 80.0), (120.0, 180.0), (40.0, 280.0)];
        let unlabeled = scheme(vec![
            mol_node("A"),
            edge_node("A", "B", None),
            mol_node("B"),
        ]);
        let labeled = scheme(vec![
            mol_node("A"),
            crate::doc::Node::Text(crate::doc::TextNode {
                type_: crate::doc::TextNodeKind::Text,
                id: Some("lab".into()),
                text: "Glutathionation".into(),
                color: None,
                scale: None,
                opts: None,
            }),
            {
                let mut e = match edge_node("A", "B", Some("lab")) {
                    crate::doc::Node::Edge(e) => e,
                    other => panic!("{other:?}"),
                };
                e.stroke_width = Some(4.8);
                crate::doc::Node::Edge(e)
            },
            mol_node("B"),
        ]);
        let a = refine_scheme_edges(
            &unlabeled,
            &layout,
            &positions,
            &[Some(seed.clone())],
            &paints,
            &sizes,
        )
        .edge_paths[0]
            .clone()
            .expect("unlabeled");
        let b = refine_scheme_edges(
            &labeled,
            &layout,
            &positions,
            &[Some(seed)],
            &paints,
            &sizes,
        )
        .edge_paths[0]
            .clone()
            .expect("labeled");
        assert_eq!(a.len(), b.len(), "control count {a:?} vs {b:?}");
        for (p, q) in a.iter().zip(&b) {
            let d = (p.0 - q.0).hypot(p.1 - q.1);
            assert!(
                d < 0.5,
                "label pass moved a shaft vertex by {d}: {a:?} vs {b:?}"
            );
        }
    }

    #[test]
    fn label_offset_dir_above_is_screen_up() {
        let horiz = vec![(0.0, 40.0), (80.0, 40.0)];
        let above = label_offset_dir(&horiz, crate::doc::LabelPos::Above);
        let below = label_offset_dir(&horiz, crate::doc::LabelPos::Below);
        assert!(above.1 < 0.0, "above should be screen-up: {above:?}");
        assert!(below.1 > 0.0, "below should be screen-down: {below:?}");
        let rev = vec![(80.0, 40.0), (0.0, 40.0)];
        let above_rev = label_offset_dir(&rev, crate::doc::LabelPos::Above);
        assert!(
            above_rev.1 < 0.0,
            "reverse path above is still screen-up: {above_rev:?}"
        );
    }

    #[test]
    fn coterminal_labels_sit_outside_opposite_sides() {
        let unlabeled = scheme(vec![
            mol_node("A"),
            mol_node("B"),
            edge_node("A", "B", None),
            edge_node("B", "A", None),
        ]);
        let labeled = scheme(vec![
            mol_node("A"),
            mol_node("B"),
            text_node("ox", "ox"),
            text_node("red", "red"),
            edge_labeled("A", "B", "ox", crate::doc::LabelPos::Above),
            edge_labeled("B", "A", "red", crate::doc::LabelPos::Below),
        ]);
        let layout = LayoutOpts::default();
        let positions = HashMap::from([("A".into(), (0.0, 0.0)), ("B".into(), (240.0, 0.0))]);
        let sizes = vec![("A".into(), 80.0, 80.0), ("B".into(), 80.0, 80.0)];
        let paints = vec![
            chem_paint("A", 80.0, 80.0, 40.0, 40.0),
            chem_paint("B", 80.0, 80.0, 40.0, 40.0),
        ];
        let fwd = vec![(80.0, 40.0), (240.0, 40.0)];
        let rev = vec![(240.0, 40.0), (80.0, 40.0)];
        let seeds = [Some(fwd.clone()), Some(rev.clone())];
        let plain = refine_scheme_edges(&unlabeled, &layout, &positions, &seeds, &paints, &sizes);
        let with_lab = refine_scheme_edges(&labeled, &layout, &positions, &seeds, &paints, &sizes);
        let a = plain.edge_paths[0].clone().expect("plain a");
        let b = with_lab.edge_paths[0].clone().expect("lab a");
        for (p, q) in a.iter().zip(&b) {
            let d = (p.0 - q.0).hypot(p.1 - q.1);
            assert!(
                d < 1.0,
                "labels must not tilt coterminal shafts: {a:?} vs {b:?}"
            );
        }
        let (ox, oy, _) = with_lab.edge_labels[0].expect("ox");
        let (rx, ry, _) = with_lab.edge_labels[1].expect("red");
        assert!(
            oy + 4.0 < 40.0,
            "ox above should sit above the bundle: oy={oy}"
        );
        assert!(
            ry > 40.0 + 4.0,
            "red below should sit below the bundle: ry={ry}"
        );
        assert!(
            (ry - oy).abs() > 16.0,
            "labels should not share one side of the pair: ox=({ox},{oy}) red=({rx},{ry})"
        );
    }
}
