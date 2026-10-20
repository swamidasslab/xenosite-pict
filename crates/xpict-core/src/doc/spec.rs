//! Live document wire types — **collocated** so opts + nodes stay aligned.
//!
//! # Cascade model
//!
//! - [`Opts`] is a **list** (or singleton) of [`OptsPatch`] rules.
//! - Patches are discriminated by document node `type` (`TypedOptsPatch`) or
//!   multi-select via reserved meta key `for_types` ([`ForTypesPatch`]).
//!   Omitting both = universal [`MolOpts`] (applies to every kind, including
//!   mol-only keys like `weight` / `halo` / `shade`).
//! - Meta keys (`type`, `for_types`) are selectors — not paint options.
//! - Resolve: walk ancestors → node; apply matching patches in order; child
//!   wins; nested objects (`shade`) deep-merge.
//! - [`MolNode`] / [`DepictSpec`] **extend** paint opts with non-cascading
//!   identity keys (`smiles`, `id`, `align_to`, shade **scores**, …). Local
//!   flat `color` / `weight` / `scale` / `halo` / `aromatic` are the leaf’s
//!   own opts (same fields as [`MolOpts`]) and merge last.

use serde::{Deserialize, Serialize};

#[cfg(feature = "codegen")]
use schemars::JsonSchema;
#[cfg(feature = "codegen")]
use ts_rs::TS;

use crate::doc::chrome::{
    annotations_from_overlay, overlay_from_mol, Annotation, AtomLabelSpec, AtomNumberSpec,
    CalloutField, CircleField, MolOverlayFields, MolText, RegionField,
};
use crate::edge::{AlignMode, AlignOpts};
use crate::scene::AromaticStyle;

// ---------------------------------------------------------------------------
// Shade: scores (non-cascading, on mol) vs style window (cascading, in opts)
// ---------------------------------------------------------------------------

/// Cascading shade window / LUT (not per-atom scores).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct ShadeStyle {
    /// Named colormap. Only ``"xenosite"`` (default) is painted; any other
    /// name is rejected at depict time (not silently ignored).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub colormap: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub vmin: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub vmax: Option<f64>,
}

impl ShadeStyle {
    pub fn merge_from(&mut self, other: &ShadeStyle) {
        if other.colormap.is_some() {
            self.colormap = other.colormap.clone();
        }
        if other.vmin.is_some() {
            self.vmin = other.vmin;
        }
        if other.vmax.is_some() {
            self.vmax = other.vmax;
        }
    }
}

/// Per-atom / per-bond colormap scores (+ legacy window fields on the mol).
///
/// Prefer putting `colormap` / `vmin` / `vmax` in cascading [`MolOpts::shade`];
/// values here still apply as local leaf overrides for compat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct ShadeSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub atoms: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub bonds: Option<Vec<f64>>,
    /// Named colormap. Only ``"xenosite"`` (default) is painted; any other
    /// name is rejected at depict time (not silently ignored).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub colormap: Option<String>,
    #[serde(default = "default_shade_vmin")]
    pub vmin: f64,
    #[serde(default = "default_shade_vmax")]
    pub vmax: f64,
}

fn default_shade_vmin() -> f64 {
    0.0
}
fn default_shade_vmax() -> f64 {
    1.0
}

impl Default for ShadeSpec {
    fn default() -> Self {
        Self {
            atoms: None,
            bonds: None,
            colormap: None,
            vmin: 0.0,
            vmax: 1.0,
        }
    }
}

impl ShadeSpec {
    /// Hoist legacy window fields into a cascading [`ShadeStyle`].
    pub fn style(&self) -> ShadeStyle {
        ShadeStyle {
            colormap: self.colormap.clone(),
            vmin: Some(self.vmin),
            vmax: Some(self.vmax),
        }
    }
}

// ---------------------------------------------------------------------------
// Align (non-cascading topology)
// ---------------------------------------------------------------------------

/// Object form of document ``align_to`` (template ref + EdgePlan-style opts).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct AlignToSpec {
    /// Id of the template mol in this group.
    #[serde(rename = "ref")]
    #[cfg_attr(feature = "codegen", ts(rename = "ref"))]
    pub ref_id: String,
    /// Pairs `(query, template)` vs the template; skips MCS when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub atom_map: Option<Vec<(u32, u32)>>,
    /// Override [`crate::edge::MIN_MCS_ATOMS`] when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub min_atoms: Option<u32>,
    /// `exact` (default) / `greedy` / `skip`. Omit = exact with house timeout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub mode: Option<AlignMode>,
    /// MCS budget in milliseconds. `None` = house 100 ms; `0` = unbounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub timeout_ms: Option<u32>,
}

/// Document align target: id string or `{ "ref", "atom_map"?, "min_atoms"?, "mode"?, "timeout_ms"? }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum AlignTo {
    /// Shorthand for `{ "ref": "…" }`.
    Ref(String),
    Spec(AlignToSpec),
}

