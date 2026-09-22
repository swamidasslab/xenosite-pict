//! Atom-label orientation and backbone bond insets.
//!
//! Caller supplies SVG-space atom coordinates. Labels split into an
//! **atom-center** part (sits on the atom) and an optional **traveling**
//! part (extends away from the molecule). `OH` on the left of a mol becomes
//! `HO` with `O` still at the atom; abbreviations like `GlcA` have no
//! traveling part — the first or last glyph is the center depending on the
//! bond approach side.

use crate::font::{self, FaceStyle};
use crate::metrics::LABEL_GAP_PX;

/// Which horizontal side the traveling text extends toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelSide {
    /// Bond approaches from the left; traveling text extends east (right).
    East,
    /// Bond approaches from the right; traveling text extends west (left).
    West,
}

/// Atom-center glyph(s) vs optional traveling suffix/prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelParts {
    pub center: String,
    pub traveling: String,
}

/// One placed atom label in SVG space (+Y down).
#[derive(Debug, Clone)]
pub struct PlacedLabel {
    /// Display string after orientation (`OH` or `HO`).
    pub text: String,
    /// Left edge of the advance (SVG text origin), baseline at [`Self::y`].
    pub origin_x: f64,
    pub y: f64,
    pub atom_x: f64,
    pub atom_y: f64,
    pub side: LabelSide,
    /// Bond inset from the atom toward neighbors (isotropic: center glyph
    /// advance/ink width + gap). Per-bond shortening uses directional ink
    /// support via [`place_backbone`].
    pub clearance: f64,
}

/// Split a raw label into center + traveling parts.
///
/// Traveling is the trailing `H` / `H2` / … on a simple element token
/// (`OH`, `NH2`, `SH`). Everything else (e.g. `GlcA`, `Cl`, `OMe`) has an
/// empty traveling part — the center glyph is chosen by side at place time.
pub fn split_label(raw: &str) -> LabelParts {
    let s = raw.trim();
    if s.is_empty() {
        return LabelParts {
            center: String::new(),
            traveling: String::new(),
        };
    }
    // Element symbol + optional implicit-H suffix.
    let bytes = s.as_bytes();
    if bytes[0].is_ascii_uppercase() {
        let mut i = 1usize;
        if bytes.len() > 1 && bytes[1].is_ascii_lowercase() {
            i = 2;
        }
        let (elem, rest) = s.split_at(i);
        if let Some(travel) = parse_h_suffix(rest) {
            return LabelParts {
                center: elem.to_string(),
                traveling: travel.to_string(),
            };
        }
    }
    LabelParts {
        center: s.to_string(),
        traveling: String::new(),
    }
}

fn parse_h_suffix(rest: &str) -> Option<&str> {
    if rest.is_empty() {
        return None;
    }
    let bytes = rest.as_bytes();
    if bytes[0] != b'H' {
        return None;
    }
    let mut i = 1usize;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    // Only pure H / Hn — no trailing junk (NHAc, etc.).
    if i == rest.len() {
        Some(rest)
    } else {
        None
    }
}

/// Orient a label from the sum of neighbor vectors (SVG +Y down).
///
/// Neighbors to the right → bond approaches from the east → West orientation
/// (traveling text goes left). Degree-0 defaults to East.
pub fn label_side(atom: (f64, f64), neighbors: &[(f64, f64)]) -> LabelSide {
    if neighbors.is_empty() {
        return LabelSide::East;
    }
    let (ax, ay) = atom;
    let mut sx = 0.0;
    let mut sy = 0.0;
    for &(nx, ny) in neighbors {
        sx += nx - ax;
        sy += ny - ay;
    }
    // Near-vertical approaches still pick E/W from the x component when present;
    // pure vertical defaults to East (H to the right), matching common drawers.
    const VERT_SLOPE: f64 = 2.747477; // tan(70°)
    if sx.abs() < 1e-9 {
        return LabelSide::East;
    }
    let slope = sy / sx;
    if slope.abs() <= VERT_SLOPE {
        if sx > 0.0 {
            LabelSide::West
        } else {
            LabelSide::East
        }
    } else {
        // Steep: still prefer E/W from horizontal component for single bonds.
        if sx > 0.0 {
            LabelSide::West
        } else {
            LabelSide::East
        }
    }
}

