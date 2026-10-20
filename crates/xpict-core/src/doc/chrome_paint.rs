//! Paint mol chrome annotations into the scene.
//!
//! Depict first freezes chem (bonds + chem labels + shade). Chrome annotations
//! are then placed by the constraint solver against that fixed chem — never by
//! mutating atom text. Shade clash uses each atom’s **actual** outer PlotDot
//! radius, not the house-max disk.
//!
//! Numbers / text use parallel tracks (NE vs SE). Regions get an analytic pad
//! hull (box / circle / bezier≈rounded hull). Callouts place text + optional
//! arrow. Aromatic circles join the same circle pipeline (deduped from bonds).

use crate::depict::frame_offset;
use crate::doc::chrome::{
    annotations_from_overlay, Annotation, AtomNumberBy, CalloutSpec, CircleSpec, FillSpec,
    LabelRole, MolText, RegionSpec, RegionStyle, RegionTextPlace, StrokeSpec, Z_ATOM_LABEL,
    Z_CALLOUT, Z_CIRCLE, Z_REGION, Z_REGION_LABEL,
};
use crate::doc::layout_constraints::{
    bezier_path_from_proxy, hull_proxy, refine_atom_number, refine_callout, refine_region_text,
    Shape, ATOM_NUMBER_TETHER_PX, ATOM_TEXT_TETHER_PX,
};
use crate::doc::layout_items::{
    callout_arrow, least_clash_side, place_greedy, place_region_edge, tip_standoff_clearing,
    Aabb, InkAtOrigin, Occupancy, PlaceRequest, Seg, Side, ATOM_DISK_R_PX, CLASH_MARGIN_PX,
    CONTAIN_MARGIN_PX,
    HALF_BOND_PX,
};
use crate::doc::spec::{LabelPos, MolNode};
use crate::doc::text_paint::{markup_bounds_at_origin, outline_markup};
use crate::font::FaceStyle;
use crate::labels::{label_ink_shape, label_side, place_backbone, LabelSide};
use crate::metrics::{
    halo_gap_for_weight, halo_stroke_for_weight, BOND_PX, FONT_PX, HALO_OPACITY, STROKE_PX,
};
use crate::rings::{aromatic_ring_circles, find_sssr};
use crate::scene::{AromaticStyle, AtomIn, Layer, LayerName, MoleculeIn, Primitive, Scene};
use std::collections::HashMap;

const NUMBER_FONT_FRAC: f64 = 0.72;
const TEXT_FONT_FRAC: f64 = 0.85;
const NUMBER_GAP_PX: f64 = 4.0;
const TEXT_GAP_PX: f64 = 5.0;

/// Fold wire chrome on `node` into `scene` (viewport-local ink).
///
/// Chem atom text is never cleared here — replace/silence via `star_labels` on
/// `*` atoms only. `atom_label` / `atom_number` are annotations beside chem.
pub fn apply_mol_chrome(node: &MolNode, mol: &MoleculeIn, mut scene: Scene) -> Scene {
    let anns = annotations_from_overlay(&node.overlay_fields());
    let aromatic = mol.aromatic == AromaticStyle::Circle;
    if aromatic {
        strip_depict_aromatic_circles(&mut scene);
    }
    if anns.is_empty() && !aromatic {
        return scene;
    }

    if scene.viewports.is_empty() {
        return scene;
    }

    // Same SVG-frame shift as depict — chrome must paint in viewBox space.
    let (dx, dy) = frame_offset(mol);
    let neighbors = neighbor_coords(mol);
    let caption_boxes = caption_aabbs_from_scene(&scene);
    let shade = shade_disks(mol, dx, dy);
    let mut underlay_prims: Vec<(i32, Primitive)> = Vec::new();
    let mut circle_prims: Vec<(i32, Primitive)> = Vec::new();
    let mut label_prims: Vec<(i32, Primitive)> = Vec::new();
    let mut callout_prims: Vec<(i32, Primitive)> = Vec::new();
    let mut halo_prims: Vec<Primitive> = Vec::new();
    // Mutual clash among chrome labels placed in this pass (+ name).
    let mut placed_label_boxes: Vec<Aabb> = caption_boxes.clone();

    if aromatic {
        for p in aromatic_circle_primitives(mol, dx, dy) {
            circle_prims.push((Z_CIRCLE, p));
        }
    }

    for ann in &anns {
        match ann {
            Annotation::Shade { .. } => {
                // Scores already painted via MoleculeIn → depict shading layer.
            }
            Annotation::Region { spec, z } => {
                // Hull under shade; region text always in the front Labels layer.
                for p in region_primitives(mol, spec, dx, dy, &caption_boxes, &shade) {
                    let is_label = matches!(
                        &p,
                        Primitive::Path {
                            class: Some(c),
                            ..
                        } if c.contains("region-label")
                    );
                    if is_label {
                        label_prims.push((Z_REGION_LABEL, p));
                    } else {
                        underlay_prims.push((*z, p));
                    }
                }
            }
            Annotation::Circle { spec, z, .. } => {
                for p in circle_primitives(mol, node, spec, dx, dy) {
                    circle_prims.push((*z, p));
                }
            }
            Annotation::AtomLabel {
                atom,
                text,
                role,
                scale,
                color,
                z,
            } => {
                let items = expand_atom_labels(mol, *atom, text, *role, node);
                for (ai, content) in items {
                    if let Some((p, box_, halo)) = atom_label_primitive(
                        mol,
                        &neighbors,
                        ai,
                        &content,
                        *role,
                        *scale,
                        color.as_deref(),
                        dx,
                        dy,
                        &shade,
                        &placed_label_boxes,
                        mol.weight,
                    ) {
                        if let Some(h) = halo {
                            halo_prims.push(h);
                        }
                        placed_label_boxes.push(box_);
                        label_prims.push((*z, p));
                    }
                }
            }
            Annotation::Callout { spec, z } => {
                for (p, box_, halo) in
                    callout_primitives(mol, spec, dx, dy, &caption_boxes, &shade, mol.weight)
                {
                    if let Some(h) = halo {
                        halo_prims.push(h);
                    }
                    if box_.width() > 0.0 || box_.height() > 0.0 {
                        placed_label_boxes.push(box_);
                    }
                    callout_prims.push((*z, p));
                }
            }
        }
    }

    underlay_prims.sort_by_key(|(z, _)| *z);
    circle_prims.sort_by_key(|(z, _)| *z);
    label_prims.sort_by_key(|(z, _)| *z);
    callout_prims.sort_by_key(|(z, _)| *z);

    if let Some(vp) = scene.viewports.first_mut() {
        if !underlay_prims.is_empty() {
            let prims: Vec<_> = underlay_prims.into_iter().map(|(_, p)| p).collect();
            prepend_layer(vp, LayerName::Shading, prims);
        }
        if !circle_prims.is_empty() {
            let prims: Vec<_> = circle_prims.into_iter().map(|(_, p)| p).collect();
            merge_layer(vp, LayerName::Marks, prims);
        }
        if !label_prims.is_empty() {
            let prims: Vec<_> = label_prims.into_iter().map(|(_, p)| p).collect();
            merge_layer(vp, LayerName::Labels, prims);
        }
        if !callout_prims.is_empty() {
            let prims: Vec<_> = callout_prims.into_iter().map(|(_, p)| p).collect();
            merge_layer(vp, LayerName::Overlay, prims);
        }
    }
    scene.halo.extend(halo_prims);

    let _ = (Z_ATOM_LABEL, Z_CIRCLE, Z_REGION, Z_REGION_LABEL, Z_CALLOUT);
    scene
}

fn prepend_layer(vp: &mut crate::scene::Viewport, name: LayerName, prims: Vec<Primitive>) {
    if let Some(layer) = vp.layers.iter_mut().find(|l| l.name == name) {
        let mut combined = prims;
        combined.append(&mut layer.primitives);
        layer.primitives = combined;
    } else {
        vp.layers.insert(
            0,
            Layer {
                name,
                primitives: prims,
            },
        );
    }
}

fn strip_depict_aromatic_circles(scene: &mut Scene) {
    for vp in &mut scene.viewports {
        for layer in &mut vp.layers {
            if layer.name != LayerName::Bonds {
                continue;
            }
            layer.primitives.retain(|p| {
                !matches!(
                    p,
                    Primitive::Circle {
                        class: Some(c),
                        ..
                    } if c.contains("aromatic")
                )
            });
        }
    }
}

fn aromatic_circle_primitives(mol: &MoleculeIn, dx: f64, dy: f64) -> Vec<Primitive> {
    let rings = find_sssr(mol, 8);
    let mut coords: HashMap<i32, (f64, f64)> = HashMap::new();
    for a in &mol.atoms {
        coords.insert(a.index, (a.x + dx, a.y + dy));
    }
    let color = mol
        .color
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("#111")
        .to_string();
    let bonds = bond_segs(mol, dx, dy);
    let floor_r = FONT_PX * 0.35;
    aromatic_ring_circles(mol, &rings, &coords)
        .into_iter()
        .map(|(cx0, cy0, r0)| {
            // Same bond-clear rule as atom circles (inside-ring aromatic).
            let (cx, cy, r) =
                circle_clear_bonds(cx0, cy0, r0, cx0, cy0, &bonds, floor_r, CLASH_MARGIN_PX);
            Primitive::Circle {
                cx,
                cy,
                r,
                fill: None,
                stroke: Some(color.clone()),
                stroke_width: STROKE_PX,
                opacity: 1.0,
                class: Some("aromatic circle".into()),
            }
        })
        .collect()
}

/// Fit a circle that clears every bond (+ margin), preferring to keep the
/// intended radius/`(ax,ay)` cover. Bond clearance wins over text coverage.
fn circle_clear_bonds(
    cx0: f64,
    cy0: f64,
    r0: f64,
    ax: f64,
    ay: f64,
    bonds: &[(f64, f64, f64, f64)],
    floor_r: f64,
    margin: f64,
) -> (f64, f64, f64) {
    let half = STROKE_PX.max(0.8) * 0.5;
    let max_r_at = |cx: f64, cy: f64| -> f64 {
        let mut max_r = f64::INFINITY;
        for &(x0, y0, x1, y1) in bonds {
            let d = point_to_seg(cx, cy, x0, y0, x1, y1) - half - margin;
            max_r = max_r.min(d);
        }
        max_r
    };
    // Already clear at the seed size?
    if max_r_at(cx0, cy0) + 1e-9 >= r0 {
        return (cx0, cy0, r0);
    }
    // Nudge center away from penetrating bonds while keeping the atom inside.
    let mut cx = cx0;
    let mut cy = cy0;
    let mut best = (cx0, cy0, max_r_at(cx0, cy0).max(floor_r).min(r0));
    for _ in 0..32 {
        let allowed = max_r_at(cx, cy);
        let atom_need = (ax - cx).hypot(ay - cy) + CLASH_MARGIN_PX;
        let r = allowed.min(r0).max(floor_r.min(allowed));
        if allowed >= atom_need - 1e-9 && allowed >= best.2 {
            best = (cx, cy, allowed.min(r0).max(r));
        }
        if allowed + 1e-9 >= r0 && (ax - cx).hypot(ay - cy) <= r0 {
            return (cx, cy, r0);
        }
        // Step away from the most violated bond.
        let mut worst_pen = 0.0f64;
        let mut nudge = (0.0, 0.0);
        for &(x0, y0, x1, y1) in bonds {
            let d = point_to_seg(cx, cy, x0, y0, x1, y1) - half - margin;
            let pen = r0 - d;
            if pen > worst_pen {
                worst_pen = pen;
                let (px, py) = closest_on_seg(cx, cy, x0, y0, x1, y1);
                let vx = cx - px;
                let vy = cy - py;
                let len = vx.hypot(vy).max(1e-6);
                nudge = (vx / len * 1.5, vy / len * 1.5);
            }
        }
        if worst_pen <= 1e-9 {
            break;
        }
        cx += nudge.0;
        cy += nudge.1;
        // Stay near the atom so the highlight still reads as that atom.
        cx = 0.7 * cx + 0.3 * ax;
        cy = 0.7 * cy + 0.3 * ay;
    }
    let (bx, by, _br) = best;
    let allowed = max_r_at(bx, by);
    let r = allowed.min(r0).max(floor_r.min(allowed.max(0.0)));
    (bx, by, r.max(0.0))
}

fn point_to_seg(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let (qx, qy) = closest_on_seg(px, py, x0, y0, x1, y1);
    (px - qx).hypot(py - qy)
}

fn closest_on_seg(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> (f64, f64) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-18 {
        return (x0, y0);
    }
    let t = ((px - x0) * dx + (py - y0) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    (x0 + t * dx, y0 + t * dy)
}

/// Translate `box_` until edge distance to every bond ≥ `need` (hard clearance).
fn nudge_aabb_clear_bonds(mut box_: Aabb, bonds: &[(f64, f64, f64, f64)], need: f64) -> (f64, f64) {
    let mut dx = 0.0;
    let mut dy = 0.0;
    for _ in 0..48 {
        let mut worst = 0.0f64;
        let mut push = (0.0, 0.0);
        for &(x0, y0, x1, y1) in bonds {
            let d = crate::doc::layout_constraints::edge_signed_distance(
                &Shape::Aabb(box_),
                &Shape::Line { x0, y0, x1, y1 },
            );
            let pen = need - d;
            if pen > worst {
                worst = pen;
                let (cx, cy) = box_.center();
                let (qx, qy) = closest_on_seg(cx, cy, x0, y0, x1, y1);
                let vx = cx - qx;
                let vy = cy - qy;
                let len = vx.hypot(vy).max(1e-6);
                push = (vx / len * (pen + 0.25), vy / len * (pen + 0.25));
            }
        }
        if worst <= 1e-6 {
            break;
        }
        box_ = box_.translated(push.0, push.1);
        dx += push.0;
        dy += push.1;
    }
    (dx, dy)
}

/// Like [`nudge_aabb_clear_bonds`], but reject steps that would push the AABB's
/// closest point more than `tether` past `anchor` (atom / chem-text home).
fn nudge_aabb_clear_bonds_tethered(
    mut box_: Aabb,
    bonds: &[(f64, f64, f64, f64)],
    need: f64,
    anchor: (f64, f64),
    tether: f64,
) -> (f64, f64) {
    let mut dx = 0.0;
    let mut dy = 0.0;
    let anchor_shape = Shape::Point {
        x: anchor.0,
        y: anchor.1,
    };
    for _ in 0..48 {
        let mut worst = 0.0f64;
        let mut push = (0.0, 0.0);
        for &(x0, y0, x1, y1) in bonds {
            let d = crate::doc::layout_constraints::edge_signed_distance(
                &Shape::Aabb(box_),
                &Shape::Line { x0, y0, x1, y1 },
            );
            let pen = need - d;
            if pen > worst {
                worst = pen;
                let (cx, cy) = box_.center();
                let (qx, qy) = closest_on_seg(cx, cy, x0, y0, x1, y1);
                let vx = cx - qx;
                let vy = cy - qy;
                let len = vx.hypot(vy).max(1e-6);
                push = (vx / len * (pen + 0.25), vy / len * (pen + 0.25));
            }
        }
        if worst <= 1e-6 {
            break;
        }
        let cand = box_.translated(push.0, push.1);
        let gap = crate::doc::layout_constraints::edge_signed_distance(
            &Shape::Aabb(cand),
            &anchor_shape,
        );
        if gap > tether + 1e-6 {
            // Would break the ½-bond home tether — stop rather than fling away.
            break;
        }
        box_ = cand;
        dx += push.0;
        dy += push.1;
    }
    (dx, dy)
}

fn atom_xy(a: &AtomIn, dx: f64, dy: f64) -> (f64, f64) {
    (a.x + dx, a.y + dy)
}

fn find_atom<'a>(mol: &'a MoleculeIn, atom: u32) -> Option<&'a AtomIn> {
    mol.atoms
        .iter()
        .find(|a| a.index == atom as i32)
        .or_else(|| mol.atoms.get(atom as usize))
}

