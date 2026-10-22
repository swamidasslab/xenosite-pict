//! ELK seed + constraint refine for reaction-scheme edges and labels.
//!
//! Molecules stay fixed. Control points and edge labels are the DOF.

use std::collections::HashMap;

use crate::arrows::{edge_anchors, ANCHOR_GAP};
use crate::metrics::BOND_PX;
use crate::scene::MoleculeIn;

use super::layout_constraints::{
    shapes_properly_cross, solve, Constraint, HardConstraint, LayoutWorld, ObjectType, Shape,
    SolveOpts, LAGRANGIAN_CLEAR, LAGRANGIAN_TETHER,
};
use super::layout_items::Aabb;
use super::{DepictSpec, DocPaint, LayoutOpts};

/// Arrow standoff from painted chem (not the mol AABB).
///
/// Temporarily unused: endpoint ink clearance is off so we can debug
/// centroid-seeking energy on its own.
pub const SCHEME_INK_STANDOFF: f64 = 1.5 * BOND_PX;
/// Edge labels sit closer than shafts; unused while endpoint standoff is off.
#[allow(dead_code)]
const SCHEME_LABEL_INK_STANDOFF: f64 = 0.5 * BOND_PX;
/// Minimum gap between nearly-parallel overlapping shafts.
pub const SCHEME_EDGE_CLEARANCE: f64 = BOND_PX;
const LABEL_TETHER_PX: f64 = 8.0;
const POSE_STRENGTH: f64 = 0.35;
/// Primary soft term: each endpoint toward its mol centroid.
const ENDPOINT_TO_CENTROID: f64 = 4.0;
/// Weaker than min-dist. Picks the ELK-facing side of the standoff circle
/// (min-dist alone is flat there) without swapping a 2-point shaft.
const TANGENT_AIM: f64 = 0.4;

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
        let pts = &seed_pts[i];
        // ELK polyline is the seed. Debug pass: no ink standoff. Energy is
        // endpoint → corresponding mol centroid.
        let guides = world.insert(
            format!("{eid}/guides"),
            &eid,
            ObjectType::Group,
            Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
            true,
        );
        let last = pts.len().saturating_sub(1);
        for (k, &(x, y)) in pts.iter().enumerate() {
            world.insert(
                format!("{eid}/p{k}"),
                &eid,
                ObjectType::EdgePoint,
                Shape::Point { x, y },
                false,
            );
            world.insert(
                format!("{guides}/p{k}"),
                &guides,
                ObjectType::EdgePoint,
                Shape::Point { x, y },
                true,
            );
            if k > 0 && k < last {
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
            cs.push(Constraint::MinimizeDist {
                a: format!("{eid}/p0"),
                b: src_centroid.clone(),
                opt_dist: 0.0,
                strength: ENDPOINT_TO_CENTROID,
            });
            cs.push(Constraint::Collinear {
                a: format!("{guides}/p1"),
                b: format!("{eid}/p0"),
                c: src_centroid,
                strength: TANGENT_AIM,
            });
            cs.push(Constraint::MinimizeDist {
                a: format!("{eid}/p{last}"),
                b: tgt_centroid.clone(),
                opt_dist: 0.0,
                strength: ENDPOINT_TO_CENTROID,
            });
            cs.push(Constraint::Collinear {
                a: format!("{guides}/p{}", last.saturating_sub(1)),
                b: format!("{eid}/p{last}"),
                c: tgt_centroid,
                strength: TANGENT_AIM,
            });
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
            let rot = readable_deg(tangent_deg(out_paths[i].as_deref().unwrap_or(&seed_pts[i])));
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
            let n = paint.molecule.atoms.len() as f64;
            let sx: f64 = paint.molecule.atoms.iter().map(|a| a.x).sum();
            let sy: f64 = paint.molecule.atoms.iter().map(|a| a.y).sum();
            return chem_to_document(paint, (x, y), sx / n, sy / n);
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

/// Parallel to the edge, but never upside-down (`(-90, 90]` degrees).
fn readable_deg(deg: f64) -> f64 {
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

    #[test]
    fn ink_standoff_constant_is_one_and_half_bonds() {
        assert!((SCHEME_INK_STANDOFF - 1.5 * BOND_PX).abs() < 1e-12);
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
        let gap = 20.0;
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
        let da0 = (x0 - ca.0).hypot(y0 - ca.1);
        let da_seed = (seed[0].0 - ca.0).hypot(seed[0].1 - ca.1);
        let db1 = (x1 - cb.0).hypot(y1 - cb.1);
        let db_seed = (seed[1].0 - cb.0).hypot(seed[1].1 - cb.1);
        assert!(
            da0 < da_seed - 1.0,
            "src end must move closer to A's centroid: {da0} vs seed {da_seed} pts={pts:?} ca={ca:?}"
        );
        assert!(
            db1 < db_seed - 1.0,
            "tgt end must move closer to B's centroid: {db1} vs seed {db_seed} pts={pts:?} cb={cb:?}"
        );
        let (_, _, rot) = refined.edge_labels[0].expect("label");
        assert!(
            (-90.0..=90.0).contains(&rot),
            "label must not be upside down: {rot}"
        );
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
        assert!(
            (x1 - x0) * (seed[1].0 - seed[0].0) + (y1 - y0) * (seed[1].1 - seed[0].1) > 0.0,
            "must keep ELK direction: {pts:?}"
        );
        assert!(
            (0.0..=200.0).contains(&x0) && (0.0..=200.0).contains(&y0),
            "src endpoint should enter A's AABB: ({x0},{y0})"
        );
        assert!(
            (400.0..=600.0).contains(&x1) && (0.0..=200.0).contains(&y1),
            "tgt endpoint should enter B's AABB: ({x1},{y1})"
        );
        let paint_of: HashMap<&str, &DocPaint> =
            paints.iter().map(|p| (p.id.as_str(), p)).collect();
        let size_of: HashMap<&str, (f64, f64)> =
            HashMap::from([("A", (200.0, 200.0)), ("B", (200.0, 200.0))]);
        let ca = mol_centroid(&paint_of, &positions, &size_of, "A");
        let cb = mol_centroid(&paint_of, &positions, &size_of, "B");
        let da = (x0 - ca.0).hypot(y0 - ca.1);
        let db = (x1 - cb.0).hypot(y1 - cb.1);
        assert!(
            da < 8.0,
            "src should sit on A's centroid with standoff off: {da} pts={pts:?} ca={ca:?}"
        );
        assert!(
            db < 8.0,
            "tgt should sit on B's centroid with standoff off: {db} pts={pts:?} cb={cb:?}"
        );
        let line_a = point_to_line(x0, y0, seed[1].0, seed[1].1, ca.0, ca.1);
        let line_b = point_to_line(x1, y1, seed[0].0, seed[0].1, cb.0, cb.1);
        assert!(
            line_a < 8.0 && line_b < 8.0,
            "each end should sit on its ELK→centroid ray: dA={line_a} dB={line_b} pts={pts:?} ca={ca:?} cb={cb:?}"
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
        let expected = readable_deg(tangent_deg(&pts));
        assert!(
            (rot - expected).abs() < 15.0,
            "label rotate {rot} vs edge {expected}"
        );
        assert!(
            (-90.0..=90.0).contains(&rot),
            "label must not be upside down: {rot}"
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
