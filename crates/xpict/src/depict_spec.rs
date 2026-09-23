//! Declarative document — thin host over [`xpict_core::doc`].
//!
//! Pass 1 [`xpict_core::plan_edge`] → [`crate::process_edge_plan`] →
//! pass 2 [`xpict_core::render_doc`].

use std::collections::HashMap;

use xpict_core::cxsmiles::smiles_base;
use xpict_core::doc::{assign_mol_ids, plan_edge, render_doc};

use crate::edge_plan::process_edge_plan_with_frames;
use crate::{Error, Rendered, SvgAtom, SvgBond};

pub use xpict_core::{AlignTo, AlignToSpec, DepictSpec, MolNode, ShadeSpec};

/// Alias of [`MolNode`].
pub type MolSpec = MolNode;

fn to_coord_list(atoms: &[xpict_core::scene::AtomIn]) -> Vec<SvgAtom> {
    atoms
        .iter()
        .map(|a| SvgAtom {
            index: a.index,
            element: a.symbol().to_string(),
            x: a.x,
            y: a.y,
            label: a.label.clone(),
            charge: a.charge,
        })
        .collect()
}

fn source_of(node: &MolNode) -> Result<String, Error> {
    node.structure().map(str::to_string).map_err(Error::Message)
}

/// Declarative document → [`Rendered`] list via core plan/paint + RDKit edge.
pub fn depict(spec: &DepictSpec) -> Result<Vec<Rendered>, Error> {
    let Some(plan) = plan_edge(spec).map_err(Error::Message)? else {
        return Ok(vec![]);
    };
    let (edge, frames) = process_edge_plan_with_frames(&plan)?;
    let painted = render_doc(spec, &edge).map_err(Error::Message)?;
    let mols = spec.mols();
    let ids = assign_mol_ids(&mols).map_err(Error::Message)?;
    let id_to_node: HashMap<&str, &MolNode> = ids
        .iter()
        .zip(mols.iter())
        .map(|(id, n)| (id.as_str(), n))
        .collect();

    let mut out = Vec::with_capacity(painted.len());
    for row in painted {
        let node = id_to_node
            .get(row.id.as_str())
            .copied()
            .ok_or_else(|| Error::Message(format!("missing mol node {}", row.id)))?;
        let source = source_of(node)?;
        let frame = frames
            .get(&row.id)
            .cloned()
            .unwrap_or_else(|| smiles_base(&source));
        let coords = to_coord_list(&row.molecule.atoms);
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
        out.push(Rendered {
            width: row.scene.width,
            height: row.scene.height,
            scene: row.scene,
            molecule: row.molecule,
            source,
            frame_molblock: frame,
            svg_coords: coords.clone(),
            coords,
            bonds,
        });
    }
    Ok(out)
}
