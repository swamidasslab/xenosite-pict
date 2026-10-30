//! Declarative document — thin host over [`xpict_core::doc::depict`].

use crate::{Error, Rendered, SvgAtom, SvgBond};

pub use xpict_core::{
    AlignTo, AlignToSpec, CircleField, CircleSpec, DepictSpec, FillSpec, MolNode, Node, ShadeSpec,
    StrokeSpec,
};

/// Alias of [`MolNode`].
pub type MolSpec = MolNode;

/// Declarative document → [`Rendered`] list via core (SMILES/molfile + CoordGen).
pub fn depict(spec: &DepictSpec) -> Result<Vec<Rendered>, Error> {
    let painted = xpict_core::doc::depict(spec).map_err(Error::Message)?;
    Ok(painted
        .into_iter()
        .map(|r| Rendered {
            width: r.width,
            height: r.height,
            scene: r.scene,
            molecule: r.molecule,
            source: r.source,
            frame_molblock: r.frame_molblock,
            coords: r
                .coords
                .into_iter()
                .map(|a| SvgAtom {
                    index: a.index,
                    element: a.element,
                    x: a.x,
                    y: a.y,
                    label: a.label,
                    charge: a.charge,
                    radical: a.radical,
                    mapno: None,
                })
                .collect(),
            svg_coords: r
                .svg_coords
                .into_iter()
                .map(|a| SvgAtom {
                    index: a.index,
                    element: a.element,
                    x: a.x,
                    y: a.y,
                    label: a.label,
                    charge: a.charge,
                    radical: a.radical,
                    mapno: None,
                })
                .collect(),
            bonds: r
                .bonds
                .into_iter()
                .map(|b| SvgBond {
                    index: b.index,
                    begin: b.begin,
                    end: b.end,
                    order: b.order,
                    stereo: b.stereo,
                })
                .collect(),
        })
        .collect())
}
