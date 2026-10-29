//! Mol chrome wire types (ABI A) and internal Annotation IR (B).
//!
//! Wire fields live on [`super::MolNode`]. Paint consumes
//! [`annotations_from_mol`]. A↔B round-trips are Rust-only (not codegen).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[cfg(feature = "codegen")]
use schemars::JsonSchema;
#[cfg(feature = "codegen")]
use ts_rs::TS;

use super::spec::{LabelPos, ShadeSpec};

// ---------------------------------------------------------------------------
// Shared ink / text (wire + IR)
// ---------------------------------------------------------------------------

/// Caption / label content: bare string or styled object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum MolText {
    /// Plain string (default placement depends on consumer: mol name → below).
    Plain(String),
    /// Explicit content + optional style / side.
    Styled(MolTextSpec),
}

impl MolText {
    pub fn content(&self) -> &str {
        match self {
            MolText::Plain(s) => s.as_str(),
            MolText::Styled(t) => t.content.as_str(),
        }
    }

    pub fn as_styled(&self) -> MolTextSpec {
        match self {
            MolText::Plain(s) => MolTextSpec {
                content: s.clone(),
                ..MolTextSpec::default()
            },
            MolText::Styled(t) => t.clone(),
        }
    }
}

/// Expanded text content for mol name, region labels, callouts, atom labels.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct MolTextSpec {
    /// Display string — chem label markup (`$R_1$`, `N^+`, `**bold**`, …).
    #[serde(default)]
    pub content: String,
    /// Side relative to the anchor (mol name / outside region label).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub pos: Option<LabelPos>,
    /// Font scale relative to house atom-label size (mol `name` defaults to 1.5×).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub scale: Option<f64>,
    /// CSS color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub color: Option<String>,
    /// Region label: inside the hull or outside (default outside).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub place: Option<RegionTextPlace>,
    /// Gap (px) between mol ink AABB and this caption (snug, no clash).
    /// Omit → house default (see caption paint `CAPTION_MARGIN_DEFAULT`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub margin: Option<f64>,
}

/// Where a region caption sits relative to the region geometry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum RegionTextPlace {
    #[default]
    Outside,
    Inside,
}

/// Fill ink for region / circle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct FillSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub opacity: Option<f64>,
}

/// Stroke ink for region / circle.
///
/// Omit / `null` with a fill → no stroke. Omit / `null` without fill → house
/// outline. Set `color: "none"` (or `width: 0`) to force no stroke; set a
/// [`StrokeSpec`] object to force an outline even when filled.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct StrokeSpec {
    /// CSS color, or `"none"` to disable stroke.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub width: Option<f64>,
    /// `solid` | `dashed` | `dotted` or a raw SVG dasharray.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub dash: Option<String>,
}

/// Region hull style (not wire `circle` highlights).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum RegionStyle {
    #[default]
    Box,
    /// Circular region hull (distinct from highlight [`CircleSpec`]).
    Circle,
    Bezier,
}

/// One region highlight on a mol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct RegionSpec {
    #[serde(default)]
    pub style: RegionStyle,
    /// Atom indices that must lie inside the region.
    #[serde(default)]
    pub atoms: Vec<u32>,
    /// Atom indices that must stay outside.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub fill: Option<FillSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub stroke: Option<StrokeSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub text: Option<MolText>,
    /// Optional z-order override (lower = further back). Default band: region base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub z: Option<i32>,
}

/// One-or-many regions on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum RegionField {
    One(RegionSpec),
    Many(Vec<RegionSpec>),
}

impl RegionField {
    pub fn iter(&self) -> impl Iterator<Item = &RegionSpec> {
        match self {
            RegionField::One(r) => std::slice::from_ref(r).iter(),
            RegionField::Many(v) => v.iter(),
        }
    }

    pub fn into_vec(self) -> Vec<RegionSpec> {
        match self {
            RegionField::One(r) => vec![r],
            RegionField::Many(v) => v,
        }
    }
}

/// Atom, bond, or ring highlight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct CircleSpec {
    /// One circle centered on each atom index.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub atoms: Vec<u32>,
    /// Bond endpoint pairs — one circle at each bond midpoint (`[begin, end]`, order-insensitive).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bonds: Vec<(u32, u32)>,
    /// Ordered ring atoms — one circle at the ring centroid (aromatic-style).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub ring: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub fill: Option<FillSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub stroke: Option<StrokeSpec>,
    /// Radius scale vs house default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub z: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum CircleField {
    One(CircleSpec),
    Many(Vec<CircleSpec>),
}