impl AlignTo {
    pub fn ref_id(&self) -> &str {
        match self {
            AlignTo::Ref(s) => s.as_str(),
            AlignTo::Spec(s) => s.ref_id.as_str(),
        }
    }

    pub fn align_opts(&self) -> AlignOpts {
        match self {
            AlignTo::Ref(_) => AlignOpts::default(),
            AlignTo::Spec(s) => AlignOpts {
                atom_map: s.atom_map.clone(),
                min_atoms: s.min_atoms,
                mode: s.mode,
                timeout_ms: s.timeout_ms,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Cascading opts — list of patches; `type` / `for_types` are meta selectors
// ---------------------------------------------------------------------------

/// Document node kinds opts may target (matches wire `"type"` discriminants).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum NodeType {
    Mol,
    Group,
    /// Multi-mol scheme with edge nodes in `children` (`"type": "reaction_scheme"`).
    ReactionScheme,
    /// Edge / reaction link node (`"type": "edge"`).
    Edge,
    /// Text caption / chrome node (`"type": "text"`).
    Text,
}

impl NodeType {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeType::Mol => "mol",
            NodeType::Group => "group",
            NodeType::ReactionScheme => "reaction_scheme",
            NodeType::Edge => "edge",
            NodeType::Text => "text",
        }
    }
}

/// Universal cascading keys (any node kind).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct CommonOpts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub scale: Option<f64>,
    /// Aromatic ring paint: ``circle`` (default) or ``kekule``.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub aromatic: Option<AromaticStyle>,
    /// Paint dashed debug rectangles for layout boxes (labels / mol / scene).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub debug_box: Option<bool>,
    /// Paint alignment correspondence (matched atom tags + mol role badge).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub debug_align: Option<bool>,
    /// Post-layout SVG viewBox pad (px). Omit → small house default (rarely override).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub pad: Option<f64>,
}

/// Copy each ``Option`` field from ``$src`` onto ``$dst`` when ``Some``.
/// Keep the field list next to the struct when adding a cascading opt.
macro_rules! merge_opt_fields {
    ($dst:expr, $src:expr, $($field:ident),+ $(,)?) => {
        $(
            if $src.$field.is_some() {
                $dst.$field = $src.$field.clone();
            }
        )+
    };
}

impl CommonOpts {
    pub fn merge_from(&mut self, other: &CommonOpts) {
        merge_opt_fields!(
            self,
            other,
            color,
            scale,
            aromatic,
            debug_box,
            debug_align,
            pad
        );
    }
}

/// Mol cascading paint opts — what a mol leaf consumes from the cascade.
///
/// [`MolNode`] carries the same fields at the top level (local leaf opts) plus
/// non-cascading identity keys.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct MolOpts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub weight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub halo: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub shade: Option<ShadeStyle>,
    /// Aromatic ring paint: ``circle`` (default) or ``kekule``.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub aromatic: Option<AromaticStyle>,
    /// Paint dashed debug rectangles for layout boxes (labels / mol / scene).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub debug_box: Option<bool>,
    /// Paint alignment correspondence (matched atom tags + mol role badge).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub debug_align: Option<bool>,
    /// Post-layout SVG viewBox pad (px). Omit → small house default (rarely override).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub pad: Option<f64>,
}

impl MolOpts {
    pub fn merge_from(&mut self, other: &MolOpts) {
        merge_opt_fields!(
            self,
            other,
            color,
            scale,
            weight,
            halo,
            aromatic,
            debug_box,
            debug_align,
            pad
        );
        if let Some(ref s) = other.shade {
            self.shade
                .get_or_insert_with(ShadeStyle::default)
                .merge_from(s);
        }
    }

    pub fn merge_common(&mut self, common: &CommonOpts) {
        // Same keys as ``CommonOpts`` — update with that struct.
        merge_opt_fields!(
            self,
            common,
            color,
            scale,
            aromatic,
            debug_box,
            debug_align,
            pad
        );
    }
}

/// Discriminated opts patch: `{ "type": "mol"|"group"|"reaction_scheme"|"edge"|"text", …opts }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum TypedOptsPatch {
    Mol {
        #[serde(flatten)]
        opts: MolOpts,
    },
    Group {
        #[serde(flatten)]
        opts: CommonOpts,
    },
    ReactionScheme {
        #[serde(flatten)]
        opts: CommonOpts,
    },
    Edge {
        #[serde(flatten)]
        opts: CommonOpts,
    },
    Text {
        #[serde(flatten)]
        opts: CommonOpts,
    },
}

/// Multi-kind patch: `{ "for_types": ["mol","group"], …mol opts }`.
///
/// Carries full [`MolOpts`] so mol-only keys (`weight` / `halo` / `shade`) can
/// target mols via `for_types` without a separate typed mol patch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct ForTypesPatch {
    pub for_types: Vec<NodeType>,
    #[serde(flatten)]
    pub opts: MolOpts,
}

