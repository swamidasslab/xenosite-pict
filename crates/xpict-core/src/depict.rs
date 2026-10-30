//! Single-molecule depiction: [`MoleculeIn`] → [`Scene`].
//!
//! Caller supplies SVG-space coords (+ optional shade / atom labels). Shade uses
//! the xenosite rainbow LUT; halo knocks out white channels through shade for
//! bond/label ink. Publication circles are [`crate::doc`] chrome on `MolNode`.

use std::collections::HashMap;

use crate::bonds::{bond_strokes, join_centered_multibonds, DrawnBond, StrokePath};
use crate::colormap::colormap_rgb;
use crate::debug_align::paint_align_debug;
use crate::debug_box::{label_debug_rect, mol_debug_rect, scene_debug_rect};
use crate::font::FaceStyle;
use crate::labels::{self, place_backbone};
use crate::metrics::{
    halo_gap_for_weight, halo_stroke_for_weight, label_weight_grow_px, stroke_px_for_weight,
    BOND_PX, FONT_PX, HALO_OPACITY, PAD_PX, RADICAL_DOT_PX, RADICAL_OFFSET_PX, RADICAL_PAIR_PX,
    SHADE_BOTH_FRAC, SHADE_FRAC,
};
use crate::plotdot::PlotDot;
use crate::rings::{aromatic_ring_circles, bond_interior_normals, find_sssr, kekule_paint_orders};
use crate::scene::{
    AromaticStyle, AtomIn, Layer, LayerName, MoleculeIn, Primitive, Scene, Viewport,
};

/// Paint one molecule into a single-viewport [`Scene`].
///
/// Layers (bottom → top): shading → halo → bonds → labels. Chrome annotations
/// (regions, circles, callouts) join those layers on the same viewport so a
/// mol-group transform applies to the whole tree. [`depict_molecule`] scales
/// after chem paint; the document path scales once after chrome + captions.
pub fn frame_offset(mol: &MoleculeIn) -> (f64, f64) {
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
    }
    let degree = atom_bond_degree(mol);
    let label_pad = FONT_PX * 0.85;
    let radical_pad = RADICAL_OFFSET_PX + RADICAL_DOT_PX;
    for a in &mol.atoms {
        if display_label(a, *degree.get(&a.index).unwrap_or(&0)).is_some() {
            min_x = min_x.min(a.x - label_pad);
            max_x = max_x.max(a.x + label_pad * 1.6);
            min_y = min_y.min(a.y - label_pad);
            max_y = max_y.max(a.y + label_pad);
        }
        if a.radical > 0 {
            min_x = min_x.min(a.x - radical_pad);
            max_x = max_x.max(a.x + radical_pad);
            min_y = min_y.min(a.y - radical_pad);
            max_y = max_y.max(a.y + radical_pad);
        }
    }
    let pad = PAD_PX;
    (pad - min_x, pad - min_y)
}

pub fn depict_molecule(mol: &MoleculeIn) -> Scene {
    depict_chem(mol).scale_uniform(mol.diagram_scale())
}

