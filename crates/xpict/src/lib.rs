//! Public Rust molecule depiction API — mirrors JS ``xpict`` / Python.
//!
//! Layout and paint both live in [`xpict_core`] (Chematic + CoordGen). This
//! crate is the native `mol` / `render` / `depict` adapter.
//!
//! **Single molecule** — `mol` / `render` / `to_svg`; `align_to` is a template
//! source (SMILES or molfile). Rust computes coordinates:
//!
//! ```ignore
//! use xpict::{mol, MolRenderOptions};
//!
//! let mut m = mol("CCO")?;
//! let rendered = m.render(MolRenderOptions::default())?;
//! let svg = rendered.to_svg();
//! ```
//!
//! **Declarative document** — [`depict`] / [`DepictSpec`]:
//!
//! ```ignore
//! use xpict::{depict, DepictSpec, MolNode, Node};
//! let out = depict(&DepictSpec::Group {
//!     id: None,
//!     align: false,
//!     children: vec![
//!         Node::Mol(MolNode { smiles: Some("CCO".into()), ..Default::default() }),
//!         Node::Mol(MolNode { smiles: Some("CCCO".into()), ..Default::default() }),
//!     ],
//!     opts: None,
//!     color: None,
//!     scale: None,
//!     aromatic: None,
//!     debug_box: None,
//!     debug_align: None,
//!     pad: None,
//! })?;
//! ```

#![allow(clippy::module_name_repetitions)]

mod depict_spec;
mod edge_plan;

pub use depict_spec::{
    depict, AlignTo, AlignToSpec, DepictSpec, MolNode, MolSpec, Node, ShadeSpec,
};
pub use edge_plan::{build_align_plan, process_edge_plan, process_edge_plan_with_frames};
pub use xpict_core::{cx_atom_labels, smiles_base};

pub use xpict_core::edge::{
    AlignOpts as EdgeAlignOpts, CoordGenMoleculeResult, CoordMethod, EdgePlan, EdgeResult,
    EdgeTask, EdgeTaskResult, MolTemplate, MIN_MCS_ATOMS,
};
pub use xpict_core::scene::{AtomIn, BondIn, MoleculeIn, Scene};
pub use xpict_core::scene_to_svg;
pub use xpict_core::{
    apply_cx_by_index, apply_star_labels, depict_molecule, plan_edge, render_doc, SCALE,
};

pub use xpict_core::layout_coordgen::{
    generate_2d_coordinates, CoordGenError, LayoutMolecule, LayoutResult, BOND_STEREO_CIS,
    BOND_STEREO_NONE, BOND_STEREO_TRANS, FLAG_BEST_PRECISION, FLAG_EVEN_ANGLES,
    FLAG_FORCE_OPEN_MACROCYCLES, FLAG_FORCE_PLANAR, FLAG_MACROCYCLE_MODE, FLAG_METAL_BONDS_AS_ZOB,
    FLAG_QUICK_PRECISION, FLAG_SKIP_MINIMIZATION,
};

use xpict_core::scene::AtomIn as CoreAtom;

/// Errors from parse / layout / render.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("mol() requires a non-empty SMILES or molfile")]
    EmptySource,
    #[error("{0}")]
    Parse(String),
    #[error("{0}")]
    Layout(String),
    #[error("{0}")]
    Message(String),
}

/// Input molecule — SMILES/CXSMILES/molfile plus optional alignment frame.
#[derive(Debug, Clone)]
pub struct Mol {
    pub source: String,
    /// Coord-bearing molblock for the alignment frame (filled on first render).
    pub frame_molblock: Option<String>,
}

impl Mol {
    pub fn from_source(smiles_or_molfile: impl AsRef<str>) -> Result<Self, Error> {
        let text = smiles_or_molfile.as_ref().trim();
        if text.is_empty() {
            return Err(Error::EmptySource);
        }
        Ok(Self {
            source: text.to_string(),
            frame_molblock: None,
        })
    }

    pub fn render(&mut self, opts: MolRenderOptions) -> Result<Rendered, Error> {
        crate::render(self, opts)
    }