/// Compose the display string for a side.
///
/// With a traveling part: East → `center+travel`, West → `travel+center`.
/// Without: the full center string is kept; placement picks first/last glyph.
pub fn compose_label(parts: &LabelParts, side: LabelSide) -> String {
    if parts.traveling.is_empty() {
        return parts.center.clone();
    }
    match side {
        LabelSide::East => format!("{}{}", parts.center, parts.traveling),
        LabelSide::West => format!("{}{}", parts.traveling, parts.center),
    }
}

fn advance_px(text: &str, font_px: f64) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    let (_shape, adv_em) = font::outline_run_em(text, FaceStyle::Regular);
    let face = font::face_metrics(FaceStyle::Regular);
    adv_em * (font_px / face.upem)
}

fn baseline_offset(font_px: f64) -> f64 {
    let face = font::face_metrics(FaceStyle::Regular);
    0.5 * (face.cap_height / face.upem) * font_px
}

/// Atom-center glyph string (center token, or first/last char).
fn center_glyph_text(parts: &LabelParts, side: LabelSide) -> String {
    if parts.center.is_empty() {
        return String::new();
    }
    if !parts.traveling.is_empty() {
        return parts.center.clone();
    }
    // No traveling part: first glyph (East) or last glyph (West) is the center.
    let ch = match side {
        LabelSide::East => parts.center.chars().next(),
        LabelSide::West => parts.center.chars().last(),
    };
    ch.map(|c| c.to_string()).unwrap_or_default()
}

/// Advance from string start to the start of the center glyph.
fn prefix_before_center(parts: &LabelParts, side: LabelSide, font_px: f64) -> f64 {
    if parts.traveling.is_empty() {
        match side {
            LabelSide::East => 0.0,
            LabelSide::West => {
                // Everything except the last glyph precedes the center.
                let mut chars = parts.center.chars();
                let last = chars.next_back();
                if last.is_none() {
                    return 0.0;
                }
                let prefix: String = chars.collect();
                advance_px(&prefix, font_px)
            }
        }
    } else {
        match side {
            LabelSide::East => 0.0,
            LabelSide::West => advance_px(&parts.traveling, font_px),
        }
    }
}

/// Ink AABB of the center glyph relative to the atom (SVG +Y down).
///
/// Advance center lies on the atom; baseline is [`baseline_offset`] below it.
/// Returns `(advance_px, Option<(xmin, xmax, ymin, ymax)>)`.
fn center_glyph_ink_rel(
    parts: &LabelParts,
    side: LabelSide,
    font_px: f64,
) -> (f64, Option<(f64, f64, f64, f64)>) {
    let text = center_glyph_text(parts, side);
    if text.is_empty() {
        return (0.0, None);
    }
    let face = font::face_metrics(FaceStyle::Regular);
    let scale = font_px / face.upem;
    let mut advance_em = 0.0;
    let mut ink_xmin: Option<f64> = None;
    let mut ink_ymin: Option<f64> = None;
    let mut ink_xmax: Option<f64> = None;
    let mut ink_ymax: Option<f64> = None;
    let mut x_cursor = 0.0;
    for ch in text.chars() {
        let Some(g) = font::glyph_metrics(ch, FaceStyle::Regular) else {
            continue;
        };
        if g.has_ink() {
            let gx0 = x_cursor + g.ink_xmin.unwrap();
            let gy0 = g.ink_ymin.unwrap();
            let gx1 = x_cursor + g.ink_xmax.unwrap();
            let gy1 = g.ink_ymax.unwrap();
            ink_xmin = Some(ink_xmin.map_or(gx0, |v| v.min(gx0)));
            ink_ymin = Some(ink_ymin.map_or(gy0, |v| v.min(gy0)));
            ink_xmax = Some(ink_xmax.map_or(gx1, |v| v.max(gx1)));
            ink_ymax = Some(ink_ymax.map_or(gy1, |v| v.max(gy1)));
        }
        x_cursor += g.advance;
        advance_em += g.advance;
    }
    let advance = advance_em * scale;
    let half = 0.5 * advance;
    let base = baseline_offset(font_px);
    let ink = match (ink_xmin, ink_ymin, ink_xmax, ink_ymax) {
        (Some(x0), Some(y0), Some(x1), Some(y1)) => {
            // Font +Y up → SVG +Y down; origin at left of advance on baseline.
            Some((
                x0 * scale - half,
                x1 * scale - half,
                base - y1 * scale,
                base - y0 * scale,
            ))
        }
        _ => None,
    };
    (advance, ink)
}