/// Chem ink in framed SVG space, **unscaled**. Document paint applies chrome
/// and captions here, then one [`Scene::scale_uniform`] so annotations share
/// `mol.scale`.
pub(crate) fn depict_chem(mol: &MoleculeIn) -> Scene {
    let color = mol.color.as_deref().unwrap_or("#111");
    let face = FaceStyle::Regular;
    // User-facing weight (default 1 = house). Helpers convert via diagram_weight.
    let weight = mol.weight;
    let _ = mol.diagram_weight(); // validate early
    let stroke_px = stroke_px_for_weight(weight);
    let label_grow = label_weight_grow_px(weight);
    let by_index: HashMap<i32, usize> = mol
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.index, i))
        .collect();

    let (dx, dy) = frame_offset(mol);
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
    }
    let degree = atom_bond_degree(mol);
    let label_pad = FONT_PX * 0.85;
    let radical_pad = RADICAL_OFFSET_PX + RADICAL_DOT_PX;
    for a in &mol.atoms {
        if display_label(a, *degree.get(&a.index).unwrap_or(&0)).is_some() {
            min_x = min_x.min(a.x - label_pad);
            max_x = max_x.max(a.x + label_pad * 1.6);
            min_y = min_y.min(a.y - label_pad);
            max_y = max_y.max(a.y + label_pad);
        }
        if a.radical > 0 {
            min_x = min_x.min(a.x - radical_pad);
            max_x = max_x.max(a.x + radical_pad);
            min_y = min_y.min(a.y - radical_pad);
            max_y = max_y.max(a.y + radical_pad);
        }
    }
    let pad = PAD_PX;
    let mut width = (max_x - min_x) + 2.0 * pad;
    let mut height = (max_y - min_y) + 2.0 * pad;

    let label_atoms: Vec<labels::AtomIn> = mol
        .atoms
        .iter()
        .map(|a| labels::AtomIn {
            x: a.x + dx,
            y: a.y + dy,
            label: display_label(a, *degree.get(&a.index).unwrap_or(&0)),
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
    let (shortened, placed) = place_backbone(&label_atoms, &label_bonds, FONT_PX, face, weight);

    // Ring bonds get interior normals → short inside offsets (not acyclic extend).
    let ring_coords: HashMap<i32, (f64, f64)> = mol
        .atoms
        .iter()
        .map(|a| (a.index, (a.x + dx, a.y + dy)))
        .collect();
    let rings = find_sssr(mol, 8);
    let ring_normals = bond_interior_normals(&rings, &ring_coords);
    let paint_orders = match mol.aromatic {
        AromaticStyle::Kekule => Some(kekule_paint_orders(mol, &rings)),
        AromaticStyle::Circle => None,
    };

    let mut prepared: Vec<DrawnBond> = Vec::new();
    for (bond, ends) in mol.bonds.iter().zip(shortened.iter()) {
        if !by_index.contains_key(&bond.begin) || !by_index.contains_key(&bond.end) {
            continue;
        }
        let i0 = by_index[&bond.begin];
        let i1 = by_index[&bond.end];
        let key = if bond.begin < bond.end {
            (bond.begin, bond.end)
        } else {
            (bond.end, bond.begin)
        };
        let order = paint_orders
            .as_ref()
            .and_then(|m| m.get(&key).copied())
            .unwrap_or(bond.order);
        let mut db = DrawnBond::new(
            bond.index, bond.begin, bond.end, ends.x1, ends.y1, ends.x2, ends.y2, order,
        );
        db.stereo = bond.stereo.clone();
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
        let tag = format!("bond-{} atom-{} atom-{}", bond.index, bond.begin, bond.end);
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
    if mol.aromatic == AromaticStyle::Circle {
        for (cx, cy, r) in aromatic_ring_circles(mol, &rings, &ring_coords) {
            bond_prims.push(Primitive::Circle {
                cx,
                cy,
                r,
                fill: None,
                stroke: Some(color.to_string()),
                stroke_width: stroke_px,
                opacity: 1.0,
                class: Some("aromatic".into()),
            });
        }
    }

    let mut label_prims: Vec<Primitive> = Vec::new();
    let mut label_ink_for_halo: Vec<crate::geom::Shape> = Vec::new();
    for (slot, pl) in placed.iter().enumerate() {
        let Some(pl) = pl else { continue };
        let atom_index = mol.atoms[slot].index;
        // Grow canvas if traveling text spills past the initial pad.
        width =
            width.max(pl.origin_x + FONT_PX * pl.text.chars().count() as f64 * 0.65 + pad * 0.25);
        height = height.max(pl.y + FONT_PX * 0.35 + pad * 0.25);
        if matches!(pl.side, labels::LabelSide::North | labels::LabelSide::South) {
            height = height.max(pl.atom_y + FONT_PX * 1.6 + pad * 0.25);
        }
        let Some(mut ink) = labels::label_ink_shape(pl, FONT_PX, face) else {
            continue;
        };
        if label_grow > 1e-9 {
            ink = ink.buffer(label_grow);
        }
        let d = ink.to_svg_d();
        label_ink_for_halo.push(ink);
        if d.is_empty() {
            continue;
        }
        label_prims.push(Primitive::Path {
            d,
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

    let radical_dots = radical_dot_prims(mol, &by_index, dx, dy, color);
    #[cfg(feature = "geom")]
    {
        for p in &radical_dots {
            if let Primitive::Circle { cx, cy, r, .. } = p {
                label_ink_for_halo.push(crate::geom::Shape::disk(*cx, *cy, *r, 12));
            }
        }
    }

    let shade_prims = paint_shade(mol, &by_index, dx, dy);
    let halo_prims = paint_halo(
        &bond_strokes_for_halo,
        &label_ink_for_halo,
        stroke_px,
        weight,
    );

    let mut debug_prims: Vec<Primitive> = Vec::new();
    if mol.debug_box {
        // Label ink AABBs only — never wrap bond/atom stroke primitives.
        // `label_ink_for_halo` is pushed in the same order as non-None `placed`.
        let mut ink_i = 0usize;
        for (slot, pl) in placed.iter().enumerate() {
            if pl.is_none() {
                continue;
            }
            if ink_i >= label_ink_for_halo.len() {
                break;
            }
            let ink = &label_ink_for_halo[ink_i];
            ink_i += 1;
            if let Some((x0, y0, x1, y1)) = ink.bounds() {
                debug_prims.push(label_debug_rect(x0, y0, x1, y1, mol.atoms[slot].index));
            }
        }
        debug_prims.push(mol_debug_rect(width, height));
    }
    if mol.debug_align {
        if let Some(ref ad) = mol.align_debug {
            debug_prims.extend(paint_align_debug(&mol.atoms, ad, dx, dy, FONT_PX * 0.55));
        }
    }

    let mut layers = Vec::new();
    if !shade_prims.is_empty() {
        layers.push(Layer {
            name: LayerName::Shading,
            primitives: shade_prims,
        });
    }
    if !halo_prims.is_empty() {
        layers.push(Layer {
            name: LayerName::Halo,
            primitives: halo_prims,
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
    if !radical_dots.is_empty() {
        layers.push(Layer {
            name: LayerName::Marks,
            primitives: radical_dots,
        });
    }
    if !debug_prims.is_empty() {
        layers.push(Layer {
            name: LayerName::Overlay,
            primitives: debug_prims,
        });
    }
    layers.sort_by_key(|l| l.name.paint_order());

    let mut overlays = Vec::new();
    if mol.debug_box {
        // Document box for the single-mol scene (compose adds one for multi-mol).
        overlays.push(scene_debug_rect(width, height));
    }
    let scene = Scene {
        width,
        height,
        x: 0.0,
        y: 0.0,
        viewports: vec![Viewport {
            id: mol.id.clone(),
            x: 0.0,
            y: 0.0,
            ox: 0.0,
            oy: 0.0,
            width,
            height,
            layers,
        }],
        overlays,
        halo: Vec::new(),
    };
    scene
}

/// Bond degree for each atom index (missing keys are 0).
fn atom_bond_degree(mol: &MoleculeIn) -> HashMap<i32, usize> {
    let mut d = HashMap::new();
    for a in &mol.atoms {
        d.entry(a.index).or_insert(0);
    }
    for b in &mol.bonds {
        *d.entry(b.begin).or_insert(0) += 1;
        *d.entry(b.end).or_insert(0) += 1;
    }
    d
}

/// Unit vector from the atom into the largest open wedge (away from bonds).
///
/// One neighbor: perpendicular to the bond, preferring SVG-up (−Y) so a
/// terminal `CH2•` sits above the carbon rather than on the traveling H.
/// Isolated atoms: north-east.
fn radical_open_dir(ax: f64, ay: f64, nbrs: &[(f64, f64)]) -> (f64, f64) {
    const TAU: f64 = std::f64::consts::TAU;
    if nbrs.is_empty() {
        let s = std::f64::consts::FRAC_1_SQRT_2;
        return (s, -s);
    }
    if nbrs.len() == 1 {
        let (nx, ny) = nbrs[0];
        let bx = nx - ax;
        let by = ny - ay;
        let len = bx.hypot(by).max(1e-9);
        let px = -by / len;
        let py = bx / len;
        // Prefer SVG-up so the dot does not sit on traveling H.
        if py > 0.0 {
            return (-px, -py);
        }
        return (px, py);
    }
    let mut angs: Vec<f64> = nbrs
        .iter()
        .map(|&(nx, ny)| (ny - ay).atan2(nx - ax))
        .collect();
    angs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = angs.len();
    let mut best_gap = -1.0;
    let mut best_bisect = 0.0;
    for i in 0..n {
        let a0 = angs[i];
        let a1 = if i + 1 == n {
            angs[0] + TAU
        } else {
            angs[i + 1]
        };
        let gap = a1 - a0;
        if gap > best_gap {
            best_gap = gap;
            best_bisect = a0 + 0.5 * gap;
        }
    }
    (best_bisect.cos(), best_bisect.sin())
}

fn radical_dot_centers(ax: f64, ay: f64, dir: (f64, f64), n: u8) -> Vec<(f64, f64)> {
    let (dx, dy) = dir;
    let (px, py) = (-dy, dx);
    let offset = RADICAL_OFFSET_PX;
    match n {
        0 => Vec::new(),
        1 => vec![(ax + dx * offset, ay + dy * offset)],
        2 => {
            let h = 0.5 * RADICAL_PAIR_PX;
            vec![
                (ax + dx * offset + px * h, ay + dy * offset + py * h),
                (ax + dx * offset - px * h, ay + dy * offset - py * h),
            ]
        }
        n => {
            let n_i = i32::from(n);
            let span = std::f64::consts::FRAC_PI_2;
            (0..n_i)
                .map(|i| {
                    let t = if n_i == 1 {
                        0.0
                    } else {
                        (i as f64 / (n_i - 1) as f64) - 0.5
                    };
                    let ang = t * span;
                    let (c, s) = (ang.cos(), ang.sin());
                    let rx = dx * c - dy * s;
                    let ry = dx * s + dy * c;
                    (ax + rx * offset, ay + ry * offset)
                })
                .collect()
        }
    }
}

fn radical_dot_prims(
    mol: &MoleculeIn,
    by_index: &HashMap<i32, usize>,
    dx: f64,
    dy: f64,
    color: &str,
) -> Vec<Primitive> {
    let n = mol.atoms.len();
    let mut nbrs: Vec<Vec<(f64, f64)>> = vec![Vec::new(); n];
    for b in &mol.bonds {
        let Some(&i) = by_index.get(&b.begin) else {
            continue;
        };
        let Some(&j) = by_index.get(&b.end) else {
            continue;
        };
        if i >= n || j >= n {
            continue;
        }
        nbrs[i].push((mol.atoms[j].x + dx, mol.atoms[j].y + dy));
        nbrs[j].push((mol.atoms[i].x + dx, mol.atoms[i].y + dy));
    }
    let mut out = Vec::new();
    for (i, a) in mol.atoms.iter().enumerate() {
        if a.radical == 0 {
            continue;
        }
        let ax = a.x + dx;
        let ay = a.y + dy;
        let dir = radical_open_dir(ax, ay, &nbrs[i]);
        for (cx, cy) in radical_dot_centers(ax, ay, dir, a.radical) {
            out.push(Primitive::Circle {
                cx,
                cy,
                r: RADICAL_DOT_PX,
                fill: Some(color.to_string()),
                stroke: None,
                stroke_width: 0.0,
                opacity: 1.0,
                class: Some(format!("atom-{} radical", a.index)),
            });
        }
    }
    out
}

/// Explicit ``label``, else heteroatom / charged / isolated-carbon symbol.
///
/// Bonded carbons stay silent (skeletal vertices). Isolated carbon (no bonds)
/// still paints so `mol("C")` is not a blank canvas.
///
/// `label: Some("")` is an **explicit silence** (no element-symbol fallback).
/// `label: None` still invents heteroatom / charged / isolated-carbon text.
/// Chem text is not owned by chrome annotations — replace via `star_labels`
/// on `*` atoms.
///
/// Appends a charge suffix when the label body does not already include one
/// (matches Python ``display_text``).
fn display_label(a: &AtomIn, n_bonds: usize) -> Option<String> {
    let mut body = if let Some(ref l) = a.label {
        let t = l.trim();
        if t.is_empty() {
            // Explicit empty silence.
            return None;
        }
        Some(t.to_string())
    } else {
        None
    };
    if body.is_none() {
        let sym = a.symbol();
        if sym == "C" && a.charge == 0 && n_bonds > 0 {
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
    // Fixed window by default — never auto-scale to the data range.
    let vmin = mol.shade_vmin.unwrap_or(0.0);
    let vmax = mol.shade_vmax.unwrap_or(1.0);
    let diverging = vmin < 0.0 && vmax > 0.0;
    let base_r = shade_base_radius_px(!atom_zs.is_empty(), !bond_zs.is_empty());
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

pub(crate) fn normalize_shade_scores(zs: &[f64], vmin: f64, vmax: f64) -> Vec<f64> {
    if vmin >= 0.0 && vmax > 0.0 {
        return zs.iter().map(|&z| (z / vmax).clamp(0.0, 1.0)).collect();
    }
    if vmax <= 0.0 && vmin < 0.0 {
        return zs
            .iter()
            .map(|&z| (z / vmin.abs()).clamp(-1.0, 0.0))
            .collect();
    }
    let scale = vmin.abs().max(vmax.abs()).max(1e-9);
    zs.iter().map(|&z| (z / scale).clamp(-1.0, 1.0)).collect()
}

/// Base radius for PlotDot shade (matches [`paint_shade`]).
pub(crate) fn shade_base_radius_px(has_atom_shade: bool, has_bond_shade: bool) -> f64 {
    BOND_PX
        * (if has_atom_shade && has_bond_shade {
            SHADE_BOTH_FRAC
        } else {
            SHADE_FRAC
        })
}

/// Outer painted shade radius for one normalized score, or `None` if paint skips it.
///
/// Clash / Halo must use this — not the house max `SHADE_FRAC` disk.
pub(crate) fn shade_outer_radius_px(norm_z: f64, base_r: f64) -> Option<f64> {
    let plot = PlotDot::default();
    let max_frac = plot
        .rings(norm_z)
        .into_iter()
        .filter(|(r, c)| !(c.abs() < 0.05 && *r < 0.35) && c.abs() >= 0.02)
        .map(|(r, _)| r)
        .fold(0.0_f64, f64::max);
    if max_frac <= 0.0 {
        None
    } else {
        Some(base_r * max_frac)
    }
}

/// White knockout under bonds/labels so shade disks don't cover ink.
///
/// xenopict: union of per-ink buffers at a weight-scaled gap (√weight vs linear
/// ink), with a floor from halo stroke thickness. Requires the `geom` feature
/// (WASM enables it via `font`).
#[cfg(feature = "geom")]
fn paint_halo(
    bond_strokes: &[StrokePath],
    label_ink: &[crate::geom::Shape],
    stroke_px: f64,
    weight: f64,
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
        let pts = &sp.pts;
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
    // Match Python BondsDrawable: max(halo_gap(w), 0.25*halo_stroke(w) - ink_r).
    let ink_r = stroke_px * 0.5;
    let halo_stroke = halo_stroke_for_weight(weight);
    let dist = halo_gap_for_weight(weight).max(0.25 * halo_stroke - ink_r);
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
    _weight: f64,
) -> Vec<Primitive> {
    Vec::new()
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
            radical: 0,
            mapno: None,
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
            shade_vmin: None,
            shade_vmax: None,
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
            ..Default::default()
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
            shade_vmin: None,
            shade_vmax: None,
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
            ..Default::default()
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
                    radical: 0,
                    mapno: None,
                },
                AtomIn {
                    index: 1,
                    element: None,
                    z: Some(8),
                    x: 20.0,
                    y: 0.0,
                    label: Some("O".into()),
                    charge: 0,
                    radical: 0,
                    mapno: None,
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
            shade_vmin: None,
            shade_vmax: None,
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
            ..Default::default()
        };
        assert_eq!(mol.atoms[0].symbol(), "C");
        assert_eq!(mol.atoms[1].symbol(), "O");
        let scene = depict_molecule(&mol);
        assert_eq!(scene.viewports.len(), 1);
        assert_eq!(scene.viewports[0].id.as_deref(), Some("z"));
        let bonds = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .expect("bonds");
        assert_eq!(bonds.primitives.len(), 1);
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
                    data_text: Some(t), ..
                } => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|t| *t == "O"),
            "expected O label from z=8 atom, got {texts:?}"
        );
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
                    data_text: Some(t), ..
                } => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|t| *t == "OH" || *t == "HO"),
            "expected OH/HO path data-text, got {texts:?}"
        );
        assert!(
            labels
                .primitives
                .iter()
                .all(|p| matches!(p, Primitive::Path { .. })),
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
    fn mol_weight_thickens_bond_stroke() {
        use crate::metrics::{stroke_px_for_weight, STROKE_PX};
        let mut mol = ethanol();
        mol.weight = 2.0;
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
        let expected = stroke_px_for_weight(2.0);
        assert!(
            (sw - expected).abs() < 0.02,
            "weight stroke {sw} vs {expected}"
        );
        assert!(sw > STROKE_PX + 0.3);
    }

    #[test]
    fn mol_scale_doubles_scene_size_and_stroke() {
        let base = depict_molecule(&ethanol());
        let mut mol = ethanol();
        mol.scale = 2.0;
        let scaled = depict_molecule(&mol);
        assert!((scaled.width - 2.0 * base.width).abs() < 1e-6);
        assert!((scaled.height - 2.0 * base.height).abs() < 1e-6);
        let base_sw = base.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .unwrap()
            .primitives
            .iter()
            .find_map(|p| match p {
                Primitive::Path { stroke_width, .. } if *stroke_width > 0.0 => Some(*stroke_width),
                _ => None,
            })
            .unwrap();
        let scaled_sw = scaled.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .unwrap()
            .primitives
            .iter()
            .find_map(|p| match p {
                Primitive::Path { stroke_width, .. } if *stroke_width > 0.0 => Some(*stroke_width),
                _ => None,
            })
            .unwrap();
        assert!((scaled_sw - 2.0 * base_sw).abs() < 1e-6);
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
        assert_eq!(
            offset_count, 2,
            "centered C=O must emit exactly two offsets"
        );
        assert!(
            bonds.primitives.iter().all(|p| match p {
                Primitive::Path {
                    stroke: Some(s), ..
                } => s == "#336699",
                _ => true,
            }),
            "all bond strokes must use molecule color #336699"
        );
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
                radical: 0,
                mapno: None,
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
            shade_vmin: None,
            shade_vmax: None,
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
            ..Default::default()
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

    fn regular_hexagon_mol(orders: [f64; 6]) -> MoleculeIn {
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
                radical: 0,
                mapno: None,
            });
        }
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
        MoleculeIn {
            id: Some("hex".into()),
            atoms,
            bonds,
            color: None,
            atom_shade: None,
            bond_shade: None,
            shade_vmin: None,
            shade_vmax: None,
            weight: 1.0,
            scale: 1.0,
            aromatic: AromaticStyle::Circle,
            debug_box: false,
            debug_align: false,
            align_debug: None,
            ..Default::default()
        }
    }

    #[test]
    fn benzene_aromatic_orders_draw_ring_circle() {
        // CTAB/Chematic aromatic encoding (order 4) — not Kekulé doubles.
        let mol = regular_hexagon_mol([4.0; 6]);
        let scene = depict_molecule(&mol);
        let layer = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .expect("bonds");
        let aromatic_circles: Vec<_> = layer
            .primitives
            .iter()
            .filter(|p| match p {
                Primitive::Circle {
                    class: Some(c),
                    fill: None,
                    ..
                } => c.contains("aromatic"),
                _ => false,
            })
            .collect();
        assert_eq!(
            aromatic_circles.len(),
            1,
            "aromatic hexagon must draw one ring circle, got {:?}",
            layer.primitives.len()
        );
        match aromatic_circles[0] {
            Primitive::Circle { r, .. } => {
                // Inradius of regular hexagon with R=20 is ≈17.32; circle ≈ 0.60× ≈ 10.4.
                assert!(
                    *r > 9.0 && *r < 12.0,
                    "circle should sit inside ring, r={r}"
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn debug_box_draws_label_mol_and_scene_rects() {
        let mut mol = acetone();
        mol.debug_box = true;
        let scene = depict_molecule(&mol);
        let overlay = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Overlay)
            .expect("overlay layer");
        let classes: Vec<&str> = overlay
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Path { class: Some(c), .. } => Some(c.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            classes.iter().any(|c| c.contains("debug-box-label")),
            "expected label debug box, got {classes:?}"
        );
        assert!(
            classes.iter().any(|c| c.contains("debug-box-mol")),
            "expected mol debug box, got {classes:?}"
        );
        // Bond paths must not be wrapped as debug boxes.
        let bond_layer = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .expect("bonds");
        assert!(bond_layer.primitives.iter().all(|p| match p {
            Primitive::Path { class: Some(c), .. } => !c.contains("debug-box"),
            _ => true,
        }));
        assert!(
            scene.overlays.iter().any(|p| match p {
                Primitive::Path { class: Some(c), .. } => c.contains("debug-box-scene"),
                _ => false,
            }),
            "expected scene debug box in overlays"
        );
    }

    #[test]
    fn debug_box_off_by_default() {
        let scene = depict_molecule(&acetone());
        assert!(scene.viewports[0]
            .layers
            .iter()
            .all(|l| l.name != LayerName::Overlay
                || l.primitives.is_empty()
                || l.primitives.iter().all(|p| match p {
                    Primitive::Path { class: Some(c), .. } => !c.contains("debug-box"),
                    _ => true,
                })));
        assert!(scene.overlays.iter().all(|p| match p {
            Primitive::Path { class: Some(c), .. } => !c.contains("debug-box"),
            _ => true,
        }));
    }

    #[test]
    fn debug_align_paints_tags_and_badge() {
        use crate::scene::{AlignDebug, AlignDebugRole};
        let mut mol = acetone();
        mol.debug_align = true;
        mol.align_debug = Some(AlignDebug {
            role: AlignDebugRole::Query,
            peer_id: Some("m0".into()),
            tags: vec![(0, 5), (1, 1)],
        });
        let scene = depict_molecule(&mol);
        let overlay = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Overlay)
            .expect("overlay layer");
        let classes: Vec<&str> = overlay
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Circle { class: Some(c), .. }
                | Primitive::Text { class: Some(c), .. } => Some(c.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            classes.iter().any(|c| c.contains("debug-align-atom")),
            "expected atom tags, got {classes:?}"
        );
        assert!(
            classes.iter().any(|c| c.contains("debug-align-badge")),
            "expected badge, got {classes:?}"
        );
        let texts: Vec<&str> = overlay
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|t| *t == "5"),
            "tag text 5 missing: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("align→Q")),
            "query badge missing: {texts:?}"
        );
    }

    #[test]
    fn benzene_kekule_style_skips_circle_uses_doubles() {
        let mut mol = regular_hexagon_mol([4.0; 6]);
        mol.aromatic = AromaticStyle::Kekule;
        let scene = depict_molecule(&mol);
        let layer = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .expect("bonds");
        assert!(
            !layer.primitives.iter().any(|p| matches!(
                p,
                Primitive::Circle {
                    class: Some(c),
                    ..
                } if c.contains("aromatic")
            )),
            "kekule style must not draw aromatic circles"
        );
        let offset_count = layer
            .primitives
            .iter()
            .filter(|p| match p {
                Primitive::Path { class: Some(c), .. } => c.contains("bond-offset"),
                _ => false,
            })
            .count();
        assert_eq!(offset_count, 3, "kekule paint should invent three doubles");
    }

    #[test]
    fn shade_layers() {
        let mut mol = ethanol();
        mol.atom_shade = Some(vec![0.0, 0.0, 0.9]);
        let scene = depict_molecule(&mol);
        let names: Vec<_> = scene.viewports[0].layers.iter().map(|l| l.name).collect();
        assert!(names.contains(&LayerName::Shading));
        assert!(names.contains(&LayerName::Bonds));
        // Rainbow LUT (not the old coral stub): high score → hot orange-red.
        let shade = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Shading)
            .expect("shading");
        assert!(shade.primitives.iter().any(|p| match p {
            Primitive::Circle { fill: Some(f), .. } =>
                f.starts_with("rgb(") && !f.starts_with("rgb(255,222"),
            _ => false,
        }));
        let has_halo = scene.viewports[0]
            .layers
            .iter()
            .any(|l| l.name == LayerName::Halo && !l.primitives.is_empty());
        assert!(
            has_halo,
            "expected bond/label halo knockout in mol Halo layer"
        );
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
                    fill: Some(f), r, ..
                } => Some((f.clone(), *r)),
                _ => None,
            })
            .collect();
        assert!(
            fills.len() >= 4,
            "atom+bond shade should emit multiple disks, got {}",
            fills.len()
        );
        // Combined atom+bond uses the smaller shared base, not SHADE_FRAC.
        let max_r = fills.iter().map(|(_, r)| *r).fold(0.0_f64, f64::max);
        assert!(
            (max_r - BOND_PX * SHADE_BOTH_FRAC).abs() < 0.05,
            "expected SHADE_BOTH_FRAC×bond when both atom+bond shaded, got {max_r}"
        );
    }

    #[test]
    fn shade_full_strength_outer_is_house_frac() {
        let mut mol = ethanol();
        mol.atom_shade = Some(vec![1.0, 0.0, 0.0]);
        let scene = depict_molecule(&mol);
        let shade = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Shading)
            .expect("shading");
        let max_r = shade
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Circle {
                    r, class: Some(c), ..
                } if c == "shade" => Some(*r),
                _ => None,
            })
            .fold(0.0_f64, f64::max);
        assert!(
            (max_r - BOND_PX * SHADE_FRAC).abs() < 1e-6,
            "full-score outer r={max_r}, expected {}×bond",
            SHADE_FRAC
        );
    }

    #[test]
    fn scene_json_roundtrips() {
        let scene = depict_molecule(&acetone());
        let json = serde_json::to_string(&scene).expect("ser");
        let back: Scene = serde_json::from_str(&json).expect("de");
        assert_eq!(back.viewports[0].id.as_deref(), Some("acetone"));
        let orig_names: Vec<_> = scene.viewports[0].layers.iter().map(|l| l.name).collect();
        let back_names: Vec<_> = back.viewports[0].layers.iter().map(|l| l.name).collect();
        assert_eq!(back_names, orig_names);
        assert!(back_names.contains(&LayerName::Bonds));
        assert!(back_names.contains(&LayerName::Labels));
        let bond_count = |s: &Scene| {
            s.viewports[0]
                .layers
                .iter()
                .find(|l| l.name == LayerName::Bonds)
                .map(|l| l.primitives.len())
                .unwrap_or(0)
        };
        assert_eq!(bond_count(&back), bond_count(&scene));
        assert_eq!(bond_count(&back), 4);
        let bond_colors = |s: &Scene| -> Vec<String> {
            s.viewports[0]
                .layers
                .iter()
                .find(|l| l.name == LayerName::Bonds)
                .into_iter()
                .flat_map(|l| l.primitives.iter())
                .filter_map(|p| match p {
                    Primitive::Path {
                        stroke: Some(s), ..
                    } => Some(s.clone()),
                    _ => None,
                })
                .collect()
        };
        let colors = bond_colors(&back);
        assert_eq!(colors, bond_colors(&scene));
        assert!(colors.iter().all(|c| c == "#336699"));
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
        assert!(scene.width.is_finite() && scene.height.is_finite());
        assert!(scene.width > PAD_PX);
        assert!(scene.height > PAD_PX);
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
                    data_text: Some(t), ..
                } => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|t| *t == "OH" || *t == "HO"),
            "expected OH/HO data-text, got {texts:?}"
        );
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
        assert_eq!(
            offsets.len(),
            2,
            "centered C=O must emit exactly two offsets"
        );
        assert!(bonds.primitives.iter().all(|p| match p {
            Primitive::Path {
                stroke: Some(s), ..
            } => s == "#336699",
            _ => true,
        }));
        assert_eq!(crate::metrics::OFFSET_PX, 4.5);
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
            shade_vmin: None,
            shade_vmax: None,
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
            ..Default::default()
        };
        let scene = depict_molecule(&mol);
        let names: std::collections::HashSet<_> =
            scene.viewports[0].layers.iter().map(|l| l.name).collect();
        assert!(names.contains(&LayerName::Shading));
        assert!(names.contains(&LayerName::Bonds));

        let shade = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Shading)
            .expect("shading");
        let disks: Vec<_> = shade
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Circle {
                    cx,
                    cy,
                    r,
                    class: Some(c),
                    ..
                } if c == "shade" => Some((*cx, *cy, *r)),
                _ => None,
            })
            .collect();
        // z=0 filtered; z=0.85 on fixed 0..1 window → 4 PlotDot rings at O.
        assert_eq!(disks.len(), 4);
        let base_r = BOND_PX * SHADE_FRAC;
        let max_r = disks.iter().map(|(_, _, r)| *r).fold(0.0_f64, f64::max);
        // PlotDot outer ring for |z|=0.85 is ~0.922×base — not stretched to 1.0.
        let expected = base_r * (0.85_f64).sqrt(); // rings use sqrt steps; 0.85→√0.85
        assert!(
            (max_r - expected).abs() < 1e-6,
            "outer shade radius = {max_r}, expected {expected}"
        );
        assert!(
            max_r < base_r - 1e-6,
            "0.85 must not auto-window to full strength (got {max_r} vs base {base_r})"
        );
        let (sx, sy, _) = disks[0];
        assert!(disks
            .iter()
            .all(|(x, y, _)| (*x - sx).abs() < 1e-9 && (*y - sy).abs() < 1e-9));
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
            shade_vmin: None,
            shade_vmax: None,
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
            ..Default::default()
        };
        let scene = depict_molecule(&empty);
        assert_eq!(scene.viewports[0].id.as_deref(), Some("empty"));
        assert!(scene.viewports[0].layers.is_empty());

        // Heteroatom without explicit label still gets a glyph path.
        let mol = MoleculeIn {
            id: None,
            atoms: vec![atom(0, "C", 0.0, 0.0, None), atom(1, "N", 20.0, 0.0, None)],
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
            shade_vmin: None,
            shade_vmax: None,
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
            ..Default::default()
        };
        let scene = depict_molecule(&mol);
        let labels = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Labels)
            .expect("labels");
        assert!(labels.primitives.iter().any(|p| match p {
            Primitive::Path {
                data_text: Some(t), ..
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
            radical: 0,
            mapno: None,
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
            shade_vmin: None,
            shade_vmax: None,
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
            ..Default::default()
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
                    data_text: Some(t), ..
                } => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|t| t.contains('⁺') || t.contains('+')));
        assert!(texts
            .iter()
            .any(|t| t.contains('⁻') || t.contains('−') || t.contains('-')));
    }

    #[test]
    fn radical_dots_paint_filled_circles_on_marks() {
        let mut methyl = atom(0, "C", 0.0, 0.0, Some("CH3"));
        methyl.radical = 1;
        let mol = MoleculeIn {
            id: None,
            atoms: vec![methyl],
            bonds: vec![],
            ..Default::default()
        };
        let scene = depict_molecule(&mol);
        let marks = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Marks)
            .expect("radical dots live on the marks layer");
        let dots: Vec<_> = marks
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Circle {
                    r,
                    class: Some(c),
                    fill: Some(f),
                    ..
                } if c.contains("radical") => Some((*r, f.clone(), c.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(dots.len(), 1, "methyl radical paints one dot, got {dots:?}");
        assert!(
            dots[0].0 > 0.5,
            "dot radius must be visible, got {}",
            dots[0].0
        );
        assert_eq!(dots[0].1, "#111");
        assert!(dots[0].2.contains("atom-0"));

        let mut carbene = atom(0, "C", 0.0, 0.0, Some("CH2"));
        carbene.radical = 2;
        let mol = MoleculeIn {
            id: None,
            atoms: vec![carbene],
            bonds: vec![],
            ..Default::default()
        };
        let scene = depict_molecule(&mol);
        let n = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Marks)
            .map(|l| {
                l.primitives
                    .iter()
                    .filter(|p| match p {
                        Primitive::Circle { class: Some(c), .. } => c.contains("radical"),
                        _ => false,
                    })
                    .count()
            })
            .unwrap_or(0);
        assert_eq!(n, 2, "carbene paints two unpaired-electron dots");
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
            shade_vmin: None,
            shade_vmax: None,
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
            ..Default::default()
        };
        let scene = depict_molecule(&mol);
        assert!(scene.height > PAD_PX * 2.0);
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
                    data_text: Some(t), ..
                } => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|t| t.contains("NH") || t.contains("HN")),
            "expected NH2/HN2 label, got {texts:?}"
        );
    }

    #[test]
    fn shade_negative_and_diverging() {
        let mut mol = ethanol();
        // Explicit diverging window — defaults are vmin=0, vmax=1 (no auto-window).
        mol.shade_vmin = Some(-1.0);
        mol.shade_vmax = Some(1.0);
        mol.atom_shade = Some(vec![-0.8, -0.2, 0.0]);
        let scene = depict_molecule(&mol);
        let shade = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Shading)
            .expect("shading");
        assert!(
            shade.primitives.len() >= 2,
            "negative scores should still paint shade disks"
        );

        mol.atom_shade = Some(vec![-0.9, 0.0, 0.9]);
        let scene2 = depict_molecule(&mol);
        let shade2 = scene2.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Shading)
            .expect("shading");
        let fills: Vec<_> = shade2
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Circle { fill: Some(f), .. } => Some(f.as_str()),
                _ => None,
            })
            .collect();
        assert!(fills.len() >= 2);
        assert!(
            fills.iter().any(|f| f.starts_with("rgb(")),
            "diverging colormap must emit rgb() fills"
        );
    }

    #[test]
    fn shade_default_window_is_zero_to_one() {
        let mut mol = ethanol();
        // Peak score 0.45 must stay mid-strength — not stretched to vmax of data.
        mol.atom_shade = Some(vec![0.0, 0.0, 0.45]);
        let scene = depict_molecule(&mol);
        let shade = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Shading)
            .expect("shading");
        assert!(
            !shade.primitives.is_empty(),
            "0.45 on a 0..1 window should still paint"
        );
        // Auto-windowing to max=0.45 would normalize to 1.0 and grow the disk;
        // with fixed vmax=1 the radius stays clearly below a full-strength hit.
        let mut mol_hot = ethanol();
        mol_hot.atom_shade = Some(vec![0.0, 0.0, 1.0]);
        let hot = depict_molecule(&mol_hot);
        let hot_shade = hot.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Shading)
            .expect("shading");
        let max_r = |scene_shade: &crate::scene::Layer| {
            scene_shade
                .primitives
                .iter()
                .filter_map(|p| match p {
                    Primitive::Circle { r, .. } => Some(r),
                    _ => None,
                })
                .fold(0.0_f64, |a, b| a.max(*b))
        };
        assert!(
            max_r(shade) < max_r(hot_shade) * 0.85,
            "0.45 must not be auto-stretched to full strength (got {} vs hot {})",
            max_r(shade),
            max_r(hot_shade)
        );
    }

    #[test]
    fn skips_bonds_with_missing_indices() {
        let mut mol = ethanol();
        mol.bonds.push(BondIn {
            index: 99,
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
            .any(|l| l.name == LayerName::Bonds && !l.primitives.is_empty()));
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
            shade_vmin: None,
            shade_vmax: None,
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
            ..Default::default()
        };
        let scene = depict_molecule(&mol);
        let has_halo = scene.viewports[0]
            .layers
            .iter()
            .any(|l| l.name == LayerName::Halo && !l.primitives.is_empty());
        assert!(has_halo, "expected mol Halo layer");
        let bonds = scene.viewports[0]
            .layers
            .iter()
            .find(|l| l.name == LayerName::Bonds)
            .expect("bonds");
        let wedges: Vec<_> = bonds
            .primitives
            .iter()
            .filter(|p| match p {
                Primitive::Path { class: Some(c), .. } => c.contains("wedge"),
                _ => false,
            })
            .collect();
        assert_eq!(wedges.len(), 1);
        match wedges[0] {
            Primitive::Path {
                d, fill: Some(f), ..
            } => {
                assert!(d.starts_with("M "), "wedge path starts with move: {d}");
                assert!(d.contains('Z'), "wedge must be closed");
                assert_eq!(f, "#111");
            }
            _ => panic!("expected filled wedge path"),
        }
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
        // Only bond 0 score 0.9 survives the 0.01 filter.
        assert!(!shade.primitives.is_empty());
        assert!(shade.primitives.iter().all(|p| matches!(
            p,
            Primitive::Circle {
                class: Some(c),
                ..
            } if c == "shade"
        )));

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
