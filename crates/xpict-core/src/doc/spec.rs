//! Live document wire types — **collocated** so opts + nodes stay aligned.
//!
//! # Cascade model
//!
//! - [`Opts`] is a **list** (or singleton) of [`OptsPatch`] rules.
//! - Patches are discriminated by document node `type` (`TypedOptsPatch`) or
//!   multi-select via reserved meta key `for_types` ([`ForTypesPatch`]).
//!   Omitting both = universal (applies to every kind).
//! - Meta keys (`type`, `for_types`) are selectors — not paint options.
//! - Resolve: walk ancestors → node; apply matching patches in order; child
//!   wins; nested objects (`shade`) deep-merge.
//! - [`MolNode`] / [`DepictSpec`] **extend** paint opts with non-cascading
//!   identity keys (`smiles`, `id`, `align_to`, shade **scores**, …). Local
//!   flat `color` / `weight` / `scale` / `halo` are the leaf’s own opts
//!   (same fields as [`MolOpts`]) and merge last.

use serde::{Deserialize, Serialize};

#[cfg(feature = "codegen")]
use schemars::JsonSchema;
#[cfg(feature = "codegen")]
use ts_rs::TS;

use crate::edge::AlignOpts;

// ---------------------------------------------------------------------------
// Shade: scores (non-cascading, on mol) vs style window (cascading, in opts)
// ---------------------------------------------------------------------------

/// Cascading shade window / LUT (not per-atom scores).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct ShadeStyle {
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct ShadeSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub atoms: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub bonds: Option<Vec<f64>>,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

/// Document align target: id string or `{ "ref", "atom_map"?, "min_atoms"? }`.
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
}

impl CommonOpts {
    pub fn merge_from(&mut self, other: &CommonOpts) {
        if other.color.is_some() {
            self.color = other.color.clone();
        }
        if other.scale.is_some() {
            self.scale = other.scale;
        }
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
}

impl MolOpts {
    pub fn merge_from(&mut self, other: &MolOpts) {
        if other.color.is_some() {
            self.color = other.color.clone();
        }
        if other.scale.is_some() {
            self.scale = other.scale;
        }
        if other.weight.is_some() {
            self.weight = other.weight;
        }
        if other.halo.is_some() {
            self.halo = other.halo;
        }
        if let Some(ref s) = other.shade {
            self.shade.get_or_insert_with(ShadeStyle::default).merge_from(s);
        }
    }

    pub fn merge_common(&mut self, common: &CommonOpts) {
        if common.color.is_some() {
            self.color = common.color.clone();
        }
        if common.scale.is_some() {
            self.scale = common.scale;
        }
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

/// Multi-kind patch: `{ "for_types": ["mol","group"], …common opts }`.
///
/// Only [`CommonOpts`] keys are allowed here (intersection of kind bags).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct ForTypesPatch {
    pub for_types: Vec<NodeType>,
    #[serde(flatten)]
    pub opts: CommonOpts,
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
    /// `{ "color": "#111" }` — applies to every kind.
    Universal(CommonOpts),
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
            OptsPatch::ForTypes(p) => out.merge_common(&p.opts),
            OptsPatch::Universal(c) => out.merge_common(c),
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
    /// Template id string, or `{ "ref", "atom_map"?, "min_atoms"? }`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub align_to: Option<AlignTo>,
    /// Caption: [`Label`] (string id, list, or `{id, pos?}` — text-node refs).
    ///
    /// `pos` on a placement chooses caption side (`above`/`below`/`left`/`right`);
    /// hosts measure the resolved text and fold that into node size for layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub label: Option<Label>,
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

/// Layout for [`DepictSpec::ReactionScheme`].
///
/// Backend-agnostic knobs — hosts map these onto ELK, Dagre, or another engine.
/// Defaults when omitted: [`LayoutDirection::Right`], [`EdgeRouting::Polyline`].
/// Individual [`EdgeNode`]s may override [`Self::edge_routing`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct LayoutOpts {
    /// Flow axis (`right` / `left` / `up` / `down`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub direction: Option<LayoutDirection>,
    /// Shaft style (`polyline` / `orthogonal` / `splines`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub edge_routing: Option<EdgeRouting>,
}

impl LayoutOpts {
    pub fn direction_or_default(&self) -> LayoutDirection {
        self.direction.unwrap_or_default()
    }

    pub fn edge_routing_or_default(&self) -> EdgeRouting {
        self.edge_routing.unwrap_or_default()
    }
}

/// Declarative document (`mol`, `group`, or `reaction_scheme` root).
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
    },
}

impl DepictSpec {
    /// Flatten to mol nodes in document order (skips edge / text children).
    pub fn mols(&self) -> Vec<MolNode> {
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
                color,
                scale,
                weight,
                halo,
                opts,
            } => vec![MolNode {
                type_: MolNodeKind::Mol,
                smiles: smiles.clone(),
                cxsmiles: cxsmiles.clone(),
                molfile: molfile.clone(),
                id: id.clone(),
                shade: shade.clone(),
                star_labels: star_labels.clone(),
                align_to: align_to.clone(),
                label: label.clone(),
                color: color.clone(),
                scale: *scale,
                weight: *weight,
                halo: *halo,
                opts: opts.clone(),
            }],
            DepictSpec::Group { children, .. }
            | DepictSpec::ReactionScheme { children, .. } => children
                .iter()
                .filter_map(Node::as_mol)
                .cloned()
                .collect(),
        }
    }

    /// Child nodes of a container (empty for a mol root).
    pub fn nodes(&self) -> &[Node] {
        match self {
            DepictSpec::Group { children, .. }
            | DepictSpec::ReactionScheme { children, .. } => children.as_slice(),
            DepictSpec::Mol { .. } => &[],
        }
    }

    /// Edge nodes in document order.
    pub fn edges(&self) -> Vec<&EdgeNode> {
        self.nodes().iter().filter_map(Node::as_edge).collect()
    }

    /// Text nodes in document order.
    pub fn texts(&self) -> Vec<&TextNode> {
        self.nodes().iter().filter_map(Node::as_text).collect()
    }

    /// Look up a mol or text node by id (for label / lane refs).
    pub fn node_by_id(&self, id: &str) -> Option<&Node> {
        self.nodes().iter().find(|n| n.id() == Some(id))
    }

    pub fn align_enabled(&self) -> bool {
        matches!(self, DepictSpec::Group { align: true, .. })
    }

    /// Container-level opts bag (`group` or `reaction_scheme`).
    pub fn container_opts(&self) -> Option<&Opts> {
        match self {
            DepictSpec::Group { opts, .. } | DepictSpec::ReactionScheme { opts, .. } => {
                opts.as_ref()
            }
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
    /// Order: container `opts` list → container flat common → node `opts` list →
    /// node local flat fields (incl. legacy shade window).
    pub fn resolve_mol_chrome(&self, i: usize) -> MolOpts {
        let mols = self.mols();
        let node = mols.get(i).expect("mol index");
        let mut o = MolOpts::default();
        if let Some(bag) = self.container_opts() {
            bag.apply_to_mol(&mut o);
        }
        match self {
            DepictSpec::Group { color, scale, .. }
            | DepictSpec::ReactionScheme { color, scale, .. } => {
                o.merge_common(&CommonOpts {
                    color: color.clone(),
                    scale: *scale,
                });
            }
            DepictSpec::Mol { .. } => {}
        }
        if let Some(ref bag) = node.opts {
            bag.apply_to_mol(&mut o);
        }
        o.merge_from(&node.local_opts());
        o
    }
}
