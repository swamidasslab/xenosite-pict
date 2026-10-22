//! Drawable scene document — engine-neutral primitives shared by Python & JS.
//!
//! One molecule → one [`Viewport`] inside a [`Scene`]. Circle overlays and shade
//! disks are layers on that viewport. Multi-mol / ELK schemes compose several
//! viewports into one scene.
//!
//! 1. [`crate::doc::render`] supplies coords + chem metadata (CoordGen + align).
//! 2. Rust builds a [`Scene`] of typed primitives (paths, circles, …).
//! 3. [`crate::scene_to_svg`] emits SVG; hosts only wrap (IPython / `<img>`).

use serde::{Deserialize, Serialize};

#[cfg(feature = "codegen")]
use schemars::JsonSchema;
#[cfg(feature = "codegen")]
use ts_rs::TS;

/// Horizontal text anchor (SVG `text-anchor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum TextAnchor {
    Start,
    Middle,
    End,
}

/// Layer name inside a molecule viewport.
///
/// Paint order is the nesting order inside `g.xpict-mol` (bottom → top).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum LayerName {
    Shading,
    Halo,
    Bonds,
    Labels,
    Marks,
    Overlay,
}

impl LayerName {
    /// Bottom → top inside one mol group (shading under halo under chem).
    pub fn paint_order(self) -> u8 {
        match self {
            LayerName::Shading => 0,
            LayerName::Halo => 1,
            LayerName::Bonds => 2,
            LayerName::Labels => 3,
            LayerName::Marks => 4,
            LayerName::Overlay => 5,
        }
    }
}

/// How fully aromatic rings are drawn.
///
/// ``circle`` (default): skeleton singles + stroked ring circle for CTAB ``4`` /
/// RDKit ``1.5`` rings. ``kekule``: alternating double/single strokes for paint
/// only (stored bond orders on the molecule are unchanged).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum AromaticStyle {
    #[default]
    Circle,
    Kekule,
}

/// One drawable primitive in SVG space (+Y down).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum Primitive {
    Path {
        d: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        stroke: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        fill: Option<String>,
        #[serde(default = "default_stroke_width")]
        stroke_width: f64,
        #[serde(default = "default_opacity")]
        opacity: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        stroke_dasharray: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        stroke_linecap: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "cls")]
        #[cfg_attr(feature = "codegen", ts(optional, rename = "cls"))]
        class: Option<String>,
        /// Plain label string for ``data-text`` (glyph paths only).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        data_text: Option<String>,
    },
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        fill: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        stroke: Option<String>,
        #[serde(default = "default_stroke_width")]
        stroke_width: f64,
        #[serde(default = "default_opacity")]
        opacity: f64,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "cls")]
        #[cfg_attr(feature = "codegen", ts(optional, rename = "cls"))]
        class: Option<String>,
    },
    /// Logical text; serializers outline to glyph paths (Liberation) or emit
    /// `<text>` when the host prefers.
    Text {
        x: f64,
        y: f64,
        text: String,
        #[serde(default = "default_fill")]
        fill: String,
        #[serde(default = "default_font_size")]
        font_size: f64,
        #[serde(default = "default_anchor")]
        anchor: TextAnchor,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "cls")]
        #[cfg_attr(feature = "codegen", ts(optional, rename = "cls"))]
        class: Option<String>,
        /// Degrees about the glyph center `(x, y)`. Omit when upright.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "codegen", ts(optional))]
        rotate: Option<f64>,
    },
}

fn default_stroke_width() -> f64 {
    1.5
}
fn default_opacity() -> f64 {
    1.0
}
fn default_fill() -> String {
    "#000".into()
}
fn default_font_size() -> f64 {
    12.0
}
fn default_anchor() -> TextAnchor {
    TextAnchor::Middle
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct Layer {
    pub name: LayerName,
    #[serde(default)]
    pub primitives: Vec<Primitive>,
}

fn skip_zero(v: &f64) -> bool {
    v.abs() < 1e-12
}

/// One molecule viewport in a diagram (after ELK/grid placement).
///
/// `x`/`y` are the mol group's placement in the parent. `ox`/`oy` are the
/// leftover fit origin of local primitive coords. SVG is
/// `translate(x,y) translate(ox,oy)` around nested layer groups — primitives
/// stay in paint space; the group carries the transform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct Viewport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub id: Option<String>,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    /// Local leftover origin (fit). Nested inside the mol group, not baked
    /// into path `d`.
    #[serde(default, skip_serializing_if = "skip_zero")]
    pub ox: f64,
    #[serde(default, skip_serializing_if = "skip_zero")]
    pub oy: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub layers: Vec<Layer>,
}

