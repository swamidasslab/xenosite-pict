//! Single document entry: plan → resolve edge in-core → render.
//!
//! Same shape as the JS wrapper (`planEdge` → `processEdgePlan` → `renderDoc`),
//! without leaving Rust or marshalling to JSON between steps.

use serde::{Deserialize, Serialize};

use super::edge_resolve::process_edge_plan_with_frames;
use super::{
    assign_mol_ids, compose_scheme, plan_edge, render_doc, DepictSpec, DocPaint, MolNode, Node,
};
use crate::scene::{MoleculeIn, Scene};

/// One painted molecule ready for hosts (parity with JS / Python ``Rendered``).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rendered {
    pub id: String,
    pub width: f64,
    pub height: f64,
    pub scene: Scene,
    pub molecule: MoleculeIn,
    pub source: String,
    pub frame_molblock: String,
    pub coords: Vec<SvgAtom>,
    pub svg_coords: Vec<SvgAtom>,
    pub bonds: Vec<SvgBond>,
}

/// Atom position in SCALE / SVG space (host ``Rendered.coords``).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SvgAtom {
    pub index: i32,
    pub element: String,
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub charge: i32,
    /// Unpaired electrons (`[CH3]` / `C[CH2]`). 0 when none.
    #[serde(default)]
    pub radical: u8,
}

/// Bond endpoints in layout order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SvgBond {
    pub index: i32,
    pub begin: i32,
    pub end: i32,
    pub order: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stereo: Option<String>,
}

fn source_of(node: &MolNode) -> Result<String, String> {
    node.structure().map(str::to_string)
}

fn pack_rendered(row: DocPaint, source: String) -> Rendered {
    let coords: Vec<SvgAtom> = row
        .molecule
        .atoms
        .iter()
        .map(|a| SvgAtom {
            index: a.index,
            element: a.symbol().to_string(),
            x: a.x,
            y: a.y,
            label: a.label.clone(),
            charge: a.charge,
            radical: a.radical,
        })
        .collect();
    let bonds: Vec<SvgBond> = row
        .molecule
        .bonds
        .iter()
        .map(|b| SvgBond {
            index: b.index,
            begin: b.begin,
            end: b.end,
            order: b.order,
            stereo: b.stereo.clone(),
        })
        .collect();
    Rendered {
        id: row.id,
        width: row.scene.width,
        height: row.scene.height,
        scene: row.scene,
        molecule: row.molecule,
        source,
        frame_molblock: row.frame_json,
        svg_coords: coords.clone(),
        coords,
        bonds,
    }
}

/// Layout every mol (CoordGen), paint the mol spec, return one row per mol.
fn paint_document(spec: &DepictSpec) -> Result<Vec<Rendered>, String> {
    let Some(plan) = plan_edge(spec)? else {
        return Ok(vec![]);
    };
    let (edge, frames) = process_edge_plan_with_frames(&plan)?;
    let mut painted = render_doc(spec, &edge)?;
    for row in &mut painted {
        if let Some(frame) = frames.get(&row.id) {
            row.frame_json = frame.clone();
        }
    }

    let mols = spec.mols();
    let ids = assign_mol_ids(&mols)?;
    let mut out = Vec::with_capacity(painted.len());
    for row in painted {
        let source = mols
            .iter()
            .zip(ids.iter())
            .find(|(_, id)| *id == &row.id)
            .map(|(n, _)| source_of(n))
            .transpose()?
            .unwrap_or_else(|| row.id.clone());
        out.push(pack_rendered(row, source));
    }
    Ok(out)
}

fn rendered_to_paint(row: &Rendered) -> DocPaint {
    DocPaint {
        id: row.id.clone(),
        molecule: row.molecule.clone(),
        scene: row.scene.clone(),
        frame_json: row.frame_molblock.clone(),
    }
}