/// One cascade rule. Untagged order: typed → for_types → universal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum OptsPatch {
    /// `{ "type": "mol", "weight": 1.2 }` — discriminated kind.
    Typed(TypedOptsPatch),
    /// `{ "for_types": ["mol","group"], "color": "#111" }`.
    ForTypes(ForTypesPatch),
    /// `{ "color": "#111", "weight": 1.2 }` — applies to every kind ([`MolOpts`]).
    ///
    /// Universal is [`MolOpts`] (not [`CommonOpts`]) so container bags can stamp
    /// mol-only keys without wrapping `{ "type": "mol", … }`.
    Universal(MolOpts),
}

impl OptsPatch {
    /// Whether this patch applies when resolving for `target`.
    pub fn applies_to(&self, target: NodeType) -> bool {
        match self {
            OptsPatch::Typed(TypedOptsPatch::Mol { .. }) => target == NodeType::Mol,
            OptsPatch::Typed(TypedOptsPatch::Group { .. }) => target == NodeType::Group,
            OptsPatch::Typed(TypedOptsPatch::ReactionScheme { .. }) => {
                target == NodeType::ReactionScheme
            }
            OptsPatch::Typed(TypedOptsPatch::Edge { .. }) => target == NodeType::Edge,
            OptsPatch::Typed(TypedOptsPatch::Text { .. }) => target == NodeType::Text,
            OptsPatch::ForTypes(p) => p.for_types.contains(&target),
            OptsPatch::Universal(_) => true,
        }
    }

    pub fn apply_to_mol(&self, out: &mut MolOpts) {
        match self {
            OptsPatch::Typed(TypedOptsPatch::Mol { opts }) => out.merge_from(opts),
            OptsPatch::Typed(TypedOptsPatch::Group { .. }) => {}
            OptsPatch::Typed(TypedOptsPatch::ReactionScheme { .. }) => {}
            OptsPatch::Typed(TypedOptsPatch::Edge { .. }) => {}
            OptsPatch::Typed(TypedOptsPatch::Text { .. }) => {}
            OptsPatch::ForTypes(p) => out.merge_from(&p.opts),
            OptsPatch::Universal(m) => out.merge_from(m),
        }
    }
}

/// Cascade bag: singleton patch or list of patches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum Opts {
    One(OptsPatch),
    Many(Vec<OptsPatch>),
}

impl Opts {
    pub fn patches(&self) -> Vec<&OptsPatch> {
        match self {
            Opts::One(p) => vec![p],
            Opts::Many(ps) => ps.iter().collect(),
        }
    }

    pub fn apply_to_mol(&self, out: &mut MolOpts) {
        for p in self.patches() {
            if p.applies_to(NodeType::Mol) {
                p.apply_to_mol(out);
            }
        }
    }
}