impl Viewport {
    /// Document-space origin of this mol's local primitive coords.
    pub fn local_origin(&self) -> (f64, f64) {
        (self.x + self.ox, self.y + self.oy)
    }

    /// Append primitives to the named layer, creating it if needed.
    pub fn extend_layer(&mut self, name: LayerName, mut prims: Vec<Primitive>) {
        if prims.is_empty() {
            return;
        }
        if let Some(layer) = self.layers.iter_mut().find(|l| l.name == name) {
            layer.primitives.append(&mut prims);
            return;
        }
        self.layers.push(Layer {
            name,
            primitives: prims,
        });
    }

    /// Put primitives under existing ink of the named layer (regions under shade).
    pub fn prepend_layer(&mut self, name: LayerName, mut prims: Vec<Primitive>) {
        if prims.is_empty() {
            return;
        }
        if let Some(layer) = self.layers.iter_mut().find(|l| l.name == name) {
            prims.append(&mut layer.primitives);
            layer.primitives = prims;
            return;
        }
        self.layers.insert(
            0,
            Layer {
                name,
                primitives: prims,
            },
        );
    }
}

/// Full drawable document before SVG/HTML serialization.
///
/// Python `contracts.scene.Scene` and JS consumers should mirror this shape
/// (JSON round-trip is the ABI).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct Scene {
    pub width: f64,
    pub height: f64,
    /// viewBox min-x. Size is [`Self::width`] × [`Self::height`]. Default 0.
    #[serde(default, skip_serializing_if = "skip_zero")]
    pub x: f64,
    /// viewBox min-y. Default 0.
    #[serde(default, skip_serializing_if = "skip_zero")]
    pub y: f64,
    pub viewports: Vec<Viewport>,
    /// Document-space primitives (reaction arrows) above viewports.
    #[serde(default)]
    pub overlays: Vec<Primitive>,
    /// Document-space knockout (scheme compose). Per-mol halo lives in the
    /// viewport `Halo` layer so it nests inside `g.xpict-mol`.
    #[serde(default)]
    pub halo: Vec<Primitive>,
}

/// Atom input for a molecule depict call (SVG-space coords from the caller).
///
/// Identity: pass ``element`` and/or atomic number ``z``. If only ``z`` is
/// set, the symbol comes from [`crate::element_symbol`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct AtomIn {
    pub index: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub element: Option<String>,
    /// Atomic number (`0` = ``*``). Used when ``element`` is omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub z: Option<u32>,
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub label: Option<String>,
    #[serde(default)]
    pub charge: i32,
    /// Reaction / SMILES atom-map number (`:n` in brackets), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub mapno: Option<u32>,
}

impl AtomIn {
    /// Resolved element symbol (`element` if set, else from ``z``, else ``"C"``).
    pub fn symbol(&self) -> &str {
        if let Some(ref el) = self.element {
            if !el.is_empty() {
                return el.as_str();
            }
        }
        match self.z {
            Some(z) => crate::element_symbol(z),
            None => "C",
        }
    }
}

/// How a ring-center stem attaches to its [`SpokeIn::endpts`].
///
/// CXSMILES ``m:`` is variable attachment ([`Attach::Any`]). Haptic / ALL is
/// reserved for a later ``C:`` / MOL ENDPTS path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum Attach {
    /// One of the endpoints (Markush / variable attachment).
    #[default]
    Any,
    /// All endpoints at once (haptic multicenter).
    All,
}

fn is_attach_any(a: &Attach) -> bool {
    *a == Attach::Any
}

