//! Shared depiction core for **xpict**.
//!
//! Python under `python/xpict/` is still the working engine. This crate holds
//! algorithms we are ready to share with PyO3 and WASM — start small, move
//! modules as they stabilize. See the crate `README.md` for the migration
//! rule.

#![forbid(unsafe_code)]

pub mod align_cost;
pub mod arrows;
pub mod bonds;
pub mod colormap;
pub mod cxsmiles;
pub mod debug_align;
pub mod debug_box;
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

#[cfg(feature = "coordgen")]
pub mod align_layout;
#[cfg(feature = "coordgen")]
pub mod layout_coordgen;

pub use align_cost::{
    cost_mapping, mapping_cost, signed_side, substituents_same_side, AtomChirality, AtomFeat,
    BondFeat, EzStereo, MappingResult, MolGraph, COST_BOND_ORDER, COST_CHARGE_RADICAL,
    COST_ELEMENT, COST_UNMATCHED,
};
#[cfg(feature = "coordgen")]
pub use align_layout::{
    align_molecule_constrained, hexagon_fit_max_dev, mol_graph_from_layout, ConstrainedAlignResult,
};
pub use arrows::{
    axis_aligned, clip_box_edge, edge_anchors, edge_primitives as arrow_edge_primitives,
    filleted_path_d, shaft_path_d, should_fillet, simplify_route, EdgePaintIn, EdgePrim,
    ANCHOR_GAP, KINK_PX, TURN_RADIUS,
};
pub use bonds::{
    bond_paths, bond_strokes, centered_displacements, depict_order, is_aromatic_order,
    join_centered_multibonds, line_intersect, multi_bond_offset, BondStrokes, DrawnBond,
    StrokePath,
};
pub use colormap::{colormap_rgb, sample_colormap, XENOSITE};
pub use cxsmiles::{apply_cx_by_index, cx_atom_labels, cx_source, smiles_base};
pub use debug_align::{
    align_debug_query, align_debug_template, paint_align_debug, DEBUG_ALIGN_BADGE,
    DEBUG_ALIGN_OPACITY, DEBUG_ALIGN_PALETTE, DEBUG_ALIGN_UNMATCHED,
};
pub use debug_box::{
    debug_rect, label_debug_rect, mol_debug_rect, scene_debug_rect, DEBUG_BOX_DASH,
    DEBUG_BOX_STROKE, DEBUG_BOX_STROKE_PX,
};
pub use depict::depict_molecule;
pub use doc::{
    apply_star_labels, assign_mol_ids, compose_scheme, plan_edge, render_doc,
    resolve_mol_opts, validate_edges, AlignTo, AlignToSpec, CommonOpts, DepictSpec, DocPaint,
    EdgeArrow, EdgeNode, EdgeNodeKind, EdgeRouting, ForTypesPatch, Label, LabelItem, LabelLanes,
    LabelPlacement, LabelPos, LayoutAlgorithm, LayoutDirection, LayoutOpts, MolIds, MolNode,
    MolNodeKind, MolOpts,
    Node, NodeType, Opts, OptsPatch, ShadeSpec, ShadeStyle, TextNode, TextNodeKind, TypedOptsPatch,
};
// Document ``depict`` lives at ``doc::depict`` (crate-root name would clash
// with the ``depict`` paint module).
#[cfg(feature = "coordgen")]
pub use doc::{
    process_edge_plan, process_edge_plan_with_frames, DepictRendered, SvgAtom as DepictSvgAtom,
    SvgBond as DepictSvgBond,
};
pub use edge::{
    AlignOpts, CoordGenMoleculeResult, CoordMethod, EdgePlan, EdgeResult, EdgeTask,
    EdgeTaskResult, MolTemplate, MIN_MCS_ATOMS as EDGE_MIN_MCS_ATOMS,
};
pub use elements::{atomic_number, element_symbol, SYMBOLS};
#[cfg(feature = "elk")]
pub use elk::{
    layout_json as elk_layout_json, scheme_layout_options, DiagramKind as ElkDiagramKind,
};
pub use geom::{capsule_halo_path_d, disk_halo_path_d, polygon_to_svg_d};
#[cfg(feature = "font")]
pub use labels::{
    atom_display_label, compose_label, label_side, place_backbone, place_label, shorten_bond,
    split_label, AtomIn, BondIn, BondOut, LabelParts, LabelSide, PlacedLabel,
};
#[cfg(feature = "font")]
pub use markup::parse_label_markup;
pub use metrics::{BOND_PX, OFFSET_FRAC, OFFSET_PX, SCALE, SHADE_FRAC, STROKE_PX};
pub use plotdot::{PlotDot, ShadeDisk};
pub use rings::{
    apply_ring_interiors, aromatic_ring_circles, bond_interior_normals, find_sssr,
    kekule_paint_orders, Ring, AROMATIC_CIRCLE_FRAC,
};
pub use scene::{
    AlignDebug, AlignDebugRole, AromaticStyle, Layer, LayerName, MoleculeIn, Primitive, Scene,
    TextAnchor, Viewport,
};
// MoleculeIn atoms/bonds are scene::AtomIn / scene::BondIn (JSON ABI).
// labels::AtomIn / BondIn remain the place_backbone wire types.