/// Support of an axis-aligned box in direction `(ux, uy)` (unit vector).
fn aabb_support(xmin: f64, xmax: f64, ymin: f64, ymax: f64, ux: f64, uy: f64) -> f64 {
    let sx = if ux >= 0.0 { xmax } else { xmin };
    let sy = if uy >= 0.0 { ymax } else { ymin };
    sx * ux + sy * uy
}

/// Bond inset toward `(ux, uy)` from glyph advance + ink metrics.
///
/// Uses `max(½ advance, ink support)` so horizontal hetero bonds keep the
/// advance floor while diagonal approaches clear the ink corners (e.g. N).
fn clearance_toward(
    advance: f64,
    ink: Option<(f64, f64, f64, f64)>,
    ux: f64,
    uy: f64,
) -> f64 {
    let half = 0.5 * advance;
    let ink_reach = ink
        .map(|(x0, x1, y0, y1)| aabb_support(x0, x1, y0, y1, ux, uy).max(0.0))
        .unwrap_or(0.0);
    half.max(ink_reach) + LABEL_GAP_PX
}

/// Isotropic label clearance (no bond direction): advance + ink width.
fn clearance_isotropic(advance: f64, ink: Option<(f64, f64, f64, f64)>) -> f64 {
    let mut half = 0.5 * advance;
    if let Some((x0, x1, _, _)) = ink {
        half = half.max(0.5 * (x1 - x0));
    }
    half + LABEL_GAP_PX
}

/// Place one label so the atom-center glyph sits on `(atom_x, atom_y)`.
pub fn place_label(
    raw: &str,
    atom_x: f64,
    atom_y: f64,
    side: LabelSide,
    font_px: f64,
) -> PlacedLabel {
    let parts = split_label(raw);
    let text = compose_label(&parts, side);
    let (center_adv, ink) = center_glyph_ink_rel(&parts, side, font_px);
    let prefix = prefix_before_center(&parts, side, font_px);
    // Center of the center-glyph advance lands on the atom.
    let origin_x = atom_x - prefix - 0.5 * center_adv;
    let y = atom_y + baseline_offset(font_px);
    let clearance = clearance_isotropic(center_adv, ink);
    PlacedLabel {
        text,
        origin_x,
        y,
        atom_x,
        atom_y,
        side,
        clearance,
    }
}

/// Shorten a bond segment by end insets (same as Python `bonds.shorten`).
pub fn shorten_bond(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    gap1: f64,
    gap2: f64,
) -> (f64, f64, f64, f64) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let length = (dx * dx + dy * dy).sqrt().max(1e-9);
    if gap1 + gap2 >= length {
        let mx = 0.5 * (x1 + x2);
        let my = 0.5 * (y1 + y2);
        return (mx, my, mx, my);
    }
    let ux = dx / length;
    let uy = dy / length;
    (
        x1 + ux * gap1,
        y1 + uy * gap1,
        x2 - ux * gap2,
        y2 - uy * gap2,
    )
}

/// Atom input for backbone + label placement.
#[derive(Debug, Clone)]
pub struct AtomIn {
    pub x: f64,
    pub y: f64,
    /// Display label (`None` / empty → no label).
    pub label: Option<String>,
}

