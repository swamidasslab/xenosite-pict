//! ELK seed + constraint refine for reaction-scheme edges and labels.
//!
//! Molecules stay fixed. Control points and edge labels are the DOF.

use std::collections::HashMap;

use crate::arrows::{edge_anchors, ANCHOR_GAP};
use crate::metrics::BOND_PX;
use crate::scene::MoleculeIn;

use super::layout_constraints::{
    push_clear_leaves, shapes_properly_cross, solve, Constraint, HardConstraint, LayoutWorld,
    ObjectType, Shape, SolveOpts, LAGRANGIAN_CLEAR, LAGRANGIAN_TETHER,
};
use super::layout_items::Aabb;
use super::{DepictSpec, DocPaint, LayoutOpts};

/// Arrow standoff from painted chem (not the mol AABB).
pub const SCHEME_INK_STANDOFF: f64 = 1.5 * BOND_PX;
/// Edge labels sit closer than shafts; shaft-scale chem clearance is infeasible
/// in a tight ELK gap and would drag the tethered shaft away.
const SCHEME_LABEL_INK_STANDOFF: f64 = 0.5 * BOND_PX;
/// Minimum gap between nearly-parallel overlapping shafts.
pub const SCHEME_EDGE_CLEARANCE: f64 = BOND_PX;
const LABEL_TETHER_PX: f64 = 8.0;
const POSE_STRENGTH: f64 = 0.35;
/// Endpoint → centroid and shaft-tangent → centroid springs.
const ENDPOINT_POSE_STRENGTH: f64 = 2.0;

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
            super::layout_constraints::install_chem_under(
                &mut world,
                &mid,
                &bonds,
                &atoms,
                &[],
                &[],
            );
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
        let src_c = mol_centroid(&paint_of, positions, &size_of, src);
        let tgt_c = mol_centroid(&paint_of, positions, &size_of, tgt);
        let pts = &seed_pts[i];
        let toward0 = if pts.len() >= 3 { pts[1] } else { tgt_c };
        let toward1 = if pts.len() >= 3 {
            pts[pts.len() - 2]
        } else {
            src_c
        };
        // ELK ports sit on the box. Seed on the ray toward the other end /
        // next bend, at ink standoff — not on the centroid, and not past
        // the midpoint (tight packing).
        let sep0 = (toward0.0 - src_c.0).hypot(toward0.1 - src_c.1);
        let sep1 = (toward1.0 - tgt_c.0).hypot(toward1.1 - tgt_c.1);
        let stand0 = SCHEME_INK_STANDOFF.min(sep0 * 0.45);
        let stand1 = SCHEME_INK_STANDOFF.min(sep1 * 0.45);
        let aim0 = ray_at_dist(src_c, toward0, stand0);
        let aim1 = ray_at_dist(tgt_c, toward1, stand1);
        for (k, &(x, y)) in pts.iter().enumerate() {
            let (px, py) = if k == 0 {
                aim0
            } else if k + 1 == pts.len() {
                aim1
            } else {
                (x, y)
            };
            world.insert(
                format!("{eid}/p{k}"),
                &eid,
                ObjectType::EdgePoint,
                Shape::Point { x: px, y: py },
                false,
            );
        }
        if pts.len() >= 2 {
            let last = pts.len() - 1;
            let src_centroid = format!("scheme/mols/{src}/centroid");
            let tgt_centroid = format!("scheme/mols/{tgt}/centroid");
            // Pull the endpoint toward the centroid (ink standoff stops it).
            cs.push(Constraint::MinimizeDist {
                a: format!("{eid}/p0"),
                b: src_centroid.clone(),
                opt_dist: 0.0,
                strength: ENDPOINT_POSE_STRENGTH,
            });
            // Pull the shaft tangent onto the ray from the next point through
            // the centroid, so the arrow points at the center.
            cs.push(Constraint::Collinear {
                a: format!("{eid}/p1"),
                b: format!("{eid}/p0"),
                c: src_centroid,
                strength: ENDPOINT_POSE_STRENGTH,
            });
            push_clear_leaves(
                &world,
                &mut hard,
                &format!("{eid}/p0"),
                &format!("scheme/mols/{src}/chem"),
                SCHEME_INK_STANDOFF,
            );
            cs.push(Constraint::MinimizeDist {
                a: format!("{eid}/p{last}"),
                b: tgt_centroid.clone(),
                opt_dist: 0.0,
                strength: ENDPOINT_POSE_STRENGTH,
            });
            cs.push(Constraint::Collinear {
                a: format!("{eid}/p{}", last.saturating_sub(1)),
                b: format!("{eid}/p{last}"),
                c: tgt_centroid,
                strength: ENDPOINT_POSE_STRENGTH,
            });
            push_clear_leaves(
                &world,
                &mut hard,
                &format!("{eid}/p{last}"),
                &format!("scheme/mols/{tgt}/chem"),
                SCHEME_INK_STANDOFF,
            );
        }
        for k in 1..pts.len().saturating_sub(1) {
            cs.push(Constraint::PreferPose {
                a: format!("{eid}/p{k}"),
                x: pts[k].0,
                y: pts[k].1,
                strength: POSE_STRENGTH,
            });
            for mid in &mol_ids {
                push_clear_leaves(
                    &world,
                    &mut hard,
                    &format!("{eid}/p{k}"),
                    &format!("scheme/mols/{mid}/chem"),
                    SCHEME_INK_STANDOFF,
                );
            }
        }
        if let Some(lab) = edge_label_seed(spec, edge, pts) {
            let (aabb, px, py) = lab;
            world.insert(
                format!("{eid}/label"),
                &eid,
                ObjectType::EdgeLabel,
                Shape::Aabb(aabb),
                false,
            );
            cs.push(Constraint::PreferPose {
                a: format!("{eid}/label"),
                x: px,
                y: py,
                strength: POSE_STRENGTH,
            });
            hard.push(HardConstraint::MaxDist {
                a: format!("{eid}/label"),
                b: eid.clone(),
                dist: LABEL_TETHER_PX,
                lagrangian: LAGRANGIAN_TETHER,
            });
            for mid in &mol_ids {
                push_clear_leaves(
                    &world,
                    &mut hard,
                    &format!("{eid}/label"),
                    &format!("scheme/mols/{mid}/chem"),
                    SCHEME_LABEL_INK_STANDOFF,
                );
            }
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
        if world.get(&format!("{a}/label")).is_some() {
            for j in 0..n_edges {
                if i == j {
                    continue;
                }
                hard.push(HardConstraint::Outside {
                    a: format!("{a}/label"),
                    b: format!("scheme/edges/e{j}"),
                    clash_dist: 4.0,
                    lagrangian: LAGRANGIAN_CLEAR,
                });
            }
            hard.push(HardConstraint::Outside {
                a: format!("{a}/label"),
                b: a.clone(),
                clash_dist: 2.0,
                lagrangian: LAGRANGIAN_CLEAR,
            });
        }
    }

    let opts = SolveOpts {
        max_iters: 120,
        noise: 0.0,
        epsilon_px: 1.0,
        ..SolveOpts::default()
    };
    let _ = solve(&mut world, &cs, &hard, &opts);

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
            let rot = tangent_deg(out_paths[i].as_deref().unwrap_or(&seed_pts[i]));
            out_labels[i] = Some((x, y, rot));
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
            let origin = mol_origin(paint, (x, y));
            let n = paint.molecule.atoms.len() as f64;
            let sx: f64 = paint.molecule.atoms.iter().map(|a| a.x).sum();
            let sy: f64 = paint.molecule.atoms.iter().map(|a| a.y).sum();
            return (origin.0 + sx / n, origin.1 + sy / n);
        }
    }
    let (x, y, w, h) = mol_box(positions, sizes, id);
    (x + w * 0.5, y + h * 0.5)
}