fn compose_cell(spec: &DepictSpec, rows: &[Rendered]) -> Result<Rendered, String> {
    let paints: Vec<DocPaint> = rows.iter().map(rendered_to_paint).collect();
    let scene = compose_scheme(spec, &paints)?;
    let mut cell = rows[0].clone();
    cell.source = "reaction_scheme".into();
    cell.width = scene.width;
    cell.height = scene.height;
    cell.scene = scene;
    Ok(cell)
}

fn take_cells(
    spec: &DepictSpec,
    painted: &[Rendered],
    i: &mut usize,
) -> Result<Vec<Rendered>, String> {
    match spec {
        DepictSpec::Mol(_) => {
            let row = painted
                .get(*i)
                .ok_or_else(|| "render paint underflow".to_string())?
                .clone();
            *i += 1;
            Ok(vec![row])
        }
        DepictSpec::Group { children, .. } => {
            let mut out = Vec::new();
            for n in children {
                if matches!(n, Node::Mol(_)) {
                    let row = painted
                        .get(*i)
                        .ok_or_else(|| "render paint underflow".to_string())?
                        .clone();
                    *i += 1;
                    out.push(row);
                }
            }
            Ok(out)
        }
        DepictSpec::ReactionScheme { .. } => {
            let n = spec.mols().len();
            if *i + n > painted.len() {
                return Err("render paint underflow".into());
            }
            let slice = &painted[*i..*i + n];
            *i += n;
            if slice.is_empty() {
                return Ok(vec![]);
            }
            Ok(vec![compose_cell(spec, slice)?])
        }
        DepictSpec::Grid { children, .. } => {
            let mut out = Vec::new();
            for row in children {
                out.extend(take_cells(row, painted, i)?);
            }
            Ok(out)
        }
    }
}

/// Product entry: layout + mol-spec paint + pack cells (schemes compose).
///
/// Every mol goes through the same paint steps. Annotations on [`MolNode`]
/// (`region`, `callout`, `circle`, `atom_label`, `atom_number`, `name`,
/// `shade`) are part of that path, not a special case.
pub fn render(spec: &DepictSpec) -> Result<Vec<Rendered>, String> {
    let painted = paint_document(spec)?;
    let mut i = 0;
    let cells = take_cells(spec, &painted, &mut i)?;
    if i != painted.len() {
        return Err(format!(
            "render pack leftover paints: used {i} of {}",
            painted.len()
        ));
    }
    Ok(cells)
}

