//! Single-molecule depiction: [`MoleculeIn`] → [`Scene`].
//!
//! MVP paint path for WASM/Python thin serializers. Caller supplies SVG-space
//! coords (+ optional shade / marks / atom labels). Shade uses the xenosite
//! rainbow LUT; halo knocks out white channels through shade for bond/label ink.

use std::collections::HashMap;

use crate::bonds::{bond_strokes, join_centered_multibonds, DrawnBond, StrokePath};
use crate::colormap::colormap_rgb;
use crate::font::FaceStyle;
use crate::labels::{self, place_backbone};
use crate::metrics::{
    halo_stroke_from_stroke, label_stem_em, stroke_px_from_stem, BOND_PX, FONT_PX, HALO_GAP_PX,
    HALO_OPACITY, MARK_FRAC, PAD_PX, SHADE_FRAC,
};
use crate::plotdot::PlotDot;
use crate::rings::{bond_interior_normals, find_sssr};
use crate::scene::{
    AtomIn, Layer, LayerName, MoleculeIn, Primitive, Scene, Viewport,
};

/// Paint one molecule into a single-viewport [`Scene`].
///
/// Layers (bottom → top): shading → bonds → labels → marks.
pub fn depict_molecule(mol: &MoleculeIn) -> Scene {
    let color = mol.color.as_deref().unwrap_or("#111");
    let face = if mol.bold_labels {
        FaceStyle::Bold
    } else {
        FaceStyle::Regular
    };
    let stroke_px = stroke_px_from_stem(label_stem_em(mol.bold_labels));
    let by_index: HashMap<i32, usize> = mol
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.index, i))
        .collect();

    // BBox from atom centers (+ mark radii), then pad into positive space.
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for a in &mol.atoms {
        min_x = min_x.min(a.x);
        min_y = min_y.min(a.y);
        max_x = max_x.max(a.x);
        max_y = max_y.max(a.y);
    }
    if mol.atoms.is_empty() {
        min_x = 0.0;
        min_y = 0.0;
        max_x = 0.0;
        max_y = 0.0;
    } else if !mol.mark_atoms.is_empty() {
        let mark_r = BOND_PX * MARK_FRAC;
        min_x -= mark_r;
        min_y -= mark_r;
        max_x += mark_r;
        max_y += mark_r;
    }
    // Extra room for heteroatom labels (OH / NH2 / …) beyond atom centers.
    let label_pad = FONT_PX * 0.85;
    for a in &mol.atoms {
        if display_label(a).is_some() {
            min_x = min_x.min(a.x - label_pad);
            max_x = max_x.max(a.x + label_pad * 1.6);
            min_y = min_y.min(a.y - label_pad);
            max_y = max_y.max(a.y + label_pad);
        }
    }
    let pad = PAD_PX;
    let dx = pad - min_x;
    let dy = pad - min_y;
    let mut width = (max_x - min_x) + 2.0 * pad;
    let mut height = (max_y - min_y) + 2.0 * pad;

    let label_atoms: Vec<labels::AtomIn> = mol
        .atoms
        .iter()
        .map(|a| labels::AtomIn {
            x: a.x + dx,
            y: a.y + dy,
            label: display_label(a),
        })
        .collect();
    let label_bonds: Vec<labels::BondIn> = mol
        .bonds
        .iter()
        .map(|b| labels::BondIn {
            begin: *by_index.get(&b.begin).unwrap_or(&0),
            end: *by_index.get(&b.end).unwrap_or(&0),
        })
        .collect();
    let (shortened, placed) = place_backbone(&label_atoms, &label_bonds, FONT_PX, face);

    // Ring bonds get interior normals → short inside offsets (not acyclic extend).
    let ring_coords: HashMap<i32, (f64, f64)> = mol
        .atoms
        .iter()
        .map(|a| (a.index, (a.x + dx, a.y + dy)))
        .collect();
    let ring_normals = bond_interior_normals(&find_sssr(mol, 8), &ring_coords);

    let mut prepared: Vec<DrawnBond> = Vec::new();
    for (bond, ends) in mol.bonds.iter().zip(shortened.iter()) {
        if !by_index.contains_key(&bond.begin) || !by_index.contains_key(&bond.end) {
            continue;
        }
        let i0 = by_index[&bond.begin];
        let i1 = by_index[&bond.end];
        let mut db = DrawnBond::new(
            bond.index,
            bond.begin,
            bond.end,
            ends.x1,
            ends.y1,
            ends.x2,
            ends.y2,
            bond.order,
        );
        db.stereo = bond.stereo.clone();
        let key = if bond.begin < bond.end {
            (bond.begin, bond.end)
        } else {
            (bond.end, bond.begin)
        };
        // Ring membership overrules centered/extend; else keep caller interior.
        db.interior = ring_normals.get(&key).copied().or(bond.interior);
        db.begin_labeled = placed[i0].is_some();
        db.end_labeled = placed[i1].is_some();
        prepared.push(db);
    }
    join_centered_multibonds(&mut prepared);

    let mut bond_prims: Vec<Primitive> = Vec::new();
    let mut bond_strokes_for_halo: Vec<StrokePath> = Vec::new();
    for bond in &prepared {
        let strokes = bond_strokes(
            bond.x1,
            bond.y1,
            bond.x2,
            bond.y2,
            bond.order,
            bond.interior,
            bond.stereo.as_deref(),
            bond.trims.as_ref(),
            Some(stroke_px),
        );
        let tag = format!(
            "bond-{} atom-{} atom-{}",
            bond.index, bond.begin, bond.end
        );
        for sp in strokes.paint_order() {
            bond_strokes_for_halo.push((*sp).clone());
        }
        for mut p in strokes.into_primitives(color) {
            if let Primitive::Path { ref mut class, .. } = p {
                if let Some(c) = class.take() {
                    *class = Some(format!("{tag} {c}"));
                } else {
                    *class = Some(tag.clone());
                }
            }
            bond_prims.push(p);
        }
    }

    let mut label_prims: Vec<Primitive> = Vec::new();
    let mut label_ink_for_halo: Vec<crate::geom::Shape> = Vec::new();
    for (slot, pl) in placed.iter().enumerate() {
        let Some(pl) = pl else { continue };
        let atom_index = mol.atoms[slot].index;
        // Grow canvas if traveling text spills past the initial pad.
        width = width.max(pl.origin_x + FONT_PX * pl.text.chars().count() as f64 * 0.65 + pad * 0.25);
        height = height.max(pl.y + FONT_PX * 0.35 + pad * 0.25);
        if matches!(pl.side, labels::LabelSide::North | labels::LabelSide::South) {
            height = height.max(pl.atom_y + FONT_PX * 1.6 + pad * 0.25);
        }
        if let Some(ink) = labels::label_ink_shape(pl, FONT_PX, face) {
            label_ink_for_halo.push(ink);
        }
        if pl.path_d.is_empty() {
            continue;
        }
        label_prims.push(Primitive::Path {
            d: pl.path_d.clone(),
            stroke: Some("none".into()),
            fill: Some(color.to_string()),
            stroke_width: 0.0,
            opacity: 1.0,
            stroke_dasharray: None,
            stroke_linecap: None,
            class: Some(format!("atom-{atom_index} label")),
            data_text: Some(pl.text.clone()),
        });
    }

    let shade_prims = paint_shade(mol, &by_index, dx, dy);
    let mark_prims = paint_marks(mol, &by_index, dx, dy);
    let halo_prims = paint_halo(&bond_strokes_for_halo, &label_ink_for_halo, stroke_px);

    let mut layers = Vec::new();
    if !shade_prims.is_empty() {
        layers.push(Layer {
            name: LayerName::Shading,
            primitives: shade_prims,
        });
    }
    if !bond_prims.is_empty() {
        layers.push(Layer {
            name: LayerName::Bonds,
            primitives: bond_prims,
        });
    }
    if !label_prims.is_empty() {
        layers.push(Layer {
            name: LayerName::Labels,
            primitives: label_prims,
        });
    }
    if !mark_prims.is_empty() {
        layers.push(Layer {
            name: LayerName::Marks,
            primitives: mark_prims,
        });
    }

    Scene {
        width,
        height,
        viewports: vec![Viewport {
            id: mol.id.clone(),
            x: 0.0,
            y: 0.0,
            width,
            height,
            layers,
        }],
        overlays: Vec::new(),
        halo: halo_prims,
    }
}