impl CircleField {
    pub fn into_vec(self) -> Vec<CircleSpec> {
        match self {
            CircleField::One(c) => vec![c],
            CircleField::Many(v) => v,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &CircleSpec> {
        match self {
            CircleField::One(c) => std::slice::from_ref(c).iter(),
            CircleField::Many(v) => v.iter(),
        }
    }
}

/// How to generate atom index / map-number labels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum AtomNumberBy {
    #[default]
    Index,
    Mapno,
}

/// Atom numbering policy (singular wire field `atom_number`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum AtomNumberSpec {
    /// Shorthand: `"index"` or `"mapno"`.
    ByName(AtomNumberBy),
    /// Full policy object.
    Policy(AtomNumberPolicy),
}

/// Expanded atom-number policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct AtomNumberPolicy {
    #[serde(default)]
    pub by: AtomNumberBy,
    /// Restrict to these atom indices (omit = all).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub only: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub z: Option<i32>,
}

impl AtomNumberSpec {
    pub fn policy(&self) -> AtomNumberPolicy {
        match self {
            AtomNumberSpec::ByName(by) => AtomNumberPolicy {
                by: *by,
                only: None,
                scale: None,
                color: None,
                z: None,
            },
            AtomNumberSpec::Policy(p) => p.clone(),
        }
    }
}

/// Per-atom arbitrary text. Bare map `{ "0": "α" }` or `{ "labels": {…}, "scale"? }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum AtomLabelSpec {
    /// Keys are atom index strings (`"0"`, `"3"`).
    Map(BTreeMap<String, MolText>),
    /// Map plus shared style.
    Detailed(AtomLabelDetailed),
}

/// Expanded atom-label bag with optional shared scale/color/z.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct AtomLabelDetailed {
    #[serde(default)]
    pub labels: BTreeMap<String, MolText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub z: Option<i32>,
}

impl AtomLabelSpec {
    pub fn entries(
        &self,
    ) -> (
        &BTreeMap<String, MolText>,
        Option<f64>,
        Option<String>,
        Option<i32>,
    ) {
        match self {
            AtomLabelSpec::Map(m) => (m, None, None, None),
            AtomLabelSpec::Detailed(d) => (&d.labels, d.scale, d.color.clone(), d.z),
        }
    }
}

/// Callout annotation (arrow / freer placement).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct CalloutSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub atoms: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub bonds: Option<Vec<(u32, u32)>>,
    pub text: MolText,
    #[serde(default = "default_true")]
    pub arrow: bool,
    /// Side bias. Omit or `"auto"` → seed farthest from other ink (not a
    /// hardcoded above), solve with no PreferSide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub prefer: Option<LabelPos>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub z: Option<i32>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum CalloutField {
    One(CalloutSpec),
    Many(Vec<CalloutSpec>),
}

impl CalloutField {
    pub fn into_vec(self) -> Vec<CalloutSpec> {
        match self {
            CalloutField::One(c) => vec![c],
            CalloutField::Many(v) => v,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &CalloutSpec> {
        match self {
            CalloutField::One(c) => std::slice::from_ref(c).iter(),
            CalloutField::Many(v) => v.iter(),
        }
    }
}

// ---------------------------------------------------------------------------
// Default z bands (back → front)
//
// **Atoms/bonds sit at 0.** Behind = negative, in front = positive.
// Gaps between bands leave room for user `z` overrides without colliding.
// ---------------------------------------------------------------------------

/// Scene / page background (behind regions).
pub const Z_BACKGROUND: i32 = -60;
/// Region fill/stroke (base underlay under shade).
pub const Z_REGION: i32 = -40;
/// Colormap wash.
pub const Z_SHADE: i32 = -20;
/// Bond / label halos.
pub const Z_HALO: i32 = -10;
/// Backbone bonds and atom element ink (origin for relative placement).
pub const Z_BONDS: i32 = 0;
/// Alias of [`Z_BONDS`] — atom centers share the skeleton band.
pub const Z_ATOMS: i32 = 0;
/// Highlight / aromatic circles (just in front of skeleton).
pub const Z_CIRCLE: i32 = 10;
/// Chem glyphs, `atom_number`, `atom_label`.
pub const Z_ATOM_LABEL: i32 = 20;
/// Region / circle text — front label layer (never under shade).
pub const Z_REGION_LABEL: i32 = Z_ATOM_LABEL;
/// Callouts and other free annotations.
pub const Z_CALLOUT: i32 = 40;
/// Mol `name` / figure labels on top.
pub const Z_NAME: i32 = 60;

// ---------------------------------------------------------------------------
// Internal Annotation IR (B) — not codegen-exported
// ---------------------------------------------------------------------------

/// Label role on the atom tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelRole {
    Number,
    Text,
}

/// Internal overlay unit for paint / occupancy.
#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    Shade {
        spec: ShadeSpec,
        z: i32,
    },
    Region {
        spec: RegionSpec,
        z: i32,
    },
    Circle {
        spec: CircleSpec,
        z: i32,
        /// True when synthesized from aromatic circle paint opt.
        from_aromatic: bool,
    },
    /// Per-atom number or text label.
    AtomLabel {
        atom: u32,
        text: MolText,
        role: LabelRole,
        scale: Option<f64>,
        color: Option<String>,
        z: i32,
    },
    Callout {
        spec: CalloutSpec,
        z: i32,
    },
}