    /// Materialize (and cache) a coord-bearing frame molblock for ``align_to``.
    pub fn ensure_frame(&mut self) -> Result<&str, Error> {
        if self.frame_molblock.is_none() {
            let _ = crate::render(self, MolRenderOptions::default())?;
        }
        Ok(self.frame_molblock.as_deref().unwrap_or(""))
    }
}

/// Painted depiction — editable ``scene`` plus alignment frame.
#[derive(Debug, Clone)]
pub struct Rendered {
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

impl Rendered {
    pub fn to_svg(&self) -> String {
        scene_to_svg(&self.scene)
    }

    /// Pose molblock (CoordGen) — usable as ``MolRenderOptions::align_to``.
    pub fn frame(&self) -> &str {
        &self.frame_molblock
    }

    /// Render ``other`` aligned onto this pose.
    pub fn render_aligned(
        &self,
        other: &mut Mol,
        mut opts: MolRenderOptions,
    ) -> Result<Rendered, Error> {
        opts.align_to = Some(self.source.clone());
        render(other, opts)
    }
}

/// Atom position in SCALE / SVG space.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgAtom {
    pub index: i32,
    pub element: String,
    pub x: f64,
    pub y: f64,
    pub label: Option<String>,
    pub charge: i32,
    pub mapno: Option<i32>,
}

/// Bond endpoints in layout order.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgBond {
    pub index: i32,
    pub begin: i32,
    pub end: i32,
    pub order: f64,
    pub stereo: Option<String>,
}

/// Render options (parity with JS ``MolRenderOptions`` / Python client).
///
/// ``align_to`` is a template **source** (SMILES or molfile). Rust computes
/// coordinates; hosts do not pass poses.
#[derive(Debug, Clone, Default)]
pub struct MolRenderOptions {
    pub id: Option<String>,
    pub color: Option<String>,
    pub atom_shade: Option<Vec<f64>>,
    pub bond_shade: Option<Vec<f64>>,
    /// Shade window low (default ``0`` in paint). Not inferred from data.
    pub shade_vmin: Option<f64>,
    /// Shade window high (default ``1`` in paint). Not inferred from data.
    pub shade_vmax: Option<f64>,
    pub mark_atoms: Option<Vec<i32>>,
    pub mark_bonds: Option<Vec<(i32, i32)>>,
    pub star_labels: Option<Vec<Option<String>>>,
    /// Uniform diagram scale (``1.0`` = house size).
    pub scale: Option<f64>,
    /// Ink weight relative to house (``1.0``); min ~``2/3``.
    pub weight: Option<f64>,
    /// Aromatic rings: circle (default) or Kekulé doubles.
    pub aromatic: Option<xpict_core::AromaticStyle>,
    /// Dashed layout boxes for labels / mol / scene (default off).
    pub debug_box: Option<bool>,
    /// Alignment correspondence overlays (default off).
    pub debug_align: Option<bool>,
    /// Template source (SMILES or molfile).
    pub align_to: Option<String>,
    /// Pairs ``(query_atom, template_atom)``. Requires ``align_to``; skips MCS.
    pub atom_map: Option<Vec<(u32, u32)>>,
}

fn structure_fields(source: &str) -> (Option<String>, Option<String>, Option<String>) {
    let text = source.trim();
    if text.contains("V2000") || text.contains("V3000") || text.lines().count() >= 4 {
        (None, None, Some(text.to_string()))
    } else if text.contains('|') {
        (None, Some(text.to_string()), None)
    } else {
        (Some(text.to_string()), None, None)
    }
}

fn shade_from_opts(opts: &MolRenderOptions) -> Option<ShadeSpec> {
    if opts.atom_shade.is_none() && opts.bond_shade.is_none() {
        return None;
    }
    Some(ShadeSpec {
        atoms: opts.atom_shade.clone(),
        bonds: opts.bond_shade.clone(),
        colormap: None,
        vmin: opts.shade_vmin.unwrap_or(0.0),
        vmax: opts.shade_vmax.unwrap_or(1.0),
    })
}