/// Explicit ``label``, else heteroatom / charged symbol (carbons stay silent).
///
/// Appends a charge suffix when the label body does not already include one
/// (matches Python ``display_text``).
fn display_label(a: &AtomIn) -> Option<String> {
    let mut body = if let Some(ref l) = a.label {
        let t = l.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    } else {
        None
    };
    if body.is_none() {
        let sym = a.symbol();
        if sym == "C" && a.charge == 0 {
            return None;
        }
        body = Some(sym.to_string());
    }
    let mut body = body.unwrap();
    let parsed_chg = labels::split_label(&body).charge;
    if a.charge != 0 && parsed_chg == 0 {
        let sign = if a.charge > 0 { "+" } else { "−" };
        let mag = a.charge.unsigned_abs();
        if mag == 1 {
            body.push_str(sign);
        } else {
            body.push_str(&format!("{mag}{sign}"));
        }
    }
    Some(body)
}

fn paint_shade(
    mol: &MoleculeIn,
    by_index: &HashMap<i32, usize>,
    dx: f64,
    dy: f64,
) -> Vec<Primitive> {
    let atom_zs = mol.atom_shade.as_deref().unwrap_or(&[]);
    let bond_zs = mol.bond_shade.as_deref().unwrap_or(&[]);
    if atom_zs.is_empty() && bond_zs.is_empty() {
        return Vec::new();
    }
    let mut samples: Vec<f64> = Vec::new();
    samples.extend_from_slice(atom_zs);
    samples.extend_from_slice(bond_zs);
    let vmin = samples.iter().cloned().fold(f64::INFINITY, f64::min);
    let vmax = samples.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let diverging = vmin < 0.0 && vmax > 0.0;
    let base_r = BOND_PX
        * (if !atom_zs.is_empty() && !bond_zs.is_empty() {
            0.8
        } else {
            SHADE_FRAC
        });
    let plot = PlotDot::default();
    // Atom + bond scores share one PlotDot pass so overlapping rings stack by
    // strength (weak first → strong on top), matching xenopict shade().
    let mut zs: Vec<f64> = Vec::new();
    let mut coords: Vec<(f64, f64)> = Vec::new();
    if !atom_zs.is_empty() {
        for (a, &z) in mol.atoms.iter().zip(atom_zs.iter()) {
            zs.push(z);
            coords.push((a.x + dx, a.y + dy));
        }
    }
    if !bond_zs.is_empty() {
        for (bond, &z) in mol.bonds.iter().zip(bond_zs.iter()) {
            let Some(&i0) = by_index.get(&bond.begin) else {
                continue;
            };
            let Some(&i1) = by_index.get(&bond.end) else {
                continue;
            };
            let a0 = &mol.atoms[i0];
            let a1 = &mol.atoms[i1];
            zs.push(z);
            coords.push(((a0.x + a1.x) * 0.5 + dx, (a0.y + a1.y) * 0.5 + dy));
        }
    }
    let norm = normalize_shade_scores(&zs, vmin, vmax);
    let n = norm.len().min(coords.len());
    let mut out = Vec::new();
    for d in plot.disks(&norm[..n], &coords[..n]) {
        if d.color_z.abs() < 0.05 && d.radius_frac < 0.35 {
            continue;
        }
        if d.color_z.abs() < 0.02 {
            continue;
        }
        out.push(Primitive::Circle {
            cx: d.x,
            cy: d.y,
            r: base_r * d.radius_frac,
            fill: Some(colormap_rgb(d.color_z, diverging)),
            stroke: None,
            stroke_width: 0.0,
            opacity: 1.0,
            class: Some("shade".into()),
        });
    }
    out
}

