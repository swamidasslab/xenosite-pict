//! Single-molecule depiction: [`MoleculeIn`] → [`Scene`].
//!
//! MVP paint path for WASM/Python thin serializers. Caller supplies SVG-space
//! coords (+ optional shade / marks / atom labels). Halo is still follow-up.

use std::collections::HashMap;

use crate::bonds::{bond_strokes, join_centered_multibonds, DrawnBond};
use crate::labels::{self, place_backbone};
use crate::metrics::{BOND_PX, FONT_PX, MARK_FRAC, PAD_PX, SHADE_FRAC, STROKE_PX};
use crate::plotdot::PlotDot;
use crate::scene::{
    AtomIn, Layer, LayerName, MoleculeIn, Primitive, Scene, TextAnchor, Viewport,
};

/// Paint one molecule into a single-viewport [`Scene`].
///
/// Layers (bottom → top): shading → bonds → labels → marks.
pub fn depict_molecule(mol: &MoleculeIn) -> Scene {
    let color = mol.color.as_deref().unwrap_or("#111");
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
    let (shortened, placed) = place_backbone(&label_atoms, &label_bonds, FONT_PX);

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
        db.interior = bond.interior;
        db.begin_labeled = placed[i0].is_some();
        db.end_labeled = placed[i1].is_some();
        prepared.push(db);
    }
    join_centered_multibonds(&mut prepared);

    let mut bond_prims: Vec<Primitive> = Vec::new();
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
        );
        let tag = format!(
            "bond-{} atom-{} atom-{}",
            bond.index, bond.begin, bond.end
        );
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
    for (slot, pl) in placed.iter().enumerate() {
        let Some(pl) = pl else { continue };
        let atom_index = mol.atoms[slot].index;
        // Grow canvas if traveling text spills past the initial pad.
        width = width.max(pl.origin_x + FONT_PX * pl.text.len() as f64 * 0.65 + pad * 0.25);
        height = height.max(pl.y + FONT_PX * 0.35 + pad * 0.25);
        label_prims.push(Primitive::Text {
            x: pl.origin_x,
            y: pl.y,
            text: pl.text.clone(),
            fill: color.to_string(),
            font_size: FONT_PX,
            anchor: TextAnchor::Start,
            class: Some(format!("atom-{atom_index} label")),
        });
    }

    let shade_prims = paint_shade(mol, &by_index, dx, dy);
    let mark_prims = paint_marks(mol, &by_index, dx, dy);

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
        halo: Vec::new(),
    }
}

/// Explicit ``label``, else heteroatom / charged symbol (carbons stay silent).
fn display_label(a: &AtomIn) -> Option<String> {
    if let Some(ref l) = a.label {
        let t = l.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    let sym = a.symbol();
    if sym == "C" && a.charge == 0 {
        None
    } else {
        Some(sym.to_string())
    }
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
    let mut out = Vec::new();

    let mut emit = |zs: &[f64], coords: &[(f64, f64)]| {
        let norm = normalize_shade_scores(zs, vmin, vmax);
        let n = norm.len().min(coords.len());
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
                fill: Some(shade_rgb(d.color_z, diverging)),
                stroke: None,
                stroke_width: 0.0,
                opacity: 1.0,
                class: Some("shade".into()),
            });
        }
    };

    if !atom_zs.is_empty() {
        let coords: Vec<(f64, f64)> = mol
            .atoms
            .iter()
            .map(|a| (a.x + dx, a.y + dy))
            .collect();
        emit(atom_zs, &coords);
    }
    if !bond_zs.is_empty() {
        let mut mids = Vec::new();
        let mut scores = Vec::new();
        for (bond, &z) in mol.bonds.iter().zip(bond_zs.iter()) {
            let Some(&i0) = by_index.get(&bond.begin) else {
                continue;
            };
            let Some(&i1) = by_index.get(&bond.end) else {
                continue;
            };
            let a0 = &mol.atoms[i0];
            let a1 = &mol.atoms[i1];
            mids.push(((a0.x + a1.x) * 0.5 + dx, (a0.y + a1.y) * 0.5 + dy));
            scores.push(z);
        }
        if !scores.is_empty() {
            emit(&scores, &mids);
        }
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

/// Minimal sequential / diverging LUT (positive → warm coral) until full
/// xenosite colormap ports.
fn shade_rgb(z: f64, diverging: bool) -> String {
    let t = if diverging {
        ((z + 1.0) * 0.5).clamp(0.0, 1.0)
    } else {
        z.clamp(0.0, 1.0)
    };
    let r = (255.0 * (0.95 + 0.05 * t)) as i32;
    let g = (255.0 * (0.95 - 0.55 * t)) as i32;
    let b = (255.0 * (0.95 - 0.70 * t)) as i32;
    format!("rgb({r},{g},{b})")
}

fn paint_marks(
    mol: &MoleculeIn,
    by_index: &HashMap<i32, usize>,
    dx: f64,
    dy: f64,
) -> Vec<Primitive> {
    let mut out = Vec::new();
    let r = BOND_PX * MARK_FRAC;
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
            stroke: Some("#c44".into()),
            stroke_width: STROKE_PX,
            opacity: 0.85,
            class: Some(format!("atom-{ai} mark")),
        });
    }
    for &(a, b) in &mol.mark_bonds {
        let Some(&ia) = by_index.get(&a) else {
            continue;
        };
        let Some(&ib) = by_index.get(&b) else {
            continue;
        };
        let a0 = &mol.atoms[ia];
        let a1 = &mol.atoms[ib];
        out.push(Primitive::Path {
            d: format!(
                "M {:.2} {:.2} L {:.2} {:.2}",
                a0.x + dx,
                a0.y + dy,
                a1.x + dx,
                a1.y + dy
            ),
            stroke: Some("#c44".into()),
            fill: Some("none".into()),
            stroke_width: crate::metrics::HALO_STROKE,
            opacity: 0.35,
            stroke_dasharray: None,
            stroke_linecap: Some("round".into()),
            class: Some(format!("bond-mark atom-{a} atom-{b}")),
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
                Primitive::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|t| *t == "OH" || *t == "HO"),
            "expected OH/HO, got {texts:?}"
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
    }

    #[test]
    fn scene_json_roundtrips() {
        let scene = depict_molecule(&acetone());
        let json = serde_json::to_string(&scene).expect("ser");
        let back: Scene = serde_json::from_str(&json).expect("de");
        assert_eq!(back.viewports[0].id.as_deref(), Some("acetone"));
        assert!(!back.viewports[0].layers.is_empty());
    }
}