fn merge_layer(vp: &mut crate::scene::Viewport, name: LayerName, prims: Vec<Primitive>) {
    if let Some(layer) = vp.layers.iter_mut().find(|l| l.name == name) {
        layer.primitives.extend(prims);
    } else {
        vp.layers.push(Layer {
            name,
            primitives: prims,
        });
    }
}

fn neighbor_coords(mol: &MoleculeIn) -> Vec<Vec<(f64, f64)>> {
    let n = mol.atoms.len();
    let mut out = vec![Vec::new(); n];
    for b in &mol.bonds {
        let (i, j) = (b.begin as usize, b.end as usize);
        if i < n && j < n {
            out[i].push((mol.atoms[j].x, mol.atoms[j].y));
            out[j].push((mol.atoms[i].x, mol.atoms[i].y));
        }
    }
    out
}

fn expand_atom_labels(
    mol: &MoleculeIn,
    atom: u32,
    text: &MolText,
    role: LabelRole,
    node: &MolNode,
) -> Vec<(usize, String)> {
    if role == LabelRole::Number && atom == u32::MAX {
        let by = match text.content() {
            "__number_mapno__" => AtomNumberBy::Mapno,
            _ => AtomNumberBy::Index,
        };
        let policy = node
            .atom_number
            .as_ref()
            .map(|s| s.policy())
            .unwrap_or_else(|| crate::doc::chrome::AtomNumberPolicy {
                by,
                only: None,
                scale: None,
                color: None,
                z: None,
            });
        let atoms: Vec<usize> = if let Some(ref only) = policy.only {
            only.iter()
                .map(|&a| a as usize)
                .filter(|&a| a < mol.atoms.len())
                .collect()
        } else {
            (0..mol.atoms.len()).collect()
        };
        return atoms
            .into_iter()
            .filter_map(|i| {
                let content = match policy.by {
                    AtomNumberBy::Index => Some(mol.atoms[i].index.to_string()),
                    AtomNumberBy::Mapno => mol.atoms[i].mapno.map(|m| m.to_string()),
                }?;
                Some((i, content))
            })
            .collect();
    }
    let i = atom as usize;
    if i >= mol.atoms.len() {
        return Vec::new();
    }
    let content = text.content().trim().to_string();
    if content.is_empty() || content.starts_with("__number_") {
        return Vec::new();
    }
    vec![(i, content)]
}

/// Returns `(primitive, ink AABB, optional glyph-buffered halo)`.
///
/// Shared path for **atom index**, **mapno**, and custom **atom_label** text —
/// all go through [`refine_atom_number`] (hard MaxDist tether to chem text or
/// atom point, hard bond Outside). Custom text uses plain markup (no chem
/// traveling H / charge layout). Numbers may sit on shade; outline halo is
/// paint-only after placement.
fn atom_label_primitive(
    mol: &MoleculeIn,
    neighbors: &[Vec<(f64, f64)>],
    atom: usize,
    content: &str,
    role: LabelRole,
    scale: Option<f64>,
    color: Option<&str>,
    dx: f64,
    dy: f64,
    shades: &[(usize, f64, f64, f64)],
    other_labels: &[Aabb],
    weight: f64,
) -> Option<(Primitive, Aabb, Option<Primitive>)> {
    let a = mol.atoms.get(atom)?;
    let (ax, ay) = atom_xy(a, dx, dy);
    let scale = scale.filter(|s| s.is_finite() && *s > 0.0).unwrap_or(1.0);
    let font_px = FONT_PX
        * scale
        * match role {
            LabelRole::Number => NUMBER_FONT_FRAC,
            LabelRole::Text => TEXT_FONT_FRAC,
        };
    let chem_side = label_side(
        (a.x, a.y),
        neighbors.get(atom).map(|v| v.as_slice()).unwrap_or(&[]),
    );
    let side = annotation_side(role, chem_side);
    let chem_preferred = label_side_to_side(side);
    let min_gap = match role {
        LabelRole::Number => NUMBER_GAP_PX,
        LabelRole::Text => TEXT_GAP_PX,
    };
    // Index/mapno: short tether; custom atom_label text: a bit longer.
    let tether = match role {
        LabelRole::Number => ATOM_NUMBER_TETHER_PX,
        LabelRole::Text => ATOM_TEXT_TETHER_PX,
    };
    let anchor = "middle";
    let (bx0, by0, bx1, by1) = markup_bounds_at_origin(content, font_px, anchor)?;
    let ink = InkAtOrigin {
        x0: bx0,
        y0: by0,
        x1: bx1,
        y1: by1,
    };
    // Bond-clear seed first (same Occupancy rule as region/callout) — PreferPose
    // alone was leaving numbers on ring bonds (kitchen-sink "2"). Also clear
    // aromatic circle strokes (inside phenol) like bond ink.
    let mut occ = Occupancy::from_bonds(mol, dx, dy, STROKE_PX.max(0.8));
    push_aromatic_circle_obstacles(&mut occ, mol, dx, dy);
    for &box_ in other_labels {
        occ.add_aabb(box_);
    }
    for box_ in chem_text_boxes(mol, dx, dy).values() {
        occ.add_aabb(*box_);
    }
    let margin = CLASH_MARGIN_PX + STROKE_PX.max(0.8) * 0.5;
    // Wide atom_label text: chem-travel alternate can point into a bond. Pick
    // the side with least clash at min_gap first so the seed does not start
    // straddling ink (AL cannot always recover from a bad PreferPose-like start).
    let preferred = match role {
        LabelRole::Text => least_clash_side(
            &occ,
            &PlaceRequest {
                anchor: (ax, ay),
                ink,
                preferred: chem_preferred,
                min_gap,
                max_gap: min_gap + tether,
                gap_step: 0.5,
                margin,
                tip_standoff: None,
                label_pullback: 0.0,
            },
        ),
        LabelRole::Number => chem_preferred,
    };
    let greedy = place_greedy(
        &occ,
        &PlaceRequest {
            anchor: (ax, ay),
            ink,
            preferred,
            min_gap,
            // Keep seeds inside the hard MaxDist tether for this role.
            max_gap: min_gap + tether,
            gap_step: 0.5,
            margin,
            tip_standoff: None,
            label_pullback: 0.0,
        },
    );
    let seed = greedy.bounds;
    let bonds = bond_segs(mol, dx, dy);
    let atoms: Vec<(usize, f64, f64)> = mol
        .atoms
        .iter()
        .map(|a| {
            let (x, y) = atom_xy(a, dx, dy);
            (a.index.max(0) as usize, x, y)
        })
        .collect();
    let mut atom_texts: Vec<(usize, Aabb)> = chem_text_boxes(mol, dx, dy)
        .into_iter()
        .map(|(idx, b)| (idx.max(0) as usize, b))
        .collect();
    for (i, &box_) in other_labels.iter().enumerate() {
        atom_texts.push((10_000 + i, box_));
    }
    let atom_idx = a.index.max(0) as usize;
    let (dtx, dty) = refine_atom_number(
        &bonds,
        &atoms,
        &[],
        &atom_texts,
        atom_idx,
        seed,
        CLASH_MARGIN_PX,
        tether,
        None,
        false,
    );
    // AL already hard-clears bonds + MaxDist-tethers to the atom. A post-nudge
    // along bond normals was shoving numbers far off their atoms after a good
    // solve — do a tether-aware residual clear instead (home = chem text or atom).
    let mut clear_segs = bonds.clone();
    clear_segs.extend(aromatic_circle_segs(mol, dx, dy));
    let home = chem_text_boxes(mol, dx, dy)
        .get(&(a.index))
        .map(|b| b.center())
        .unwrap_or((ax, ay));
    let (nudge_x, nudge_y) = nudge_aabb_clear_bonds_tethered(
        seed.translated(dtx, dty),
        &clear_segs,
        CLASH_MARGIN_PX + STROKE_PX.max(0.8) * 0.5,
        home,
        tether,
    );
    let origin_x = greedy.x + dtx + nudge_x;
    let origin_y = greedy.y + dty + nudge_y;

    let (d, data_text, _) = outline_markup(content, origin_x, origin_y, font_px, anchor)?;
    if d.is_empty() {
        return None;
    }
    let box_ = seed.translated(dtx + nudge_x, dty + nudge_y);
    let on_shade = aabb_hits_shade(box_, shades);
    let fill = color
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("#444")
        .to_string();
    let halo = if on_shade {
        halo_for_markup(content, origin_x, origin_y, font_px, anchor, weight)
    } else {
        None
    };
    Some((
        Primitive::Path {
            d,
            stroke: Some("none".into()),
            fill: Some(fill),
            stroke_width: 0.0,
            opacity: 1.0,
            stroke_dasharray: None,
            stroke_linecap: None,
            class: Some(annotation_class(role).into()),
            data_text: Some(data_text),
        },
        box_,
        halo,
    ))
}

fn annotation_class(role: LabelRole) -> &'static str {
    match role {
        LabelRole::Number => "atom-number label",
        LabelRole::Text => "atom-label label",
    }
}

/// Numbers sit opposite chem travel when possible; text takes the other track.
fn annotation_side(role: LabelRole, chem: LabelSide) -> LabelSide {
    match (role, chem) {
        (LabelRole::Number, LabelSide::East) => LabelSide::North,
        (LabelRole::Number, LabelSide::West) => LabelSide::North,
        (LabelRole::Number, LabelSide::North) => LabelSide::East,
        (LabelRole::Number, LabelSide::South) => LabelSide::East,
        (LabelRole::Text, LabelSide::East) => LabelSide::South,
        (LabelRole::Text, LabelSide::West) => LabelSide::South,
        (LabelRole::Text, LabelSide::North) => LabelSide::West,
        (LabelRole::Text, LabelSide::South) => LabelSide::West,
    }
}

fn label_side_to_side(side: LabelSide) -> Side {
    match side {
        LabelSide::East => Side::Right,
        LabelSide::West => Side::Left,
        LabelSide::North => Side::Above,
        LabelSide::South => Side::Below,
    }
}

/// Approximate aromatic ring strokes as segment obstacles (same clear rule as bonds).
fn aromatic_circle_segs(mol: &MoleculeIn, dx: f64, dy: f64) -> Vec<(f64, f64, f64, f64)> {
    if mol.aromatic != AromaticStyle::Circle {
        return Vec::new();
    }
    let rings = find_sssr(mol, 8);
    let mut coords: HashMap<i32, (f64, f64)> = HashMap::new();
    for a in &mol.atoms {
        coords.insert(a.index, (a.x + dx, a.y + dy));
    }
    let n = 24usize;
    let mut out = Vec::new();
    for (cx, cy, r) in aromatic_ring_circles(mol, &rings, &coords) {
        for i in 0..n {
            let t0 = std::f64::consts::TAU * i as f64 / n as f64;
            let t1 = std::f64::consts::TAU * (i + 1) as f64 / n as f64;
            out.push((
                cx + r * t0.cos(),
                cy + r * t0.sin(),
                cx + r * t1.cos(),
                cy + r * t1.sin(),
            ));
        }
    }
    out
}

fn push_aromatic_circle_obstacles(occ: &mut Occupancy, mol: &MoleculeIn, dx: f64, dy: f64) {
    for (x0, y0, x1, y1) in aromatic_circle_segs(mol, dx, dy) {
        occ.segs.push(Seg::new(x0, y0, x1, y1));
    }
}

/// CC-bond length: callout text stands off its target by one bond.
const CALLOUT_TEXT_GAP_PX: f64 = BOND_PX;
/// Air between callout label edge and the arrow start (line pulled back).
const CALLOUT_LABEL_PULLBACK_PX: f64 = CLASH_MARGIN_PX.max(3.0);

