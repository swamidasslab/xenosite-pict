//! Shared depiction core for **xpict**.
//!
//! Python under `python/xpict/` is still the working engine. This crate holds
//! algorithms we are ready to share with PyO3 and WASM — start small, move
//! modules as they stabilize. See the crate `README.md` for the migration
//! rule.

#![forbid(unsafe_code)]

pub mod align;
pub mod bonds;
pub mod colormap;
pub mod cxsmiles;
pub mod depict;
pub mod doc;
pub mod edge;
pub mod elements;
#[cfg(feature = "elk")]
pub mod elk;
pub mod font;
pub mod geom;
#[cfg(feature = "font")]
pub mod labels;
#[cfg(feature = "font")]
pub mod markup;
pub mod metrics;
pub mod plotdot;
pub mod rings;
pub mod scene;

pub use align::{kabsch_2d, rigid_align_coords, RigidTransform};
pub use bonds::{
    bond_paths, bond_strokes, centered_displacements, depict_order, join_centered_multibonds,
    line_intersect, multi_bond_offset, BondStrokes, DrawnBond, StrokePath,
};
pub use colormap::{colormap_rgb, sample_colormap, XENOSITE};
pub use cxsmiles::{apply_cx_by_index, cx_atom_labels, cx_source, smiles_base};
pub use depict::depict_molecule;
pub use doc::{
    apply_star_labels, assign_mol_ids, plan_edge, render_doc, AlignTo, AlignToSpec, DepictSpec,
    DocPaint, MolNode, ShadeSpec,
};
pub use edge::{
    AlignOpts, CoordGenMoleculeResult, CoordMethod, EdgePlan, EdgeResult, EdgeTask,
    EdgeTaskResult, MolTemplate, MIN_MCS_ATOMS as EDGE_MIN_MCS_ATOMS,
};
pub use elements::{atomic_number, element_symbol, SYMBOLS};
#[cfg(feature = "elk")]
pub use elk::layout_json as elk_layout_json;
pub use geom::{capsule_halo_path_d, disk_halo_path_d, polygon_to_svg_d};
#[cfg(feature = "font")]
pub use labels::{
    compose_label, label_side, place_backbone, place_label, shorten_bond, split_label, AtomIn,
    BondIn, BondOut, LabelParts, LabelSide, PlacedLabel,
};
#[cfg(feature = "font")]
pub use markup::parse_label_markup;
pub use metrics::{BOND_PX, OFFSET_FRAC, OFFSET_PX, SCALE, SHADE_FRAC, STROKE_PX};
pub use plotdot::{PlotDot, ShadeDisk};
pub use rings::{apply_ring_interiors, bond_interior_normals, find_sssr, Ring};
pub use scene::{Layer, LayerName, MoleculeIn, Primitive, Scene, TextAnchor, Viewport};
// MoleculeIn atoms/bonds are scene::AtomIn / scene::BondIn (JSON ABI).
// labels::AtomIn / BondIn remain the place_backbone wire types.
