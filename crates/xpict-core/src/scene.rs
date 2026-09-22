//! Drawable scene document — engine-neutral primitives shared by Python & JS.
//!
//! **MVP:** one molecule → one [`Viewport`] inside a [`Scene`]. Marks (atom/bond
//! circles) and shade disks are layers on that viewport. Multi-mol diagrams /
//! ELK are out of scope for the first shared paint path.
//!
//! 1. Language edge supplies **coords + chem metadata** (RDKit/Indigo/native
//!    layout + align stay outside this crate).
//! 2. Rust builds a [`Scene`] of typed primitives (paths, circles, …).
//! 3. Thin Python / JS serializers emit SVG / data-URI `<img>`.
//!
//! Do not invent new depiction rules here — port proven Python behavior.

use serde::{Deserialize, Serialize};

/// Horizontal text anchor (SVG `text-anchor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAnchor {
    Start,
    Middle,
    End,
}

/// Layer name inside a molecule viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayerName {
    Shading,
    Halo,
    Bonds,
    Labels,
    Marks,
    Overlay,
}

/// One drawable primitive in SVG space (+Y down).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Primitive {
    Path {
        d: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<String>,
        #[serde(default = "default_stroke_width")]
        stroke_width: f64,
        #[serde(default = "default_opacity")]
        opacity: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke_dasharray: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke_linecap: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        class: Option<String>,
    },
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke: Option<String>,
        #[serde(default = "default_stroke_width")]
        stroke_width: f64,
        #[serde(default = "default_opacity")]
        opacity: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        class: Option<String>,
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
pub struct Layer {
    pub name: LayerName,
    #[serde(default)]
    pub primitives: Vec<Primitive>,
}

/// One molecule viewport in a diagram (after ELK/grid placement).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub layers: Vec<Layer>,
}

/// Full drawable document before SVG/HTML serialization.
///
/// Python `contracts.scene.Scene` and JS consumers should mirror this shape
/// (JSON round-trip is the ABI).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub width: f64,
    pub height: f64,
    pub viewports: Vec<Viewport>,
    /// Document-space primitives (reaction arrows) above viewports.
    #[serde(default)]
    pub overlays: Vec<Primitive>,
    /// Single document-space unioned knockout under molecule ink.
    #[serde(default)]
    pub halo: Vec<Primitive>,
}

/// Atom input for a molecule depict call (SVG-space coords from the caller).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtomIn {
    pub index: i32,
    pub element: String,
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub charge: i32,
}

/// Bond input (indices into [`AtomIn::index`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BondIn {
    pub index: i32,
    pub begin: i32,
    pub end: i32,
    pub order: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stereo: Option<String>,
}

/// One molecule ready to paint (coords already in SVG / ``SCALE`` space).
///
/// MVP extras: optional per-atom / per-bond shade scores and mark indices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoleculeIn {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub atoms: Vec<AtomIn>,
    pub bonds: Vec<BondIn>,
    /// Ink color for backbone bonds and atom labels (CSS). Default ``#111``.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Per-atom shade scores (same order as [`Self::atoms`]); omit if unshaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atom_shade: Option<Vec<f64>>,
    /// Per-bond shade scores (same order as [`Self::bonds`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bond_shade: Option<Vec<f64>>,
    /// Atom indices to circle (publication marks).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mark_atoms: Vec<i32>,
    /// Bond endpoint index pairs to circle/stroke-mark.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mark_bonds: Vec<(i32, i32)>,
}

impl Scene {
    pub fn empty(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            viewports: Vec::new(),
            overlays: Vec::new(),
            halo: Vec::new(),
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
            viewports: vec![Viewport {
                id: Some("m0".into()),
                x: 0.0,
                y: 0.0,
                width: 70.0,
                height: 75.0,
                layers: vec![Layer {
                    name: LayerName::Bonds,
                    primitives: vec![Primitive::Path {
                        d: "M 0 0 L 20 0".into(),
                        stroke: Some("#111".into()),
                        fill: Some("none".into()),
                        stroke_width: 1.12,
                        opacity: 1.0,
                        stroke_dasharray: None,
                        stroke_linecap: Some("round".into()),
                        class: Some("bond bond-skeleton".into()),
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
}