/// Apply a chain of opts bags (ancestor → … → leaf) for a mol.
pub fn resolve_mol_opts<'a, I>(bags: I) -> MolOpts
where
    I: IntoIterator<Item = Option<&'a Opts>>,
{
    let mut out = MolOpts::default();
    for bag in bags {
        if let Some(opts) = bag {
            opts.apply_to_mol(&mut out);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Nodes — extend opts with non-cascading identity / topology keys
// ---------------------------------------------------------------------------

/// Discriminator for mol nodes (`"type": "mol"`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum MolNodeKind {
    #[default]
    Mol,
}

/// Mol node — [`MolOpts`] fields + non-cascading identity / scores / align.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct MolNode {
    #[serde(rename = "type", default)]
    pub type_: MolNodeKind,
    // --- non-cascading identity / topology ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub smiles: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub cxsmiles: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub molfile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub id: Option<String>,
    /// Shade **scores** (+ legacy window); window also cascades via [`Opts`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub shade: Option<ShadeSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub star_labels: Option<Vec<Option<String>>>,
    /// Template id string, or `{ "ref", "atom_map"?, "min_atoms"?, "mode"?, "timeout_ms"? }`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub align_to: Option<AlignTo>,
    /// Caption: [`Label`] (string id, list, or `{id, pos?}` — text-node refs).
    ///
    /// Prefer privileged [`Self::name`] for inline mol captions. `label` id-refs
    /// remain for legacy / multi-caption layouts.
    ///
    /// `pos` on a placement chooses caption side (`above`/`below`/`left`/`right`);
    /// hosts measure the resolved text and fold that into node size for layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub label: Option<Label>,
    /// Privileged figure name / caption painted inside the mol SVG (default below).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub name: Option<MolText>,
    /// Atom index / map-number labels (`"index"` | `"mapno"` | policy object).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub atom_number: Option<AtomNumberSpec>,
    /// Per-atom arbitrary text map (`{ "1": "α" }` or `{ "labels": {…}, "scale"? }`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub atom_label: Option<AtomLabelSpec>,
    /// Region highlight(s) — one object or a list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub region: Option<RegionField>,
    /// Atom / ring highlight circle(s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub circle: Option<CircleField>,
    /// Callout annotation(s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub callout: Option<CalloutField>,
    // --- local leaf opts (same keys as MolOpts; merge last) ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub weight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub halo: Option<bool>,
    /// Aromatic ring paint: ``circle`` (default) or ``kekule``.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub aromatic: Option<AromaticStyle>,
    /// Paint dashed debug rectangles for layout boxes (labels / mol / scene).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub debug_box: Option<bool>,
    /// Paint alignment correspondence (matched atom tags + mol role badge).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub debug_align: Option<bool>,
    /// Post-layout SVG viewBox pad (px). Omit → small house default (rarely override).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub pad: Option<f64>,
    /// Cascade patches for this node (list or singleton).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub opts: Option<Opts>,
}

impl MolNode {
    /// Local leaf opts from flat fields (+ legacy shade window).
    pub fn local_opts(&self) -> MolOpts {
        let mut o = MolOpts {
            color: self.color.clone(),
            scale: self.scale,
            weight: self.weight,
            halo: self.halo,
            shade: None,
            aromatic: self.aromatic,
            debug_box: self.debug_box,
            debug_align: self.debug_align,
            pad: self.pad,
        };
        if let Some(ref shade) = self.shade {
            o.shade = Some(shade.style());
        }
        o
    }

    pub fn structure(&self) -> Result<&str, String> {
        for s in [&self.molfile, &self.cxsmiles, &self.smiles] {
            if let Some(t) = s.as_ref().filter(|x| !x.trim().is_empty()) {
                return Ok(t.as_str());
            }
        }
        Err("mol node needs smiles, cxsmiles, or molfile".into())
    }

    /// Wire overlay bag (excludes privileged [`Self::name`]).
    pub fn overlay_fields(&self) -> MolOverlayFields {
        overlay_from_mol(
            &self.shade,
            &self.atom_number,
            &self.atom_label,
            &self.region,
            &self.circle,
            &self.callout,
        )
    }

    /// Internal Annotation IR for paint / occupancy (sorted by z).
    pub fn annotations(&self) -> Vec<Annotation> {
        annotations_from_overlay(&self.overlay_fields())
    }
}

// ---------------------------------------------------------------------------
// Edges / nodes — reaction links are edge nodes in `children`
// ---------------------------------------------------------------------------

/// Arrow head / shaft style for diagram edges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum EdgeArrow {
    /// Single →
    #[default]
    Forward,
    /// ⇌ stacked half-arrows
    Equilibrium,
    /// ⇒ hollow head (retrosynthetic-style)
    Open,
    /// Connector without arrowhead
    Line,
}

/// Discriminator for edge nodes (`"type": "edge"`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum EdgeNodeKind {
    #[default]
    Edge,
}

/// Discriminator for text nodes (`"type": "text"`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum TextNodeKind {
    #[default]
    Text,
}

/// Text **node** — caption / chrome referenced by mols and edges via id.
///
/// ```json
/// { "type": "text", "id": "adh", "text": "ADH" }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct TextNode {
    #[serde(rename = "type")]
    #[cfg_attr(feature = "codegen", ts(rename = "type"))]
    pub type_: TextNodeKind,
    /// Stable id — required when other nodes [`MolNode::label`] / edge lanes ref it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub id: Option<String>,
    /// Display text (markup-capable later).
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub opts: Option<Opts>,
}

/// Side of an edge shaft (or mol caption) for a label placement.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum LabelPos {
    #[default]
    Above,
    Below,
    Left,
    Right,
}

/// Placed label: `{ "id": "adh", "pos": "below" }` (`pos` optional → above).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct LabelPlacement {
    /// Id of a [`TextNode`] or (on edges) [`MolNode`].
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub pos: Option<LabelPos>,
}

impl LabelPlacement {
    pub fn pos_or_default(&self) -> LabelPos {
        self.pos.unwrap_or(LabelPos::Above)
    }
}

/// One entry in a label list: bare id or `{ id, pos? }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum LabelItem {
    Id(String),
    Placed(LabelPlacement),
}

impl LabelItem {
    pub fn id(&self) -> &str {
        match self {
            LabelItem::Id(s) => s.as_str(),
            LabelItem::Placed(p) => p.id.as_str(),
        }
    }

    pub fn pos(&self) -> LabelPos {
        match self {
            LabelItem::Id(_) => LabelPos::Above,
            LabelItem::Placed(p) => p.pos_or_default(),
        }
    }
}

/// Lane bag: `{ "above": [...], "below": [...], "left": [...], "right": [...] }`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct LabelLanes {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub above: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub below: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub left: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub right: Vec<String>,
}