/// Point `dist` from `from` toward `toward` (centroid-aiming standoff).
fn ray_at_dist(from: (f64, f64), toward: (f64, f64), dist: f64) -> (f64, f64) {
    let dx = toward.0 - from.0;
    let dy = toward.1 - from.1;
    let l = dx.hypot(dy).max(1e-9);
    (from.0 + dx / l * dist, from.1 + dy / l * dist)
}

fn mol_origin(paint: &DocPaint, elk: (f64, f64)) -> (f64, f64) {
    match paint.scene.viewports.first() {
        Some(vp) => (elk.0 + vp.x + vp.ox, elk.1 + vp.y + vp.oy),
        None => elk,
    }
}

fn chem_in_document(
    paint: &DocPaint,
    elk: (f64, f64),
) -> (Vec<(f64, f64, f64, f64)>, Vec<(usize, f64, f64)>) {
    let origin = mol_origin(paint, elk);
    let mol: &MoleculeIn = &paint.molecule;
    let mut by_idx: HashMap<i32, (f64, f64)> = HashMap::new();
    let mut atoms = Vec::with_capacity(mol.atoms.len());
    for a in &mol.atoms {
        let p = (a.x + origin.0, a.y + origin.1);
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

fn edge_label_seed(
    spec: &DepictSpec,
    edge: &super::EdgeNode,
    pts: &[(f64, f64)],
) -> Option<(Aabb, f64, f64)> {
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
    let (mx, my, mut px, mut py) = label_point(pts);
    match pos {
        super::LabelPos::Below | super::LabelPos::Right => {
            px = -px;
            py = -py;
        }
        _ => {}
    }
    let cx = mx + px * 10.0;
    let cy = my + py * 10.0;
    let w = text.len() as f64 * 11.0 * 0.55 + 4.0;
    let h = 15.0;
    Some((
        Aabb::new(cx - w * 0.5, cy - h * 0.5, cx + w * 0.5, cy + h * 0.5),
        cx,
        cy,
    ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::layout_constraints::Shape;
    use crate::doc::layout_constraints::{
        hard_g, HardConstraint, LAGRANGIAN_CLEAR, LAGRANGIAN_TETHER,
    };

    #[test]
    fn ink_standoff_constant_is_one_and_half_bonds() {
        assert!((SCHEME_INK_STANDOFF - 1.5 * BOND_PX).abs() < 1e-12);
    }

    #[test]
    fn ray_standoff_aims_at_other_centroid() {
        let from = (0.0, 0.0);
        let toward = (100.0, 0.0);
        let p = ray_at_dist(from, toward, 60.0);
        assert!((p.0 - 60.0).abs() < 1e-9 && p.1.abs() < 1e-9, "{p:?}");
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
    fn label_tether_is_hard_maxdist() {
        let h = HardConstraint::MaxDist {
            a: "e0/label".into(),
            b: "e0".into(),
            dist: LABEL_TETHER_PX,
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
        let refined =
            refine_scheme_edges(&spec, &layout, &positions, &[Some(seed)], &paints, &sizes);
        let pts = refined.edge_paths[0].clone().expect("path");
        assert!(pts.len() >= 2);
        let (x0, y0) = pts[0];
        let (x1, y1) = pts[pts.len() - 1];
        assert!(
            (0.0..=200.0).contains(&x0) && (0.0..=200.0).contains(&y0),
            "src endpoint should enter A's AABB: ({x0},{y0})"
        );
        assert!(
            (400.0..=600.0).contains(&x1) && (0.0..=200.0).contains(&y1),
            "tgt endpoint should enter B's AABB: ({x1},{y1})"
        );
        let ca = (100.0, 100.0);
        let cb = (500.0, 100.0);
        let da = (x0 - ca.0).hypot(y0 - ca.1);
        let db = (x1 - cb.0).hypot(y1 - cb.1);
        assert!(
            da >= SCHEME_INK_STANDOFF * 0.75,
            "src must not reach centroid: {da}"
        );
        assert!(
            db >= SCHEME_INK_STANDOFF * 0.75,
            "tgt must not reach centroid: {db}"
        );
        let line_a = point_to_line(ca.0, ca.1, x0, y0, x1, y1);
        let line_b = point_to_line(cb.0, cb.1, x0, y0, x1, y1);
        assert!(
            line_a < 8.0 && line_b < 8.0,
            "shaft should aim through centroids: dA={line_a} dB={line_b} pts={pts:?}"
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
        let refined =
            refine_scheme_edges(&spec, &layout, &positions, &[Some(seed)], &paints, &sizes);
        let pts = refined.edge_paths[0].clone().expect("path");
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
        let expected = tangent_deg(&pts);
        let diff = (rot - expected).abs();
        assert!(
            diff.min((diff - 180.0).abs()) < 15.0,
            "label rotate {rot} vs edge {expected}"
        );
        let mut d_shaft = f64::INFINITY;
        for w in pts.windows(2) {
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
        assert!(
            d_shaft > 1.0,
            "label sits on shaft: d={d_shaft} at ({lx},{ly})"
        );
    }
}
