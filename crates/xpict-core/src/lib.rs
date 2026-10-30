//! Shared depiction core for **xpict**.
//!
//! Product path: [`doc::render`] (`DepictSpec` → `Rendered[]`). Language
//! wrappers call this crate; they do not reimplement layout, paint, or SVG.

#![forbid(unsafe_code)]

pub mod align_cost;
pub mod arrows;
pub mod bonds;
pub mod catmull;
pub mod colormap;
pub mod cxsmiles;
pub mod debug_align;
pub mod debug_box;
pub mod depict;
pub mod doc;
pub mod edge;
pub mod elements;
pub mod elk;
pub mod font;
pub mod geom;
#[cfg(feature = "font")]
pub mod labels;
#[cfg(feature = "font")]
pub mod markup;
pub mod metrics;
pub(crate) mod molfile;
pub mod plotdot;
pub mod rings;
pub mod scene;
pub(crate) mod smiles;
pub mod spokes;
pub(crate) mod structure;
pub mod svg;
pub(crate) mod valence;

pub mod align_layout;
pub mod layout_coordgen;
pub mod layout_spoke;

pub use align_cost::{
    cost_mapping, cost_mapping_with_generators_limited, mapping_cost, signed_side,
    substituents_same_side, AtomChirality, AtomFeat, BondFeat, EzStereo, MappingResult, MolGraph,
    SearchLimits, SearchStats, ALIGN_HOUSE_MAX_STATES, ALIGN_HOUSE_TIMEOUT_MS, COST_BOND_ORDER,
    COST_CHARGE_RADICAL, COST_ELEMENT, COST_UNMATCHED,
};
pub use align_layout::{
    align_molecule_constrained, hexagon_fit_max_dev, mol_graph_from_layout, ConstrainedAlignResult,
};
pub use arrows::{
    axis_aligned, clip_box_edge, edge_anchors, edge_primitives as arrow_edge_primitives,
    filleted_path_d, head_px, min_shaft_px, shaft_path_d, should_fillet, simplify_route,
    EdgePaintIn, EdgePrim, ANCHOR_GAP, KINK_PX, TURN_RADIUS,
};
pub use bonds::{
    bond_paths, bond_strokes, centered_displacements, depict_order, is_aromatic_order,
    join_centered_multibonds, line_intersect, multi_bond_offset, BondStrokes, DrawnBond,
    StrokePath,
};
pub use catmull::{
    centripetal_catmull_rom_closed_path, centripetal_catmull_rom_open_path, open_catmull_end_frame,
    open_catmull_with_arrow, sample_open_catmull,
};
pub use colormap::{colormap_rgb, sample_colormap, XENOSITE};
pub use cxsmiles::{
    apply_cx_by_index, cx_atom_labels, cx_multicenters, cx_source, smiles_base, Multicenter,
};
pub use debug_align::{
    align_debug_query, align_debug_template, paint_align_debug, DEBUG_ALIGN_BADGE,
    DEBUG_ALIGN_OPACITY, DEBUG_ALIGN_PALETTE, DEBUG_ALIGN_UNMATCHED,
};
pub use debug_box::{
    debug_rect, label_debug_rect, mol_debug_rect, scene_debug_rect, DEBUG_BOX_DASH,
    DEBUG_BOX_STROKE, DEBUG_BOX_STROKE_PX,
};
pub use depict::{depict_molecule, frame_offset};
pub use doc::{
    annotations_from_overlay, apply_star_labels, assign_mol_ids, canonicalize_overlay,
    compose_scheme, overlay_from_annotations, plan_edge, render_doc, resolve_mol_opts,
    validate_edges, validate_shade_colormap, AlignTo, AlignToSpec, Annotation, AtomLabelDetailed,
    AtomLabelSpec, AtomNumberBy, AtomNumberPolicy, AtomNumberSpec, CalloutField, CalloutSpec,
    CircleField, CircleSpec, CommonOpts, DepictSpec, DocPaint, EdgeArrow, EdgeNode, EdgeNodeKind,
    EdgeRouting, FillSpec, ForTypesPatch, Label, LabelItem, LabelLanes, LabelPlacement, LabelPos,
    LabelRole, LayoutAlgorithm, LayoutDirection, LayoutOpts, MolIds, MolNode, MolNodeKind, MolOpts,
    MolOverlayFields, MolText, MolTextSpec, Node, NodeType, Opts, OptsPatch, RegionField,
    RegionSpec, RegionStyle, RegionTextPlace, ShadeSpec, ShadeStyle, StrokeSpec, TextNode,
    TextNodeKind, TypedOptsPatch, Z_ATOMS, Z_ATOM_LABEL, Z_BACKGROUND, Z_BONDS, Z_CALLOUT,
    Z_CIRCLE, Z_HALO, Z_NAME, Z_REGION, Z_SHADE,
};
// Document ``render`` / ``depict`` live at ``doc::`` (crate-root ``depict`` is paint).
pub use doc::{
    process_edge_plan, process_edge_plan_with_frames, render, DepictRendered,
    SvgAtom as DepictSvgAtom, SvgBond as DepictSvgBond,
};
pub use edge::{
    AlignMode, AlignOpts, CoordGenMoleculeResult, CoordMethod, EdgePlan, EdgeResult, EdgeTask,
    EdgeTaskResult, MolTemplate, MIN_MCS_ATOMS as EDGE_MIN_MCS_ATOMS,
};
pub use elements::{atomic_number, element_symbol, SYMBOLS};
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
    apply_ring_interiors, aromatic_ring_circles, bond_interior_normals,
    find_relevant_rings_from_adj, find_sssr, find_sssr_from_adj, kekule_paint_orders, Ring,
    AROMATIC_CIRCLE_FRAC,
};
pub use scene::{
    AlignDebug, AlignDebugRole, AromaticStyle, Attach, Layer, LayerName, MoleculeIn, Primitive,
    Scene, SpokeIn, TextAnchor, Viewport,
};
pub use svg::{scene_to_svg, scene_to_svg_json};
// MoleculeIn atoms/bonds are scene::AtomIn / scene::BondIn (JSON ABI).
// labels::AtomIn / BondIn remain the place_backbone wire types.