fn region_primitives(
    mol: &MoleculeIn,
    spec: &RegionSpec,
    dx: f64,
    dy: f64,
    caption_boxes: &[Aabb],
    shades: &[(usize, f64, f64, f64)],
) -> Vec<Primitive> {
    let included: Vec<i32> = spec
        .atoms
        .iter()
        .filter_map(|&i| find_atom(mol, i).map(|a| a.index))
        .collect();
    if included.is_empty() {
        return Vec::new();
    }
    let include_set: std::collections::HashSet<i32> = included.iter().copied().collect();
    let text_boxes = chem_text_boxes(mol, dx, dy);
    let mut cover: Vec<(f64, f64)> = Vec::new();
    for &idx in &included {
        // Atoms are disks (~0.3 bond), never bare points, for contain.
        if let Some(a) = mol.atoms.iter().find(|a| a.index == idx) {
            let (ax, ay) = atom_xy(a, dx, dy);
            push_disk_samples(&mut cover, ax, ay, ATOM_DISK_R_PX);
        }
        if let Some(box_) = text_boxes.get(&idx) {
            cover.push((box_.x0, box_.y0));
            cover.push((box_.x1, box_.y0));
            cover.push((box_.x1, box_.y1));
            cover.push((box_.x0, box_.y1));
        }
    }
    // Interior-atom shade — actual painted outer disks (fixed with chem).
    for &(idx, cx, cy, r) in shades {
        let idx_i32 = idx as i32;
        if !include_set.contains(&idx_i32) {
            continue;
        }
        if boundary_atom(mol, idx_i32, &include_set) {
            continue;
        }
        for k in 0..8 {
            let t = std::f64::consts::TAU * k as f64 / 8.0;
            cover.push((cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    if cover.is_empty() {
        return Vec::new();
    }
    let exclude_xy: Vec<(f64, f64)> = spec
        .exclude
        .iter()
        .filter_map(|&i| find_atom(mol, i).map(|a| atom_xy(a, dx, dy)))
        .collect();
    let mut pad = CONTAIN_MARGIN_PX;
    for _ in 0..8 {
        let (x0, y0, x1, y1) = aabb(&cover, pad);
        let ok = exclude_xy
            .iter()
            .all(|&(ex, ey)| ex < x0 - 1.0 || ex > x1 + 1.0 || ey < y0 - 1.0 || ey > y1 + 1.0);
        if ok {
            break;
        }
        pad += 4.0;
    }
    let (x0, y0, x1, y1) = aabb(&cover, pad);
    let fill = fill_parts(spec.fill.as_ref());
    let stroke = stroke_parts(spec.stroke.as_ref(), fill.is_some());
    let opacity = fill_opacity(spec.fill.as_ref()).min(1.0);

    // Circle geometry (may grow for an inside label seeded as outside).
    let (circ_cx, circ_cy, mut circ_r) = {
        let (cx, cy, r0) = covering_circle(&cover);
        (cx, cy, (r0 + pad).max(pad))
    };

    let mut region_box = Aabb::new(x0, y0, x1, y1);
    let mut cover_hull = cover.clone();
    let mut pad_hull = pad;
    let mut inside_seed: Option<crate::doc::layout_items::Placement> = None;

    // Inside label: seed like outside, then grow the region so the seed fits.
    if let Some(ref text) = spec.text {
        let styled = text.as_styled();
        let content = styled.content.trim();
        let place = styled.place.unwrap_or(RegionTextPlace::Outside);
        if !content.is_empty() && matches!(place, RegionTextPlace::Inside) {
            let pos = styled.pos.unwrap_or(LabelPos::Above);
            let font_px = FONT_PX
                * styled
                    .scale
                    .filter(|s| s.is_finite() && *s > 0.0)
                    .unwrap_or(1.0)
                * 0.95;
            if let Some((bx0, by0, bx1, by1)) = markup_bounds_at_origin(content, font_px, "middle")
            {
                let ink = InkAtOrigin {
                    x0: bx0,
                    y0: by0,
                    x1: bx1,
                    y1: by1,
                };
                let mut occ = Occupancy::from_bonds(mol, dx, dy, STROKE_PX.max(0.8));
                for &(_, cx, cy, r) in shades {
                    occ.add_aabb(Aabb::new(cx - r, cy - r, cx + r, cy + r));
                }
                for &box_ in caption_boxes {
                    occ.add_aabb(box_);
                }
                let side = label_pos_side(pos);
                let seed = match spec.style {
                    RegionStyle::Circle => {
                        place_circle_label(circ_cx, circ_cy, circ_r, ink, false, side, &occ)
                    }
                    RegionStyle::Box | RegionStyle::Bezier => {
                        // Clash-free seed on the preferred side of the atom
                        // hull (outside). Never park in the ring center.
                        place_region_edge(
                            &occ,
                            region_box,
                            ink,
                            /* inside */ false,
                            side,
                            CLASH_MARGIN_PX,
                            CLASH_MARGIN_PX,
                        )
                    }
                };
                match spec.style {
                    RegionStyle::Circle => {
                        circ_r = grow_circle_to_contain_aabb(
                            circ_cx,
                            circ_cy,
                            circ_r,
                            seed.bounds,
                            CONTAIN_MARGIN_PX,
                        );
                        inside_seed = Some(seed);
                    }
                    RegionStyle::Box => {
                        region_box =
                            grow_aabb_to_contain(region_box, seed.bounds, CONTAIN_MARGIN_PX);
                        inside_seed = Some(seed);
                    }
                    RegionStyle::Bezier => {
                        // Place far from clashes, then grow the proxy so it
                        // surrounds **atoms + label**. Soft refine snugs the
                        // label toward chem; hull is rebuilt after that move.
                        let b = seed.bounds.inflate(CONTAIN_MARGIN_PX);
                        cover_hull.push((b.x0, b.y0));
                        cover_hull.push((b.x1, b.y0));
                        cover_hull.push((b.x1, b.y1));
                        cover_hull.push((b.x0, b.y1));
                        pad_hull = grow_pad_until_hull_contains(
                            &cover_hull,
                            &exclude_xy,
                            pad_hull,
                            seed.bounds,
                            CONTAIN_MARGIN_PX,
                        );
                        inside_seed = Some(seed);
                    }
                }
            }
        }
    }

    let mut out = Vec::new();
    // Solver hull for text refine (box AABB / circle / bezier proxy).
    let mut solve_hull = Shape::Aabb(region_box);

    match spec.style {
        RegionStyle::Box => {
            let Aabb { x0, y0, x1, y1 } = region_box;
            let d = format!(
                "M {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2} Z",
                x0, y0, x1, y0, x1, y1, x0, y1
            );
            out.push(Primitive::Path {
                d,
                stroke: stroke.0.clone(),
                fill: fill.clone().or(Some("none".into())),
                stroke_width: stroke.1,
                opacity,
                stroke_dasharray: dash_array(spec.stroke.as_ref()),
                stroke_linecap: None,
                class: Some("region box".into()),
                data_text: None,
            });
        }
        RegionStyle::Circle => {
            solve_hull = Shape::Circle {
                cx: circ_cx,
                cy: circ_cy,
                r: circ_r,
            };
            out.push(Primitive::Circle {
                cx: circ_cx,
                cy: circ_cy,
                r: circ_r,
                fill: fill.clone(),
                stroke: stroke.0.clone(),
                stroke_width: stroke.1,
                opacity,
                class: Some("region circle".into()),
            });
        }
        RegionStyle::Bezier => {
            // Placeholder — final path is written after inside-label snug so the
            // hull can shrink around atoms + the moved label.
            let proxy = hull_proxy(&cover_hull, &exclude_xy, pad_hull, pad_hull + 6.0);
            solve_hull = proxy.clone();
            out.push(Primitive::Path {
                d: String::new(),
                stroke: stroke.0.clone(),
                fill: fill.clone().or(Some("none".into())),
                stroke_width: stroke.1,
                opacity,
                stroke_dasharray: dash_array(spec.stroke.as_ref()),
                stroke_linecap: None,
                class: Some("region bezier".into()),
                data_text: None,
            });
        }
    }

    if let Some(ref text) = spec.text {
        let styled = text.as_styled();
        let content = styled.content.trim();
        if !content.is_empty() {
            let place = styled.place.unwrap_or(RegionTextPlace::Outside);
            let pos = styled.pos.unwrap_or(LabelPos::Above);
            let font_px = FONT_PX
                * styled
                    .scale
                    .filter(|s| s.is_finite() && *s > 0.0)
                    .unwrap_or(1.0)
                * 0.95;
            let anchor = "middle";
            let fill_c = styled
                .color
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("#333")
                .to_string();
            if let Some((bx0, by0, bx1, by1)) = markup_bounds_at_origin(content, font_px, anchor) {
                let ink = InkAtOrigin {
                    x0: bx0,
                    y0: by0,
                    x1: bx1,
                    y1: by1,
                };
                let mut occ = Occupancy::from_bonds(mol, dx, dy, STROKE_PX.max(0.8));
                for &(_, cx, cy, r) in shades {
                    occ.add_aabb(Aabb::new(cx - r, cy - r, cx + r, cy + r));
                }
                for &box_ in caption_boxes {
                    occ.add_aabb(box_);
                }
                let inside = matches!(place, RegionTextPlace::Inside);
                let side = label_pos_side(pos);
                let bonds = bond_segs(mol, dx, dy);
                let atoms: Vec<(usize, f64, f64)> = included
                    .iter()
                    .filter_map(|&idx| {
                        find_atom(mol, idx as u32).map(|a| {
                            let (x, y) = atom_xy(a, dx, dy);
                            (idx as usize, x, y)
                        })
                    })
                    .collect();
                let region_shades: Vec<(usize, f64, f64, f64)> = shades
                    .iter()
                    .copied()
                    .filter(|(i, _, _, _)| included.iter().any(|&a| a as usize == *i))
                    .collect();
                let seed = if inside {
                    inside_seed.unwrap_or_else(|| match spec.style {
                        RegionStyle::Circle => {
                            place_circle_label(circ_cx, circ_cy, circ_r, ink, false, side, &occ)
                        }
                        RegionStyle::Box | RegionStyle::Bezier => place_region_edge(
                            &occ,
                            region_box,
                            ink,
                            true,
                            side,
                            CLASH_MARGIN_PX,
                            CLASH_MARGIN_PX,
                        ),
                    })
                } else {
                    match spec.style {
                        RegionStyle::Circle => {
                            place_circle_label(circ_cx, circ_cy, circ_r, ink, false, side, &occ)
                        }
                        RegionStyle::Box | RegionStyle::Bezier => place_region_edge(
                            &occ,
                            region_box,
                            ink,
                            false,
                            side,
                            CLASH_MARGIN_PX,
                            CLASH_MARGIN_PX,
                        ),
                    }
                };
                let (dtx, dty) = refine_region_text(
                    &bonds,
                    &atoms,
                    &region_shades,
                    solve_hull,
                    seed.bounds,
                    inside,
                    side,
                    CLASH_MARGIN_PX,
                );
                let mut clear_segs = bonds.clone();
                clear_segs.extend(aromatic_circle_segs(mol, dx, dy));
                let (nudge_x, nudge_y) = nudge_aabb_clear_bonds(
                    seed.bounds.translated(dtx, dty),
                    &clear_segs,
                    CLASH_MARGIN_PX + STROKE_PX.max(0.8) * 0.5,
                );
                let placed = crate::doc::layout_items::Placement {
                    x: seed.x + dtx + nudge_x,
                    y: seed.y + dty + nudge_y,
                    bounds: seed.bounds.translated(dtx + nudge_x, dty + nudge_y),
                    side: seed.side,
                    arrow_from: None,
                    arrow_to: None,
                };
                // After snug, rebuild Bezier around atoms + final label so the
                // outline shrinks toward the molecule instead of staying large.
                if matches!(spec.style, RegionStyle::Bezier) && inside {
                    let mut snug_cover = cover.clone();
                    let b = placed.bounds.inflate(CONTAIN_MARGIN_PX);
                    snug_cover.push((b.x0, b.y0));
                    snug_cover.push((b.x1, b.y0));
                    snug_cover.push((b.x1, b.y1));
                    snug_cover.push((b.x0, b.y1));
                    pad_hull = grow_pad_until_hull_contains(
                        &snug_cover,
                        &exclude_xy,
                        pad,
                        placed.bounds,
                        CONTAIN_MARGIN_PX,
                    );
                    cover_hull = snug_cover;
                }
                if let Some((d, data_text, _)) =
                    outline_markup(content, placed.x, placed.y, font_px, anchor)
                {
                    if !d.is_empty() {
                        out.push(Primitive::Path {
                            d,
                            stroke: Some("none".into()),
                            fill: Some(fill_c),
                            stroke_width: 0.0,
                            opacity: 1.0,
                            stroke_dasharray: None,
                            stroke_linecap: None,
                            class: Some("region-label label".into()),
                            data_text: Some(data_text),
                        });
                    }
                }
            }
        }
    }
    if matches!(spec.style, RegionStyle::Bezier) {
        let proxy = hull_proxy(&cover_hull, &exclude_xy, pad_hull, pad_hull + 6.0);
        let d = match &proxy {
            Shape::PointSet(verts) if verts.len() >= 3 => bezier_path_from_proxy(&proxy, 0.55),
            _ => {
                let hull = convex_hull(&cover_hull);
                if hull.len() >= 3 {
                    rounded_hull_path(&hull, pad_hull)
                } else {
                    let Aabb { x0, y0, x1, y1 } = region_box;
                    format!(
                        "M {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2} Z",
                        x0, y0, x1, y0, x1, y1, x0, y1
                    )
                }
            }
        };
        if let Some(Primitive::Path { d: slot, .. }) = out.iter_mut().find(|p| {
            matches!(
                p,
                Primitive::Path {
                    class: Some(c),
                    ..
                } if c == "region bezier"
            )
        }) {
            *slot = d;
        }
    }
    out
}

fn label_pos_side(pos: LabelPos) -> Side {
    match pos {
        LabelPos::Left => Side::Left,
        LabelPos::Right => Side::Right,
        LabelPos::Above => Side::Above,
        LabelPos::Below => Side::Below,
    }
}

fn aabb(pts: &[(f64, f64)], pad: f64) -> (f64, f64, f64, f64) {
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for &(x, y) in pts {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    (x0 - pad, y0 - pad, x1 + pad, y1 + pad)
}

fn convex_hull(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut p: Vec<(f64, f64)> = pts.to_vec();
    p.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap()
            .then(a.1.partial_cmp(&b.1).unwrap())
    });
    p.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9);
    if p.len() <= 2 {
        return p;
    }
    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    let mut lower = Vec::new();
    for &pt in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], pt) <= 0.0 {
            lower.pop();
        }
        lower.push(pt);
    }
    let mut upper = Vec::new();
    for &pt in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], pt) <= 0.0 {
            upper.pop();
        }
        upper.push(pt);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Closed smooth path: outward-pad hull vertices, then Q mid→vertex→mid segments.
fn rounded_hull_path(hull: &[(f64, f64)], pad: f64) -> String {
    let n = hull.len();
    if n < 3 {
        return String::new();
    }
    let cx = hull.iter().map(|p| p.0).sum::<f64>() / n as f64;
    let cy = hull.iter().map(|p| p.1).sum::<f64>() / n as f64;
    let verts: Vec<(f64, f64)> = hull
        .iter()
        .map(|&(x, y)| {
            let vx = x - cx;
            let vy = y - cy;
            let len = vx.hypot(vy).max(1e-6);
            (x + vx / len * pad, y + vy / len * pad)
        })
        .collect();
    // Midpoints of each edge; start at mid(last, first).
    let mid = |i: usize, j: usize| {
        (
            (verts[i].0 + verts[j].0) * 0.5,
            (verts[i].1 + verts[j].1) * 0.5,
        )
    };
    let start = mid(n - 1, 0);
    let mut d = format!("M {:.2} {:.2}", start.0, start.1);
    for i in 0..n {
        let next = (i + 1) % n;
        let m = mid(i, next);
        d.push_str(&format!(
            " Q {:.2} {:.2} {:.2} {:.2}",
            verts[i].0, verts[i].1, m.0, m.1
        ));
    }
    d.push_str(" Z");
    d
}

fn dash_array(stroke: Option<&StrokeSpec>) -> Option<String> {
    let dash = stroke?.dash.as_deref()?.trim();
    match dash {
        "" | "solid" | "none" => None,
        "dashed" => Some("4 3".into()),
        "dotted" => Some("1 2".into()),
        other => Some(other.to_string()),
    }
}