/// Alias of [`render`].
pub fn depict(spec: &DepictSpec) -> Result<Vec<Rendered>, String> {
    render(spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::chrome::{
        AtomLabelSpec, AtomNumberBy, AtomNumberSpec, CalloutField, CalloutSpec, CircleField,
        CircleSpec, FillSpec, MolText, RegionField, RegionSpec, RegionStyle, StrokeSpec,
    };
    use crate::doc::{
        AlignTo, EdgeArrow, EdgeNode, EdgeNodeKind, MolIds, MolNode, Node, ShadeSpec,
    };
    use crate::scene::{LayerName, Primitive};
    use std::collections::BTreeMap;

    fn scene_classes(scene: &Scene) -> Vec<String> {
        let mut out = Vec::new();
        for vp in &scene.viewports {
            for layer in &vp.layers {
                for p in &layer.primitives {
                    if let Some(c) = prim_class(p) {
                        out.push(c.to_string());
                    }
                }
            }
        }
        for p in scene.overlays.iter().chain(scene.halo.iter()) {
            if let Some(c) = prim_class(p) {
                out.push(c.to_string());
            }
        }
        out
    }

    fn prim_class(p: &Primitive) -> Option<&str> {
        match p {
            Primitive::Path { class, .. }
            | Primitive::Circle { class, .. }
            | Primitive::Text { class, .. } => class.as_deref(),
        }
    }

    #[test]
    fn depict_ethanol() {
        let spec = DepictSpec::Mol(MolNode {
            smiles: Some("CCO".into()),
            cxsmiles: None,
            molfile: None,
            id: None,
            shade: None,
            star_labels: None,
            align_to: None,
            label: None,
            name: None,
            atom_number: None,
            atom_label: None,
            region: None,
            circle: None,
            callout: None,
            color: None,
            scale: None,
            weight: None,
            halo: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
            opts: None,
            ..Default::default()
        });
        let painted = depict(&spec).unwrap();
        assert_eq!(painted.len(), 1);
        assert_eq!(painted[0].molecule.atoms.len(), 3);
        assert_eq!(painted[0].source, "CCO");
        assert_eq!(painted[0].coords.len(), 3);
        assert!(!painted[0].frame_molblock.is_empty());
    }

    #[test]
    fn depict_ethanol_and_aniline_paint_implicit_h_labels() {
        for (smiles, needle) in [("CCO", "OH"), ("c1ccccc1N", "NH₂")] {
            let spec = DepictSpec::Mol(MolNode {
                smiles: Some(smiles.into()),
                cxsmiles: None,
                molfile: None,
                id: None,
                shade: None,
                star_labels: None,
                align_to: None,
                label: None,
                name: None,
                atom_number: None,
                atom_label: None,
                region: None,
                circle: None,
                callout: None,
                color: None,
                scale: None,
                weight: None,
                halo: None,
                aromatic: None,
                debug_box: None,
                debug_align: None,
                pad: None,
                opts: None,
                ..Default::default()
            });
            let painted = depict(&spec).unwrap();
            let scene = &painted[0].scene;
            let texts: Vec<&str> = scene
                .viewports
                .iter()
                .flat_map(|v| v.layers.iter())
                .flat_map(|l| l.primitives.iter())
                .filter_map(|p| match p {
                    crate::scene::Primitive::Path {
                        data_text: Some(t), ..
                    } => Some(t.as_str()),
                    _ => None,
                })
                .collect();
            let ok = texts.iter().any(|t| {
                t.contains(needle)
                    || (needle == "OH" && (*t == "HO" || *t == "OH"))
                    || (needle == "NH₂" && (t.contains("H₂N") || t.contains("NH₂")))
            });
            assert!(
                ok,
                "{smiles}: expected {needle:?} (or flipped) in label texts {texts:?}"
            );
            assert!(
                !texts.iter().any(|t| *t == "O" || *t == "N"),
                "{smiles}: bare O/N without H, got {texts:?}"
            );
        }
    }

    #[test]
    fn depict_radicals_paint_dots_and_ch2() {
        for smiles in ["[CH3]", "C[CH2]", "[OH]"] {
            let spec = DepictSpec::Mol(MolNode {
                smiles: Some(smiles.into()),
                ..Default::default()
            });
            let painted = depict(&spec).unwrap();
            assert_eq!(painted.len(), 1, "{smiles}");
            let mol = &painted[0].molecule;
            assert!(
                mol.atoms.iter().any(|a| a.radical > 0),
                "{smiles}: parser must set radical, got {:?}",
                mol.atoms
                    .iter()
                    .map(|a| (a.label.clone(), a.radical))
                    .collect::<Vec<_>>()
            );
            let dots: Vec<_> = painted[0]
                .scene
                .viewports
                .iter()
                .flat_map(|v| v.layers.iter())
                .filter(|l| l.name == crate::scene::LayerName::Marks)
                .flat_map(|l| l.primitives.iter())
                .filter(|p| match p {
                    crate::scene::Primitive::Circle {
                        class: Some(c), r, ..
                    } => c.contains("radical") && *r > 0.5,
                    _ => false,
                })
                .collect();
            assert!(
                !dots.is_empty(),
                "{smiles}: expected a painted radical dot, scene layers {:?}",
                painted[0]
                    .scene
                    .viewports
                    .iter()
                    .flat_map(|v| v.layers.iter())
                    .map(|l| (l.name, l.primitives.len()))
                    .collect::<Vec<_>>()
            );
            if smiles == "C[CH2]" {
                let texts: Vec<&str> = painted[0]
                    .scene
                    .viewports
                    .iter()
                    .flat_map(|v| v.layers.iter())
                    .flat_map(|l| l.primitives.iter())
                    .filter_map(|p| match p {
                        crate::scene::Primitive::Path {
                            data_text: Some(t), ..
                        } => Some(t.as_str()),
                        _ => None,
                    })
                    .collect();
                assert!(
                    texts
                        .iter()
                        .any(|t| t.contains("CH₂") || t.contains("CH2") || t.contains("H₂C")),
                    "C[CH2] must paint a CH2 label plus the dot, got {texts:?}"
                );
            }
        }
    }

    #[test]
    fn depict_ch_radical_dot_misses_label_ink() {
        // `[CH3][CH]CC` used to park the unpaired-electron dot on the
        // traveling H. Label occupancy (same ink bonds already clear) must
        // keep the disk off the CH glyph, not a CH-specific rule.
        let spec = DepictSpec::Mol(MolNode {
            smiles: Some("[CH3][CH]CC".into()),
            ..Default::default()
        });
        let painted = depict(&spec).unwrap();
        assert_eq!(painted.len(), 1);
        let mol = &painted[0].molecule;
        let ch = mol
            .atoms
            .iter()
            .find(|a| a.radical > 0)
            .expect("CH carbon has unpaired electrons");
        let dots: Vec<(f64, f64, f64)> = painted[0]
            .scene
            .viewports
            .iter()
            .flat_map(|v| v.layers.iter())
            .filter(|l| l.name == crate::scene::LayerName::Marks)
            .flat_map(|l| l.primitives.iter())
            .filter_map(|p| match p {
                crate::scene::Primitive::Circle {
                    cx,
                    cy,
                    r,
                    class: Some(c),
                    ..
                } if c.contains(&format!("atom-{}", ch.index)) && c.contains("radical") => {
                    Some((*cx, *cy, *r))
                }
                _ => None,
            })
            .collect();
        assert_eq!(dots.len(), 1, "expected one radical dot, got {dots:?}");

        let (dx, dy) = crate::depict::frame_offset(mol);
        let by_index: std::collections::HashMap<i32, usize> = mol
            .atoms
            .iter()
            .enumerate()
            .map(|(i, a)| (a.index, i))
            .collect();
        let label_atoms: Vec<crate::labels::AtomIn> = mol
            .atoms
            .iter()
            .map(|a| crate::labels::AtomIn {
                x: a.x + dx,
                y: a.y + dy,
                label: a.label.clone(),
            })
            .collect();
        let label_bonds: Vec<crate::labels::BondIn> = mol
            .bonds
            .iter()
            .map(|b| crate::labels::BondIn {
                begin: by_index[&b.begin],
                end: by_index[&b.end],
            })
            .collect();
        let (_, placed) = crate::labels::place_backbone(
            &label_atoms,
            &label_bonds,
            crate::metrics::FONT_PX,
            crate::font::FaceStyle::Regular,
            mol.weight,
        );
        let slot = by_index[&ch.index];
        let pl = placed[slot].as_ref().expect("CH radical keeps a CH label");
        for (cx, cy, r) in dots {
            assert!(
                !crate::labels::disk_hits_label_ink(
                    pl,
                    crate::metrics::FONT_PX,
                    crate::font::FaceStyle::Regular,
                    cx,
                    cy,
                    r
                ),
                "CH radical disk at ({cx}, {cy}) r={r} overlaps label ink (side={:?} text={})",
                pl.side,
                pl.text
            );
        }
    }

    #[test]
    fn depict_methane_paints_ch4_not_blank() {
        let spec = DepictSpec::Mol(MolNode {
            smiles: Some("C".into()),
            ..Default::default()
        });
        let painted = depict(&spec).unwrap();
        assert_eq!(painted.len(), 1);
        let texts: Vec<&str> = painted[0]
            .scene
            .viewports
            .iter()
            .flat_map(|v| v.layers.iter())
            .flat_map(|l| l.primitives.iter())
            .filter_map(|p| match p {
                crate::scene::Primitive::Path {
                    data_text: Some(t), ..
                } => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts
                .iter()
                .any(|t| t.contains("CH₄") || t.contains("CH4") || t.contains('C')),
            "mol(\"C\") must paint a carbon label, got {texts:?}"
        );
        assert!(
            painted[0].scene.width > 8.0 && painted[0].scene.height > 8.0,
            "methane canvas must not be empty: {}x{}",
            painted[0].scene.width,
            painted[0].scene.height
        );
    }

    #[test]
    fn depict_aligned_pair() {
        let spec = DepictSpec::Group {
            id: None,
            align: true,
            children: vec![
                Node::Mol(MolNode {
                    id: Some("a".into()),
                    smiles: Some("c1ccccc1".into()),
                    ..Default::default()
                }),
                Node::Mol(MolNode {
                    id: Some("b".into()),
                    smiles: Some("Cc1ccccc1".into()),
                    align_to: Some(AlignTo::Ref("a".into())),
                    ..Default::default()
                }),
            ],
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        };
        let painted = depict(&spec).unwrap();
        assert_eq!(painted.len(), 2);
        assert!(painted[1].molecule.align_debug.is_some());
        assert_eq!(painted[0].source, "c1ccccc1");
    }

    /// Same overlay bag a lone mol and a scheme child both accept (one MolNode).
    fn kitchen_sink_mol() -> MolNode {
        MolNode {
            id: Some("ph".into()),
            smiles: Some("c1ccccc1O".into()),
            name: Some(MolText::Plain("phenol".into())),
            shade: Some(ShadeSpec {
                atoms: Some(vec![0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.0]),
                ..Default::default()
            }),
            atom_number: Some(AtomNumberSpec::ByName(AtomNumberBy::Index)),
            atom_label: Some(AtomLabelSpec::Map(BTreeMap::from([(
                "6".into(),
                MolText::Plain("exo".into()),
            )]))),
            circle: Some(CircleField::One(CircleSpec {
                atoms: vec![6],
                bonds: vec![],
                ring: None,
                fill: None,
                stroke: None,
                scale: None,
                z: None,
            })),
            region: Some(RegionField::One(RegionSpec {
                style: RegionStyle::Box,
                atoms: vec![0, 1, 2, 3, 4, 5],
                exclude: vec![],
                fill: None,
                stroke: None,
                text: Some(MolText::Plain("ring".into())),
                z: None,
            })),
            callout: Some(CalloutField::One(CalloutSpec {
                atoms: Some(vec![6]),
                bonds: None,
                text: MolText::Plain("OH".into()),
                arrow: true,
                prefer: None,
                color: None,
                scale: None,
                z: None,
            })),
            ..Default::default()
        }
    }

    fn assert_annotation_classes(classes: &str) {
        for needle in [
            "region",
            "callout",
            "atom-circle",
            "atom-number",
            "atom-label",
            "mol-caption",
            "shade",
        ] {
            assert!(
                classes.contains(needle),
                "missing {needle} in classes: {classes}"
            );
        }
    }

    #[test]
    fn root_mol_and_scheme_child_share_molnode_json() {
        let node = kitchen_sink_mol();
        let root = serde_json::to_value(&DepictSpec::Mol(node.clone())).unwrap();
        let child = serde_json::to_value(&Node::Mol(node)).unwrap();
        assert_eq!(root, child);
        for key in [
            "region",
            "circle",
            "callout",
            "atom_label",
            "atom_number",
            "name",
            "shade",
        ] {
            assert!(
                root.get(key).is_some(),
                "missing {key} on shared MolNode JSON"
            );
        }
    }

    #[test]
    fn render_annotates_mol_in_group() {
        let spec = DepictSpec::Group {
            id: None,
            align: false,
            children: vec![Node::Mol(kitchen_sink_mol())],
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        };
        let painted = render(&spec).unwrap();
        assert_eq!(painted.len(), 1);
        assert_annotation_classes(&scene_classes(&painted[0].scene).join(" "));
    }

    #[test]
    fn render_annotates_mol_in_scheme() {
        let spec = DepictSpec::ReactionScheme {
            id: None,
            children: vec![
                Node::Mol(kitchen_sink_mol()),
                Node::Mol(MolNode {
                    id: Some("et".into()),
                    smiles: Some("CCO".into()),
                    ..Default::default()
                }),
                Node::Edge(crate::doc::EdgeNode {
                    type_: crate::doc::EdgeNodeKind::Edge,
                    sources: MolIds::One("ph".into()),
                    targets: MolIds::One("et".into()),
                    label: None,
                    edge_routing: None,
                    arrow: crate::doc::EdgeArrow::Forward,
                    color: None,
                    dashed: false,
                    role: None,
                    stroke_width: None,
                }),
            ],
            layout: None,
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        };
        let painted = render(&spec).unwrap();
        assert_eq!(painted.len(), 1);
        assert_eq!(painted[0].source, "reaction_scheme");
        assert_annotation_classes(&scene_classes(&painted[0].scene).join(" "));
    }

    #[test]
    fn render_annotates_lone_mol_same_as_scheme() {
        let painted = render(&DepictSpec::Mol(kitchen_sink_mol())).unwrap();
        assert_eq!(painted.len(), 1);
        assert_annotation_classes(&scene_classes(&painted[0].scene).join(" "));
    }

    #[test]
    fn render_paints_circle_bond_midpoints() {
        let spec = DepictSpec::Mol(MolNode {
            smiles: Some("CCO".into()),
            circle: Some(CircleField::One(CircleSpec {
                atoms: vec![],
                bonds: vec![(0, 1), (1, 2)],
                ring: None,
                fill: None,
                stroke: Some(StrokeSpec {
                    color: Some("#c00".into()),
                    width: Some(1.4),
                    dash: None,
                }),
                scale: None,
                z: None,
            })),
            ..Default::default()
        });
        let painted = render(&spec).unwrap();
        let classes = scene_classes(&painted[0].scene).join(" ");
        assert!(classes.contains("bond-circle"), "{classes}");
        let svg = crate::scene_to_svg(&painted[0].scene);
        assert!(svg.contains("bond-circle"), "{svg}");
        assert!(svg.contains("#c00"), "{svg}");
    }

    #[test]
    fn mol_svg_is_one_group_with_nested_shading() {
        let spec = DepictSpec::Mol(MolNode {
            smiles: Some("*C |$GSH;$|".into()),
            region: Some(RegionField::One(RegionSpec {
                style: RegionStyle::Bezier,
                atoms: vec![0],
                exclude: vec![],
                fill: Some(FillSpec {
                    color: Some("#2a9d8f".into()),
                    opacity: Some(0.25),
                }),
                stroke: None,
                text: None,
                z: None,
            })),
            ..Default::default()
        });
        let painted = render(&spec).unwrap();
        let svg = crate::scene_to_svg(&painted[0].scene);
        assert_eq!(
            svg.matches(r#"class="xpict-mol""#).count(),
            1,
            "shading must nest inside the mol, not a sibling group: {svg}"
        );
        let mol = svg.find(r#"class="xpict-mol""#).unwrap();
        let shade = svg
            .find(r#"class="xpict-layer xpict-shading""#)
            .expect("shading layer");
        assert!(mol < shade, "shading group nested in mol: {svg}");
    }

    #[test]
    fn scaled_mol_aromatic_circle_sits_in_the_ring() {
        let spec = DepictSpec::Mol(MolNode {
            smiles: Some("NCCc1ccccc1".into()),
            scale: Some(0.6),
            ..Default::default()
        });
        let painted = render(&spec).unwrap();
        let vp = &painted[0].scene.viewports[0];
        let circle = vp
            .layers
            .iter()
            .filter(|l| l.name == LayerName::Marks)
            .flat_map(|l| l.primitives.iter())
            .find_map(|p| match p {
                Primitive::Circle {
                    cx,
                    cy,
                    r,
                    class: Some(c),
                    ..
                } if c.contains("aromatic") => Some((*cx, *cy, *r)),
                _ => None,
            })
            .expect("aromatic circle");
        let mut near = 0usize;
        for layer in &vp.layers {
            if layer.name != LayerName::Bonds {
                continue;
            }
            for p in &layer.primitives {
                let Primitive::Path {
                    d, class: Some(c), ..
                } = p
                else {
                    continue;
                };
                if !c.contains("bond-skeleton") {
                    continue;
                }
                for (x, y) in path_xy(d) {
                    if (x - circle.0).hypot(y - circle.1) <= 2.5 * circle.2 {
                        near += 1;
                    }
                }
            }
        }
        assert!(
            near >= 6,
            "scale must apply to chrome and chem together; circle {:?} only near {near} skeleton ends",
            circle
        );
    }

    #[test]
    fn scheme_gsh_region_covers_star_label() {
        let spec = DepictSpec::ReactionScheme {
            id: None,
            children: vec![
                Node::Mol(MolNode {
                    id: Some("a".into()),
                    smiles: Some("CCO".into()),
                    ..Default::default()
                }),
                Node::Mol(MolNode {
                    id: Some("b".into()),
                    smiles: Some("*C |$GSH;$|".into()),
                    region: Some(RegionField::One(RegionSpec {
                        style: RegionStyle::Bezier,
                        atoms: vec![0],
                        exclude: vec![],
                        fill: Some(FillSpec {
                            color: Some("#2a9d8f".into()),
                            opacity: Some(0.25),
                        }),
                        stroke: None,
                        text: None,
                        z: None,
                    })),
                    ..Default::default()
                }),
                Node::Edge(EdgeNode {
                    type_: EdgeNodeKind::Edge,
                    sources: MolIds::One("a".into()),
                    targets: MolIds::One("b".into()),
                    label: None,
                    edge_routing: None,
                    role: None,
                    arrow: EdgeArrow::Forward,
                    color: None,
                    stroke_width: None,
                    dashed: false,
                }),
            ],
            layout: None,
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        };
        let painted = render(&spec).unwrap();
        let scene = &painted[0].scene;
        let gsh = scene
            .viewports
            .iter()
            .find(|vp| vp.id.as_deref() == Some("b"))
            .expect("GSH mol");
        let region = gsh
            .layers
            .iter()
            .filter(|l| l.name == LayerName::Shading)
            .flat_map(|l| l.primitives.iter())
            .find_map(|p| match p {
                Primitive::Path {
                    d, class: Some(c), ..
                } if c.contains("region") => Some(path_aabb_pts(d)),
                _ => None,
            })
            .expect("region hull");
        let label = gsh
            .layers
            .iter()
            .filter(|l| l.name == LayerName::Labels)
            .flat_map(|l| l.primitives.iter())
            .find_map(|p| match p {
                Primitive::Path {
                    d,
                    data_text: Some(t),
                    ..
                } if t.contains("GSH") => Some(path_aabb_pts(d)),
                _ => None,
            })
            .expect("GSH label");
        let overlap_x = region.0 < label.2 && label.0 < region.2;
        let overlap_y = region.1 < label.3 && label.1 < region.3;
        assert!(
            overlap_x && overlap_y,
            "region {region:?} must cover GSH label {label:?} (same mol group, leftover not baked)"
        );
    }

    fn path_xy(d: &str) -> Vec<(f64, f64)> {
        let mut nums = Vec::new();
        let mut cur = String::new();
        for ch in d.chars() {
            if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E'
            {
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
        nums.chunks(2)
            .filter_map(|c| (c.len() == 2).then_some((c[0], c[1])))
            .collect()
    }

    fn path_aabb_pts(d: &str) -> (f64, f64, f64, f64) {
        let pts = path_xy(d);
        let xs = pts.iter().map(|p| p.0);
        let ys = pts.iter().map(|p| p.1);
        (
            xs.clone().fold(f64::INFINITY, f64::min),
            ys.clone().fold(f64::INFINITY, f64::min),
            xs.fold(f64::NEG_INFINITY, f64::max),
            ys.fold(f64::NEG_INFINITY, f64::max),
        )
    }
}
