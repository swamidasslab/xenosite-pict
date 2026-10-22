//! ELK seed + constraint refine for reaction-scheme edges and labels.
//!
//! Molecules stay fixed. ELK endpoints/routing points and edge labels (xy + rot)
//! are the DOF. Catmull–Rom is how those points are stroked, not extra vertices.

use std::collections::HashMap;

use crate::arrows::{edge_anchors, ANCHOR_GAP};
use crate::metrics::BOND_PX;
use crate::scene::{LayerName, MoleculeIn, Primitive, TextAnchor};

use super::layout_constraints::{
    edge_signed_distance, shapes_properly_cross, solve, Constraint, HardConstraint, LayoutWorld,
    ObjectType, Shape, SolveOpts, LAGRANGIAN_CLEAR, LAGRANGIAN_TETHER,
};
use super::layout_items::Aabb;
use super::{DepictSpec, DocPaint, LayoutOpts};

/// Desired gap from painted mol ink to the shaft tip (two bonds).
///
/// Actual standoff is `min(this, ELK endpoint's distance to the clash grid)`
/// so a tight pack cannot ask for air the seed did not leave.
pub const SCHEME_INK_STANDOFF: f64 = 2.0 * BOND_PX;
/// Minimum gap between nearly-parallel overlapping shafts.
pub const SCHEME_EDGE_CLEARANCE: f64 = BOND_PX;
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
/// When two ink standoffs do not fit in the gap, keep a visible shaft.
const MIN_SHAFT: f64 = 20.0;

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
            world.add_ink(&chem, scene_ink_shapes(paint, (x, y)));
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

    for (i, edge) in spec.edges().iter().enumerate() {
        let eid = format!("scheme/edges/e{i}");
        world.insert(
            &eid,
            &edges_root,
            ObjectType::Edge,
            Shape::Polyline(vec![]),
            true,
        );
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
        let pts = &seed_pts[i];
        // Painted ends stop at ink+standoff along a ray through the centroid.
        // The ray itself may cross chem — do not emit isotropic Outside vs ink.
        // Standoff is min(two bonds, how far the ELK seed already sits from ink)
        // so a tight ELK pack cannot demand an infeasible gap.
        let last = pts.len().saturating_sub(1);
        let src_c = mol_centroid(&paint_of, positions, &size_of, src);
        let tgt_c = mol_centroid(&paint_of, positions, &size_of, tgt);
        let span = (tgt_c.0 - src_c.0).hypot(tgt_c.1 - src_c.1);
        let src_chem = format!("scheme/mols/{src}/chem");
        let tgt_chem = format!("scheme/mols/{tgt}/chem");
        let clash0 = if pts.len() >= 2 {
            elk_capped_standoff(&world, &src_chem, pts[0], SCHEME_INK_STANDOFF)
        } else {
            SCHEME_INK_STANDOFF
        };
        let clash1 = if pts.len() >= 2 {
            elk_capped_standoff(&world, &tgt_chem, pts[last], SCHEME_INK_STANDOFF)
        } else {
            SCHEME_INK_STANDOFF
        };
        let max_t = span.max(clash0).max(clash1);
        let mut pose0 = if pts.len() >= 2 {
            ray_ink_pose(&world, &src_chem, src_c, tgt_c, clash0, max_t)
        } else {
            src_c
        };
        let mut pose1 = if pts.len() >= 2 {
            ray_ink_pose(&world, &tgt_chem, tgt_c, src_c, clash1, max_t)
        } else {
            tgt_c
        };
        if pts.len() >= 2 {
            let ux = (tgt_c.0 - src_c.0) / span.max(1e-9);
            let uy = (tgt_c.1 - src_c.1) / span.max(1e-9);
            if (pose1.0 - pose0.0) * ux + (pose1.1 - pose0.1) * uy < MIN_SHAFT {
                let mx = (pose0.0 + pose1.0) * 0.5;
                let my = (pose0.1 + pose1.1) * 0.5;
                let half = MIN_SHAFT * 0.5;
                pose0 = (mx - ux * half, my - uy * half);
                pose1 = (mx + ux * half, my + uy * half);
            }
        }
        for (k, &(x, y)) in pts.iter().enumerate() {
            let pose = if pts.len() >= 2 && k == 0 {
                pose0
            } else if pts.len() >= 2 && k == last {
                pose1
            } else {
                (x, y)
            };
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
            if k == 0 || k == last {
                cs.push(Constraint::PreferPose {
                    a: format!("{eid}/p{k}"),
                    x: pose.0,
                    y: pose.1,
                    strength: ENDPOINT_POSE,
                });
            } else {
                cs.push(Constraint::PreferPose {
                    a: format!("{eid}/p{k}"),
                    x,
                    y,
                    strength: POSE_STRENGTH,
                });
            }
        }
        if pts.len() >= 2 {
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
        }
    }

    let n_edges = spec.edges().len();
    for i in 0..n_edges {
        let a = format!("scheme/edges/e{i}");
        for j in (i + 1)..n_edges {
            let b = format!("scheme/edges/e{j}");
            hard.push(HardConstraint::ParallelClear {
                a: a.clone(),
                b: b.clone(),
                clash_dist: SCHEME_EDGE_CLEARANCE,
                lagrangian: LAGRANGIAN_CLEAR,
            });
            let sa = Shape::Polyline(seed_pts[i].clone());
            let sb = Shape::Polyline(seed_pts[j].clone());
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

    // Shafts stay put: the arrow is the frozen endpoints. Labels cannot shove them.
    for (i, pts) in seed_pts.iter().enumerate() {
        let eid = format!("scheme/edges/e{i}");
        for k in 0..pts.len() {
            world.freeze(&format!("{eid}/p{k}"));
        }
    }

    let mut label_cs = Vec::new();
    let mut label_hard = Vec::new();
    for (i, edge) in spec.edges().iter().enumerate() {
        let eid = format!("scheme/edges/e{i}");
        let live = match world.eval_shape(&eid) {
            Some(Shape::Polyline(p)) if p.len() >= 2 => p,
            _ => seed_pts[i].clone(),
        };
        let Some((px, py)) = edge_label_seed(spec, edge, &live) else {
            continue;
        };
        // Circle at the glyph center, not the unrotated text AABB: a long
        // word's box swallows a Catmull bulge so Outside cannot push it off.
        world.insert(
            format!("{eid}/label"),
            &eid,
            ObjectType::EdgeLabel,
            Shape::Circle {
                cx: px,
                cy: py,
                r: LABEL_FONT_PX * 0.5,
            },
            false,
        );
        world.set_rot(
            &format!("{eid}/label"),
            readable_deg(tangent_at(&live, px, py)),
        );
        label_cs.push(Constraint::Midpoint {
            a: format!("{eid}/label"),
            b: eid.clone(),
            strength: POSE_STRENGTH,
        });
        label_cs.push(Constraint::Parallel {
            a: format!("{eid}/label"),
            b: eid.clone(),
            strength: 0.8,
        });
        label_hard.push(HardConstraint::MaxDist {
            a: format!("{eid}/label"),
            b: eid.clone(),
            dist: label_shaft_tether(edge.stroke_width),
            lagrangian: LAGRANGIAN_TETHER,
        });
        let self_stroke = edge.stroke_width;
        for j in 0..n_edges {
            if i == j {
                continue;
            }
            let other_stroke = spec.edges().get(j).and_then(|e| e.stroke_width);
            label_hard.push(HardConstraint::Outside {
                a: format!("{eid}/label"),
                b: format!("scheme/edges/e{j}"),
                clash_dist: shaft_air(other_stroke),
                lagrangian: LAGRANGIAN_CLEAR,
            });
        }
        label_hard.push(HardConstraint::Outside {
            a: format!("{eid}/label"),
            b: eid,
            clash_dist: shaft_air(self_stroke),
            lagrangian: LAGRANGIAN_CLEAR,
        });
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

/// `min(max_standoff, ELK seed’s clash-grid clearance)`. Never negative.
fn elk_capped_standoff(
    world: &LayoutWorld,
    chem: &str,
    elk_pt: (f64, f64),
    max_standoff: f64,
) -> f64 {
    let d = min_ink_signed(world, chem, elk_pt);
    if !d.is_finite() {
        return max_standoff;
    }
    max_standoff.min(d.max(0.0))
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

/// Document-space origin of unframed atom coords: ELK + leftover + scaled
/// [`crate::depict::frame_offset`] (the same shift chem paint uses).
fn mol_frame_origin(paint: &DocPaint, elk: (f64, f64)) -> (f64, f64) {
    let leftover = match paint.scene.viewports.first() {
        Some(vp) => (elk.0 + vp.x + vp.ox, elk.1 + vp.y + vp.oy),
        None => elk,
    };
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

fn prim_ink_shapes(p: &Primitive, ox: f64, oy: f64) -> Vec<Shape> {
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
    let (ox, oy) = match paint.scene.viewports.first() {
        Some(vp) => (elk.0 + vp.x + vp.ox, elk.1 + vp.y + vp.oy),
        None => elk,
    };
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
    let (mx, my, mut px, mut py) = label_point(&painted);
    match pos {
        super::LabelPos::Below | super::LabelPos::Right => {
            px = -px;
            py = -py;
        }
        _ => {}
    }
    let off = label_shaft_clash(edge.stroke_width);
    Some((mx + px * off, my + py * off))
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
    fn ink_standoff_constant_is_two_bonds() {
        assert!((SCHEME_INK_STANDOFF - 2.0 * BOND_PX).abs() < 1e-12);
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
            &LayoutOpts {
                refine: Some(true),
                ..Default::default()
            },
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
        let want_a =
            SCHEME_INK_STANDOFF.min(paint_ink_signed(&paints[0], positions["A"], seed[0]).max(0.0));
        let want_b =
            SCHEME_INK_STANDOFF.min(paint_ink_signed(&paints[1], positions["B"], seed[1]).max(0.0));
        assert!(
            da0 + 4.0 >= want_a,
            "src should sit ≥min(2bond, ELK clearance) from A's ink: {da0} want={want_a} pts={pts:?} ca={ca:?}"
        );
        assert!(
            db1 + 4.0 >= want_b,
            "tgt should sit ≥min(2bond, ELK clearance) from B's ink: {db1} want={want_b} pts={pts:?} cb={cb:?}"
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

    #[test]
    fn elk_standoff_is_min_of_two_bonds_and_seed_clearance() {
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
        let tight = (128.0, 40.0);
        let roomy = (200.0, 40.0);
        let cap_tight = elk_capped_standoff(&world, "mol/chem", tight, SCHEME_INK_STANDOFF);
        let cap_roomy = elk_capped_standoff(&world, "mol/chem", roomy, SCHEME_INK_STANDOFF);
        let d_tight = min_ink_signed(&world, "mol/chem", tight);
        assert!(
            (cap_tight - d_tight.max(0.0)).abs() < 1e-9,
            "tight ELK seed must not demand two bonds: cap={cap_tight} d={d_tight}"
        );
        assert!(
            cap_tight < SCHEME_INK_STANDOFF,
            "tight clearance {cap_tight} should be below two bonds"
        );
        assert!(
            (cap_roomy - SCHEME_INK_STANDOFF).abs() < 1e-9,
            "roomy ELK seed should use the two-bond cap: {cap_roomy}"
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
            layout: Some(LayoutOpts {
                refine: Some(true),
                ..Default::default()
            }),
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
        let want_a =
            SCHEME_INK_STANDOFF.min(paint_ink_signed(&paints[0], positions["A"], seed[0]).max(0.0));
        let want_b =
            SCHEME_INK_STANDOFF.min(paint_ink_signed(&paints[1], positions["B"], seed[1]).max(0.0));
        assert!(
            da + 4.0 >= want_a,
            "src should sit ≥min(2bond, ELK clearance) from A's ink: {da} want={want_a} pts={pts:?} ca={ca:?}"
        );
        assert!(
            db + 4.0 >= want_b,
            "tgt should sit ≥min(2bond, ELK clearance) from B's ink: {db} want={want_b} pts={pts:?} cb={cb:?}"
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
        assert!(
            (rot - expected).abs() < 20.0,
            "label should follow the curve tangent: rot={rot} tangent={expected}"
        );
        assert!(
            (-90.0..=90.0).contains(&rot),
            "upside-down labels must flip 180° after the solve: {rot}"
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
}