/// Segment drawn from a ring centroid to a substituent (CX ``m:``).
///
/// The SMILES dummy ``*`` is **not** a neighbor in the spoke graph (it is a
/// silent paint handle at the centroid). [`Self::atom`] is the first real atom
/// of the substituent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct SpokeIn {
    /// Silent centroid atom (original CX dummy index), or omitted when dropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub dummy: Option<i32>,
    /// First real atom of the substituent (stem end).
    pub atom: i32,
    /// Ring atoms the stem represents (CX ``m:dummy:a.b.c``).
    pub endpts: Vec<i32>,
    #[serde(default, skip_serializing_if = "is_attach_any")]
    pub attach: Attach,
}

/// Bond input (indices into [`AtomIn::index`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct BondIn {
    pub index: i32,
    pub begin: i32,
    pub end: i32,
    pub order: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub stereo: Option<String>,
    /// Unit normal toward ring interior (ring doubles); omit for centered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub interior: Option<(f64, f64)>,
}

/// One molecule ready to paint (coords already in SVG / ``SCALE`` space).
///
/// Publication circles live on the document ``MolNode.circle`` overlay, not here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct MoleculeIn {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub id: Option<String>,
    pub atoms: Vec<AtomIn>,
    pub bonds: Vec<BondIn>,
    /// Ink color for backbone bonds and atom labels (CSS). Default ``#111``.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub color: Option<String>,
    /// Per-atom shade scores (same order as [`Self::atoms`]); omit if unshaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub atom_shade: Option<Vec<f64>>,
    /// Per-bond shade scores (same order as [`Self::bonds`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub bond_shade: Option<Vec<f64>>,
    /// Shade colormap window low (default ``0``). Not inferred from data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub shade_vmin: Option<f64>,
    /// Shade colormap window high (default ``1``). Not inferred from data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub shade_vmax: Option<f64>,
    /// Uniform diagram scale (font, stroke, pad, geometry). ``1.0`` = house size.
    #[serde(
        default = "default_mol_scale",
        skip_serializing_if = "is_default_mol_scale"
    )]
    pub scale: f64,
    /// Ink weight for backbone stroke and label glyph thicken. ``1.0`` = house;
    /// may go down to ~``2/3`` (Regular stem); typical thicken up to ~2.
    #[serde(
        default = "default_mol_weight",
        skip_serializing_if = "is_default_mol_weight"
    )]
    pub weight: f64,
    /// Aromatic ring paint: circle (default) or Kekulé doubles.
    #[serde(default, skip_serializing_if = "is_default_aromatic")]
    pub aromatic: AromaticStyle,
    /// When true, paint dashed debug rectangles for label / mol / scene boxes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub debug_box: bool,
    /// When true, paint alignment correspondence overlays (needs [`Self::align_debug`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub debug_align: bool,
    /// Stamped match tags from the host/core align step (not cascading).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub align_debug: Option<AlignDebug>,
    /// Ring-center stems (CX ``m:``). Empty when the mol has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spokes: Vec<SpokeIn>,
}

/// Role of a molecule in an alignment debug overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum AlignDebugRole {
    Template,
    Query,
}

/// Stamped alignment correspondence for [`MoleculeIn::debug_align`] paint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct AlignDebug {
    pub role: AlignDebugRole,
    /// Peer mol id(s): template lists queries; query names its template.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub peer_id: Option<String>,
    /// `(atom_index_on_this_mol, tag)` — tag is the **template atom index**.
    #[serde(default)]
    pub tags: Vec<(i32, i32)>,
}

fn default_mol_scale() -> f64 {
    1.0
}

fn is_default_mol_scale(s: &f64) -> bool {
    (*s - 1.0).abs() < 1e-12
}

fn default_mol_weight() -> f64 {
    1.0
}

fn is_default_mol_weight(s: &f64) -> bool {
    (*s - 1.0).abs() < 1e-12
}

fn is_default_aromatic(s: &AromaticStyle) -> bool {
    *s == AromaticStyle::Circle
}

