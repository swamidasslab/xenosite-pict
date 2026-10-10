//! Single document entry: plan → resolve edge in-core → render.
//!
//! Same shape as the JS wrapper (`planEdge` → `processEdgePlan` → `renderDoc`),
//! without leaving Rust or marshalling to JSON between steps.

use serde::{Deserialize, Serialize};

use super::edge_resolve::process_edge_plan_with_frames;
use super::{assign_mol_ids, plan_edge, render_doc, DepictSpec, DocPaint, MolNode};
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

/// Declarative document → [`Rendered`] rows (Chematic + CoordGen inside).
pub fn depict(spec: &DepictSpec) -> Result<Vec<Rendered>, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{AlignTo, MolNode, Node};

    #[test]
    fn depict_ethanol() {
        let spec = DepictSpec::Mol {
            smiles: Some("CCO".into()),
            cxsmiles: None,
            molfile: None,
            id: None,
            shade: None,
            star_labels: None,
            align_to: None,
            label: None,
            color: None,
            scale: None,
            weight: None,
            halo: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            opts: None,
        };
        let painted = depict(&spec).unwrap();
        assert_eq!(painted.len(), 1);
        assert_eq!(painted[0].molecule.atoms.len(), 3);
        assert_eq!(painted[0].source, "CCO");
        assert_eq!(painted[0].coords.len(), 3);
        assert!(!painted[0].frame_molblock.is_empty());
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
        };
        let painted = depict(&spec).unwrap();
        assert_eq!(painted.len(), 2);
        assert!(painted[1].molecule.align_debug.is_some());
        assert_eq!(painted[0].source, "c1ccccc1");
    }
}