/// Unified label: string id, list, placed object, or lane object.
///
/// ```json
/// "label": "adh"
/// "label": ["adh", {"id": "rt", "pos": "below"}]
/// "label": { "id": "adh", "pos": "left" }
/// "label": { "above": ["adh"], "below": ["rt"], "left": ["nabh4"] }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum Label {
    /// Bare node id (default position: above).
    Id(String),
    /// Ordered list of ids / placements.
    Items(Vec<LabelItem>),
    /// Single placement with optional `pos`.
    Placed(LabelPlacement),
    /// Explicit above / below / left / right id lists.
    Lanes(LabelLanes),
}

impl Label {
    /// Flatten to `(id, pos)` pairs in document order.
    pub fn placements(&self) -> Vec<(String, LabelPos)> {
        match self {
            Label::Id(id) => vec![(id.clone(), LabelPos::Above)],
            Label::Items(items) => items
                .iter()
                .map(|it| (it.id().to_string(), it.pos()))
                .collect(),
            Label::Placed(p) => vec![(p.id.clone(), p.pos_or_default())],
            Label::Lanes(lanes) => {
                let mut out = Vec::new();
                for id in &lanes.above {
                    out.push((id.clone(), LabelPos::Above));
                }
                for id in &lanes.below {
                    out.push((id.clone(), LabelPos::Below));
                }
                for id in &lanes.left {
                    out.push((id.clone(), LabelPos::Left));
                }
                for id in &lanes.right {
                    out.push((id.clone(), LabelPos::Right));
                }
                out
            }
        }
    }
}

/// One or more mol ids (reactants / products).
///
/// Wire: `"a"` or `["a", "b"]` — reactions may have multiple reactants and products.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum MolIds {
    One(String),
    Many(Vec<String>),
}

impl MolIds {
    pub fn as_slice(&self) -> &[String] {
        match self {
            MolIds::One(s) => std::slice::from_ref(s),
            MolIds::Many(v) => v.as_slice(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.as_slice().iter().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        match self {
            MolIds::One(s) => s.trim().is_empty(),
            MolIds::Many(v) => v.is_empty() || v.iter().all(|s| s.trim().is_empty()),
        }
    }

    pub fn len(&self) -> usize {
        self.as_slice().len()
    }
}

/// Edge **node** — a reaction / network link between mol ids.
///
/// [`Self::sources`] / [`Self::targets`] are one or many mol ids (A+B → C+D).
/// Label chrome refs sibling text/mol nodes via [`Self::label`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct EdgeNode {
    /// Wire discriminant — required so untagged [`Node`] does not absorb edges as mols.
    #[serde(rename = "type")]
    #[cfg_attr(feature = "codegen", ts(rename = "type"))]
    pub type_: EdgeNodeKind,
    /// Reactant mol id(s) — `"a"` or `["a","b"]` (alias: `source`).
    #[serde(alias = "source")]
    pub sources: MolIds,
    /// Product mol id(s) — `"c"` or `["c","d"]` (alias: `target`).
    #[serde(alias = "target")]
    pub targets: MolIds,
    /// Label chrome: id / list / `{id, pos?}` / `{above,below,left,right}`.
    ///
    /// Hosts should measure each placement’s text (or mol viewport) and pass
    /// those boxes to the layout engine with the placement `pos` (edge side).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub label: Option<Label>,
    /// Per-edge override of scheme [`LayoutOpts::edge_routing`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub edge_routing: Option<EdgeRouting>,
    /// Optional semantic role (e.g. enzyme) — not drawn by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub role: Option<String>,
    #[serde(default)]
    pub arrow: EdgeArrow,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub stroke_width: Option<f64>,
    #[serde(default)]
    pub dashed: bool,
}

impl EdgeNode {
    /// Effective shaft routing: edge override, else scheme layout default.
    pub fn edge_routing_or(&self, scheme: &LayoutOpts) -> EdgeRouting {
        self.edge_routing
            .unwrap_or_else(|| scheme.edge_routing_or_default())
    }
}

/// Document **node**: mol, edge, or text.
///
/// Untagged so each variant keeps its own `"type"` field.
/// - [`DepictSpec::Group`]: mol | text (no edges).
/// - [`DepictSpec::ReactionScheme`]: mol | edge | text.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum Node {
    Mol(MolNode),
    Edge(EdgeNode),
    Text(TextNode),
}