impl Annotation {
    pub fn z(&self) -> i32 {
        match self {
            Annotation::Shade { z, .. }
            | Annotation::Region { z, .. }
            | Annotation::Circle { z, .. }
            | Annotation::AtomLabel { z, .. }
            | Annotation::Callout { z, .. } => *z,
        }
    }
}

/// Overlay fields extracted from / applied to a mol (excludes privileged `name`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MolOverlayFields {
    pub shade: Option<ShadeSpec>,
    pub atom_number: Option<AtomNumberSpec>,
    pub atom_label: Option<AtomLabelSpec>,
    pub region: Option<RegionField>,
    pub circle: Option<CircleField>,
    pub callout: Option<CalloutField>,
}

/// Read overlay chrome from a mol node (wire A → bag).
pub fn overlay_from_mol(
    shade: &Option<ShadeSpec>,
    atom_number: &Option<AtomNumberSpec>,
    atom_label: &Option<AtomLabelSpec>,
    region: &Option<RegionField>,
    circle: &Option<CircleField>,
    callout: &Option<CalloutField>,
) -> MolOverlayFields {
    MolOverlayFields {
        shade: shade.clone(),
        atom_number: atom_number.clone(),
        atom_label: atom_label.clone(),
        region: region.clone(),
        circle: circle.clone(),
        callout: callout.clone(),
    }
}

/// A→B: expand wire overlay fields into a sorted annotation list.
///
/// Privileged mol `name` is **not** included (caption path stays separate).
/// `atom_number` expands to one [`Annotation::AtomLabel`] per selected atom
/// with placeholder text; callers that know mapnos resolve text at paint time.
pub fn annotations_from_overlay(overlay: &MolOverlayFields) -> Vec<Annotation> {
    let mut out = Vec::new();

    if let Some(ref shade) = overlay.shade {
        out.push(Annotation::Shade {
            spec: shade.clone(),
            z: Z_SHADE,
        });
    }

    if let Some(ref regions) = overlay.region {
        for r in regions.iter() {
            let z = r.z.unwrap_or(Z_REGION);
            out.push(Annotation::Region { spec: r.clone(), z });
        }
    }

    if let Some(ref circles) = overlay.circle {
        for c in circles.iter() {
            let z = c.z.unwrap_or(Z_CIRCLE);
            out.push(Annotation::Circle {
                spec: c.clone(),
                z,
                from_aromatic: false,
            });
        }
    }

    if let Some(ref num) = overlay.atom_number {
        let policy = num.policy();
        let z = policy.z.unwrap_or(Z_ATOM_LABEL);
        let atoms: Vec<u32> = policy.only.clone().unwrap_or_else(|| {
            // Without a molecule, emit a sentinel empty — paint fills from layout.
            Vec::new()
        });
        if atoms.is_empty() {
            // Keep policy as a single marker annotation using atom 0 placeholder
            // only when `only` is set; otherwise store via AtomLabel with atom=u32::MAX
            // is messy. Instead push one Annotation per explicit `only`, or a
            // NumberPolicy carrier — use AtomLabel with text encoding by:.
            out.push(Annotation::AtomLabel {
                atom: u32::MAX,
                text: MolText::Plain(match policy.by {
                    AtomNumberBy::Index => "__number_index__".into(),
                    AtomNumberBy::Mapno => "__number_mapno__".into(),
                }),
                role: LabelRole::Number,
                scale: policy.scale,
                color: policy.color.clone(),
                z,
            });
        } else {
            for a in atoms {
                let text = match policy.by {
                    AtomNumberBy::Index => MolText::Plain(a.to_string()),
                    AtomNumberBy::Mapno => MolText::Plain(format!("m{a}")),
                };
                out.push(Annotation::AtomLabel {
                    atom: a,
                    text,
                    role: LabelRole::Number,
                    scale: policy.scale,
                    color: policy.color.clone(),
                    z,
                });
            }
        }
    }

    if let Some(ref labels) = overlay.atom_label {
        let (map, scale, color, z_opt) = labels.entries();
        let z = z_opt.unwrap_or(Z_ATOM_LABEL);
        for (k, text) in map {
            let Ok(atom) = k.parse::<u32>() else {
                continue;
            };
            out.push(Annotation::AtomLabel {
                atom,
                text: text.clone(),
                role: LabelRole::Text,
                scale,
                color: color.clone(),
                z,
            });
        }
    }

    if let Some(ref callouts) = overlay.callout {
        for c in callouts.iter() {
            let z = c.z.unwrap_or(Z_CALLOUT);
            out.push(Annotation::Callout { spec: c.clone(), z });
        }
    }

    out.sort_by_key(|a| a.z());
    out
}