fn mol_node_from(source: &str, opts: &MolRenderOptions, id: Option<String>) -> MolNode {
    let (smiles, cxsmiles, molfile) = structure_fields(source);
    MolNode {
        smiles,
        cxsmiles,
        molfile,
        id,
        color: opts.color.clone(),
        shade: shade_from_opts(opts),
        star_labels: opts.star_labels.clone(),
        scale: opts.scale,
        weight: opts.weight,
        aromatic: opts.aromatic,
        debug_box: opts.debug_box,
        debug_align: opts.debug_align,
        ..Default::default()
    }
}

fn spec_from_mol(source: &str, opts: &MolRenderOptions) -> Result<DepictSpec, Error> {
    if opts.atom_map.is_some() && opts.align_to.as_deref().unwrap_or("").is_empty() {
        return Err(Error::Parse("atom_map requires align_to".into()));
    }
    let query_id = opts.id.clone().unwrap_or_else(|| "m_1".into());
    if opts.align_to.as_deref().unwrap_or("").is_empty() {
        return Ok(DepictSpec::Mol(mol_node_from(
            source,
            opts,
            opts.id.clone(),
        )));
    }
    let template_src = opts.align_to.as_deref().unwrap_or("");
    let ref_id = "m_0".to_string();
    let qid = if query_id == ref_id {
        "m_1".to_string()
    } else {
        query_id
    };
    let mut query = mol_node_from(source, opts, Some(qid));
    query.align_to = Some(match &opts.atom_map {
        Some(map) => AlignTo::Spec(AlignToSpec {
            ref_id: ref_id.clone(),
            atom_map: Some(map.clone()),
            ..Default::default()
        }),
        None => AlignTo::Ref(ref_id.clone()),
    });
    Ok(DepictSpec::Group {
        id: None,
        align: false,
        children: vec![
            Node::Mol(mol_node_from(
                template_src,
                &MolRenderOptions::default(),
                Some(ref_id),
            )),
            Node::Mol(query),
        ],
        opts: None,
        color: None,
        scale: None,
        aromatic: None,
        debug_box: None,
        debug_align: None,
        pad: None,
    })
}

fn apply_marks(molecule: &mut MoleculeIn, opts: &MolRenderOptions) {
    if let Some(ref marks) = opts.mark_atoms {
        molecule.mark_atoms = marks.clone();
    }
    if let Some(ref marks) = opts.mark_bonds {
        molecule.mark_bonds = marks.clone();
    }
}

fn to_coord_list(atoms: &[CoreAtom]) -> Vec<SvgAtom> {
    atoms
        .iter()
        .map(|a| SvgAtom {
            index: a.index,
            element: a.symbol().to_string(),
            x: a.x,
            y: a.y,
            label: a.label.clone(),
            charge: a.charge,
            mapno: None,
        })
        .collect()
}

/// Construct a [`Mol`] (JS-shaped helper; prefer [`Mol::from_source`]).
pub fn mol(smiles_or_molfile: impl AsRef<str>) -> Result<Mol, Error> {
    Mol::from_source(smiles_or_molfile)
}

/// Layout → paint via core ``depict`` → [`Rendered`] (JS ``xpict.render``).
pub fn render(input: &mut Mol, opts: MolRenderOptions) -> Result<Rendered, Error> {
    let spec = spec_from_mol(&input.source, &opts)?;
    let mut rows = crate::depict(&spec)?;
    let mut painted = rows
        .pop()
        .ok_or_else(|| Error::Message("render returned no molecules".into()))?;
    apply_marks(&mut painted.molecule, &opts);
    if input.frame_molblock.is_none() && opts.align_to.as_deref().unwrap_or("").is_empty() {
        input.frame_molblock = Some(painted.frame_molblock.clone());
    }
    let coords = to_coord_list(&painted.molecule.atoms);
    if painted.coords.is_empty() {
        painted.coords = coords.clone();
    }
    if painted.svg_coords.is_empty() {
        painted.svg_coords = coords;
    }
    Ok(painted)
}

/// Scene → SVG (JS ``xpict.toSvg``).
pub fn to_svg(scene: &Scene) -> String {
    scene_to_svg(scene)
}