/// Each item: `(primitive, ink-or-empty AABB, optional glyph-buffered halo)`.
fn callout_primitives(
    mol: &MoleculeIn,
    spec: &CalloutSpec,
    dx: f64,
    dy: f64,
    caption_boxes: &[Aabb],
    shades: &[(usize, f64, f64, f64)],
    weight: f64,
) -> Vec<(Primitive, Aabb, Option<Primitive>)> {
    let mut targets: Vec<(f64, f64)> = Vec::new();
    if let Some(ref atoms) = spec.atoms {
        for &ai in atoms {
            if let Some(a) = find_atom(mol, ai) {
                targets.push(atom_xy(a, dx, dy));
            }
        }
    }
    if let Some(ref bonds) = spec.bonds {
        for &(b0, b1) in bonds {
            if let (Some(a), Some(b)) = (find_atom(mol, b0), find_atom(mol, b1)) {
                let (ax0, ay0) = atom_xy(a, dx, dy);
                let (ax1, ay1) = atom_xy(b, dx, dy);
                targets.push(((ax0 + ax1) * 0.5, (ay0 + ay1) * 0.5));
            }
        }
    }
    if targets.is_empty() {
        return Vec::new();
    }
    let ax = targets.iter().map(|p| p.0).sum::<f64>() / targets.len() as f64;
    let ay = targets.iter().map(|p| p.1).sum::<f64>() / targets.len() as f64;
    let styled = spec.text.as_styled();
    let content = styled.content.trim();
    if content.is_empty() {
        return Vec::new();
    }
    let font_px = FONT_PX
        * styled
            .scale
            .or(spec.scale)
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(1.0)
        * 0.9;
    // One resolved color for both label fill and arrow stroke.
    let color = styled
        .color
        .as_deref()
        .or(spec.color.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("#333")
        .to_string();
    let prefer = spec.prefer.unwrap_or(LabelPos::Right);
    let anchor = "middle";
    let Some((bx0, by0, bx1, by1)) = markup_bounds_at_origin(content, font_px, anchor) else {
        return Vec::new();
    };
    let ink = InkAtOrigin {
        x0: bx0,
        y0: by0,
        x1: bx1,
        y1: by1,
    };
    // Tip must clear chem atom text and stroked bonds — not just the nucleus.
    let chem_boxes: Vec<Aabb> = chem_text_boxes(mol, dx, dy).into_values().collect();
    let bonds = bond_segs(mol, dx, dy);
    let bond_half = STROKE_PX.max(0.8) * 0.5;
    let mut tip_obstacles = chem_boxes.clone();
    for &(x0, y0, x1, y1) in &bonds {
        tip_obstacles.push(Seg::new(x0, y0, x1, y1).thick_aabb(bond_half));
    }
    let side = label_pos_side(prefer);
    let base_tip = (FONT_PX * 0.55).max(4.0);
    let tip_standoff = if spec.arrow {
        tip_standoff_clearing(
            (ax, ay),
            side,
            base_tip,
            &tip_obstacles,
            CLASH_MARGIN_PX,
            CALLOUT_TEXT_GAP_PX + BOND_PX,
        )
    } else {
        base_tip
    };
    // Label edge: past the tip by ≥ half a bond, and ≥ one bond from the atom.
    let min_gap = CALLOUT_TEXT_GAP_PX.max(tip_standoff + HALF_BOND_PX);
    let mut occ = Occupancy::from_molecule(mol, dx, dy, Some(ATOM_DISK_R_PX.max(FONT_PX * 0.45)));
    for &(_, cx, cy, r) in shades {
        occ.add_aabb(Aabb::new(cx - r, cy - r, cx + r, cy + r));
    }
    for &box_ in &chem_boxes {
        occ.add_aabb(box_);
    }
    for &box_ in caption_boxes {
        occ.add_aabb(box_);
    }
    let seed = place_greedy(
        &occ,
        &PlaceRequest {
            anchor: (ax, ay),
            ink,
            preferred: side,
            min_gap,
            max_gap: min_gap + BOND_PX * 1.5,
            gap_step: 4.0,
            margin: CLASH_MARGIN_PX,
            tip_standoff: if spec.arrow { Some(tip_standoff) } else { None },
            label_pullback: CALLOUT_LABEL_PULLBACK_PX,
        },
    );
    let tip0 = seed
        .arrow_to
        .unwrap_or_else(|| tip_point_fallback((ax, ay), side, tip_standoff));
    let (label_dxy, tip_dxy, _) = refine_callout(
        &bonds,
        seed.bounds,
        tip0,
        (ax, ay),
        tip_standoff,
        side,
        CLASH_MARGIN_PX,
        &[],
    );
    let mut clear_segs = bonds.clone();
    clear_segs.extend(aromatic_circle_segs(mol, dx, dy));
    let (nudge_x, nudge_y) = nudge_aabb_clear_bonds(
        seed.bounds.translated(label_dxy.0, label_dxy.1),
        &clear_segs,
        CLASH_MARGIN_PX + STROKE_PX.max(0.8) * 0.5,
    );
    let placed_x = seed.x + label_dxy.0 + nudge_x;
    let placed_y = seed.y + label_dxy.1 + nudge_y;
    let placed_bounds = seed
        .bounds
        .translated(label_dxy.0 + nudge_x, label_dxy.1 + nudge_y);
    let tip = clear_tip_from_obstacles(
        (tip0.0 + tip_dxy.0, tip0.1 + tip_dxy.1),
        (ax, ay),
        &tip_obstacles,
        CLASH_MARGIN_PX,
    );
    // Rebuild arrow: start pulled off the label, tip short of chem/bonds.
    let (arrow_from, arrow_to) = if spec.arrow {
        let (from, _) = callout_arrow(
            placed_bounds,
            (ax, ay),
            side,
            tip_standoff,
            CALLOUT_LABEL_PULLBACK_PX,
        );
        (from, Some(tip))
    } else {
        (None, None)
    };
    let on_shade = aabb_hits_shade(placed_bounds, shades);
    let mut out = Vec::new();
    if spec.arrow {
        if let (Some(from), Some(to)) = (arrow_from, arrow_to) {
            let d = format!("M {:.2} {:.2} L {:.2} {:.2}", from.0, from.1, to.0, to.1);
            out.push((
                Primitive::Path {
                    d,
                    stroke: Some(color.clone()),
                    fill: Some("none".into()),
                    stroke_width: 1.2,
                    opacity: 1.0,
                    stroke_dasharray: None,
                    stroke_linecap: Some("round".into()),
                    class: Some("callout-arrow".into()),
                    data_text: None,
                },
                Aabb::new(0.0, 0.0, 0.0, 0.0),
                None,
            ));
        }
    }
    if let Some((d, data_text, _)) = outline_markup(content, placed_x, placed_y, font_px, anchor) {
        if !d.is_empty() {
            let halo = if on_shade {
                halo_for_markup(content, placed_x, placed_y, font_px, anchor, weight)
            } else {
                None
            };
            out.push((
                Primitive::Path {
                    d,
                    stroke: Some("none".into()),
                    fill: Some(color),
                    stroke_width: 0.0,
                    opacity: 1.0,
                    stroke_dasharray: None,
                    stroke_linecap: None,
                    class: Some("callout-label label".into()),
                    data_text: Some(data_text),
                },
                placed_bounds,
                halo,
            ));
        }
    }
    out
}

fn tip_point_fallback(anchor: (f64, f64), side: Side, standoff: f64) -> (f64, f64) {
    crate::doc::layout_items::tip_point_from_anchor(anchor, side, standoff)
}

/// Nudge tip along the vector away from `anchor` until it clears obstacles.
fn clear_tip_from_obstacles(
    tip: (f64, f64),
    anchor: (f64, f64),
    obstacles: &[Aabb],
    margin: f64,
) -> (f64, f64) {
    let (ax, ay) = anchor;
    let (mut tx, mut ty) = tip;
    let dx = tx - ax;
    let dy = ty - ay;
    let len = dx.hypot(dy).max(1e-6);
    let ux = dx / len;
    let uy = dy / len;
    let mut st = len;
    for _ in 0..40 {
        let tip_box = Aabb::new(tx, ty, tx, ty).inflate(margin.max(0.0));
        if !obstacles.iter().any(|&o| tip_box.overlaps(o)) {
            return (tx, ty);
        }
        st += 2.0;
        tx = ax + ux * st;
        ty = ay + uy * st;
    }
    (tx, ty)
}

fn chem_text_boxes(mol: &MoleculeIn, dx: f64, dy: f64) -> HashMap<i32, Aabb> {
    let mut by_index: HashMap<i32, usize> = HashMap::new();
    for (i, a) in mol.atoms.iter().enumerate() {
        by_index.insert(a.index, i);
    }
    let label_atoms: Vec<crate::labels::AtomIn> = mol
        .atoms
        .iter()
        .map(|a| crate::labels::AtomIn {
            x: a.x + dx,
            y: a.y + dy,
            // Explicit empty = silent; not chem ink.
            label: a
                .label
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        })
        .collect();
    let label_bonds: Vec<crate::labels::BondIn> = mol
        .bonds
        .iter()
        .map(|b| crate::labels::BondIn {
            begin: *by_index.get(&b.begin).unwrap_or(&0),
            end: *by_index.get(&b.end).unwrap_or(&0),
        })
        .collect();
    let (_, placed) = place_backbone(
        &label_atoms,
        &label_bonds,
        FONT_PX,
        FaceStyle::Regular,
        mol.weight,
    );
    let mut out = HashMap::new();
    for (slot, pl) in placed.iter().enumerate() {
        let Some(pl) = pl else { continue };
        let Some(shape) = label_ink_shape(pl, FONT_PX, FaceStyle::Regular) else {
            continue;
        };
        let Some((x0, y0, x1, y1)) = shape.bounds() else {
            continue;
        };
        out.insert(mol.atoms[slot].index, Aabb::new(x0, y0, x1, y1));
    }
    out
}

fn boundary_atom(mol: &MoleculeIn, idx: i32, included: &std::collections::HashSet<i32>) -> bool {
    mol.bonds.iter().any(|b| {
        let other = if b.begin == idx {
            Some(b.end)
        } else if b.end == idx {
            Some(b.begin)
        } else {
            None
        };
        other.is_some_and(|o| !included.contains(&o))
    })
}

fn push_disk_samples(out: &mut Vec<(f64, f64)>, cx: f64, cy: f64, r: f64) {
    for k in 0..8 {
        let t = std::f64::consts::TAU * k as f64 / 8.0;
        out.push((cx + r * t.cos(), cy + r * t.sin()));
    }
}

/// Grow `r` so `box_` sits entirely inside the circle with `margin` clearance.
fn grow_circle_to_contain_aabb(cx: f64, cy: f64, r: f64, box_: Aabb, margin: f64) -> f64 {
    let corners = [
        (box_.x0, box_.y0),
        (box_.x1, box_.y0),
        (box_.x1, box_.y1),
        (box_.x0, box_.y1),
    ];
    let mut need = r;
    for &(x, y) in &corners {
        need = need.max((x - cx).hypot(y - cy) + margin);
    }
    need
}

fn grow_aabb_to_contain(outer: Aabb, inner: Aabb, margin: f64) -> Aabb {
    Aabb::new(
        outer.x0.min(inner.x0 - margin),
        outer.y0.min(inner.y0 - margin),
        outer.x1.max(inner.x1 + margin),
        outer.y1.max(inner.y1 + margin),
    )
}

fn hull_contains_aabb(hull: &Shape, box_: Aabb, margin: f64) -> bool {
    let corners = [
        (box_.x0, box_.y0),
        (box_.x1, box_.y0),
        (box_.x1, box_.y1),
        (box_.x0, box_.y1),
    ];
    corners.iter().all(|&(x, y)| {
        crate::doc::layout_constraints::edge_signed_distance(&Shape::Point { x, y }, hull)
            <= -margin
    })
}

/// Grow outward pad until the hull proxy contains `label` (+ margin).
fn grow_pad_until_hull_contains(
    cover: &[(f64, f64)],
    exclude: &[(f64, f64)],
    pad0: f64,
    label: Aabb,
    margin: f64,
) -> f64 {
    let mut pad = pad0;
    for _ in 0..32 {
        let proxy = hull_proxy(cover, exclude, pad, pad + 6.0);
        if hull_contains_aabb(&proxy, label, margin) {
            return pad;
        }
        pad += 2.0;
    }
    pad
}

fn bond_segs(mol: &MoleculeIn, dx: f64, dy: f64) -> Vec<(f64, f64, f64, f64)> {
    let mut out = Vec::new();
    for b in &mol.bonds {
        if b.begin < 0 || b.end < 0 {
            continue;
        }
        let Some(a) = find_atom(mol, b.begin as u32) else {
            continue;
        };
        let Some(c) = find_atom(mol, b.end as u32) else {
            continue;
        };
        out.push((a.x + dx, a.y + dy, c.x + dx, c.y + dy));
    }
    out
}

/// Smallest circle covering `pts`, center free to move off the centroid.
fn covering_circle(pts: &[(f64, f64)]) -> (f64, f64, f64) {
    if pts.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    if pts.len() == 1 {
        return (pts[0].0, pts[0].1, 0.0);
    }
    let n = pts.len() as f64;
    let mut cx = pts.iter().map(|p| p.0).sum::<f64>() / n;
    let mut cy = pts.iter().map(|p| p.1).sum::<f64>() / n;
    for _ in 0..20 {
        let mut sx = 0.0;
        let mut sy = 0.0;
        let mut wsum = 0.0;
        for &(x, y) in pts {
            let d = (x - cx).hypot(y - cy).max(1e-6);
            let w = d * d;
            sx += x * w;
            sy += y * w;
            wsum += w;
        }
        if wsum < 1e-12 {
            break;
        }
        cx = sx / wsum;
        cy = sy / wsum;
    }
    let r = pts
        .iter()
        .map(|&(x, y)| (x - cx).hypot(y - cy))
        .fold(0.0_f64, f64::max);
    (cx, cy, r)
}

fn place_circle_label(
    cx: f64,
    cy: f64,
    r: f64,
    ink: InkAtOrigin,
    inside: bool,
    side: Side,
    occ: &Occupancy,
) -> crate::doc::layout_items::Placement {
    let gaps = if inside {
        vec![CLASH_MARGIN_PX, 6.0, 10.0]
    } else {
        // Hug the stroke; never farther than half a bond.
        let mut g = Vec::new();
        let mut gap = CLASH_MARGIN_PX;
        while gap <= HALF_BOND_PX + 1e-6 {
            g.push(gap);
            gap += 3.0;
        }
        g
    };
    let slides = [-0.35, -0.15, 0.0, 0.15, 0.35];
    let mut best = circle_label_at(cx, cy, r, ink, inside, side, CLASH_MARGIN_PX, 0.0);
    for &gap in &gaps {
        for &slide in &slides {
            let p = circle_label_at(cx, cy, r, ink, inside, side, gap, slide);
            if !occ.clashes(p.bounds, CLASH_MARGIN_PX) {
                return p;
            }
            best = p;
        }
    }
    best
}

fn circle_label_at(
    cx: f64,
    cy: f64,
    r: f64,
    ink: InkAtOrigin,
    inside: bool,
    side: Side,
    gap: f64,
    slide: f64,
) -> crate::doc::layout_items::Placement {
    let (nx, ny) = match side {
        Side::Right => (1.0, 0.0),
        Side::Left => (-1.0, 0.0),
        Side::Below => (0.0, 1.0),
        Side::Above => (0.0, -1.0),
    };
    let tx = -ny;
    let ty = nx;
    let (x, y) = if inside {
        let dist = (r - gap).max(0.0);
        match side {
            Side::Right => {
                let edge = cx + dist * nx + slide * r * tx;
                let x = edge - ink.x1;
                let y = cy + dist * ny + slide * r * ty - (ink.y0 + ink.y1) * 0.5;
                (x, y)
            }
            Side::Left => {
                let edge = cx + dist * nx + slide * r * tx;
                let x = edge - ink.x0;
                let y = cy + dist * ny + slide * r * ty - (ink.y0 + ink.y1) * 0.5;
                (x, y)
            }
            Side::Above => {
                let edge = cy + dist * ny + slide * r * ty;
                let y = edge - ink.y0;
                let x = cx + dist * nx + slide * r * tx - (ink.x0 + ink.x1) * 0.5;
                (x, y)
            }
            Side::Below => {
                let edge = cy + dist * ny + slide * r * ty;
                let y = edge - ink.y1;
                let x = cx + dist * nx + slide * r * tx - (ink.x0 + ink.x1) * 0.5;
                (x, y)
            }
        }
    } else {
        let dist = r + gap;
        match side {
            Side::Right => {
                let edge = cx + dist * nx + slide * r * tx;
                let x = edge - ink.x0;
                let y = cy + slide * r * ty - (ink.y0 + ink.y1) * 0.5;
                (x, y)
            }
            Side::Left => {
                let edge = cx + dist * nx + slide * r * tx;
                let x = edge - ink.x1;
                let y = cy + slide * r * ty - (ink.y0 + ink.y1) * 0.5;
                (x, y)
            }
            Side::Above => {
                let edge = cy + dist * ny + slide * r * ty;
                let y = edge - ink.y1;
                let x = cx + slide * r * tx - (ink.x0 + ink.x1) * 0.5;
                (x, y)
            }
            Side::Below => {
                let edge = cy + dist * ny + slide * r * ty;
                let y = edge - ink.y0;
                let x = cx + slide * r * tx - (ink.x0 + ink.x1) * 0.5;
                (x, y)
            }
        }
    };
    crate::doc::layout_items::Placement {
        x,
        y,
        bounds: ink.translated(x, y),
        side,
        arrow_from: None,
        arrow_to: None,
    }
}

fn caption_aabbs_from_scene(scene: &Scene) -> Vec<Aabb> {
    let mut out = Vec::new();
    for p in &scene.overlays {
        let Primitive::Path {
            d, class: Some(c), ..
        } = p
        else {
            continue;
        };
        if !(c.contains("mol-caption-metrics") || c.contains("mol-caption")) {
            continue;
        }
        if let Some(box_) = path_aabb(d) {
            out.push(box_);
        }
    }
    out
}

fn path_aabb(d: &str) -> Option<Aabb> {
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
    if nums.len() < 2 {
        return None;
    }
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    let mut i = 0;
    while i + 1 < nums.len() {
        x0 = x0.min(nums[i]);
        x1 = x1.max(nums[i]);
        y0 = y0.min(nums[i + 1]);
        y1 = y1.max(nums[i + 1]);
        i += 2;
    }
    if !x0.is_finite() {
        return None;
    }
    Some(Aabb::new(x0, y0, x1, y1))
}

fn aabb_hits_shade(box_: Aabb, shades: &[(usize, f64, f64, f64)]) -> bool {
    shades
        .iter()
        .any(|&(_, cx, cy, r)| box_.overlaps(Aabb::new(cx - r, cy - r, cx + r, cy + r)))
}

/// Paint-only text halo (like a shadow): same glyph path as the label, white
/// fill + white stroke so the silhouette reads as an **outline** (shape +
/// buffer). Never a layout constraint — call only after placement is fixed.
fn halo_for_markup(
    content: &str,
    x: f64,
    y: f64,
    font_px: f64,
    anchor: &str,
    weight: f64,
) -> Option<Primitive> {
    let (d, _, _) = outline_markup(content, x, y, font_px, anchor)?;
    if d.is_empty() {
        return None;
    }
    // Outward buffer ≈ half the stroke; match chem halo-stroke metric.
    let stroke_w = halo_stroke_for_weight(weight)
        .max(2.0 * halo_gap_for_weight(weight))
        .max(1.0);
    Some(Primitive::Path {
        d,
        stroke: Some("#fff".into()),
        fill: Some("#fff".into()),
        stroke_width: stroke_w,
        opacity: HALO_OPACITY,
        stroke_dasharray: None,
        stroke_linecap: Some("round".into()),
        class: Some("halo chrome-halo".into()),
        data_text: None,
    })
}

/// Shade clash disks: **actual** outer painted radius per atom (PlotDot), not
/// the house-max `SHADE_FRAC` circle.
fn shade_disks(mol: &MoleculeIn, dx: f64, dy: f64) -> Vec<(usize, f64, f64, f64)> {
    let Some(ref scores) = mol.atom_shade else {
        return Vec::new();
    };
    if scores.is_empty() {
        return Vec::new();
    }
    let has_bond = mol
        .bond_shade
        .as_ref()
        .map(|b| !b.is_empty())
        .unwrap_or(false);
    let base_r = crate::depict::shade_base_radius_px(true, has_bond);
    let vmin = mol.shade_vmin.unwrap_or(0.0);
    let vmax = mol.shade_vmax.unwrap_or(1.0);
    let norm = crate::depict::normalize_shade_scores(scores, vmin, vmax);
    let mut out = Vec::new();
    for (i, a) in mol.atoms.iter().enumerate() {
        let Some(&z) = norm.get(i) else {
            continue;
        };
        if !z.is_finite() {
            continue;
        }
        let Some(r) = crate::depict::shade_outer_radius_px(z, base_r) else {
            continue;
        };
        let (cx, cy) = atom_xy(a, dx, dy);
        out.push((a.index.max(0) as usize, cx, cy, r));
    }
    out
}

/// Text that should sit inside an atom circle (custom chrome label or chem).
fn atom_display_text_for_circle<'a>(node: &'a MolNode, atom: &'a AtomIn) -> Option<&'a str> {
    if let Some(ref al) = node.atom_label {
        let (map, ..) = al.entries();
        let key = atom.index.to_string();
        if let Some(t) = map.get(&key) {
            let s = t.content().trim();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    if let Some(s) = atom
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(s);
    }
    // Heteroatom element symbol when chem would paint a label.
    let el = atom.element.as_deref()?.trim();
    if el.is_empty() || el == "C" {
        None
    } else {
        Some(el)
    }
}

/// Radius that encloses atom-center chem/custom text (or a silent-atom floor).
fn atom_circle_radius(text: Option<&str>, r_scale: f64) -> f64 {
    let floor = FONT_PX * 0.55 * r_scale;
    let Some(text) = text.map(str::trim).filter(|s| !s.is_empty()) else {
        return floor;
    };
    let Some((x0, y0, x1, y1)) = markup_bounds_at_origin(text, FONT_PX, "middle") else {
        return floor.max(FONT_PX * 0.75 * r_scale);
    };
    let hw = (x1 - x0) * 0.5;
    let hh = (y1 - y0) * 0.5;
    // Circumscribe label AABB + small air gap.
    ((hw.hypot(hh) + FONT_PX * 0.25) * r_scale).max(floor)
}

fn circle_primitives(
    mol: &MoleculeIn,
    node: &MolNode,
    spec: &CircleSpec,
    dx: f64,
    dy: f64,
) -> Vec<Primitive> {
    let mut out = Vec::new();
    let fill = fill_parts(spec.fill.as_ref());
    let stroke = stroke_parts(spec.stroke.as_ref(), fill.is_some());
    let r_scale = spec
        .scale
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(1.0);
    let floor_r = FONT_PX * 0.55 * r_scale;
    let bonds = bond_segs(mol, dx, dy);

    let text_boxes = chem_text_boxes(mol, dx, dy);
    for &ai in &spec.atoms {
        if let Some(a) = find_atom(mol, ai) {
            let (ax, ay) = atom_xy(a, dx, dy);
            let (cx0, cy0, r0) = if let Some(box_) = text_boxes.get(&a.index) {
                let (ccx, ccy, rr) = covering_circle(&[
                    (box_.x0, box_.y0),
                    (box_.x1, box_.y0),
                    (box_.x1, box_.y1),
                    (box_.x0, box_.y1),
                ]);
                (ccx, ccy, (rr + CLASH_MARGIN_PX) * r_scale)
            } else {
                (
                    ax,
                    ay,
                    atom_circle_radius(atom_display_text_for_circle(node, a), r_scale),
                )
            };
            // Clear non-incident bonds (bonds into this atom must enter the disk).
            let foreign: Vec<_> = bonds
                .iter()
                .copied()
                .filter(|&(x0, y0, x1, y1)| {
                    let d0 = (x0 - ax).hypot(y0 - ay);
                    let d1 = (x1 - ax).hypot(y1 - ay);
                    d0 > ATOM_DISK_R_PX * 2.0 && d1 > ATOM_DISK_R_PX * 2.0
                })
                .collect();
            let (cx, cy, r) =
                circle_clear_bonds(cx0, cy0, r0, ax, ay, &foreign, floor_r, CLASH_MARGIN_PX);
            out.push(Primitive::Circle {
                cx,
                cy,
                r,
                fill: fill.clone(),
                stroke: stroke.0.clone(),
                stroke_width: stroke.1,
                opacity: fill_opacity(spec.fill.as_ref()),
                class: Some("atom-circle".into()),
            });
        }
    }

    if let Some(ref ring) = spec.ring {
        if ring.len() >= 3 {
            let mut sx = 0.0;
            let mut sy = 0.0;
            let mut n = 0usize;
            let mut ring_xy = Vec::new();
            for &ai in ring {
                if let Some(a) = find_atom(mol, ai) {
                    let xy = atom_xy(a, dx, dy);
                    sx += xy.0;
                    sy += xy.1;
                    ring_xy.push(xy);
                    n += 1;
                }
            }
            if n > 0 {
                let mut cover = ring_xy.clone();
                for &ai in ring {
                    if let Some(a) = find_atom(mol, ai) {
                        if let Some(box_) = text_boxes.get(&a.index) {
                            cover.push((box_.x0, box_.y0));
                            cover.push((box_.x1, box_.y1));
                        }
                    }
                }
                let (cx0, cy0, r0) = covering_circle(&cover);
                let rr0 = ((r0 + CLASH_MARGIN_PX) * r_scale).max(floor_r);
                let (cx, cy, rr) = circle_clear_bonds(
                    cx0,
                    cy0,
                    rr0,
                    sx / n as f64,
                    sy / n as f64,
                    &bonds,
                    floor_r,
                    CLASH_MARGIN_PX,
                );
                out.push(Primitive::Circle {
                    cx,
                    cy,
                    r: rr,
                    fill: fill.clone(),
                    stroke: stroke.0.clone(),
                    stroke_width: stroke.1,
                    opacity: fill_opacity(spec.fill.as_ref()),
                    class: Some("ring-circle".into()),
                });
            }
        }
    }
    out
}

/// Resolve stroke for region / circle.
///
/// - `stroke: null` / omitted + **fill present** → no stroke (fill-only).
/// - `stroke: null` / omitted + no fill → house outline `#06c`.
/// - Explicit [`StrokeSpec`] with `color: "none"` (or width ≤ 0) → no stroke.
/// - Explicit [`StrokeSpec`] otherwise → that color (default `#06c`) + width.
fn stroke_parts(stroke: Option<&StrokeSpec>, has_fill: bool) -> (Option<String>, f64) {
    let Some(s) = stroke else {
        if has_fill {
            return (None, 0.0);
        }
        return (Some("#06c".into()), 1.5);
    };
    if s.width.is_some_and(|w| w.is_finite() && w <= 0.0) {
        return (None, 0.0);
    }
    let color = s.color.as_deref().map(str::trim).filter(|c| !c.is_empty());
    match color {
        Some(c) if c.eq_ignore_ascii_case("none") => (None, 0.0),
        Some(c) => {
            let width = s.width.filter(|w| w.is_finite() && *w > 0.0).unwrap_or(1.5);
            (Some(c.to_string()), width)
        }
        // Explicit stroke object without color → house outline (even with fill).
        None => {
            let width = s.width.filter(|w| w.is_finite() && *w > 0.0).unwrap_or(1.5);
            (Some("#06c".into()), width)
        }
    }
}

fn fill_parts(fill: Option<&FillSpec>) -> Option<String> {
    fill.and_then(|f| {
        f.color
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_string)
    })
}

fn fill_opacity(fill: Option<&FillSpec>) -> f64 {
    fill.and_then(|f| f.opacity)
        .filter(|o| o.is_finite() && *o >= 0.0)
        .unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::chrome::{
        AtomNumberBy, AtomNumberSpec, CalloutSpec, FillSpec, MolText, RegionSpec, RegionStyle,
        StrokeSpec,
    };
    use crate::doc::spec::LabelPos;
    use crate::scene::{AtomIn, BondIn};

    #[test]
    fn stroke_none_with_fill_means_no_stroke() {
        assert_eq!(stroke_parts(None, true), (None, 0.0));
    }

    #[test]
    fn stroke_omitted_without_fill_is_house_outline() {
        let (c, w) = stroke_parts(None, false);
        assert_eq!(c.as_deref(), Some("#06c"));
        assert!((w - 1.5).abs() < 1e-12);
    }

    #[test]
    fn stroke_color_none_disables() {
        let s = StrokeSpec {
            color: Some("none".into()),
            ..Default::default()
        };
        assert_eq!(stroke_parts(Some(&s), false), (None, 0.0));
        assert_eq!(stroke_parts(Some(&s), true), (None, 0.0));
    }

    #[test]
    fn fill_only_region_emits_no_stroke_attr() {
        let mol = ethanol();
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            region: Some(crate::doc::chrome::RegionField::One(RegionSpec {
                style: RegionStyle::Box,
                atoms: vec![0, 1],
                exclude: vec![],
                fill: Some(FillSpec {
                    color: Some("#fc0".into()),
                    opacity: Some(0.35),
                }),
                stroke: None,
                text: None,
                z: None,
            })),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let hit = layer_prims(&out).into_iter().find(|p| {
            matches!(
                p,
                Primitive::Path { class: Some(c), .. } if c.contains("region") && c.contains("box")
            )
        });
        match hit.expect("region box") {
            Primitive::Path {
                stroke,
                fill,
                stroke_width,
                ..
            } => {
                assert!(
                    stroke.is_none() || stroke.as_deref() == Some("none"),
                    "fill-only must not outline: stroke={stroke:?}"
                );
                assert!(*stroke_width == 0.0 || stroke.is_none());
                assert_eq!(fill.as_deref(), Some("#fc0"));
            }
            _ => unreachable!(),
        }
    }

    fn ethanol() -> MoleculeIn {
        MoleculeIn {
            id: Some("e".into()),
            atoms: vec![
                AtomIn {
                    index: 0,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 0.0,
                    y: 0.0,
                    label: None,
                    charge: 0,
                    mapno: None,
                },
                AtomIn {
                    index: 1,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 30.0,
                    y: 0.0,
                    label: None,
                    charge: 0,
                    mapno: None,
                },
                AtomIn {
                    index: 2,
                    element: Some("O".into()),
                    z: Some(8),
                    x: 45.0,
                    y: 20.0,
                    label: Some("OH".into()),
                    charge: 0,
                    mapno: None,
                },
            ],
            bonds: vec![
                BondIn {
                    index: 0,
                    begin: 0,
                    end: 1,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
                BondIn {
                    index: 1,
                    begin: 1,
                    end: 2,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
            ],
            color: None,
            atom_shade: None,
            bond_shade: None,
            shade_vmin: None,
            shade_vmax: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            scale: 1.0,
            weight: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
        }
    }

    /// Zig-zag propane; atom index 1 is the middle carbon (degree 2).
    fn propane() -> MoleculeIn {
        MoleculeIn {
            id: Some("ccc".into()),
            atoms: vec![
                AtomIn {
                    index: 0,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 0.0,
                    y: 15.0,
                    label: None,
                    charge: 0,
                    mapno: None,
                },
                AtomIn {
                    index: 1,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 26.0,
                    y: 0.0,
                    label: None,
                    charge: 0,
                    mapno: None,
                },
                AtomIn {
                    index: 2,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 52.0,
                    y: 15.0,
                    label: None,
                    charge: 0,
                    mapno: None,
                },
            ],
            bonds: vec![
                BondIn {
                    index: 0,
                    begin: 0,
                    end: 1,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
                BondIn {
                    index: 1,
                    begin: 1,
                    end: 2,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
            ],
            color: None,
            atom_shade: None,
            bond_shade: None,
            shade_vmin: None,
            shade_vmax: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            scale: 1.0,
            weight: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
        }
    }

    fn path_nums(d: &str) -> Vec<f64> {
        let mut out = Vec::new();
        let mut cur = String::new();
        for c in d.chars() {
            if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E' {
                cur.push(c);
            } else if !cur.is_empty() {
                if let Ok(v) = cur.parse::<f64>() {
                    out.push(v);
                }
                cur.clear();
            }
        }
        if !cur.is_empty() {
            if let Ok(v) = cur.parse::<f64>() {
                out.push(v);
            }
        }
        out
    }

    fn path_centroid(d: &str) -> (f64, f64) {
        let nums = path_nums(d);
        let mut sx = 0.0;
        let mut sy = 0.0;
        let mut n = 0usize;
        let mut i = 0;
        while i + 1 < nums.len() {
            sx += nums[i];
            sy += nums[i + 1];
            n += 1;
            i += 2;
        }
        assert!(n > 0, "empty path");
        (sx / n as f64, sy / n as f64)
    }

    fn layer_prims(scene: &Scene) -> Vec<&Primitive> {
        scene
            .viewports
            .iter()
            .flat_map(|v| v.layers.iter())
            .flat_map(|l| l.primitives.iter())
            .collect()
    }

    #[test]
    fn shade_clash_disk_uses_painted_outer_radius_not_house_max() {
        use crate::metrics::SHADE_FRAC;
        let mut mol = ethanol();
        // Mid score → PlotDot outer < base; clash must not use house-max disk.
        mol.atom_shade = Some(vec![0.0, 0.0, 0.45]);
        let (dx, dy) = frame_offset(&mol);
        let disks = shade_disks(&mol, dx, dy);
        assert_eq!(disks.len(), 1, "only O is shaded; got {disks:?}");
        let (_, _, _, r) = disks[0];
        let base = BOND_PX * SHADE_FRAC;
        assert!(
            r < base - 1e-6,
            "clash r={r} must be painted outer, not house max {base}"
        );
        let expected = crate::depict::shade_outer_radius_px(0.45, base).unwrap();
        assert!(
            (r - expected).abs() < 1e-9,
            "clash r={r} != painted outer {expected}"
        );
    }

    #[test]
    fn atom_number_on_shade_uses_outline_halo_not_box() {
        let mut mol = ethanol();
        mol.atom_shade = Some(vec![1.0, 1.0, 1.0]);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            atom_number: Some(AtomNumberSpec::ByName(AtomNumberBy::Index)),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        // Chem depict also emits class="halo" (bond knockout). Chrome text uses
        // "chrome-halo" — match that specifically.
        let (halo_d, stroke, stroke_w) = out
            .halo
            .iter()
            .find_map(|p| match p {
                Primitive::Path {
                    d,
                    stroke,
                    stroke_width,
                    class: Some(c),
                    ..
                } if c.contains("chrome-halo") => Some((d.as_str(), stroke.clone(), *stroke_width)),
                _ => None,
            })
            .expect("chrome-halo for numbers on shade");
        let n_coords = path_nums(halo_d).len() / 2;
        assert!(
            n_coords > 4,
            "outline halo should follow the glyph, not a 4-corner AABB; got {n_coords} verts"
        );
        assert!(
            stroke
                .as_deref()
                .is_some_and(|s| s != "none" && !s.is_empty()),
            "text halo must stroke the glyph outline (shape + buffer), got stroke={stroke:?}"
        );
        assert!(
            stroke_w > 0.5,
            "outline buffer stroke_width should be > 0; got {stroke_w}"
        );
    }

    #[test]
    fn atom_number_placed_near_svg_atom() {
        let mol = ethanol();
        let (dx, dy) = frame_offset(&mol);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            atom_number: Some(AtomNumberSpec::ByName(AtomNumberBy::Index)),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        // Index numbers use the same MaxDist tether as custom labels: chem text
        // when present (OH on O), else the atom point.
        let oh = chem_text_boxes(&mol, dx, dy)
            .get(&mol.atoms[2].index)
            .copied()
            .expect("OH chem box");
        let (ox, oy) = oh.center();
        let num2 = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                d,
                data_text: Some(t),
                class: Some(c),
                ..
            } if c.contains("atom-number") && t == "2" => Some(path_centroid(d)),
            _ => None,
        });
        let (nx, ny) = num2.expect("atom-number 2");
        let dist = (nx - ox).hypot(ny - oy);
        assert!(
            dist < ATOM_NUMBER_TETHER_PX + 20.0,
            "index number '2' centroid ({nx},{ny}) far from OH center ({ox},{oy}); dist={dist}"
        );
    }

    /// CCC middle carbon (index 1): number sits snug on the atom, and the two
    /// bonds form an arrow that points at the label (chevron tip → number).
    #[test]
    fn atom_number_ccc_index1_snug_on_bond_arrow() {
        use crate::doc::layout_constraints::edge_signed_distance;

        let mol = propane();
        let (dx, dy) = frame_offset(&mol);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            atom_number: Some(AtomNumberSpec::ByName(AtomNumberBy::Index)),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);

        let a0 = &mol.atoms[0];
        let a1 = &mol.atoms[1];
        let a2 = &mol.atoms[2];
        assert_eq!(a1.index, 1);
        let (ax, ay) = (a1.x + dx, a1.y + dy);
        let (n0x, n0y) = (a0.x + dx, a0.y + dy);
        let (n2x, n2y) = (a2.x + dx, a2.y + dy);

        let (nx, ny, nbox) = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                d,
                data_text: Some(t),
                class: Some(c),
                ..
            } if c.contains("atom-number") && t == "1" => {
                let (cx, cy) = path_centroid(d);
                let nums = path_nums(d);
                let mut x0 = f64::INFINITY;
                let mut y0 = f64::INFINITY;
                let mut x1 = f64::NEG_INFINITY;
                let mut y1 = f64::NEG_INFINITY;
                let mut i = 0;
                while i + 1 < nums.len() {
                    x0 = x0.min(nums[i]);
                    y0 = y0.min(nums[i + 1]);
                    x1 = x1.max(nums[i]);
                    y1 = y1.max(nums[i + 1]);
                    i += 2;
                }
                Some((cx, cy, Aabb::new(x0, y0, x1, y1)))
            }
            _ => None,
        })
        .expect("atom-number 1 on CCC");

        // Snug: closest edge of the glyph within a small air gap of the atom.
        let gap = edge_signed_distance(
            &Shape::Aabb(nbox),
            &Shape::Point { x: ax, y: ay },
        );
        assert!(
            gap >= -2.0,
            "number must not sit deep through atom 1; gap={gap}"
        );
        assert!(
            gap <= CLASH_MARGIN_PX + 6.0,
            "number 1 should be snug (near clash margin), gap={gap} (margin={CLASH_MARGIN_PX})"
        );
        assert!(
            gap <= ATOM_NUMBER_TETHER_PX + 1e-3,
            "number 1 must stay inside MaxDist tether; gap={gap}"
        );

        // Arrow: midpoint of the two neighbors → atom 1 → label.
        let mid = ((n0x + n2x) * 0.5, (n0y + n2y) * 0.5);
        let mut adx = ax - mid.0;
        let mut ady = ay - mid.1;
        let alen = adx.hypot(ady).max(1e-6);
        adx /= alen;
        ady /= alen;
        let mut ldx = nx - ax;
        let mut ldy = ny - ay;
        let llen = ldx.hypot(ldy).max(1e-6);
        ldx /= llen;
        ldy /= llen;
        let align = adx * ldx + ady * ldy;
        assert!(
            align > 0.5,
            "bonds should point at number 1 like an arrow: align={align} \
             atom=({ax:.1},{ay:.1}) num=({nx:.1},{ny:.1}) mid=({:.1},{:.1})",
            mid.0,
            mid.1
        );
    }

    #[test]
    fn atom_mapno_placed_near_chem_or_atom() {
        let mut mol = ethanol();
        mol.atoms[0].mapno = Some(11);
        mol.atoms[2].mapno = Some(22);
        let (dx, dy) = frame_offset(&mol);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            atom_number: Some(AtomNumberSpec::ByName(AtomNumberBy::Mapno)),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let oh = chem_text_boxes(&mol, dx, dy)
            .get(&mol.atoms[2].index)
            .copied()
            .expect("OH chem box");
        let (ox, oy) = oh.center();
        let m22 = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                d,
                data_text: Some(t),
                class: Some(c),
                ..
            } if c.contains("atom-number") && t == "22" => Some(path_centroid(d)),
            _ => None,
        });
        let (nx, ny) = m22.expect("mapno 22 on O");
        let dist = (nx - ox).hypot(ny - oy);
        assert!(
            dist < ATOM_NUMBER_TETHER_PX + 20.0,
            "mapno '22' centroid ({nx},{ny}) far from OH center ({ox},{oy}); dist={dist}"
        );
        // Carbon with no chem text: tether to atom point.
        let c_xy = (mol.atoms[0].x + dx, mol.atoms[0].y + dy);
        let m11 = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                d,
                data_text: Some(t),
                class: Some(c),
                ..
            } if c.contains("atom-number") && t == "11" => Some(path_centroid(d)),
            _ => None,
        });
        let (nx, ny) = m11.expect("mapno 11 on C");
        let dist = (nx - c_xy.0).hypot(ny - c_xy.1);
        assert!(
            dist < ATOM_NUMBER_TETHER_PX + 20.0,
            "mapno '11' centroid ({nx},{ny}) far from C ({}, {}); dist={dist}",
            c_xy.0,
            c_xy.1
        );
    }

    #[test]
    fn atom_number_clears_own_chem_oh_box() {
        let mol = ethanol();
        let (dx, dy) = frame_offset(&mol);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            atom_number: Some(AtomNumberSpec::ByName(AtomNumberBy::Index)),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let oh = chem_text_boxes(&mol, dx, dy)
            .get(&mol.atoms[2].index)
            .copied()
            .expect("OH chem box");
        let num2 = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                d,
                data_text: Some(t),
                class: Some(c),
                ..
            } if c.contains("atom-number") && t == "2" => {
                let (cx, cy) = path_centroid(d);
                // Ink AABB approx from path extents.
                let nums = path_nums(d);
                let mut x0 = f64::INFINITY;
                let mut y0 = f64::INFINITY;
                let mut x1 = f64::NEG_INFINITY;
                let mut y1 = f64::NEG_INFINITY;
                let mut i = 0;
                while i + 1 < nums.len() {
                    x0 = x0.min(nums[i]);
                    y0 = y0.min(nums[i + 1]);
                    x1 = x1.max(nums[i]);
                    y1 = y1.max(nums[i + 1]);
                    i += 2;
                }
                Some((cx, cy, Aabb::new(x0, y0, x1, y1)))
            }
            _ => None,
        });
        let (_nx, _ny, nbox) = num2.expect("atom-number 2");
        let overlap_x = nbox.x0 < oh.x1 && nbox.x1 > oh.x0;
        let overlap_y = nbox.y0 < oh.y1 && nbox.y1 > oh.y0;
        assert!(
            !(overlap_x && overlap_y),
            "number '2' AABB {nbox:?} overlaps OH chem box {oh:?}"
        );
    }

    #[test]
    fn atom_label_does_not_clear_chem_oh() {
        use crate::doc::chrome::{AtomLabelDetailed, AtomLabelSpec};
        use std::collections::BTreeMap;
        let mol = ethanol();
        let scene = crate::depict::depict_molecule(&mol);
        let mut labels = BTreeMap::new();
        labels.insert("2".into(), MolText::Plain("exo".into()));
        let node = MolNode {
            atom_label: Some(AtomLabelSpec::Detailed(AtomLabelDetailed {
                labels,
                scale: None,
                color: None,
                z: None,
            })),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let texts: Vec<&str> = layer_prims(&out)
            .into_iter()
            .filter_map(|p| match p {
                Primitive::Path {
                    data_text: Some(t), ..
                } => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|t| *t == "OH" || *t == "HO"),
            "chem OH must remain when atom_label is set; got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| *t == "exo"),
            "custom atom_label must paint; got {texts:?}"
        );
    }

    #[test]
    fn circle_covers_atom_and_clears_bonds() {
        let mol = ethanol();
        let (dx, dy) = frame_offset(&mol);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            circle: Some(crate::doc::chrome::CircleField::One(CircleSpec {
                atoms: vec![2],
                ring: None,
                fill: None,
                stroke: Some(StrokeSpec {
                    color: Some("#06c".into()),
                    width: Some(1.5),
                    dash: None,
                }),
                scale: None,
                z: None,
            })),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let circ = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Circle {
                cx,
                cy,
                r,
                class: Some(c),
                ..
            } if c.contains("atom-circle") => Some((*cx, *cy, *r)),
            _ => None,
        });
        let (cx, cy, r) = circ.expect("atom-circle");
        let (ax, ay) = (mol.atoms[2].x + dx, mol.atoms[2].y + dy);
        assert!(
            (ax - cx).hypot(ay - cy) <= r + 0.75,
            "atom nucleus ({ax},{ay}) outside circle r={r} at ({cx},{cy})"
        );
        // Cover OH text when bonds allow (ethanol O only has one incident bond).
        let box_ = chem_text_boxes(&mol, dx, dy)
            .get(&mol.atoms[2].index)
            .copied()
            .expect("OH text box");
        for (x, y) in [
            (box_.x0, box_.y0),
            (box_.x1, box_.y0),
            (box_.x1, box_.y1),
            (box_.x0, box_.y1),
        ] {
            let d = (x - cx).hypot(y - cy);
            assert!(
                d <= r + 0.75,
                "OH corner ({x},{y}) outside circle r={r} at ({cx},{cy}), d={d}"
            );
        }
        // Non-incident bonds must stay clear (incident C–O may enter the disk).
        let o_idx = mol.atoms[2].index;
        for b in &mol.bonds {
            if b.begin == o_idx || b.end == o_idx {
                continue;
            }
            let a = mol.atoms.iter().find(|a| a.index == b.begin).unwrap();
            let c = mol.atoms.iter().find(|a| a.index == b.end).unwrap();
            let d = crate::doc::layout_constraints::edge_signed_distance(
                &Shape::Circle { cx, cy, r },
                &Shape::Line {
                    x0: a.x + dx,
                    y0: a.y + dy,
                    x1: c.x + dx,
                    y1: c.y + dy,
                },
            );
            assert!(
                d + 1e-6 >= CLASH_MARGIN_PX,
                "atom-circle crosses non-incident bond {}-{}: d={d}",
                b.begin,
                b.end
            );
        }
    }

    #[test]
    fn region_label_lives_in_labels_layer_not_under_shade() {
        use crate::scene::LayerName;
        let mut mol = ethanol();
        mol.atom_shade = Some(vec![0.9, 0.9, 0.9]);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            region: Some(crate::doc::chrome::RegionField::One(RegionSpec {
                style: RegionStyle::Bezier,
                atoms: vec![0, 1, 2],
                exclude: vec![],
                fill: Some(FillSpec {
                    color: Some("#eee".into()),
                    opacity: Some(0.5),
                }),
                stroke: None,
                text: Some(MolText::Styled(crate::doc::chrome::MolTextSpec {
                    content: "aryl".into(),
                    place: Some(RegionTextPlace::Inside),
                    pos: Some(LabelPos::Right),
                    ..Default::default()
                })),
                z: None,
            })),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let vp = &out.viewports[0];
        let shade_i = vp
            .layers
            .iter()
            .position(|l| l.name == LayerName::Shading)
            .expect("shading");
        let labels_i = vp
            .layers
            .iter()
            .position(|l| l.name == LayerName::Labels)
            .expect("labels");
        assert!(
            labels_i > shade_i,
            "Labels layer must paint after Shading (got shade={shade_i} labels={labels_i})"
        );
        let in_labels = vp.layers[labels_i].primitives.iter().any(|p| match p {
            Primitive::Path {
                class: Some(c),
                data_text: Some(t),
                ..
            } if c.contains("region-label") && t == "aryl" => true,
            _ => false,
        });
        assert!(in_labels, "region-label must be in Labels layer");
        let in_shading = vp.layers[shade_i].primitives.iter().any(|p| match p {
            Primitive::Path { class: Some(c), .. } if c.contains("region-label") => true,
            _ => false,
        });
        assert!(
            !in_shading,
            "region-label must not sit under shade in Shading"
        );
    }

    #[test]
    fn atom_label_ortho_clears_bonds() {
        // Custom text labels (e.g. "ortho") use the same bond-clear path as numbers.
        let mol = ethanol();
        let (dx, dy) = frame_offset(&mol);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            atom_label: Some(crate::doc::chrome::AtomLabelSpec::Map(
                [
                    ("0".into(), MolText::Plain("ortho".into())),
                    ("1".into(), MolText::Plain("meta".into())),
                ]
                .into_iter()
                .collect(),
            )),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let boxes: Vec<(String, Aabb)> = layer_prims(&out)
            .into_iter()
            .filter_map(|p| match p {
                Primitive::Path {
                    d,
                    class: Some(c),
                    data_text: Some(t),
                    ..
                } if c.contains("atom-label") => {
                    let nums = path_nums(d);
                    let mut x0 = f64::INFINITY;
                    let mut y0 = f64::INFINITY;
                    let mut x1 = f64::NEG_INFINITY;
                    let mut y1 = f64::NEG_INFINITY;
                    let mut i = 0;
                    while i + 1 < nums.len() {
                        x0 = x0.min(nums[i]);
                        y0 = y0.min(nums[i + 1]);
                        x1 = x1.max(nums[i]);
                        y1 = y1.max(nums[i + 1]);
                        i += 2;
                    }
                    Some((t.clone(), Aabb::new(x0, y0, x1, y1)))
                }
                _ => None,
            })
            .collect();
        assert!(
            boxes.iter().any(|(t, _)| t == "ortho"),
            "expected ortho label; got {boxes:?}"
        );
        for (text, lb) in &boxes {
            for b in &mol.bonds {
                let a = mol.atoms.iter().find(|a| a.index == b.begin).unwrap();
                let c = mol.atoms.iter().find(|a| a.index == b.end).unwrap();
                let d = crate::doc::layout_constraints::edge_signed_distance(
                    &Shape::Aabb(*lb),
                    &Shape::Line {
                        x0: a.x + dx,
                        y0: a.y + dy,
                        x1: c.x + dx,
                        y1: c.y + dy,
                    },
                );
                assert!(
                    d + 1e-6 >= CLASH_MARGIN_PX,
                    "atom-label '{text}' {lb:?} on bond {}-{}: d={d}",
                    b.begin,
                    b.end
                );
            }
        }
    }

    /// Middle carbon of CCC: chem free-space alternate can point along the
    /// chain. Wide "substituent" must still seed off the bonds, not across them.
    #[test]
    fn atom_label_on_chain_avoids_bond_seed() {
        let mol = propane();
        let (dx, dy) = frame_offset(&mol);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            atom_label: Some(crate::doc::chrome::AtomLabelSpec::Map(
                [("1".into(), MolText::Plain("substituent".into()))]
                    .into_iter()
                    .collect(),
            )),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let lb = layer_prims(&out)
            .into_iter()
            .find_map(|p| match p {
                Primitive::Path {
                    d,
                    class: Some(c),
                    data_text: Some(t),
                    ..
                } if c.contains("atom-label") && t == "substituent" => {
                    let nums = path_nums(d);
                    let mut x0 = f64::INFINITY;
                    let mut y0 = f64::INFINITY;
                    let mut x1 = f64::NEG_INFINITY;
                    let mut y1 = f64::NEG_INFINITY;
                    let mut i = 0;
                    while i + 1 < nums.len() {
                        x0 = x0.min(nums[i]);
                        y0 = y0.min(nums[i + 1]);
                        x1 = x1.max(nums[i]);
                        y1 = y1.max(nums[i + 1]);
                        i += 2;
                    }
                    Some(Aabb::new(x0, y0, x1, y1))
                }
                _ => None,
            })
            .expect("substituent label");
        for b in &mol.bonds {
            let a = mol.atoms.iter().find(|a| a.index == b.begin).unwrap();
            let c = mol.atoms.iter().find(|a| a.index == b.end).unwrap();
            let d = crate::doc::layout_constraints::edge_signed_distance(
                &Shape::Aabb(lb),
                &Shape::Line {
                    x0: a.x + dx,
                    y0: a.y + dy,
                    x1: c.x + dx,
                    y1: c.y + dy,
                },
            );
            assert!(
                d + 1e-6 >= CLASH_MARGIN_PX,
                "substituent {lb:?} crosses bond {}-{}: d={d}",
                b.begin,
                b.end
            );
        }
    }

    #[test]
    fn atom_number_clears_bonds() {
        let mol = ethanol();
        let (dx, dy) = frame_offset(&mol);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            atom_number: Some(AtomNumberSpec::ByName(AtomNumberBy::Index)),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let boxes: Vec<Aabb> = layer_prims(&out)
            .into_iter()
            .filter_map(|p| match p {
                Primitive::Path {
                    d, class: Some(c), ..
                } if c.contains("atom-number") => {
                    let nums = path_nums(d);
                    let mut x0 = f64::INFINITY;
                    let mut y0 = f64::INFINITY;
                    let mut x1 = f64::NEG_INFINITY;
                    let mut y1 = f64::NEG_INFINITY;
                    let mut i = 0;
                    while i + 1 < nums.len() {
                        x0 = x0.min(nums[i]);
                        y0 = y0.min(nums[i + 1]);
                        x1 = x1.max(nums[i]);
                        y1 = y1.max(nums[i + 1]);
                        i += 2;
                    }
                    Some(Aabb::new(x0, y0, x1, y1))
                }
                _ => None,
            })
            .collect();
        assert!(!boxes.is_empty(), "expected atom-number glyphs");
        for lb in &boxes {
            for b in &mol.bonds {
                let a = mol.atoms.iter().find(|a| a.index == b.begin).unwrap();
                let c = mol.atoms.iter().find(|a| a.index == b.end).unwrap();
                let d = crate::doc::layout_constraints::edge_signed_distance(
                    &Shape::Aabb(*lb),
                    &Shape::Line {
                        x0: a.x + dx,
                        y0: a.y + dy,
                        x1: c.x + dx,
                        y1: c.y + dy,
                    },
                );
                assert!(
                    d + 1e-6 >= CLASH_MARGIN_PX,
                    "atom-number {lb:?} crosses/near bond {}-{}: d={d} need ≥{}",
                    b.begin,
                    b.end,
                    CLASH_MARGIN_PX
                );
            }
        }
    }

    #[test]
    fn region_inside_label_clears_bonds() {
        let mol = ethanol();
        let (dx, dy) = frame_offset(&mol);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            region: Some(crate::doc::chrome::RegionField::One(RegionSpec {
                style: RegionStyle::Bezier,
                atoms: vec![0, 1, 2],
                exclude: vec![],
                fill: Some(FillSpec {
                    color: Some("#eee".into()),
                    opacity: Some(0.4),
                }),
                stroke: None,
                text: Some(MolText::Styled(crate::doc::chrome::MolTextSpec {
                    content: "aryl".into(),
                    place: Some(RegionTextPlace::Inside),
                    pos: Some(LabelPos::Left),
                    ..Default::default()
                })),
                z: None,
            })),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let label_box = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                d,
                class: Some(c),
                data_text: Some(t),
                ..
            } if c.contains("region-label") && t == "aryl" => {
                let nums = path_nums(d);
                let mut x0 = f64::INFINITY;
                let mut y0 = f64::INFINITY;
                let mut x1 = f64::NEG_INFINITY;
                let mut y1 = f64::NEG_INFINITY;
                let mut i = 0;
                while i + 1 < nums.len() {
                    x0 = x0.min(nums[i]);
                    y0 = y0.min(nums[i + 1]);
                    x1 = x1.max(nums[i]);
                    y1 = y1.max(nums[i + 1]);
                    i += 2;
                }
                Some(Aabb::new(x0, y0, x1, y1))
            }
            _ => None,
        });
        let lb = label_box.expect("aryl region-label");
        // edge_signed vs Line already subtracts half stroke; need clash air only.
        for b in &mol.bonds {
            let a = mol.atoms.iter().find(|a| a.index == b.begin).unwrap();
            let c = mol.atoms.iter().find(|a| a.index == b.end).unwrap();
            let d = crate::doc::layout_constraints::edge_signed_distance(
                &Shape::Aabb(lb),
                &Shape::Line {
                    x0: a.x + dx,
                    y0: a.y + dy,
                    x1: c.x + dx,
                    y1: c.y + dy,
                },
            );
            assert!(
                d + 1e-6 >= CLASH_MARGIN_PX,
                "region label {lb:?} too close to bond {}-{}: d={d} need ≥{}",
                b.begin,
                b.end,
                CLASH_MARGIN_PX
            );
        }
    }

    #[test]
    fn region_bezier_inside_label_grows_hull() {
        // Ex6-style: inside "aryl" must enlarge the bezier proxy vs outside.
        let mol = ethanol();
        let scene = crate::depict::depict_molecule(&mol);
        let mk = |place: RegionTextPlace| {
            let node = MolNode {
                region: Some(crate::doc::chrome::RegionField::One(RegionSpec {
                    style: RegionStyle::Bezier,
                    atoms: vec![0, 1, 2],
                    exclude: vec![],
                    fill: Some(FillSpec {
                        color: Some("#eee".into()),
                        opacity: None,
                    }),
                    stroke: Some(StrokeSpec {
                        color: Some("#c44".into()),
                        width: Some(1.2),
                        dash: None,
                    }),
                    text: Some(MolText::Styled(crate::doc::chrome::MolTextSpec {
                        content: "aryl".into(),
                        place: Some(place),
                        pos: Some(LabelPos::Left),
                        ..Default::default()
                    })),
                    z: None,
                })),
                ..Default::default()
            };
            let out = apply_mol_chrome(&node, &mol, scene.clone());
            let d = layer_prims(&out)
                .into_iter()
                .find_map(|p| match p {
                    Primitive::Path {
                        d, class: Some(c), ..
                    } if c.contains("region") && c.contains("bezier") => Some(d.clone()),
                    _ => None,
                })
                .expect("bezier region");
            let nums = path_nums(&d);
            let area_proxy = {
                let xs: Vec<_> = nums.iter().step_by(2).copied().collect();
                let ys: Vec<_> = nums.iter().skip(1).step_by(2).copied().collect();
                let w = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                    - xs.iter().cloned().fold(f64::INFINITY, f64::min);
                let h = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                    - ys.iter().cloned().fold(f64::INFINITY, f64::min);
                w * h
            };
            area_proxy
        };
        let out_a = mk(RegionTextPlace::Outside);
        let in_a = mk(RegionTextPlace::Inside);
        assert!(
            in_a > out_a + 1.0,
            "inside label should grow bezier AABB proxy: inside={in_a} outside={out_a}"
        );
    }

    /// Ex6 ethylbenzene: ring include + ethyl exclude + inside "aryl".
    /// Must not emit a self-cutting path with wild cubic handles.
    #[test]
    fn region_bezier_exclude_inside_label_stays_sane() {
        // Regular hexagon (side ~30) + ethyl up from atom 0.
        let mut atoms = Vec::new();
        for i in 0..6 {
            let t = std::f64::consts::TAU * i as f64 / 6.0 - std::f64::consts::FRAC_PI_2;
            atoms.push(AtomIn {
                index: i as i32,
                element: Some("C".into()),
                z: Some(6),
                x: 50.0 + 30.0 * t.cos(),
                y: 50.0 + 30.0 * t.sin(),
                label: None,
                charge: 0,
                mapno: None,
            });
        }
        atoms.push(AtomIn {
            index: 6,
            element: Some("C".into()),
            z: Some(6),
            x: 50.0,
            y: 50.0 - 30.0 - 26.0,
            label: None,
            charge: 0,
            mapno: None,
        });
        atoms.push(AtomIn {
            index: 7,
            element: Some("C".into()),
            z: Some(6),
            x: 50.0 + 20.0,
            y: 50.0 - 30.0 - 26.0 - 20.0,
            label: None,
            charge: 0,
            mapno: None,
        });
        let mut bonds = Vec::new();
        for i in 0..6 {
            bonds.push(BondIn {
                index: i as i32,
                begin: i as i32,
                end: ((i + 1) % 6) as i32,
                order: 1.0,
                stereo: None,
                interior: None,
            });
        }
        bonds.push(BondIn {
            index: 6,
            begin: 0,
            end: 6,
            order: 1.0,
            stereo: None,
            interior: None,
        });
        bonds.push(BondIn {
            index: 7,
            begin: 6,
            end: 7,
            order: 1.0,
            stereo: None,
            interior: None,
        });
        let mol = MoleculeIn {
            id: Some("etph".into()),
            atoms,
            bonds,
            color: None,
            atom_shade: None,
            bond_shade: None,
            shade_vmin: None,
            shade_vmax: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            scale: 1.0,
            weight: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
        };
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            region: Some(crate::doc::chrome::RegionField::One(RegionSpec {
                style: RegionStyle::Bezier,
                atoms: vec![0, 1, 2, 3, 4, 5],
                exclude: vec![6, 7],
                fill: Some(FillSpec {
                    color: Some("#eee".into()),
                    opacity: None,
                }),
                stroke: Some(StrokeSpec {
                    color: Some("#c44".into()),
                    width: Some(1.2),
                    dash: None,
                }),
                text: Some(MolText::Styled(crate::doc::chrome::MolTextSpec {
                    content: "aryl".into(),
                    place: Some(RegionTextPlace::Inside),
                    pos: Some(LabelPos::Left),
                    ..Default::default()
                })),
                z: None,
            })),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let d = layer_prims(&out)
            .into_iter()
            .find_map(|p| match p {
                Primitive::Path {
                    d, class: Some(c), ..
                } if c.contains("region") && c.contains("bezier") => Some(d.clone()),
                _ => None,
            })
            .expect("bezier region");
        assert!(
            d.contains('C') || d.contains('c'),
            "expected centripetal Catmull–Rom cubics: {d}"
        );
        let nums = path_nums(&d);
        let xs: Vec<_> = nums.iter().step_by(2).copied().collect();
        let ys: Vec<_> = nums.iter().skip(1).step_by(2).copied().collect();
        let (min_x, max_x) = (
            xs.iter().cloned().fold(f64::INFINITY, f64::min),
            xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        );
        let (min_y, max_y) = (
            ys.iter().cloned().fold(f64::INFINITY, f64::min),
            ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        );
        // Ring diameter ~60 + pad; exclude clips the ethyl side. A blown-up
        // bowtie used to span 150+ px with handles far outside.
        assert!(
            (max_x - min_x) < 160.0 && (max_y - min_y) < 130.0,
            "hull AABB blew up: {min_x}..{max_x} x {min_y}..{max_y}; d={d}"
        );
        let (dx, dy) = frame_offset(&mol);
        let (cx, cy) = (50.0 + dx, 50.0 + dy);
        assert!(
            cx >= min_x && cx <= max_x && cy >= min_y && cy <= max_y,
            "ring center ({cx},{cy}) outside hull AABB"
        );
        // Inside label must not sit in the ring nucleus (outside-seed + grow).
        let aryl = layer_prims(&out)
            .into_iter()
            .find_map(|p| match p {
                Primitive::Path {
                    d,
                    class: Some(c),
                    data_text: Some(t),
                    ..
                } if c.contains("region-label") && t == "aryl" => {
                    let nums = path_nums(d);
                    let xs: Vec<_> = nums.iter().step_by(2).copied().collect();
                    let ys: Vec<_> = nums.iter().skip(1).step_by(2).copied().collect();
                    Some((
                        (xs.iter().sum::<f64>() / xs.len() as f64),
                        (ys.iter().sum::<f64>() / ys.len() as f64),
                    ))
                }
                _ => None,
            })
            .expect("aryl region-label");
        let dist_center = (aryl.0 - cx).hypot(aryl.1 - cy);
        assert!(
            dist_center > 18.0,
            "aryl label at {:?} too close to ring center ({cx},{cy}); dist={dist_center}",
            aryl
        );
        // Soft snug to chem must pull the outside seed inward — not leave it
        // parked at the clash-free far edge of a grown hull.
        let nearest_ring = (0..6)
            .map(|i| {
                let t = std::f64::consts::TAU * i as f64 / 6.0 - std::f64::consts::FRAC_PI_2;
                let ax = 50.0 + dx + 30.0 * t.cos();
                let ay = 50.0 + dy + 30.0 * t.sin();
                (aryl.0 - ax).hypot(aryl.1 - ay)
            })
            .fold(f64::INFINITY, f64::min);
        assert!(
            nearest_ring < 42.0,
            "aryl label at {:?} still far from ring (nearest={nearest_ring}); expected snug to chem",
            aryl
        );
        assert!(
            aryl.0 < cx - 8.0,
            "aryl label should stay on the preferred left of the ring: {:?}",
            aryl
        );
    }

    #[test]
    fn region_bezier_path_has_q_and_covers_atoms() {
        let mol = ethanol();
        let (dx, dy) = frame_offset(&mol);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            region: Some(crate::doc::chrome::RegionField::One(RegionSpec {
                style: RegionStyle::Bezier,
                atoms: vec![0, 1, 2],
                exclude: vec![],
                fill: Some(FillSpec {
                    color: Some("#c44".into()),
                    opacity: Some(0.15),
                }),
                stroke: Some(StrokeSpec {
                    color: Some("#c44".into()),
                    width: Some(1.2),
                    dash: None,
                }),
                text: Some(MolText::Plain("Et".into())),
                z: None,
            })),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let d = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                d, class: Some(c), ..
            } if c.contains("region") && c.contains("bezier") => Some(d.clone()),
            _ => None,
        });
        let d = d.expect("bezier region path");
        assert!(
            d.contains('C') || d.contains('c') || d.contains('Q') || d.contains('q'),
            "bezier region missing cubic/quadratic curves: {d}"
        );
        // Each atom (SVG) should lie inside the AABB of the path coords (+ pad slack).
        let nums = path_nums(&d);
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut i = 0;
        while i + 1 < nums.len() {
            min_x = min_x.min(nums[i]);
            max_x = max_x.max(nums[i]);
            min_y = min_y.min(nums[i + 1]);
            max_y = max_y.max(nums[i + 1]);
            i += 2;
        }
        for a in &mol.atoms {
            let (x, y) = (a.x + dx, a.y + dy);
            assert!(
                x >= min_x - 1.0 && x <= max_x + 1.0 && y >= min_y - 1.0 && y <= max_y + 1.0,
                "atom ({x},{y}) outside bezier AABB ({min_x},{min_y})-({max_x},{max_y})"
            );
        }
    }

    #[test]
    fn callout_arrow_tip_stands_off_atom() {
        let mol = ethanol();
        let (dx, dy) = frame_offset(&mol);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            callout: Some(crate::doc::chrome::CalloutField::One(CalloutSpec {
                atoms: Some(vec![2]),
                bonds: None,
                text: MolText::Plain("OH".into()),
                arrow: true,
                prefer: Some(LabelPos::Left),
                color: None,
                scale: None,
                z: None,
            })),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let arrow = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                d,
                stroke,
                class: Some(c),
                ..
            } if c.contains("callout-arrow") => Some((d.clone(), stroke.clone())),
            _ => None,
        });
        let (d, arrow_stroke) = arrow.expect("callout arrow");
        let nums = path_nums(&d);
        assert!(nums.len() >= 4, "arrow needs ≥2 points: {d}");
        let (tx, ty) = (nums[nums.len() - 2], nums[nums.len() - 1]);
        let (ax, ay) = (mol.atoms[2].x + dx, mol.atoms[2].y + dy);
        let tip_dist = (tx - ax).hypot(ty - ay);
        let floor = FONT_PX * 0.55;
        assert!(
            tip_dist + 1e-6 >= floor * 0.6,
            "tip dist {tip_dist} should clear atom ink (~{floor})"
        );
        // Tip must not sit inside the chem HO text (west-facing H).
        let chem = chem_text_boxes(&mol, dx, dy);
        if let Some(oh) = chem.get(&mol.atoms[2].index) {
            let tip_box = Aabb::new(tx, ty, tx, ty).inflate(CLASH_MARGIN_PX);
            assert!(
                !tip_box.overlaps(*oh),
                "tip ({tx},{ty}) overlaps chem OH box {oh:?}"
            );
            assert!(
                tip_dist + 1e-6 >= (ax - oh.x0) + CLASH_MARGIN_PX * 0.5
                    || tx <= oh.x0 - CLASH_MARGIN_PX + 1.0,
                "tip should clear past HO left edge x0={}",
                oh.x0
            );
        }
        let label = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                d,
                fill,
                class: Some(c),
                ..
            } if c.contains("callout-label") => Some((path_centroid(d), fill.clone())),
            _ => None,
        });
        let ((lx, _ly), label_fill) = label.expect("callout label");
        assert!(
            lx < ax - 8.0,
            "callout label x={lx} should be left of atom x={ax}"
        );
        // Label edge → tip separation ≥ half a bond (tip not glued to label).
        let arrow_from_x = nums[0];
        let tip_label_gap = (tx - arrow_from_x).abs();
        assert!(
            tip_label_gap + 1e-6 >= HALF_BOND_PX * 0.85,
            "tip↔label gap {tip_label_gap} < half-bond {}",
            HALF_BOND_PX
        );
        // Arrow start pulled back from the label AABB (not flush with ink).
        let label_box = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                d, class: Some(c), ..
            } if c.contains("callout-label") => {
                let nums = path_nums(d);
                let mut x0 = f64::INFINITY;
                let mut y0 = f64::INFINITY;
                let mut x1 = f64::NEG_INFINITY;
                let mut y1 = f64::NEG_INFINITY;
                let mut i = 0;
                while i + 1 < nums.len() {
                    x0 = x0.min(nums[i]);
                    y0 = y0.min(nums[i + 1]);
                    x1 = x1.max(nums[i]);
                    y1 = y1.max(nums[i + 1]);
                    i += 2;
                }
                Some(Aabb::new(x0, y0, x1, y1))
            }
            _ => None,
        });
        let lb = label_box.expect("callout label box");
        let from_box = Aabb::new(nums[0], nums[1], nums[0], nums[1]).inflate(0.5);
        assert!(
            !from_box.overlaps(lb.inflate(-0.25)),
            "arrow start ({},{}) must be pulled back off label {lb:?}",
            nums[0],
            nums[1]
        );
        assert!(
            arrow_from_x + 1e-6 >= lb.x1 + CALLOUT_LABEL_PULLBACK_PX * 0.5
                || arrow_from_x <= lb.x0 - CALLOUT_LABEL_PULLBACK_PX * 0.5,
            "arrow from x={arrow_from_x} should clear label x1={} by pullback",
            lb.x1
        );
        assert_eq!(
            arrow_stroke.as_deref(),
            label_fill.as_deref(),
            "default: arrow stroke == label fill"
        );
    }

    #[test]
    fn callout_color_applies_to_text_and_arrow() {
        let mol = ethanol();
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            callout: Some(crate::doc::chrome::CalloutField::One(CalloutSpec {
                atoms: Some(vec![2]),
                bonds: None,
                text: MolText::Plain("OH".into()),
                arrow: true,
                prefer: Some(LabelPos::Right),
                color: Some("#c00".into()),
                scale: None,
                z: None,
            })),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let arrow_stroke = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                stroke: Some(s),
                class: Some(c),
                ..
            } if c.contains("callout-arrow") => Some(s.clone()),
            _ => None,
        });
        let label_fill = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                fill: Some(f),
                class: Some(c),
                ..
            } if c.contains("callout-label") => Some(f.clone()),
            _ => None,
        });
        assert_eq!(arrow_stroke.as_deref(), Some("#c00"));
        assert_eq!(label_fill.as_deref(), Some("#c00"));
    }

    #[test]
    fn circle_inside_label_seeds_like_outside_not_center() {
        let mol = ethanol();
        let scene = crate::depict::depict_molecule(&mol);
        let mk = |place: RegionTextPlace| {
            let node = MolNode {
                region: Some(crate::doc::chrome::RegionField::One(RegionSpec {
                    style: RegionStyle::Circle,
                    atoms: vec![0, 1],
                    exclude: vec![],
                    fill: None,
                    stroke: None,
                    text: Some(MolText::Styled(crate::doc::chrome::MolTextSpec {
                        content: "aryl".into(),
                        pos: Some(LabelPos::Above),
                        place: Some(place),
                        ..Default::default()
                    })),
                    z: None,
                })),
                ..Default::default()
            };
            let out = apply_mol_chrome(&node, &mol, scene.clone());
            let circ = layer_prims(&out).into_iter().find_map(|p| match p {
                Primitive::Circle {
                    cx,
                    cy,
                    r,
                    class: Some(c),
                    ..
                } if c.contains("region") => Some((*cx, *cy, *r)),
                _ => None,
            });
            let label = layer_prims(&out).into_iter().find_map(|p| match p {
                Primitive::Path {
                    d, class: Some(c), ..
                } if c.contains("region-label") => Some(path_centroid(d)),
                _ => None,
            });
            (circ.expect("region circle"), label.expect("region label"))
        };
        let ((ocx, ocy, or_), (olx, oly)) = mk(RegionTextPlace::Outside);
        let ((icx, icy, ir), (ilx, ily)) = mk(RegionTextPlace::Inside);
        assert!((ocx - icx).abs() < 1e-6 && (ocy - icy).abs() < 1e-6);
        assert!(
            ir + 1e-6 >= or_,
            "inside should grow circle: outside_r={or_} inside_r={ir}"
        );
        let to_outside = (ilx - olx).hypot(ily - oly);
        let to_center = (ilx - icx).hypot(ily - icy);
        assert!(
            to_outside < to_center,
            "inside label should stay near outside seed, not center: \
             dist_seed={to_outside} dist_center={to_center}"
        );
        assert!(
            ily < icy,
            "Above seed should keep label above center: ily={ily} icy={icy}"
        );
    }

    #[test]
    fn region_inside_label_hugs_top_edge_not_center() {
        let mol = ethanol();
        let (dx, dy) = frame_offset(&mol);
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            region: Some(crate::doc::chrome::RegionField::One(RegionSpec {
                style: RegionStyle::Box,
                atoms: vec![0, 1, 2],
                exclude: vec![],
                fill: None,
                stroke: None,
                text: Some(MolText::Styled(crate::doc::chrome::MolTextSpec {
                    content: "aryl".into(),
                    pos: Some(LabelPos::Above),
                    place: Some(RegionTextPlace::Inside),
                    ..Default::default()
                })),
                z: None,
            })),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let region_box = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                d, class: Some(c), ..
            } if c.contains("region") && c.contains("box") => {
                let nums = path_nums(d);
                let mut x0 = f64::INFINITY;
                let mut y0 = f64::INFINITY;
                let mut x1 = f64::NEG_INFINITY;
                let mut y1 = f64::NEG_INFINITY;
                let mut i = 0;
                while i + 1 < nums.len() {
                    x0 = x0.min(nums[i]);
                    x1 = x1.max(nums[i]);
                    y0 = y0.min(nums[i + 1]);
                    y1 = y1.max(nums[i + 1]);
                    i += 2;
                }
                Some((x0, y0, x1, y1))
            }
            _ => None,
        });
        let (rx0, ry0, rx1, ry1) = region_box.expect("region box");
        let label_cy = layer_prims(&out).into_iter().find_map(|p| match p {
            Primitive::Path {
                d, class: Some(c), ..
            } if c.contains("region-label") => Some(path_centroid(d).1),
            _ => None,
        });
        let ly = label_cy.expect("region label");
        assert!(
            ly > ry0 && ly < ry1,
            "inside label y={ly} should sit in the region box {ry0}..{ry1}"
        );
        let _ = (dx, dy, rx0, rx1);
    }

    #[test]
    fn region_inside_label_grows_box_to_contain() {
        let mol = ethanol();
        let scene = crate::depict::depict_molecule(&mol);
        let mk = |place: RegionTextPlace| {
            let node = MolNode {
                region: Some(crate::doc::chrome::RegionField::One(RegionSpec {
                    style: RegionStyle::Box,
                    atoms: vec![0, 1],
                    exclude: vec![],
                    fill: None,
                    stroke: None,
                    text: Some(MolText::Styled(crate::doc::chrome::MolTextSpec {
                        content: "aryl".into(),
                        place: Some(place),
                        pos: Some(LabelPos::Left),
                        ..Default::default()
                    })),
                    z: None,
                })),
                ..Default::default()
            };
            let out = apply_mol_chrome(&node, &mol, scene.clone());
            let region_d = layer_prims(&out)
                .into_iter()
                .find_map(|p| match p {
                    Primitive::Path {
                        d, class: Some(c), ..
                    } if c.contains("region") && c.contains("box") => Some(d.clone()),
                    _ => None,
                })
                .expect("region path");
            let label_box = layer_prims(&out).into_iter().find_map(|p| match p {
                Primitive::Path {
                    d, class: Some(c), ..
                } if c.contains("region-label") => {
                    let nums = path_nums(&d);
                    let xs: Vec<_> = nums.iter().step_by(2).copied().collect();
                    let ys: Vec<_> = nums.iter().skip(1).step_by(2).copied().collect();
                    Some(Aabb::new(
                        xs.iter().cloned().fold(f64::INFINITY, f64::min),
                        ys.iter().cloned().fold(f64::INFINITY, f64::min),
                        xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                        ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                    ))
                }
                _ => None,
            });
            (region_d, label_box)
        };
        let (out_d, _) = mk(RegionTextPlace::Outside);
        let (in_d, label) = mk(RegionTextPlace::Inside);
        assert_ne!(
            out_d, in_d,
            "inside label must grow the region to contain the label"
        );
        let label = label.expect("inside region label");
        // Region box path corners are the four L endpoints after M.
        let nums = path_nums(&in_d);
        assert!(nums.len() >= 8, "box path coords");
        let rx0 = nums[0].min(nums[2]).min(nums[4]).min(nums[6]);
        let ry0 = nums[1].min(nums[3]).min(nums[5]).min(nums[7]);
        let rx1 = nums[0].max(nums[2]).max(nums[4]).max(nums[6]);
        let ry1 = nums[1].max(nums[3]).max(nums[5]).max(nums[7]);
        assert!(
            label.x0 >= rx0 - 1e-6
                && label.y0 >= ry0 - 1e-6
                && label.x1 <= rx1 + 1e-6
                && label.y1 <= ry1 + 1e-6,
            "inside label {:?} must sit inside grown box ({rx0},{ry0})-({rx1},{ry1})",
            label
        );
    }

    #[test]
    fn region_and_callout_use_markup() {
        let mol = ethanol();
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            region: Some(crate::doc::chrome::RegionField::One(RegionSpec {
                style: RegionStyle::Box,
                atoms: vec![0, 1],
                exclude: vec![],
                fill: None,
                stroke: None,
                text: Some(MolText::Plain("$R_1$".into())),
                z: None,
            })),
            callout: Some(crate::doc::chrome::CalloutField::One(CalloutSpec {
                atoms: Some(vec![2]),
                bonds: None,
                text: MolText::Plain("$N^+$".into()),
                arrow: false,
                prefer: Some(LabelPos::Right),
                color: None,
                scale: None,
                z: None,
            })),
            ..Default::default()
        };
        let out = apply_mol_chrome(&node, &mol, scene);
        let texts: Vec<_> = layer_prims(&out)
            .into_iter()
            .filter_map(|p| match p {
                Primitive::Path {
                    data_text: Some(t),
                    class: Some(c),
                    ..
                } if c.contains("region-label") || c.contains("callout-label") => {
                    Some((c.as_str(), t.as_str()))
                }
                _ => None,
            })
            .collect();
        assert!(
            texts
                .iter()
                .any(|(c, t)| c.contains("region") && (t.contains('₁') || t.contains('R'))),
            "region markup data_text missing: {texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|(c, t)| c.contains("callout") && (t.contains('⁺') || t.contains('+'))),
            "callout markup data_text missing: {texts:?}"
        );
    }
}