/// B→A: rebuild wire overlay fields from annotations (canonical form).
pub fn overlay_from_annotations(anns: &[Annotation]) -> MolOverlayFields {
    let mut shade: Option<ShadeSpec> = None;
    let mut regions: Vec<RegionSpec> = Vec::new();
    let mut circles: Vec<CircleSpec> = Vec::new();
    let mut number_policy: Option<AtomNumberPolicy> = None;
    let mut number_atoms: Vec<(u32, MolText)> = Vec::new();
    let mut text_labels: BTreeMap<String, MolText> = BTreeMap::new();
    let mut text_scale: Option<f64> = None;
    let mut text_color: Option<String> = None;
    let mut text_z: Option<i32> = None;
    let mut callouts: Vec<CalloutSpec> = Vec::new();

    for a in anns {
        match a {
            Annotation::Shade { spec, .. } => {
                shade = Some(spec.clone());
            }
            Annotation::Region { spec, .. } => {
                regions.push(spec.clone());
            }
            Annotation::Circle {
                spec,
                from_aromatic,
                ..
            } => {
                if !from_aromatic {
                    circles.push(spec.clone());
                }
            }
            Annotation::AtomLabel {
                atom,
                text,
                role,
                scale,
                color,
                z,
            } => match role {
                LabelRole::Number => {
                    if *atom == u32::MAX {
                        let by = match text.content() {
                            "__number_mapno__" => AtomNumberBy::Mapno,
                            _ => AtomNumberBy::Index,
                        };
                        number_policy = Some(AtomNumberPolicy {
                            by,
                            only: None,
                            scale: *scale,
                            color: color.clone(),
                            z: Some(*z),
                        });
                    } else {
                        number_atoms.push((*atom, text.clone()));
                        number_policy.get_or_insert(AtomNumberPolicy {
                            by: AtomNumberBy::Index,
                            only: None,
                            scale: *scale,
                            color: color.clone(),
                            z: Some(*z),
                        });
                    }
                }
                LabelRole::Text => {
                    text_labels.insert(atom.to_string(), text.clone());
                    if text_scale.is_none() {
                        text_scale = *scale;
                    }
                    if text_color.is_none() {
                        text_color = color.clone();
                    }
                    if text_z.is_none() {
                        text_z = Some(*z);
                    }
                }
            },
            Annotation::Callout { spec, .. } => {
                callouts.push(spec.clone());
            }
        }
    }

    let atom_number = if let Some(mut p) = number_policy {
        if !number_atoms.is_empty() {
            let mut only: Vec<u32> = number_atoms.iter().map(|(a, _)| *a).collect();
            only.sort_unstable();
            only.dedup();
            p.only = Some(only);
        }
        // Canonical shorthand when possible.
        let spec = if p.only.is_none() && p.scale.is_none() && p.color.is_none() && p.z.is_none() {
            AtomNumberSpec::ByName(p.by)
        } else if p.scale.is_none()
            && p.color.is_none()
            && (p.z.is_none() || p.z == Some(Z_ATOM_LABEL))
            && p.only.is_none()
        {
            AtomNumberSpec::ByName(p.by)
        } else {
            // Drop default z for cleaner wire.
            if p.z == Some(Z_ATOM_LABEL) {
                p.z = None;
            }
            AtomNumberSpec::Policy(p)
        };
        Some(spec)
    } else {
        None
    };

    let atom_label = if text_labels.is_empty() {
        None
    } else if text_scale.is_none()
        && text_color.is_none()
        && text_z.filter(|&z| z != Z_ATOM_LABEL).is_none()
    {
        Some(AtomLabelSpec::Map(text_labels))
    } else {
        Some(AtomLabelSpec::Detailed(AtomLabelDetailed {
            labels: text_labels,
            scale: text_scale,
            color: text_color,
            z: text_z.filter(|&z| z != Z_ATOM_LABEL),
        }))
    };

    let region = match regions.len() {
        0 => None,
        1 => Some(RegionField::One(regions.pop().unwrap())),
        _ => Some(RegionField::Many(regions)),
    };
    let circle = match circles.len() {
        0 => None,
        1 => Some(CircleField::One(circles.pop().unwrap())),
        _ => Some(CircleField::Many(circles)),
    };
    let callout = match callouts.len() {
        0 => None,
        1 => Some(CalloutField::One(callouts.pop().unwrap())),
        _ => Some(CalloutField::Many(callouts)),
    };

    MolOverlayFields {
        shade,
        atom_number,
        atom_label,
        region,
        circle,
        callout,
    }
}