impl Default for MoleculeIn {
    fn default() -> Self {
        Self {
            id: None,
            atoms: Vec::new(),
            bonds: Vec::new(),
            color: None,
            atom_shade: None,
            bond_shade: None,
            shade_vmin: None,
            shade_vmax: None,
            scale: 1.0,
            weight: 1.0,
            aromatic: AromaticStyle::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
            spokes: Vec::new(),
        }
    }
}

impl MoleculeIn {
    /// Effective diagram scale; non-positive values fall back to ``1.0``.
    pub fn diagram_scale(&self) -> f64 {
        if self.scale > 0.0 && self.scale.is_finite() {
            self.scale
        } else {
            1.0
        }
    }

    /// Absolute ink multiplier for this mol (user ``weight`` × house base).
    ///
    /// # Panics
    /// Panics if [`Self::weight`] is invalid (see [`crate::metrics::diagram_weight`]).
    pub fn diagram_weight(&self) -> f64 {
        crate::metrics::diagram_weight(self.weight)
    }

    /// Atom with [`AtomIn::index`] ``idx``.
    pub fn atom(&self, idx: i32) -> Option<&AtomIn> {
        self.atoms
            .iter()
            .find(|a| a.index == idx)
            .or_else(|| self.atoms.get(idx as usize))
    }

    /// Mean of ``endpts`` coordinates (ring-center for a spoke).
    pub fn centroid_of(&self, endpts: &[i32]) -> Option<(f64, f64)> {
        if endpts.is_empty() {
            return None;
        }
        let mut sx = 0.0;
        let mut sy = 0.0;
        let mut n = 0usize;
        for &i in endpts {
            let Some(a) = self.atom(i) else {
                continue;
            };
            sx += a.x;
            sy += a.y;
            n += 1;
        }
        (n > 0).then_some((sx / n as f64, sy / n as f64))
    }
}