/// Bond as atom index pair into [`place_backbone`] atoms slice.
#[derive(Debug, Clone, Copy)]
pub struct BondIn {
    pub begin: usize,
    pub end: usize,
}

/// Shortened bond endpoints in SVG space.
#[derive(Debug, Clone, Copy)]
pub struct BondOut {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// Place all atom labels and shorten bond endpoints into label clearances.
///
/// `atoms[i]` coords are caller-supplied SVG positions. Bond indices refer to
/// that slice. Returns `(bonds, labels)` where `labels[i]` is `Some` only when
/// atom `i` has a non-empty label.
///
/// Bond insets use the center glyph's advance floor and ink AABB support in
/// the bond direction (not a single ad-hoc radius).
pub fn place_backbone(
    atoms: &[AtomIn],
    bonds: &[BondIn],
    font_px: f64,
) -> (Vec<BondOut>, Vec<Option<PlacedLabel>>) {
    let n = atoms.len();
    let mut nbrs: Vec<Vec<(f64, f64)>> = vec![Vec::new(); n];
    for b in bonds {
        if b.begin < n && b.end < n {
            nbrs[b.begin].push((atoms[b.end].x, atoms[b.end].y));
            nbrs[b.end].push((atoms[b.begin].x, atoms[b.begin].y));
        }
    }

    // Per-atom center-glyph metrics for directional bond insets.
    let mut metrics: Vec<Option<(f64, Option<(f64, f64, f64, f64)>)>> = Vec::with_capacity(n);
    let mut labels: Vec<Option<PlacedLabel>> = Vec::with_capacity(n);
    for (i, atom) in atoms.iter().enumerate() {
        let raw = atom.label.as_deref().unwrap_or("").trim();
        if raw.is_empty() {
            labels.push(None);
            metrics.push(None);
            continue;
        }
        let side = label_side((atom.x, atom.y), &nbrs[i]);
        let parts = split_label(raw);
        metrics.push(Some(center_glyph_ink_rel(&parts, side, font_px)));
        labels.push(Some(place_label(raw, atom.x, atom.y, side, font_px)));
    }

    let mut out_bonds = Vec::with_capacity(bonds.len());
    for b in bonds {
        if b.begin >= n || b.end >= n {
            out_bonds.push(BondOut {
                x1: 0.0,
                y1: 0.0,
                x2: 0.0,
                y2: 0.0,
            });
            continue;
        }
        let a = &atoms[b.begin];
        let c = &atoms[b.end];
        let dx = c.x - a.x;
        let dy = c.y - a.y;
        let len = (dx * dx + dy * dy).sqrt().max(1e-9);
        let ux = dx / len;
        let uy = dy / len;
        let gap1 = match metrics[b.begin] {
            Some((adv, ink)) => clearance_toward(adv, ink, ux, uy),
            None => 0.0,
        };
        let gap2 = match metrics[b.end] {
            Some((adv, ink)) => clearance_toward(adv, ink, -ux, -uy),
            None => 0.0,
        };
        let (x1, y1, x2, y2) = shorten_bond(a.x, a.y, c.x, c.y, gap1, gap2);
        out_bonds.push(BondOut { x1, y1, x2, y2 });
    }

    (out_bonds, labels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{FONT_PX, LABEL_GAP_PX};

    #[test]
    fn split_oh_and_nh2() {
        let oh = split_label("OH");
        assert_eq!(oh.center, "O");
        assert_eq!(oh.traveling, "H");
        let nh2 = split_label("NH2");
        assert_eq!(nh2.center, "N");
        assert_eq!(nh2.traveling, "H2");
        let cl = split_label("Cl");
        assert_eq!(cl.center, "Cl");
        assert!(cl.traveling.is_empty());
        let glc = split_label("GlcA");
        assert_eq!(glc.center, "GlcA");
        assert!(glc.traveling.is_empty());
    }

    #[test]
    fn oh_flips_on_west() {
        let parts = split_label("OH");
        assert_eq!(compose_label(&parts, LabelSide::East), "OH");
        assert_eq!(compose_label(&parts, LabelSide::West), "HO");
    }

    #[test]
    fn glca_does_not_flip() {
        let parts = split_label("GlcA");
        assert_eq!(compose_label(&parts, LabelSide::East), "GlcA");
        assert_eq!(compose_label(&parts, LabelSide::West), "GlcA");
    }

    #[test]
    fn side_from_neighbors() {
        // Neighbor to the right → West (label extends left).
        assert_eq!(
            label_side((0.0, 0.0), &[(10.0, 0.0)]),
            LabelSide::West
        );
        assert_eq!(
            label_side((0.0, 0.0), &[(-10.0, 0.0)]),
            LabelSide::East
        );
    }

    #[test]
    fn oh_center_stays_on_atom() {
        let east = place_label("OH", 100.0, 50.0, LabelSide::East, FONT_PX);
        let west = place_label("OH", 100.0, 50.0, LabelSide::West, FONT_PX);
        assert_eq!(east.text, "OH");
        assert_eq!(west.text, "HO");
        let o_adv = advance_px("O", FONT_PX);
        // Origin places the midpoint of O on the atom.
        assert!((east.origin_x + 0.5 * o_adv - 100.0).abs() < 1e-6);
        let h_adv = advance_px("H", FONT_PX);
        assert!((west.origin_x + h_adv + 0.5 * o_adv - 100.0).abs() < 1e-6);
        // Clearance uses O metrics, not the full OH width.
        assert!((east.clearance - (0.5 * o_adv + LABEL_GAP_PX)).abs() < 1e-6);
        assert!(east.clearance < advance_px("OH", FONT_PX) * 0.5 + LABEL_GAP_PX - 0.1);
    }

    #[test]
    fn n_diagonal_clearance_uses_ink_support() {
        // Horizontal: advance floor. Diagonal: ink corner extends past half-advance.
        let parts = split_label("N");
        let (adv, ink) = center_glyph_ink_rel(&parts, LabelSide::East, FONT_PX);
        let horiz = clearance_toward(adv, ink, 1.0, 0.0);
        let diag = clearance_toward(adv, ink, std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2);
        assert!((horiz - (0.5 * adv + LABEL_GAP_PX)).abs() < 1e-6);
        assert!(diag > horiz + 0.3);
    }

    #[test]
    fn backbone_shortens_into_oh() {
        let atoms = vec![
            AtomIn {
                x: 0.0,
                y: 0.0,
                label: None,
            },
            AtomIn {
                x: 40.0,
                y: 0.0,
                label: Some("OH".into()),
            },
        ];
        let bonds = vec![BondIn { begin: 0, end: 1 }];
        let (out, labels) = place_backbone(&atoms, &bonds, FONT_PX);
        assert!(labels[1].is_some());
        let lab = labels[1].as_ref().unwrap();
        assert_eq!(lab.text, "OH"); // neighbor on the left → East
        assert!(out[0].x2 < 40.0);
        assert!((40.0 - out[0].x2 - lab.clearance).abs() < 1e-6);
    }

    #[test]
    fn backbone_n_diagonal_standoff_exceeds_half_advance() {
        let atoms = vec![
            AtomIn {
                x: 0.0,
                y: 0.0,
                label: None,
            },
            AtomIn {
                x: 30.0,
                y: 30.0,
                label: Some("N".into()),
            },
        ];
        let bonds = vec![BondIn { begin: 0, end: 1 }];
        let (out, labels) = place_backbone(&atoms, &bonds, FONT_PX);
        let lab = labels[1].as_ref().unwrap();
        let n_adv = advance_px("N", FONT_PX);
        let gap = ((out[0].x2 - 30.0).powi(2) + (out[0].y2 - 30.0).powi(2)).sqrt();
        assert!(gap > 0.5 * n_adv + LABEL_GAP_PX + 0.3);
        // Isotropic label clearance stays on the advance floor (ink width < advance).
        assert!((lab.clearance - (0.5 * n_adv + LABEL_GAP_PX)).abs() < 1e-6);
    }
}