fn normalize_shade_scores(zs: &[f64], vmin: f64, vmax: f64) -> Vec<f64> {
    if vmin >= 0.0 && vmax > 0.0 {
        return zs
            .iter()
            .map(|&z| (z / vmax).clamp(0.0, 1.0))
            .collect();
    }
    if vmax <= 0.0 && vmin < 0.0 {
        return zs
            .iter()
            .map(|&z| (z / vmin.abs()).clamp(-1.0, 0.0))
            .collect();
    }
    let scale = vmin.abs().max(vmax.abs()).max(1e-9);
    zs.iter()
        .map(|&z| (z / scale).clamp(-1.0, 1.0))
        .collect()
}

/// White knockout under bonds/labels so shade disks don't cover ink.
///
/// xenopict: union of per-ink buffers at [`HALO_GAP_PX`] (with a floor from
/// stroke thickness). Requires the `geom` feature (WASM enables it via `font`).
#[cfg(feature = "geom")]
fn paint_halo(
    bond_strokes: &[StrokePath],
    label_ink: &[crate::geom::Shape],
    stroke_px: f64,
) -> Vec<Primitive> {
    use crate::geom::Shape;

    let mut ink: Option<Shape> = None;
    let mut absorb = |piece: Shape| {
        if piece.is_empty() {
            return;
        }
        ink = Some(match ink.take() {
            Some(acc) => acc.union(&piece),
            None => piece,
        });
    };

    for sp in bond_strokes {
        let pts = path_points(&sp.d);
        if pts.len() < 2 {
            continue;
        }
        let ink_r = sp.stroke_width.max(stroke_px) * 0.5;
        // Filled wedges: treat the polygon as ink; stroked lines → capsules.
        if sp.fill.as_deref().is_some_and(|f| f != "none") && pts.len() >= 3 {
            let poly = Shape::from_ring(&pts);
            if !poly.is_empty() {
                let grown = if ink_r > 0.0 {
                    poly.buffer(ink_r)
                } else {
                    poly
                };
                absorb(grown);
            }
            continue;
        }
        for w in pts.windows(2) {
            let (x1, y1) = w[0];
            let (x2, y2) = w[1];
            if (x1 - x2).hypot(y1 - y2) < 1e-6 {
                continue;
            }
            absorb(Shape::capsule(x1, y1, x2, y2, ink_r));
        }
    }

    for shape in label_ink {
        absorb(shape.clone());
    }

    let Some(ink) = ink.filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    // Match Python BondsDrawable: max(HALO_GAP, 0.25*HALO_STROKE - ink_r).
    let ink_r = stroke_px * 0.5;
    let halo_stroke = halo_stroke_from_stroke(stroke_px);
    let dist = HALO_GAP_PX.max(0.25 * halo_stroke - ink_r);
    let grown = ink.halo(dist);
    if grown.is_empty() {
        return Vec::new();
    }
    let d = grown.to_svg_d();
    if d.is_empty() {
        return Vec::new();
    }
    vec![Primitive::Path {
        d,
        stroke: Some("none".into()),
        fill: Some("#fff".into()),
        stroke_width: 0.0,
        opacity: HALO_OPACITY,
        stroke_dasharray: None,
        stroke_linecap: None,
        class: Some("halo".into()),
        data_text: None,
    }]
}