impl Node {
    pub fn as_mol(&self) -> Option<&MolNode> {
        match self {
            Node::Mol(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_edge(&self) -> Option<&EdgeNode> {
        match self {
            Node::Edge(e) => Some(e),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&TextNode> {
        match self {
            Node::Text(t) => Some(t),
            _ => None,
        }
    }

    /// Stable id when present (mol / text); edges have no id.
    pub fn id(&self) -> Option<&str> {
        match self {
            Node::Mol(m) => m.id.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            Node::Text(t) => t.id.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            Node::Edge(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Layout — reaction_scheme (backend-agnostic)
// ---------------------------------------------------------------------------

/// Flow axis for scheme layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum LayoutDirection {
    #[default]
    Right,
    Left,
    Up,
    Down,
}

/// How edge shafts are drawn between nodes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum EdgeRouting {
    Orthogonal,
    #[default]
    Polyline,
    Splines,
}

/// Placement algorithm for scheme layout.
///
/// Hosts map these onto ELK (or another engine):
/// - [`Self::Layered`] — Sugiyama / layered (default; best for reactions)
/// - [`Self::Radial`] — hub-and-spoke / concentric rings (star-like nets)
/// - [`Self::Force`] — force-directed (undirected / cyclic networks)
/// - [`Self::Stress`] — stress majorization (compact network alternative)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum LayoutAlgorithm {
    #[default]
    Layered,
    Radial,
    Force,
    Stress,
}

/// Layout for [`DepictSpec::ReactionScheme`].
///
/// Backend-agnostic knobs — hosts map these onto ELK, Dagre, or another engine.
/// Defaults when omitted: [`LayoutDirection::Right`], [`EdgeRouting::Polyline`],
/// [`LayoutAlgorithm::Layered`].
/// Individual [`EdgeNode`]s may override [`Self::edge_routing`].
///
/// Spacing (px, document space) controls packing tightness:
/// - [`Self::node_spacing`] — gap between nodes in the same layer (default ~20)
/// - [`Self::layer_spacing`] — gap between reactant/product layers (default ~0)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct LayoutOpts {
    /// Placement algorithm (`layered` / `radial` / `force` / `stress`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub algorithm: Option<LayoutAlgorithm>,
    /// Flow axis (`right` / `left` / `up` / `down`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub direction: Option<LayoutDirection>,
    /// Shaft style (`polyline` / `orthogonal` / `splines`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub edge_routing: Option<EdgeRouting>,
    /// Within-layer node gap (px). Smaller → tighter pack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub node_spacing: Option<f64>,
    /// Between-layer gap along the flow axis (px). Smaller → tighter pack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub layer_spacing: Option<f64>,
}

impl LayoutOpts {
    /// Default within-layer gap for reaction schemes (px).
    pub const DEFAULT_NODE_SPACING: f64 = 20.0;
    /// Default between-layer gap for reaction schemes (px).
    pub const DEFAULT_LAYER_SPACING: f64 = 0.0;

    pub fn algorithm_or_default(&self) -> LayoutAlgorithm {
        self.algorithm.unwrap_or_default()
    }

    pub fn direction_or_default(&self) -> LayoutDirection {
        self.direction.unwrap_or_default()
    }

    pub fn edge_routing_or_default(&self) -> EdgeRouting {
        self.edge_routing.unwrap_or_default()
    }

    pub fn node_spacing_or_default(&self) -> f64 {
        self.node_spacing.unwrap_or(Self::DEFAULT_NODE_SPACING)
    }

    pub fn layer_spacing_or_default(&self) -> f64 {
        self.layer_spacing.unwrap_or(Self::DEFAULT_LAYER_SPACING)
    }
}

/// One ancestor container’s contribution to the opts cascade (outer → inner).
#[derive(Debug, Clone)]
struct CascadeLayer {
    opts: Option<Opts>,
    common: CommonOpts,
}

/// Declarative document (`mol`, `group`, `reaction_scheme`, or `grid` root).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum DepictSpec {
    Mol {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        smiles: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        cxsmiles: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        molfile: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        shade: Option<ShadeSpec>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        star_labels: Option<Vec<Option<String>>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        align_to: Option<AlignTo>,
        /// Caption: [`Label`] (string id / list / `{id, pos?}` — text-node refs).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        label: Option<Label>,
        /// Privileged figure name / caption (inline mol SVG).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        name: Option<MolText>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        atom_number: Option<AtomNumberSpec>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        atom_label: Option<AtomLabelSpec>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        region: Option<RegionField>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        circle: Option<CircleField>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        callout: Option<CalloutField>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        color: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        scale: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        weight: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        halo: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        aromatic: Option<AromaticStyle>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        debug_box: Option<bool>,
        /// Paint alignment correspondence (matched atom tags + mol role badge).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        debug_align: Option<bool>,
        /// Post-layout SVG viewBox pad (px). Omit → small house default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        pad: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        opts: Option<Opts>,
    },
    Group {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        id: Option<String>,
        /// When true, later children align onto the first (or each `align_to`).
        #[serde(default)]
        align: bool,
        /// Child **nodes** — mol | text (no edges).
        #[serde(default)]
        children: Vec<Node>,
        /// Group-level cascade bag (list container for child inheritance).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        opts: Option<Opts>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        color: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        scale: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        aromatic: Option<AromaticStyle>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        debug_box: Option<bool>,
        /// Paint alignment correspondence (matched atom tags + mol role badge).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        debug_align: Option<bool>,
        /// Post-layout SVG viewBox pad (px). Omit → small house default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        pad: Option<f64>,
    },
    /// Reaction / pathway scheme: mixed **node** children (`mol` | `edge` | `text`).
    ReactionScheme {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        id: Option<String>,
        /// Child **nodes** — mols, edges, and text (any order).
        #[serde(default)]
        children: Vec<Node>,
        /// Scheme layout (direction, edge routing); backend maps these.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        layout: Option<LayoutOpts>,
        /// Scheme-level cascade bag for child mol inheritance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        opts: Option<Opts>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        color: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        scale: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        aromatic: Option<AromaticStyle>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        debug_box: Option<bool>,
        /// Paint alignment correspondence (matched atom tags + mol role badge).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        debug_align: Option<bool>,
        /// Post-layout SVG viewBox pad (px). Omit → small house default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        pad: Option<f64>,
    },
    /// Grid root — each child is one row (`mol` / `group` / `reaction_scheme` / nested `grid`).
    Grid {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        id: Option<String>,
        /// Row roots (nested grids flatten in document order when walking mols).
        #[serde(default)]
        children: Vec<DepictSpec>,
        /// Grid-level cascade bag (inherits onto every row / mol).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        opts: Option<Opts>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        color: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        scale: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        aromatic: Option<AromaticStyle>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        debug_box: Option<bool>,
        /// Paint alignment correspondence (matched atom tags + mol role badge).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        debug_align: Option<bool>,
        /// Post-layout SVG viewBox pad (px). Omit → small house default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        pad: Option<f64>,
    },
}

impl DepictSpec {
    fn cascade_layer(&self) -> Option<CascadeLayer> {
        match self {
            DepictSpec::Mol { .. } => None,
            DepictSpec::Group {
                opts,
                color,
                scale,
                aromatic,
                debug_box,
                debug_align,
                pad,
                ..
            }
            | DepictSpec::ReactionScheme {
                opts,
                color,
                scale,
                aromatic,
                debug_box,
                debug_align,
                pad,
                ..
            }
            | DepictSpec::Grid {
                opts,
                color,
                scale,
                aromatic,
                debug_box,
                debug_align,
                pad,
                ..
            } => Some(CascadeLayer {
                opts: opts.clone(),
                common: CommonOpts {
                    color: color.clone(),
                    scale: *scale,
                    aromatic: *aromatic,
                    debug_box: *debug_box,
                    debug_align: *debug_align,
                    pad: *pad,
                },
            }),
        }
    }

    fn mol_node_from_root_fields(
        smiles: &Option<String>,
        cxsmiles: &Option<String>,
        molfile: &Option<String>,
        id: &Option<String>,
        shade: &Option<ShadeSpec>,
        star_labels: &Option<Vec<Option<String>>>,
        align_to: &Option<AlignTo>,
        label: &Option<Label>,
        name: &Option<MolText>,
        atom_number: &Option<AtomNumberSpec>,
        atom_label: &Option<AtomLabelSpec>,
        region: &Option<RegionField>,
        circle: &Option<CircleField>,
        callout: &Option<CalloutField>,
        color: &Option<String>,
        scale: Option<f64>,
        weight: Option<f64>,
        halo: Option<bool>,
        aromatic: Option<AromaticStyle>,
        debug_box: Option<bool>,
        debug_align: Option<bool>,
        pad: Option<f64>,
        opts: &Option<Opts>,
    ) -> MolNode {
        MolNode {
            type_: MolNodeKind::Mol,
            smiles: smiles.clone(),
            cxsmiles: cxsmiles.clone(),
            molfile: molfile.clone(),
            id: id.clone(),
            shade: shade.clone(),
            star_labels: star_labels.clone(),
            align_to: align_to.clone(),
            label: label.clone(),
            name: name.clone(),
            atom_number: atom_number.clone(),
            atom_label: atom_label.clone(),
            region: region.clone(),
            circle: circle.clone(),
            callout: callout.clone(),
            color: color.clone(),
            scale,
            weight,
            halo,
            aromatic,
            debug_box,
            debug_align,
            pad,
            opts: opts.clone(),
        }
    }

    /// Walk mols in document order with ancestor cascade layers (outer → inner).
    fn collect_mols_with_layers(
        &self,
        stack: &mut Vec<CascadeLayer>,
        out: &mut Vec<(MolNode, Vec<CascadeLayer>)>,
    ) {
        match self {
            DepictSpec::Mol {
                smiles,
                cxsmiles,
                molfile,
                id,
                shade,
                star_labels,
                align_to,
                label,
                name,
                atom_number,
                atom_label,
                region,
                circle,
                callout,
                color,
                scale,
                weight,
                halo,
                aromatic,
                debug_box,
                debug_align,
                pad,
                opts,
            } => {
                out.push((
                    Self::mol_node_from_root_fields(
                        smiles,
                        cxsmiles,
                        molfile,
                        id,
                        shade,
                        star_labels,
                        align_to,
                        label,
                        name,
                        atom_number,
                        atom_label,
                        region,
                        circle,
                        callout,
                        color,
                        *scale,
                        *weight,
                        *halo,
                        *aromatic,
                        *debug_box,
                        *debug_align,
                        *pad,
                        opts,
                    ),
                    stack.clone(),
                ));
            }
            DepictSpec::Group { children, .. } | DepictSpec::ReactionScheme { children, .. } => {
                let layer = self.cascade_layer();
                if let Some(layer) = layer.clone() {
                    stack.push(layer);
                }
                for n in children {
                    if let Some(m) = n.as_mol() {
                        out.push((m.clone(), stack.clone()));
                    }
                }
                if layer.is_some() {
                    stack.pop();
                }
            }
            DepictSpec::Grid { children, .. } => {
                let layer = self.cascade_layer();
                if let Some(layer) = layer.clone() {
                    stack.push(layer);
                }
                for row in children {
                    row.collect_mols_with_layers(stack, out);
                }
                if layer.is_some() {
                    stack.pop();
                }
            }
        }
    }

    fn mols_with_layers(&self) -> Vec<(MolNode, Vec<CascadeLayer>)> {
        let mut stack = Vec::new();
        let mut out = Vec::new();
        self.collect_mols_with_layers(&mut stack, &mut out);
        out
    }

    /// Flatten to mol nodes in document order (skips edge / text; walks grid rows).
    pub fn mols(&self) -> Vec<MolNode> {
        self.mols_with_layers()
            .into_iter()
            .map(|(m, _)| m)
            .collect()
    }

    /// Grid row roots (empty when not a grid).
    pub fn grid_rows(&self) -> &[DepictSpec] {
        match self {
            DepictSpec::Grid { children, .. } => children.as_slice(),
            _ => &[],
        }
    }

    /// Child nodes of a flat container (empty for mol / grid roots).
    pub fn nodes(&self) -> &[Node] {
        match self {
            DepictSpec::Group { children, .. } | DepictSpec::ReactionScheme { children, .. } => {
                children.as_slice()
            }
            DepictSpec::Mol { .. } | DepictSpec::Grid { .. } => &[],
        }
    }

    /// Edge nodes in document order (includes edges inside grid rows).
    pub fn edges(&self) -> Vec<&EdgeNode> {
        match self {
            DepictSpec::Grid { children, .. } => {
                children.iter().flat_map(|row| row.edges()).collect()
            }
            _ => self.nodes().iter().filter_map(Node::as_edge).collect(),
        }
    }

    /// Text nodes in document order (includes texts inside grid rows).
    pub fn texts(&self) -> Vec<&TextNode> {
        match self {
            DepictSpec::Grid { children, .. } => {
                children.iter().flat_map(|row| row.texts()).collect()
            }
            _ => self.nodes().iter().filter_map(Node::as_text).collect(),
        }
    }

    /// Look up a mol or text node by id (for label / lane refs).
    pub fn node_by_id(&self, id: &str) -> Option<&Node> {
        match self {
            DepictSpec::Grid { children, .. } => children.iter().find_map(|row| row.node_by_id(id)),
            _ => self.nodes().iter().find(|n| n.id() == Some(id)),
        }
    }

    pub fn align_enabled(&self) -> bool {
        matches!(self, DepictSpec::Group { align: true, .. })
    }

    /// Container-level opts bag (`group`, `reaction_scheme`, or `grid`).
    pub fn container_opts(&self) -> Option<&Opts> {
        match self {
            DepictSpec::Group { opts, .. }
            | DepictSpec::ReactionScheme { opts, .. }
            | DepictSpec::Grid { opts, .. } => opts.as_ref(),
            DepictSpec::Mol { .. } => None,
        }
    }

    /// Group-level opts bag (if this is a group).
    #[inline]
    pub fn group_opts(&self) -> Option<&Opts> {
        match self {
            DepictSpec::Group { opts, .. } => opts.as_ref(),
            _ => None,
        }
    }

    /// Resolve cascading mol opts for child index `i` (0 for a mol root).
    ///
    /// Order: each ancestor (opts bag → flat common), outer → inner, then node
    /// `opts` → node local flat fields. Grid rows contribute their own layers.
    pub fn resolve_mol_chrome(&self, i: usize) -> MolOpts {
        let entries = self.mols_with_layers();
        let (node, layers) = entries.get(i).expect("mol index");
        let mut o = MolOpts::default();
        for layer in layers {
            if let Some(ref bag) = layer.opts {
                bag.apply_to_mol(&mut o);
            }
            o.merge_common(&layer.common);
        }
        if let Some(ref bag) = node.opts {
            bag.apply_to_mol(&mut o);
        }
        o.merge_from(&node.local_opts());
        o
    }

    /// Resolved chrome for every mol in document order (host paint / tests).
    pub fn resolve_all_mol_chrome(&self) -> Vec<MolOpts> {
        let n = self.mols_with_layers().len();
        (0..n).map(|i| self.resolve_mol_chrome(i)).collect()
    }
}