/// Canonicalize overlay by A→B→A (stable for round-trip tests).
pub fn canonicalize_overlay(overlay: &MolOverlayFields) -> MolOverlayFields {
    overlay_from_annotations(&annotations_from_overlay(overlay))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mol_text_plain_and_styled() {
        let plain = MolText::Plain("ethanol".into());
        assert_eq!(plain.content(), "ethanol");
        let styled = MolText::Styled(MolTextSpec {
            content: "pocket".into(),
            place: Some(RegionTextPlace::Inside),
            ..Default::default()
        });
        assert_eq!(styled.as_styled().place, Some(RegionTextPlace::Inside));
    }

    #[test]
    fn round_trip_shade_and_region() {
        let overlay = MolOverlayFields {
            shade: Some(ShadeSpec {
                atoms: Some(vec![0.1, 0.5]),
                ..Default::default()
            }),
            region: Some(RegionField::One(RegionSpec {
                style: RegionStyle::Box,
                atoms: vec![0, 1, 2],
                exclude: vec![3],
                fill: Some(FillSpec {
                    color: Some("#c44".into()),
                    opacity: Some(0.15),
                }),
                stroke: None,
                text: Some(MolText::Plain("pocket".into())),
                z: None,
            })),
            ..Default::default()
        };
        let back = canonicalize_overlay(&overlay);
        assert_eq!(
            back.shade.as_ref().unwrap().atoms,
            overlay.shade.as_ref().unwrap().atoms
        );
        let r = match back.region.unwrap() {
            RegionField::One(r) => r,
            RegionField::Many(_) => panic!("expected one region"),
        };
        assert_eq!(r.atoms, vec![0, 1, 2]);
        assert_eq!(r.exclude, vec![3]);
    }

    #[test]
    fn round_trip_atom_label_map() {
        let mut labels = BTreeMap::new();
        labels.insert("1".into(), MolText::Plain("α".into()));
        labels.insert("2".into(), MolText::Plain("N-Me".into()));
        let overlay = MolOverlayFields {
            atom_label: Some(AtomLabelSpec::Detailed(AtomLabelDetailed {
                labels,
                scale: Some(0.9),
                color: None,
                z: None,
            })),
            ..Default::default()
        };
        let back = canonicalize_overlay(&overlay);
        let atom_label = back.atom_label.unwrap();
        let (map, scale, _, _) = atom_label.entries();
        assert_eq!(map.get("1").unwrap().content(), "α");
        assert_eq!(map.get("2").unwrap().content(), "N-Me");
        assert_eq!(scale, Some(0.9));
    }

    #[test]
    fn round_trip_atom_number_shorthand() {
        let overlay = MolOverlayFields {
            atom_number: Some(AtomNumberSpec::ByName(AtomNumberBy::Index)),
            ..Default::default()
        };
        let anns = annotations_from_overlay(&overlay);
        assert!(matches!(
            &anns[0],
            Annotation::AtomLabel {
                role: LabelRole::Number,
                atom: u32::MAX,
                ..
            }
        ));
        let back = overlay_from_annotations(&anns);
        assert!(matches!(
            back.atom_number,
            Some(AtomNumberSpec::ByName(AtomNumberBy::Index))
        ));
    }

    #[test]
    fn round_trip_circle_and_callout() {
        let overlay = MolOverlayFields {
            circle: Some(CircleField::One(CircleSpec {
                atoms: vec![1, 2],
                bonds: vec![(0, 1)],
                ring: None,
                fill: None,
                stroke: Some(StrokeSpec {
                    color: Some("#06c".into()),
                    width: Some(1.5),
                    dash: None,
                }),
                scale: None,
                z: None,
            })),
            callout: Some(CalloutField::One(CalloutSpec {
                atoms: Some(vec![2]),
                bonds: None,
                text: MolText::Plain("note".into()),
                arrow: true,
                prefer: Some(LabelPos::Left),
                color: None,
                scale: None,
                z: None,
            })),
            ..Default::default()
        };
        let back = canonicalize_overlay(&overlay);
        assert!(back.circle.is_some());
        let circ = match back.circle.as_ref().unwrap() {
            CircleField::One(c) => c,
            CircleField::Many(v) => &v[0],
        };
        assert_eq!(circ.bonds, vec![(0, 1)]);
        assert!(back.callout.is_some());
    }

    #[test]
    fn omitted_callout_prefer_is_auto() {
        let c: CalloutSpec =
            serde_json::from_str(r#"{"atoms":[6],"text":"OH","arrow":true}"#).unwrap();
        assert_eq!(c.prefer, None);
        let explicit: CalloutSpec =
            serde_json::from_str(r#"{"atoms":[6],"text":"OH","arrow":true,"prefer":"auto"}"#)
                .unwrap();
        assert_eq!(explicit.prefer, Some(crate::doc::LabelPos::Auto));
    }

    #[test]
    fn annotations_sort_by_z() {
        let overlay = MolOverlayFields {
            shade: Some(ShadeSpec::default()),
            region: Some(RegionField::One(RegionSpec {
                style: RegionStyle::Box,
                atoms: vec![0],
                exclude: vec![],
                fill: None,
                stroke: None,
                text: None,
                z: None,
            })),
            callout: Some(CalloutField::One(CalloutSpec {
                atoms: Some(vec![0]),
                bonds: None,
                text: MolText::Plain("x".into()),
                arrow: false,
                prefer: None,
                color: None,
                scale: None,
                z: None,
            })),
            ..Default::default()
        };
        let zs: Vec<_> = annotations_from_overlay(&overlay)
            .iter()
            .map(Annotation::z)
            .collect();
        assert!(zs.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(zs.first(), Some(&Z_REGION));
        assert_eq!(zs.last(), Some(&Z_CALLOUT));
        assert!(Z_BACKGROUND < Z_REGION);
        assert!(Z_REGION < Z_SHADE);
        assert!(Z_REGION_LABEL > Z_SHADE);
        assert_eq!(Z_REGION_LABEL, Z_ATOM_LABEL);
        assert!(Z_SHADE < Z_HALO);
        assert!(Z_HALO < Z_BONDS);
        assert_eq!(Z_BONDS, Z_ATOMS);
        assert_eq!(Z_BONDS, 0);
        assert!(Z_CIRCLE > Z_BONDS);
        assert!(Z_ATOM_LABEL > Z_CIRCLE);
        assert!(Z_CALLOUT > Z_ATOM_LABEL);
        assert!(Z_NAME > Z_CALLOUT);
    }

    #[test]
    fn json_wire_shapes_round_trip_serde() {
        let name: MolText = serde_json::from_str(r#""ethanol""#).unwrap();
        assert_eq!(name.content(), "ethanol");
        let region: RegionField = serde_json::from_str(
            r#"{"style":"bezier","atoms":[0,1],"exclude":[2],"text":{"content":"A","place":"inside"}}"#,
        )
        .unwrap();
        match region {
            RegionField::One(r) => {
                assert_eq!(r.style, RegionStyle::Bezier);
                assert_eq!(
                    r.text.unwrap().as_styled().place,
                    Some(RegionTextPlace::Inside)
                );
            }
            _ => panic!(),
        }
        let num: AtomNumberSpec = serde_json::from_str(r#""mapno""#).unwrap();
        assert!(matches!(num, AtomNumberSpec::ByName(AtomNumberBy::Mapno)));
    }
}