#[cfg(not(feature = "geom"))]
fn paint_halo(
    _bond_strokes: &[StrokePath],
    _label_ink: &[crate::geom::Shape],
    _stroke_px: f64,
) -> Vec<Primitive> {
    Vec::new()
}

/// Extract absolute M/L coordinates from a simple path `d` (bond strokes).
fn path_points(d: &str) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut nums = Vec::new();
    let mut cur = String::new();
    let flush_num = |cur: &mut String, nums: &mut Vec<f64>| {
        if !cur.is_empty() {
            if let Ok(v) = cur.parse::<f64>() {
                nums.push(v);
            }
            cur.clear();
        }
    };
    for ch in d.chars() {
        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E' {
            cur.push(ch);
        } else {
            flush_num(&mut cur, &mut nums);
        }
    }
    flush_num(&mut cur, &mut nums);
    for pair in nums.chunks(2) {
        if pair.len() == 2 {
            out.push((pair[0], pair[1]));
        }
    }
    out
}

fn paint_marks(
    mol: &MoleculeIn,
    by_index: &HashMap<i32, usize>,
    dx: f64,
    dy: f64,
) -> Vec<Primitive> {
    use crate::geom::{capsule_polygon, polygon_to_svg_d};
    use crate::metrics::{
        MARK_HALO_COLOR, MARK_HALO_OPACITY, MARK_HALO_STROKE_PX, MARK_OPACITY, MARK_STROKE_PX,
    };

    let mut out = Vec::new();
    let r = BOND_PX * MARK_FRAC;
    // xenopict: mark halo via `<use href="#mark" stroke="#555" …>`; mark ink
    // sits in a separate group with stroke-width scale×0.1 / opacity 0.7 but
    // **no stroke color** (host CSS on `.mark` may add one). Xpict does not
    // style by class: bake the visible ring on the halo attrs only. Backbone
    // color never recolors marks.
    // shapely `resolution=6` → 24 verts/circle; keep capsules smooth.
    const CAPSULE_QUAD_SEGS: u32 = 16;

    // Bond capsules (outline path `d`) — xenopict buffers the bond at mark radius.
    let mut bond_capsules: Vec<(i32, i32, String)> = Vec::new();
    for &(a, b) in &mol.mark_bonds {
        let Some(&ia) = by_index.get(&a) else {
            continue;
        };
        let Some(&ib) = by_index.get(&b) else {
            continue;
        };
        let a0 = &mol.atoms[ia];
        let a1 = &mol.atoms[ib];
        let d = polygon_to_svg_d(&capsule_polygon(
            a0.x + dx,
            a0.y + dy,
            a1.x + dx,
            a1.y + dy,
            r,
            CAPSULE_QUAD_SEGS,
        ));
        if !d.is_empty() {
            bond_capsules.push((a, b, d));
        }
    }

    // Halo underlay first (xenopict `<use href="#mark">`: #555, scale*0.2, 0.45).
    for &ai in &mol.mark_atoms {
        let Some(&i) = by_index.get(&ai) else {
            continue;
        };
        let a = &mol.atoms[i];
        out.push(Primitive::Circle {
            cx: a.x + dx,
            cy: a.y + dy,
            r,
            fill: Some("none".into()),
            stroke: Some(MARK_HALO_COLOR.into()),
            stroke_width: MARK_HALO_STROKE_PX,
            opacity: MARK_HALO_OPACITY,
            class: Some(format!("atom-{ai} mark-halo")),
        });
    }
    for (a, b, d) in &bond_capsules {
        out.push(Primitive::Path {
            d: d.clone(),
            stroke: Some(MARK_HALO_COLOR.into()),
            fill: Some("none".into()),
            stroke_width: MARK_HALO_STROKE_PX,
            opacity: MARK_HALO_OPACITY,
            stroke_dasharray: None,
            stroke_linecap: Some("round".into()),
            class: Some(format!("bond-mark-halo atom-{a} atom-{b}")),
            data_text: None,
        });
    }

    // Mark ink (xenopict mark group: fill none, stroke-width scale×0.1,
    // opacity 0.7, **no stroke color**). Visible color is the halo above.
    for &ai in &mol.mark_atoms {
        let Some(&i) = by_index.get(&ai) else {
            continue;
        };
        let a = &mol.atoms[i];
        out.push(Primitive::Circle {
            cx: a.x + dx,
            cy: a.y + dy,
            r,
            fill: Some("none".into()),
            stroke: None,
            stroke_width: MARK_STROKE_PX,
            opacity: MARK_OPACITY,
            class: Some(format!("atom-{ai} mark")),
        });
    }
    for (a, b, d) in bond_capsules {
        out.push(Primitive::Path {
            d,
            stroke: None,
            fill: Some("none".into()),
            stroke_width: MARK_STROKE_PX,
            opacity: MARK_OPACITY,
            stroke_dasharray: None,
            stroke_linecap: Some("round".into()),
            class: Some(format!("bond-mark atom-{a} atom-{b}")),
            data_text: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{AtomIn, BondIn};

    fn atom(index: i32, element: &str, x: f64, y: f64, label: Option<&str>) -> AtomIn {
        AtomIn {
            index,
            element: Some(element.into()),
            z: None,
            x,
            y,
            label: label.map(str::to_string),
            charge: 0,
        }
    }

    fn ethanol() -> MoleculeIn {
        MoleculeIn {
            id: Some("etoh".into()),
            atoms: vec![
                atom(0, "C", 0.0, 0.0, None),
                atom(1, "C", 20.0, 0.0, None),
                atom(2, "O", 30.0, 10.0, Some("OH")),
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
            mark_atoms: vec![],
            mark_bonds: vec![],
            bold_labels: false,
        }
    }

    fn acetone() -> MoleculeIn {
        MoleculeIn {
            id: Some("acetone".into()),
            atoms: vec![
                atom(0, "C", -20.0, 8.0, None),
                atom(1, "C", 0.0, 0.0, None),
                atom(2, "O", 0.0, -20.0, Some("O")),
                atom(3, "C", 20.0, 8.0, None),
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
                    order: 2.0,
                    stereo: None,
                    interior: None,
                },
                BondIn {
                    index: 2,
                    begin: 1,
                    end: 3,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
            ],
            color: Some("#336699".into()),
            atom_shade: None,
            bond_shade: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            bold_labels: false,
        }
    }

    #[test]
    fn z_only_atoms_resolve_symbols() {
        let mol = MoleculeIn {
            id: Some("z".into()),
            atoms: vec![
                AtomIn {
                    index: 0,
                    element: None,
                    z: Some(6),
                    x: 0.0,
                    y: 0.0,
                    label: None,
                    charge: 0,
                },
                AtomIn {
                    index: 1,
                    element: None,
                    z: Some(8),
                    x: 20.0,
                    y: 0.0,
                    label: Some("O".into()),
                    charge: 0,
                },
            ],
            bonds: vec![BondIn {
                index: 0,
                begin: 0,
                end: 1,
                order: 1.0,
                stereo: None,
                interior: None,
            }],
            color: None,
            atom_shade: None,
            bond_shade: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            bold_labels: false,
        };
        assert_eq!(mol.atoms[0].symbol(), "C");
        assert_eq!(mol.atoms[1].symbol(), "O");
        let scene = depict_molecule(&mol);
        assert_eq!(scene.viewports.len(), 1);
    }

    #[test]
    fn ethanol_emits_oh_label() {
        let scene = depict_molecule(&ethanol());
        let labels = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Labels)
            .expect("labels layer");
        let texts: Vec<_> = labels
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Path {
                    data_text: Some(t),
                    ..
                } => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|t| *t == "OH" || *t == "HO"),
            "expected OH/HO path data-text, got {texts:?}"
        );
        assert!(
            labels.primitives.iter().all(|p| matches!(p, Primitive::Path { .. })),
            "labels must be glyph paths, not text nodes"
        );
    }

    #[test]
    fn ethanol_has_bond_layer() {
        let scene = depict_molecule(&ethanol());
        assert_eq!(scene.viewports.len(), 1);
        let bonds = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .expect("bonds");
        assert_eq!(bonds.primitives.len(), 2);
        assert!(scene.width > PAD_PX);
        assert!(scene.height > PAD_PX);
    }

    #[test]
    fn bold_labels_thicken_bond_stroke() {
        use crate::metrics::{label_stem_em, stroke_px_from_stem, STROKE_PX};
        let mut mol = ethanol();
        mol.bold_labels = true;
        let scene = depict_molecule(&mol);
        let bonds = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .expect("bonds");
        let sw = bonds
            .primitives
            .iter()
            .find_map(|p| match p {
                Primitive::Path { stroke_width, .. } if *stroke_width > 0.0 => Some(*stroke_width),
                _ => None,
            })
            .expect("bond stroke");
        let expected = stroke_px_from_stem(label_stem_em(true));
        assert!(
            (sw - expected).abs() < 0.02,
            "bold stroke {sw} vs {expected}"
        );
        assert!(sw > STROKE_PX + 0.3);
    }

    #[test]
    fn acetone_centered_double_has_two_offsets() {
        let scene = depict_molecule(&acetone());
        let bonds = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .expect("bonds");
        let offset_count = bonds
            .primitives
            .iter()
            .filter(|p| match p {
                Primitive::Path { class: Some(c), .. } => c.contains("bond-offset"),
                _ => false,
            })
            .count();
        assert!(offset_count >= 2, "expected centered C=O offsets");
        assert!(bonds.primitives.iter().any(|p| match p {
            Primitive::Path {
                stroke: Some(s), ..
            } => s == "#336699",
            _ => false,
        }));
    }

    #[test]
    fn benzene_ring_doubles_are_single_sided_short() {
        // Regular hexagon — without ring interiors, join_centered would extend
        // doubles onto singles and emit two parallel offsets per double.
        let mut atoms = Vec::new();
        for i in 0..6 {
            let ang = std::f64::consts::PI / 2.0 + i as f64 * std::f64::consts::TAU / 6.0;
            atoms.push(AtomIn {
                index: i,
                element: Some("C".into()),
                z: None,
                x: 20.0 * ang.cos(),
                y: -20.0 * ang.sin(),
                label: None,
                charge: 0,
            });
        }
        let orders = [1.0, 2.0, 1.0, 2.0, 1.0, 2.0];
        let mut bonds = Vec::new();
        for i in 0..6 {
            bonds.push(BondIn {
                index: i,
                begin: i,
                end: (i + 1) % 6,
                order: orders[i as usize],
                stereo: None,
                interior: None,
            });
        }
        let mol = MoleculeIn {
            id: Some("phh".into()),
            atoms,
            bonds,
            color: None,
            atom_shade: None,
            bond_shade: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            bold_labels: false,
        };
        let scene = depict_molecule(&mol);
        let layer = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .expect("bonds");
        let offset_count = layer
            .primitives
            .iter()
            .filter(|p| match p {
                Primitive::Path { class: Some(c), .. } => c.contains("bond-offset"),
                _ => false,
            })
            .count();
        // Three ring doubles → one interior offset each (not two centered lines).
        assert_eq!(offset_count, 3, "ring doubles should be single-sided");
    }

    #[test]
    fn marks_and_shade_layers() {
        let mut mol = ethanol();
        mol.mark_atoms = vec![2];
        mol.atom_shade = Some(vec![0.0, 0.0, 0.9]);
        let scene = depict_molecule(&mol);
        let names: Vec<_> = scene.viewports[0]
            .layers
            .iter()
            .map(|l| l.name)
            .collect();
        assert!(names.contains(&LayerName::Shading));
        assert!(names.contains(&LayerName::Bonds));
        assert!(names.contains(&LayerName::Marks));
        // Rainbow LUT (not the old coral stub): high score → hot orange-red.
        let shade = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Shading)
            .expect("shading");
        assert!(shade.primitives.iter().any(|p| match p {
            Primitive::Circle {
                fill: Some(f), ..
            } => f.starts_with("rgb(") && !f.starts_with("rgb(255,222"),
            _ => false,
        }));
        assert!(!scene.halo.is_empty(), "expected bond/label halo knockout");
    }

    #[test]
    fn shade_atom_and_bond_share_one_sorted_layer() {
        let mut mol = ethanol();
        // Strong bond mid + weaker atoms → bond disk should paint after (on top).
        mol.atom_shade = Some(vec![0.3, 0.3, 0.3]);
        mol.bond_shade = Some(vec![1.0, 0.2]);
        let scene = depict_molecule(&mol);
        let shade = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Shading)
            .expect("shading");
        let fills: Vec<_> = shade
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Circle {
                    fill: Some(f),
                    r,
                    ..
                } => Some((f.clone(), *r)),
                _ => None,
            })
            .collect();
        assert!(fills.len() >= 2);
        // Combined atom+bond → base radius 0.8×bond (not 0.9).
        let max_r = fills.iter().map(|(_, r)| *r).fold(0.0_f64, f64::max);
        assert!(
            (max_r - BOND_PX * 0.8).abs() < 0.05 || max_r < BOND_PX * 0.8 + 0.05,
            "expected ≤0.8×bond when both atom+bond shaded, got {max_r}"
        );
    }

    #[test]
    fn mark_circles_match_xenopict_style() {
        let mut mol = ethanol();
        mol.mark_atoms = vec![1];
        mol.mark_bonds = vec![(0, 1)];
        let scene = depict_molecule(&mol);
        let marks = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Marks)
            .expect("marks");
        let atom = marks.primitives.iter().find_map(|p| match p {
            Primitive::Circle {
                r,
                stroke_width,
                opacity,
                class: Some(c),
                stroke,
                ..
            } if c.contains(" mark") && !c.contains("halo") => {
                Some((*r, *stroke_width, *opacity, stroke.clone()))
            }
            _ => None,
        });
        let (r, sw, op, stroke) = atom.expect("atom mark");
        assert!((r - BOND_PX).abs() < 1e-9, "mark radius = scale");
        assert!(
            (sw - BOND_PX * 0.1).abs() < 1e-9,
            "mark stroke = scale*0.1 (xenopict), got {sw}"
        );
        assert!((op - 0.7).abs() < 1e-9);
        assert!(
            stroke.is_none(),
            "mark ink has no baked stroke (xenopict); visible color is halo #555"
        );
        // Colored backbone must not recolor marks.
        mol.color = Some("#0b6e4f".into());
        let scene2 = depict_molecule(&mol);
        let marks2 = scene2.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Marks)
            .expect("marks");
        let ink = marks2.primitives.iter().find_map(|p| match p {
            Primitive::Circle {
                stroke,
                class: Some(c),
                ..
            } if c.contains(" mark") && !c.contains("halo") => Some(stroke.clone()),
            _ => None,
        });
        assert_eq!(ink, Some(None), "backbone color must not paint mark ink");
        let halo = marks.primitives.iter().any(|p| match p {
            Primitive::Circle {
                stroke: Some(s),
                stroke_width,
                opacity,
                class: Some(c),
                ..
            } => c.contains("mark-halo") && s == "#555" && (*stroke_width - BOND_PX * 0.2).abs() < 1e-9 && (*opacity - 0.45).abs() < 1e-9,
            _ => false,
        });
        assert!(halo, "expected #555 mark halo underlay");
        let bond = marks.primitives.iter().any(|p| match p {
            Primitive::Path {
                d,
                stroke,
                stroke_width,
                opacity,
                class: Some(c),
                ..
            } => {
                c.contains("bond-mark")
                    && !c.contains("halo")
                    && d.contains('Z')
                    && stroke.is_none()
                    && (*stroke_width - BOND_PX * 0.1).abs() < 1e-9
                    && (*opacity - 0.7).abs() < 1e-9
            }
            _ => false,
        });
        assert!(bond, "bond mark should be a closed capsule outline");
    }

    #[test]
    fn scene_json_roundtrips() {
        let scene = depict_molecule(&acetone());
        let json = serde_json::to_string(&scene).expect("ser");
        let back: Scene = serde_json::from_str(&json).expect("de");
        assert_eq!(back.viewports[0].id.as_deref(), Some("acetone"));
        assert!(!back.viewports[0].layers.is_empty());
    }

    /// Parity: Python ``test_depict_molecule_ethanol_two_bonds``.
    #[test]
    fn ethanol_viewport_id_and_two_bonds() {
        let scene = depict_molecule(&ethanol());
        assert_eq!(scene.viewports[0].id.as_deref(), Some("etoh"));
        let bonds = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .expect("bonds");
        assert_eq!(bonds.primitives.len(), 2);
        assert!(scene.width > 0.0 && scene.height > 0.0);
    }

    /// Parity: Python ``test_depict_acetone_centered_double_offsets`` + joined offsets.
    #[test]
    fn acetone_joined_offsets_and_color() {
        let scene = depict_molecule(&acetone());
        let bonds = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .expect("bonds");
        let offsets: Vec<_> = bonds
            .primitives
            .iter()
            .filter(|p| match p {
                Primitive::Path { class: Some(c), .. } => c.contains("bond-offset"),
                _ => false,
            })
            .collect();
        assert!(offsets.len() >= 2);
        assert!(bonds.primitives.iter().all(|p| match p {
            Primitive::Path {
                stroke: Some(s), ..
            } => s == "#336699",
            _ => true,
        }));
        assert!((crate::metrics::OFFSET_PX - 3.0).abs() < 1e-9);
    }

    /// Parity: Python ``test_depict_marks_and_shade_layers``.
    #[test]
    fn marks_and_shade_co_o_fixture() {
        let mol = MoleculeIn {
            id: None,
            atoms: vec![
                atom(0, "C", 0.0, 0.0, None),
                atom(1, "O", 20.0, 0.0, Some("O")),
            ],
            bonds: vec![BondIn {
                index: 0,
                begin: 0,
                end: 1,
                order: 1.0,
                stereo: None,
                interior: None,
            }],
            color: None,
            atom_shade: Some(vec![0.0, 0.85]),
            bond_shade: None,
            mark_atoms: vec![1],
            mark_bonds: vec![],
            bold_labels: false,
        };
        let scene = depict_molecule(&mol);
        let names: std::collections::HashSet<_> = scene.viewports[0]
            .layers
            .iter()
            .map(|l| l.name)
            .collect();
        assert!(names.contains(&LayerName::Shading));
        assert!(names.contains(&LayerName::Bonds));
        assert!(names.contains(&LayerName::Marks));
    }

    #[test]
    fn empty_molecule_and_hetero_auto_label() {
        let empty = MoleculeIn {
            id: Some("empty".into()),
            atoms: vec![],
            bonds: vec![],
            color: None,
            atom_shade: None,
            bond_shade: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            bold_labels: false,
        };
        let scene = depict_molecule(&empty);
        assert_eq!(scene.viewports[0].id.as_deref(), Some("empty"));
        assert!(scene.viewports[0].layers.is_empty());

        // Heteroatom without explicit label still gets a glyph path.
        let mol = MoleculeIn {
            id: None,
            atoms: vec![
                atom(0, "C", 0.0, 0.0, None),
                atom(1, "N", 20.0, 0.0, None),
            ],
            bonds: vec![BondIn {
                index: 0,
                begin: 0,
                end: 1,
                order: 1.0,
                stereo: None,
                interior: None,
            }],
            color: None,
            atom_shade: None,
            bond_shade: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            bold_labels: false,
        };
        let scene = depict_molecule(&mol);
        let labels = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Labels)
            .expect("labels");
        assert!(labels.primitives.iter().any(|p| match p {
            Primitive::Path {
                data_text: Some(t),
                ..
            } => t == "N",
            _ => false,
        }));
    }

    #[test]
    fn charged_label_and_empty_label_string() {
        let mut charged = atom(0, "N", 0.0, 0.0, Some("N"));
        charged.charge = 1;
        let mut anion = atom(1, "O", 20.0, 0.0, None);
        anion.charge = -2;
        let blank = AtomIn {
            index: 2,
            element: Some("C".into()),
            z: None,
            x: 40.0,
            y: 0.0,
            label: Some("  ".into()),
            charge: 0,
        };
        let mol = MoleculeIn {
            id: None,
            atoms: vec![charged, anion, blank],
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
            mark_atoms: vec![],
            mark_bonds: vec![],
            bold_labels: false,
        };
        let scene = depict_molecule(&mol);
        let labels = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Labels)
            .expect("labels");
        let texts: Vec<_> = labels
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Path {
                    data_text: Some(t),
                    ..
                } => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|t| t.contains('⁺') || t.contains('+')));
        assert!(texts.iter().any(|t| t.contains('⁻') || t.contains('−') || t.contains('-')));
    }

    #[test]
    fn north_south_label_grows_canvas() {
        // Degree-2 steep neighbors → North/South orientation for NH2.
        let mol = MoleculeIn {
            id: None,
            atoms: vec![
                atom(0, "C", 0.0, 20.0, None),
                atom(1, "C", 0.0, -20.0, None),
                atom(2, "N", 0.0, 0.0, Some("NH2")),
            ],
            bonds: vec![
                BondIn {
                    index: 0,
                    begin: 2,
                    end: 0,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
                BondIn {
                    index: 1,
                    begin: 2,
                    end: 1,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
            ],
            color: None,
            atom_shade: None,
            bond_shade: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            bold_labels: false,
        };
        let scene = depict_molecule(&mol);
        assert!(scene.height > PAD_PX * 2.0);
        let labels = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Labels)
            .expect("labels");
        assert!(!labels.primitives.is_empty());
    }

    #[test]
    fn shade_negative_and_diverging() {
        let mut mol = ethanol();
        mol.atom_shade = Some(vec![-0.8, -0.2, 0.0]);
        let scene = depict_molecule(&mol);
        assert!(scene.viewports[0]
            .layers
            .iter()
            .any(|l| l.name == LayerName::Shading));

        mol.atom_shade = Some(vec![-0.9, 0.0, 0.9]);
        let scene2 = depict_molecule(&mol);
        let shade = scene2.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Shading)
            .expect("shading");
        assert!(!shade.primitives.is_empty());
    }

    #[test]
    fn skips_bonds_and_marks_with_missing_indices() {
        let mut mol = ethanol();
        mol.bonds.push(BondIn {
            index: 99,
            begin: 0,
            end: 999,
            order: 1.0,
            stereo: None,
            interior: None,
        });
        mol.mark_atoms = vec![2, 999];
        mol.mark_bonds = vec![(0, 1), (0, 999)];
        let scene = depict_molecule(&mol);
        let marks = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Marks)
            .expect("marks");
        // Valid atom mark + bond mark only (missing indices skipped).
        assert!(marks.primitives.len() >= 2);
    }

    #[test]
    fn wedge_stereo_fills_halo_from_polygon() {
        let mol = MoleculeIn {
            id: None,
            atoms: vec![atom(0, "C", 0.0, 0.0, None), atom(1, "C", 20.0, 0.0, None)],
            bonds: vec![BondIn {
                index: 0,
                begin: 0,
                end: 1,
                order: 1.0,
                stereo: Some("up".into()),
                interior: None,
            }],
            color: None,
            atom_shade: Some(vec![0.5, 0.5]),
            bond_shade: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            bold_labels: false,
        };
        let scene = depict_molecule(&mol);
        assert!(!scene.halo.is_empty());
        let bonds = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .expect("bonds");
        assert!(bonds.primitives.iter().any(|p| match p {
            Primitive::Path { class: Some(c), .. } => c.contains("wedge"),
            _ => false,
        }));
    }

    #[test]
    fn shade_bond_only_and_tiny_scores_filtered() {
        let mut mol = ethanol();
        mol.atom_shade = None;
        mol.bond_shade = Some(vec![0.9, 0.01]);
        let scene = depict_molecule(&mol);
        let shade = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Shading)
            .expect("shading");
        assert!(!shade.primitives.is_empty());

        // Near-zero-only scores: either no shading layer or only filtered disks.
        mol.bond_shade = Some(vec![0.0, 0.0]);
        mol.atom_shade = Some(vec![0.0, 0.0, 0.0]);
        let scene2 = depict_molecule(&mol);
        let shade2 = scene2.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Shading);
        assert!(
            shade2.is_none() || shade2.unwrap().primitives.is_empty(),
            "expected no shade disks for zero scores"
        );
    }

    #[test]
    fn shade_skips_bond_with_unknown_endpoints() {
        let mut mol = ethanol();
        mol.bond_shade = Some(vec![0.8, 0.8, 0.8]);
        mol.bonds.push(BondIn {
            index: 50,
            begin: 0,
            end: 999,
            order: 1.0,
            stereo: None,
            interior: None,
        });
        let scene = depict_molecule(&mol);
        assert!(scene.viewports[0]
            .layers
            .iter()
            .any(|l| l.name == LayerName::Shading));
    }
}
