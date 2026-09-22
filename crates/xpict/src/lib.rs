//! Public Rust molecule depiction API — mirrors JS ``xpict`` / Python ``Mol``.
//!
//! Layout uses the crates.io [`rdkit`] package (SMILES/molblock) plus a local
//! Depictor FFI for 2D coords / template align. Paint is [`xpict_core`].
//!
//! ```ignore
//! use xpict::{mol, render, to_svg};
//!
//! let m = mol("CCO")?;
//! let rendered = render(&m, Default::default())?;
//! let svg = to_svg(&rendered.scene);
//! ```
//!
//! Prefer method style:
//!
//! ```ignore
//! let svg = xpict::Mol::from_source("c1ccccc1")?.render(Default::default())?.to_svg();
//! ```

#![allow(clippy::module_name_repetitions)]

mod cxsmiles;
mod ffi;
mod layout;
mod svg;

pub use cxsmiles::{cx_atom_labels, smiles_base};
pub use layout::{layout_with_rdkit, sanitize_dummy_molblock, source_to_molblock};
pub use svg::scene_to_svg;
pub use xpict_core::scene::{AtomIn, BondIn, MoleculeIn, Scene};
pub use xpict_core::{depict_molecule, SCALE};

use xpict_core::scene::AtomIn as CoreAtom;

/// Errors from parse / layout / render.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("mol() requires a non-empty SMILES or molfile")]
    EmptySource,
    #[error("RDKit parse failed: {0}")]
    Parse(String),
    #[error("RDKit layout failed: {0}")]
    Layout(String),
    #[error("{0}")]
    Message(String),
}

/// Input molecule — SMILES/CXSMILES/molfile plus optional alignment frame.
#[derive(Debug, Clone)]
pub struct Mol {
    pub source: String,
    /// Coord-bearing molblock for the home frame (filled on first render).
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
            let (_, mb) = layout_with_rdkit(&self.source, None, None)?;
            self.frame_molblock = Some(mb);
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

    /// Pose molblock for ``MolRenderOptions::align_to``.
    pub fn frame(&self) -> &str {
        &self.frame_molblock
    }

    /// Render ``other`` aligned onto this pose.
    pub fn render_aligned(
        &self,
        other: &mut Mol,
        mut opts: MolRenderOptions,
    ) -> Result<Rendered, Error> {
        opts.align_to = Some(self.frame_molblock.clone());
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
/// ``align_to`` is a coord-bearing molblock (from [`Rendered::frame`] or
/// [`Mol::ensure_frame`]). JS/Python accept a ``Mol`` / ``Rendered`` object;
/// Rust takes the pose string to avoid awkward lifetimes.
#[derive(Debug, Clone, Default)]
pub struct MolRenderOptions {
    pub id: Option<String>,
    pub color: Option<String>,
    pub atom_shade: Option<Vec<f64>>,
    pub bond_shade: Option<Vec<f64>>,
    pub mark_atoms: Option<Vec<i32>>,
    pub mark_bonds: Option<Vec<(i32, i32)>>,
    pub star_labels: Option<Vec<Option<String>>>,
    pub bold_labels: Option<bool>,
    /// Template molblock for RDKit depiction matching.
    pub align_to: Option<String>,
}

fn is_star(a: &CoreAtom) -> bool {
    if a.element.as_deref() == Some("*") {
        return true;
    }
    a.z == Some(0)
}

fn apply_star_labels(mut molecule: MoleculeIn, labels: &[Option<String>]) -> MoleculeIn {
    if labels.is_empty() {
        return molecule;
    }
    let stars: Vec<usize> = molecule
        .atoms
        .iter()
        .enumerate()
        .filter(|(_, a)| is_star(a))
        .map(|(i, _)| i)
        .collect();
    if stars.is_empty() {
        return molecule;
    }
    for (k, raw) in labels.iter().enumerate() {
        if k >= stars.len() {
            break;
        }
        let label = match raw {
            None => "*".to_string(),
            Some(s) if s.trim().is_empty() => "*".to_string(),
            Some(s) => s.trim().to_string(),
        };
        molecule.atoms[stars[k]].label = Some(label);
    }
    molecule
}

fn apply_cx_by_index(mut molecule: MoleculeIn, source: &str) -> MoleculeIn {
    let aliases = cx_atom_labels(source);
    if aliases.is_empty() {
        return molecule;
    }
    for a in &mut molecule.atoms {
        let idx = a.index as usize;
        if let Some(Some(label)) = aliases.get(idx) {
            let label = label.trim();
            if !label.is_empty() {
                a.label = Some(label.to_string());
            }
        }
    }
    molecule
}

fn apply_opts(mut molecule: MoleculeIn, opts: &MolRenderOptions, source: &str) -> MoleculeIn {
    if let Some(ref id) = opts.id {
        molecule.id = Some(id.clone());
    }
    if let Some(ref color) = opts.color {
        molecule.color = Some(color.clone());
    }
    if let Some(ref shade) = opts.atom_shade {
        molecule.atom_shade = Some(shade.clone());
    }
    if let Some(ref shade) = opts.bond_shade {
        molecule.bond_shade = Some(shade.clone());
    }
    if let Some(ref marks) = opts.mark_atoms {
        molecule.mark_atoms = marks.clone();
    }
    if let Some(ref marks) = opts.mark_bonds {
        molecule.mark_bonds = marks.clone();
    }
    if let Some(bold) = opts.bold_labels {
        molecule.bold_labels = bold;
    }
    if let Some(ref labels) = opts.star_labels {
        molecule = apply_star_labels(molecule, labels);
    } else {
        molecule = apply_cx_by_index(molecule, source);
    }
    molecule
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
        })
        .collect()
}

/// Construct a [`Mol`] (JS-shaped helper; prefer [`Mol::from_source`]).
pub fn mol(smiles_or_molfile: impl AsRef<str>) -> Result<Mol, Error> {
    Mol::from_source(smiles_or_molfile)
}


/// Layout → Rust ``depict_molecule`` → [`Rendered`] (JS ``xpict.render``).
pub fn render(input: &mut Mol, opts: MolRenderOptions) -> Result<Rendered, Error> {
    let template = opts.align_to.as_deref().filter(|s| !s.is_empty());

    let (laid, pose_mb) =
        layout_with_rdkit(&input.source, template, opts.id.clone())?;
    if input.frame_molblock.is_none() {
        input.frame_molblock = Some(pose_mb.clone());
    }

    let molecule = apply_opts(laid, &opts, &input.source);
    let scene = depict_molecule(&molecule);
    let coords = to_coord_list(&molecule.atoms);
    let bonds: Vec<SvgBond> = molecule
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

    Ok(Rendered {
        width: scene.width,
        height: scene.height,
        scene,
        molecule,
        source: input.source.clone(),
        frame_molblock: pose_mb,
        svg_coords: coords.clone(),
        coords,
        bonds,
    })
}

/// Scene → SVG (JS ``xpict.toSvg``).
pub fn to_svg(scene: &Scene) -> String {
    scene_to_svg(scene)
}