/// Multiply every numeric token in an SVG path ``d`` (M/L/Z polygons).
pub fn scale_path_d(d: &str, s: f64) -> String {
    if (s - 1.0).abs() < 1e-12 || d.is_empty() {
        return d.to_string();
    }
    let bytes = d.as_bytes();
    let mut out = String::with_capacity(d.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '-' || c == '+' || c == '.' || c.is_ascii_digit() {
            let start = i;
            if c == '-' || c == '+' {
                i += 1;
            }
            while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                i += 1;
                while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                i += 1;
                if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
                    i += 1;
                }
                while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
            }
            let raw = std::str::from_utf8(&bytes[start..i]).unwrap_or("0");
            let num: f64 = raw.parse().unwrap_or(0.0);
            out.push_str(&format!("{:.2}", num * s));
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn scale_dasharray(dash: &str, s: f64) -> String {
    if (s - 1.0).abs() < 1e-12 {
        return dash.to_string();
    }
    dash.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .map(|t| {
            t.parse::<f64>()
                .map(|n| format!("{:.2}", n * s))
                .unwrap_or_else(|_| t.to_string())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl Primitive {
    /// Uniform scale about the origin (stroke widths and path coords).
    pub fn scale_uniform(&self, s: f64) -> Self {
        if (s - 1.0).abs() < 1e-12 {
            return self.clone();
        }
        match self {
            Primitive::Path {
                d,
                stroke,
                fill,
                stroke_width,
                opacity,
                stroke_dasharray,
                stroke_linecap,
                class,
                data_text,
            } => Primitive::Path {
                d: scale_path_d(d, s),
                stroke: stroke.clone(),
                fill: fill.clone(),
                stroke_width: *stroke_width * s,
                opacity: *opacity,
                stroke_dasharray: stroke_dasharray.as_ref().map(|d| scale_dasharray(d, s)),
                stroke_linecap: stroke_linecap.clone(),
                class: class.clone(),
                data_text: data_text.clone(),
            },
            Primitive::Circle {
                cx,
                cy,
                r,
                fill,
                stroke,
                stroke_width,
                opacity,
                class,
            } => Primitive::Circle {
                cx: *cx * s,
                cy: *cy * s,
                r: *r * s,
                fill: fill.clone(),
                stroke: stroke.clone(),
                stroke_width: *stroke_width * s,
                opacity: *opacity,
                class: class.clone(),
            },
            Primitive::Text {
                x,
                y,
                text,
                fill,
                font_size,
                anchor,
                class,
                rotate,
            } => Primitive::Text {
                x: *x * s,
                y: *y * s,
                text: text.clone(),
                fill: fill.clone(),
                font_size: *font_size * s,
                anchor: *anchor,
                class: class.clone(),
                rotate: *rotate,
            },
        }
    }
}

impl Viewport {
    pub fn scale_uniform(&self, s: f64) -> Self {
        if (s - 1.0).abs() < 1e-12 {
            return self.clone();
        }
        Self {
            id: self.id.clone(),
            x: self.x * s,
            y: self.y * s,
            ox: self.ox * s,
            oy: self.oy * s,
            width: self.width * s,
            height: self.height * s,
            layers: self
                .layers
                .iter()
                .map(|layer| Layer {
                    name: layer.name,
                    primitives: layer
                        .primitives
                        .iter()
                        .map(|p| p.scale_uniform(s))
                        .collect(),
                })
                .collect(),
        }
    }
}

impl Scene {
    pub fn empty(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            x: 0.0,
            y: 0.0,
            viewports: Vec::new(),
            overlays: Vec::new(),
            halo: Vec::new(),
        }
    }

    /// Scale the whole diagram about the origin (equiv. to scaled font_px / metrics).
    pub fn scale_uniform(&self, s: f64) -> Self {
        if (s - 1.0).abs() < 1e-12 {
            return self.clone();
        }
        Self {
            width: self.width * s,
            height: self.height * s,
            x: self.x * s,
            y: self.y * s,
            viewports: self
                .viewports
                .iter()
                .map(|vp| vp.scale_uniform(s))
                .collect(),
            overlays: self.overlays.iter().map(|p| p.scale_uniform(s)).collect(),
            halo: self.halo.iter().map(|p| p.scale_uniform(s)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_json_roundtrip_minimal() {
        let scene = Scene {
            width: 70.0,
            height: 75.0,
            x: 0.0,
            y: 0.0,
            viewports: vec![Viewport {
                id: Some("m0".into()),
                x: 0.0,
                y: 0.0,
                ox: 0.0,
                oy: 0.0,
                width: 70.0,
                height: 75.0,
                layers: vec![Layer {
                    name: LayerName::Bonds,
                    primitives: vec![Primitive::Path {
                        d: "M 0 0 L 20 0".into(),
                        stroke: Some("#111".into()),
                        fill: Some("none".into()),
                        stroke_width: 0.84,
                        opacity: 1.0,
                        stroke_dasharray: None,
                        stroke_linecap: Some("round".into()),
                        class: Some("bond bond-skeleton".into()),
                        data_text: None,
                    }],
                }],
            }],
            overlays: vec![],
            halo: vec![],
        };
        let json = serde_json::to_string(&scene).expect("serialize");
        let back: Scene = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.width, 70.0);
        assert_eq!(back.viewports[0].layers[0].primitives.len(), 1);
    }

    #[test]
    fn primitive_defaults_via_partial_json() {
        // serde default fns (stroke_width / opacity / fill / font_size / anchor).
        let path: Primitive = serde_json::from_str(r#"{"kind":"path","d":"M 0 0 L 1 0"}"#).unwrap();
        match path {
            Primitive::Path {
                stroke_width,
                opacity,
                ..
            } => {
                assert!((stroke_width - 1.5).abs() < 1e-12);
                assert!((opacity - 1.0).abs() < 1e-12);
            }
            _ => panic!("expected path"),
        }
        let circle: Primitive =
            serde_json::from_str(r#"{"kind":"circle","cx":0,"cy":0,"r":1}"#).unwrap();
        match circle {
            Primitive::Circle {
                stroke_width,
                opacity,
                ..
            } => {
                assert!((stroke_width - 1.5).abs() < 1e-12);
                assert!((opacity - 1.0).abs() < 1e-12);
            }
            _ => panic!("expected circle"),
        }
        let text: Primitive =
            serde_json::from_str(r#"{"kind":"text","x":0,"y":0,"text":"C"}"#).unwrap();
        match text {
            Primitive::Text {
                fill,
                font_size,
                anchor,
                ..
            } => {
                assert_eq!(fill, "#000");
                assert!((font_size - 12.0).abs() < 1e-12);
                assert_eq!(anchor, TextAnchor::Middle);
            }
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn atom_symbol_resolves_element_z_and_fallback() {
        let with_el = AtomIn {
            index: 0,
            element: Some("N".into()),
            z: Some(6),
            x: 0.0,
            y: 0.0,
            label: None,
            charge: 0,
            mapno: None,
        };
        assert_eq!(with_el.symbol(), "N");

        let empty_el = AtomIn {
            index: 1,
            element: Some(String::new()),
            z: Some(8),
            x: 0.0,
            y: 0.0,
            label: None,
            charge: 0,
            mapno: None,
        };
        assert_eq!(empty_el.symbol(), "O");

        let z_only = AtomIn {
            index: 2,
            element: None,
            z: Some(7),
            x: 0.0,
            y: 0.0,
            label: None,
            charge: 0,
            mapno: None,
        };
        assert_eq!(z_only.symbol(), "N");

        let bare = AtomIn {
            index: 3,
            element: None,
            z: None,
            x: 0.0,
            y: 0.0,
            label: None,
            charge: 0,
            mapno: None,
        };
        assert_eq!(bare.symbol(), "C");
    }

    #[test]
    fn scene_empty_has_no_viewports() {
        let s = Scene::empty(100.0, 80.0);
        assert_eq!(s.width, 100.0);
        assert_eq!(s.height, 80.0);
        assert!(s.viewports.is_empty());
        assert!(s.overlays.is_empty());
        assert!(s.halo.is_empty());
    }

    #[test]
    fn mol_scale_weight_defaults_and_fallbacks() {
        // Omitted fields → serde defaults (covers default_mol_weight / scale).
        let mol: MoleculeIn = serde_json::from_str(r#"{"id":"m","atoms":[],"bonds":[]}"#).unwrap();
        assert!((mol.scale - 1.0).abs() < 1e-12);
        assert!((mol.weight - 1.0).abs() < 1e-12);
        assert!((mol.diagram_scale() - 1.0).abs() < 1e-12);
        assert!((mol.diagram_weight() - crate::metrics::WEIGHT_AT_ONE).abs() < 1e-12);

        let mut bad = mol.clone();
        bad.scale = 0.0;
        assert!((bad.diagram_scale() - 1.0).abs() < 1e-12);
        bad.scale = f64::NAN;
        assert!((bad.diagram_scale() - 1.0).abs() < 1e-12);
        bad.scale = -2.0;
        assert!((bad.diagram_scale() - 1.0).abs() < 1e-12);
        bad.scale = 1.5;
        assert!((bad.diagram_scale() - 1.5).abs() < 1e-12);

        // skip_serializing_if: defaults omitted; non-defaults kept.
        let json_default = serde_json::to_value(&mol).unwrap();
        assert!(json_default.get("scale").is_none());
        assert!(json_default.get("weight").is_none());
        let mut thick = mol;
        thick.weight = 2.0;
        thick.scale = 1.5;
        let json_thick = serde_json::to_value(&thick).unwrap();
        assert_eq!(json_thick["weight"], 2.0);
        assert_eq!(json_thick["scale"], 1.5);
    }

    #[test]
    fn scale_path_d_and_dasharray_cover_token_shapes() {
        assert_eq!(scale_path_d("M 0 0", 1.0), "M 0 0");
        assert_eq!(scale_path_d("", 2.0), "");
        // Signed ints, decimals, scientific notation.
        let d = scale_path_d("M -1.5e1 +2.0E+1 L 3 4Z", 2.0);
        assert!(d.contains("-30.00") || d.contains("-30"), "{d}");
        assert!(d.contains("40.00") || d.contains("40"), "{d}");
        assert!(d.ends_with('Z') || d.contains('Z'), "{d}");

        assert_eq!(scale_dasharray("1 2", 1.0), "1 2");
        assert_eq!(scale_dasharray("1,2 3", 2.0), "2.00 4.00 6.00");
        // Non-numeric token preserved.
        assert_eq!(scale_dasharray("1 foo", 2.0), "2.00 foo");
    }

    #[test]
    fn scale_uniform_covers_path_circle_text_and_identity() {
        let path = Primitive::Path {
            d: "M 10 0 L 20 0".into(),
            stroke: Some("#111".into()),
            fill: Some("none".into()),
            stroke_width: 1.0,
            opacity: 1.0,
            stroke_dasharray: Some("2 1".into()),
            stroke_linecap: Some("round".into()),
            class: Some("bond".into()),
            data_text: None,
        };
        let circle = Primitive::Circle {
            cx: 5.0,
            cy: 6.0,
            r: 2.0,
            fill: None,
            stroke: Some("#f00".into()),
            stroke_width: 0.5,
            opacity: 0.7,
            class: Some("mark".into()),
        };
        let text = Primitive::Text {
            x: 1.0,
            y: 2.0,
            text: "C".into(),
            fill: "#000".into(),
            font_size: 12.0,
            anchor: TextAnchor::Middle,
            class: None,
            rotate: None,
        };

        // Identity early-outs.
        assert_eq!(path.scale_uniform(1.0), path);
        assert_eq!(circle.scale_uniform(1.0), circle);
        assert_eq!(text.scale_uniform(1.0), text);

        match path.scale_uniform(2.0) {
            Primitive::Path {
                d,
                stroke_width,
                stroke_dasharray,
                ..
            } => {
                assert!(d.contains("20.00") && d.contains("40.00"), "{d}");
                assert!((stroke_width - 2.0).abs() < 1e-12);
                assert_eq!(stroke_dasharray.as_deref(), Some("4.00 2.00"));
            }
            _ => panic!("path"),
        }
        match circle.scale_uniform(2.0) {
            Primitive::Circle {
                cx,
                cy,
                r,
                stroke_width,
                ..
            } => {
                assert!((cx - 10.0).abs() < 1e-12);
                assert!((cy - 12.0).abs() < 1e-12);
                assert!((r - 4.0).abs() < 1e-12);
                assert!((stroke_width - 1.0).abs() < 1e-12);
            }
            _ => panic!("circle"),
        }
        match text.scale_uniform(2.0) {
            Primitive::Text {
                x, y, font_size, ..
            } => {
                assert!((x - 2.0).abs() < 1e-12);
                assert!((y - 4.0).abs() < 1e-12);
                assert!((font_size - 24.0).abs() < 1e-12);
            }
            _ => panic!("text"),
        }

        let vp = Viewport {
            id: Some("m0".into()),
            x: 1.0,
            y: 2.0,
            ox: 0.0,
            oy: 0.0,
            width: 10.0,
            height: 20.0,
            layers: vec![Layer {
                name: LayerName::Marks,
                primitives: vec![circle.clone()],
            }],
        };
        assert_eq!(vp.scale_uniform(1.0), vp);
        let vp2 = vp.scale_uniform(2.0);
        assert!((vp2.x - 2.0).abs() < 1e-12);
        assert!((vp2.width - 20.0).abs() < 1e-12);

        let scene = Scene {
            width: 10.0,
            height: 20.0,
            x: 1.0,
            y: 2.0,
            viewports: vec![vp],
            overlays: vec![text],
            halo: vec![path],
        };
        assert_eq!(scene.scale_uniform(1.0), scene);
        let s2 = scene.scale_uniform(2.0);
        assert!((s2.width - 20.0).abs() < 1e-12);
        assert!((s2.height - 40.0).abs() < 1e-12);
        assert_eq!(s2.overlays.len(), 1);
        assert_eq!(s2.halo.len(), 1);
        assert!((s2.x - 2.0).abs() < 1e-12);
        assert!((s2.y - 4.0).abs() < 1e-12);
    }
}
