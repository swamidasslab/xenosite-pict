//! Atom-label orientation and backbone bond insets.
//!
//! Caller supplies SVG-space atom coordinates. Labels split into an
//! **atom-center** part (sits on the atom) and an optional **traveling**
//! part (extends away from the molecule). `OH` on the left of a mol becomes
//! `HO` with `O` still at the atom; abbreviations like `GlcA` have no
//! traveling part — the first or last glyph is the center depending on the
//! bond approach side.
//!
//! Orientation follows RDKit MolDraw2D (`E`/`W`/`N`/`S`). **All** chem scripts
//! (H-counts, charges, star-label markup, symbols, bold/italic) go through
//! [`crate::markup::parse_label_markup`] → [`ScriptRole`] fake scripts,
//! outlined as glyph **paths**, not ``<text>``.

use crate::font::{self, ChemGlyph, FaceStyle, ScriptRole};
use crate::geom::Shape;
use crate::markup;
use crate::metrics::{label_weight_standoff_px, LABEL_GAP_PX};

/// Which side the traveling text extends toward (RDKit OrientType).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelSide {
    /// Bond approaches from the left; traveling text extends east (right).
    East,
    /// Bond approaches from the right; traveling text extends west (left).
    West,
    /// Bond approaches from below; traveling text extends north (up, −Y SVG).
    North,
    /// Bond approaches from above; traveling text extends south (down, +Y SVG).
    South,
}

/// Atom-center glyph(s) vs optional traveling H / charge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelParts {
    pub center: String,
    /// Traveling H run (`"H"`, `"H2"`, …) or empty.
    pub traveling: String,
    /// Formal charge (`0` = none). Drawn as a superscript.
    pub charge: i32,
}

/// One placed atom label in SVG space (+Y down).
#[derive(Debug, Clone)]
pub struct PlacedLabel {
    /// Original input label (for re-outline / halo).
    pub raw: String,
    /// Display string for ``data-text`` (unicode scripts when possible).
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
    /// Outlined glyph path `d` (empty when no ink).
    pub path_d: String,
}

/// Build a display label from element, implicit H count, and formal charge.
///
/// Silent carbons (`C` with charge 0) return `None`. Hydrogens become
/// ``OH`` / ``NH2``; charge is left for [`crate::depict`] / callers that
/// append via `AtomIn.charge`, or pass `charge` here to bake it in (RDKit
/// host path).
pub fn atom_display_label(element: &str, imp_hs: u32, charge: i32) -> Option<String> {
    if element == "C" && charge == 0 {
        return None;
    }
    if element == "*" {
        return Some("*".into());
    }
    let mut text = element.to_string();
    if imp_hs == 1 {
        text = format!("{element}H");
    } else if imp_hs > 1 {
        text = format!("{element}H{imp_hs}");
    }
    if charge != 0 {
        let sign = if charge > 0 { "+" } else { "−" };
        let mag = charge.unsigned_abs();
        text = if mag == 1 {
            format!("{text}{sign}")
        } else {
            format!("{text}{mag}{sign}")
        };
    }
    Some(text)
}

/// Split a raw label into center + traveling H + charge.
///
/// Accepts backend strings like ``NH2``, ``OH``, ``NH4+``, ``O−``, ``*``,
/// and marked forms ``$R_1$`` / ``R^2`` (markup stays on ``center`` for
/// [`parse_label_markup`]). Bare underscores (``my_name``) are **not**
/// treated as scripts at split time.
pub fn split_label(raw: &str) -> LabelParts {
    let s = raw.trim();
    if s.is_empty() {
        return LabelParts {
            center: String::new(),
            traveling: String::new(),
            charge: 0,
        };
    }
    // Keep `$…$` intact so the markup parser still sees the chem zone.
    let (body, charge) = if s.starts_with('$') && s.ends_with('$') && s.len() >= 2 {
        // Charge rarely wraps the dollars; strip charge outside only.
        (s.to_string(), 0)
    } else {
        strip_charge(s)
    };
    if body == "*" {
        return LabelParts {
            center: "*".into(),
            traveling: String::new(),
            charge,
        };
    }
    // Explicit markup: `$…$`, `^`, or braced `_{…}` — no H-split.
    let has_markup = body.contains('$')
        || body.contains('^')
        || body.contains("_{")
        || body.contains("\\")
        || body.contains("**")
        || (body.contains('*') && body != "*");
    if has_markup {
        return LabelParts {
            center: body,
            traveling: String::new(),
            charge,
        };
    }
    let bytes = body.as_bytes();
    if !bytes.is_empty() && bytes[0].is_ascii_uppercase() {
        let mut i = 1usize;
        if bytes.len() > 1 && bytes[1].is_ascii_lowercase() {
            i = 2;
        }
        let (elem, rest) = body.split_at(i);
        if let Some(travel) = parse_h_suffix(rest) {
            return LabelParts {
                center: elem.to_string(),
                traveling: travel.to_string(),
                charge,
            };
        }
    }
    LabelParts {
        center: body.to_string(),
        traveling: String::new(),
        charge,
    }
}

fn strip_charge(s: &str) -> (String, i32) {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return (String::new(), 0);
    }
    // Trailing + / − / - / ⁺ / ⁻, optional leading magnitude or repeated signs.
    let last = *chars.last().unwrap();
    let is_plus = last == '+' || last == '⁺';
    let is_minus = last == '-' || last == '−' || last == '⁻';
    if !is_plus && !is_minus {
        return (s.to_string(), 0);
    }
    let sign: i32 = if is_plus { 1 } else { -1 };
    let mut i = chars.len() - 1;
    // Collapse repeated ++++ / ----
    while i > 0 {
        let c = chars[i - 1];
        let same = (sign > 0 && (c == '+' || c == '⁺'))
            || (sign < 0 && (c == '-' || c == '−' || c == '⁻'));
        if same {
            i -= 1;
        } else {
            break;
        }
    }
    let n_signs = (chars.len() - i) as i32;
    let without_sign: String = chars[..i].iter().collect();
    // ``NH4+`` → H-count owns the digits; charge mag is just the sign count.
    // ``Fe3+`` / ``N2+`` → digits before the sign are the charge magnitude.
    if without_sign.contains('H') {
        // Element…H… — don't steal H-count digits for the charge.
        return (without_sign, sign * n_signs);
    }
    let mut j = i;
    while j > 0 && chars[j - 1].is_ascii_digit() {
        j -= 1;
    }
    let mag = if j < i {
        let digits: String = chars[j..i].iter().collect();
        digits.parse::<i32>().unwrap_or(n_signs).max(1)
    } else {
        n_signs
    };
    let body: String = chars[..j].iter().collect();
    (body, sign * mag)
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

fn h_count(parts: &LabelParts) -> u32 {
    let t = parts.traveling.as_str();
    if t.is_empty() || !t.starts_with('H') {
        return 0;
    }
    if t.len() == 1 {
        return 1;
    }
    t[1..].parse().unwrap_or(1)
}

/// Orient a label from the sum of neighbor vectors (SVG +Y down).
///
/// Matches RDKit ``getAtomOrientation`` (70° vertical slope; degree-1 prefers
/// E/W; degree-0 defaults East except O/F/S/Cl-class → West).
pub fn label_side(atom: (f64, f64), neighbors: &[(f64, f64)]) -> LabelSide {
    label_side_for(atom, neighbors, None)
}

/// Like [`label_side`] but degree-0 Hs-first elements use West (RDKit).
pub fn label_side_for(
    atom: (f64, f64),
    neighbors: &[(f64, f64)],
    element: Option<&str>,
) -> LabelSide {
    const VERT_SLOPE: f64 = 2.747477; // tan(70°)
    if neighbors.is_empty() {
        // RDKit: O, F, S, Cl, … list H first → West.
        const HS_FIRST: &[&str] = &["O", "F", "S", "Cl", "Br", "I", "Se", "Te", "At"];
        if element.is_some_and(|e| HS_FIRST.contains(&e)) {
            return LabelSide::West;
        }
        return LabelSide::East;
    }
    let (ax, ay) = atom;
    let mut sx = 0.0;
    let mut sy = 0.0;
    for &(nx, ny) in neighbors {
        sx += nx - ax;
        sy += ny - ay;
    }
    let islope = if sx.abs() > 1e-4 { sy / sx } else { 1000.0 };
    let mut orient = if islope.abs() <= VERT_SLOPE {
        if sx > 0.0 {
            LabelSide::West
        } else {
            LabelSide::East
        }
    } else if sy > 0.0 {
        // SVG +Y down → neighbor sum south → label extends North (away).
        LabelSide::North
    } else {
        LabelSide::South
    };

    // Degree-1: never N/S (RDKit).
    if neighbors.len() == 1 && matches!(orient, LabelSide::North | LabelSide::South) {
        if islope.abs() > VERT_SLOPE {
            orient = LabelSide::East;
        } else if sx > 0.0 {
            orient = LabelSide::West;
        } else {
            orient = LabelSide::East;
        }
    }

    // Degree-3: keep N/S when a near-vertical bond points that way.
    if neighbors.len() == 3 && matches!(orient, LabelSide::North | LabelSide::South) {
        for &(nx, ny) in neighbors {
            let dx = nx - ax;
            let dy = ny - ay;
            if dx.abs() < 1e-9 {
                if dy > 0.0 {
                    orient = LabelSide::North;
                } else {
                    orient = LabelSide::South;
                }
                break;
            }
            let ang = (dy / dx).atan().to_degrees();
            if (80.0..100.0).contains(&ang) && orient == LabelSide::North {
                break;
            }
            if (-100.0..-80.0).contains(&ang) && orient == LabelSide::South {
                break;
            }
        }
    }

    orient
}

fn charge_glyphs(charge: i32, style: FaceStyle) -> Vec<ChemGlyph> {
    if charge == 0 {
        return Vec::new();
    }
    let mag = charge.unsigned_abs();
    let sign = if charge > 0 { '+' } else { '-' };
    // Formal charge stays on the element baseline (not a fake superscript).
    // N/S orientation already stacks travel+charge on a second line; raising
    // the glyph again made top charges float and bottom charges crowd the N.
    // Explicit caret markup (`R^{2+}`) still goes through parse_label_markup.
    let mut out = Vec::new();
    if mag != 1 {
        for ch in mag.to_string().chars() {
            out.push(ChemGlyph {
                ch,
                role: ScriptRole::Normal,
                face: style,
            });
        }
    }
    out.push(ChemGlyph {
        ch: sign,
        role: ScriptRole::Normal,
        face: style,
    });
    out
}

fn h_travel_glyphs(parts: &LabelParts, style: FaceStyle) -> Vec<ChemGlyph> {
    let n = h_count(parts);
    if n == 0 {
        return Vec::new();
    }
    // Braced ``H_{n}`` so subscript works outside ``$…$`` (bare ``_`` does not).
    let marked = if n == 1 {
        "H".to_string()
    } else {
        format!("H_{{{n}}}")
    };
    markup::parse_label_markup(&marked, style)
}

fn center_glyphs(parts: &LabelParts, style: FaceStyle) -> Vec<ChemGlyph> {
    markup::parse_label_markup(&parts.center, style)
}

/// Glyphs in draw order for an orientation (atom-center first for E; …).
fn glyphs_for_side(parts: &LabelParts, side: LabelSide, style: FaceStyle) -> Vec<ChemGlyph> {
    let center = center_glyphs(parts, style);
    let travel = h_travel_glyphs(parts, style);
    let charge = charge_glyphs(parts.charge, style);
    match side {
        LabelSide::East | LabelSide::South | LabelSide::North => {
            let mut g = center;
            g.extend(travel);
            g.extend(charge);
            g
        }
        LabelSide::West => {
            // Travel + charge left of center (RDKit reverses pieces for W).
            let mut g = charge;
            g.extend(travel);
            g.extend(center);
            g
        }
    }
}

/// Plain display string after orientation (``OH`` / ``HO`` / ``NH₂+``).
pub fn compose_label(parts: &LabelParts, side: LabelSide) -> String {
    let glyphs = glyphs_for_side(parts, side, FaceStyle::Regular);
    glyphs_to_data_text(&glyphs)
}

fn glyphs_to_data_text(glyphs: &[ChemGlyph]) -> String {
    let mut out = String::new();
    for g in glyphs {
        match g.role {
            ScriptRole::Normal => out.push(g.ch),
            ScriptRole::Subscript => out.push(subscript_char(g.ch)),
            ScriptRole::Superscript => out.push(superscript_char(g.ch)),
        }
    }
    out
}

fn subscript_char(ch: char) -> char {
    match ch {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        _ => ch,
    }
}

fn superscript_char(ch: char) -> char {
    match ch {
        '+' => '⁺',
        '-' => '⁻',
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        _ => ch,
    }
}

fn advance_glyphs_px(glyphs: &[ChemGlyph], font_px: f64, style: FaceStyle) -> f64 {
    if glyphs.is_empty() {
        return 0.0;
    }
    let face = font::face_metrics(style);
    let (_shape, adv_em) = font::outline_chem_run_em(glyphs, style);
    adv_em * (font_px / face.upem)
}

#[cfg(test)]
fn advance_px(text: &str, font_px: f64, style: FaceStyle) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    let glyphs: Vec<_> = text
        .chars()
        .map(|ch| ChemGlyph {
            ch,
            role: ScriptRole::Normal,
            face: style,
        })
        .collect();
    advance_glyphs_px(&glyphs, font_px, style)
}

fn baseline_offset(font_px: f64, style: FaceStyle) -> f64 {
    let face = font::face_metrics(style);
    0.5 * (face.cap_height / face.upem) * font_px
}

/// Atom-center glyph(s) for clearance / anchoring (element token, or one
/// base glyph of an abbreviation — never a script mark or digit alone).
fn center_anchor_glyphs(parts: &LabelParts, side: LabelSide, style: FaceStyle) -> Vec<ChemGlyph> {
    let center = center_glyphs(parts, style);
    if center.is_empty() {
        return Vec::new();
    }
    if !parts.traveling.is_empty() || parts.charge != 0 {
        return center;
    }
    let normals: Vec<ChemGlyph> = center
        .iter()
        .copied()
        .filter(|g| g.role == ScriptRole::Normal)
        .collect();
    if normals.is_empty() {
        return center;
    }
    let g = match side {
        LabelSide::East | LabelSide::South | LabelSide::North => normals[0],
        LabelSide::West => *normals.last().unwrap(),
    };
    vec![g]
}

/// Advance from string start to the start of the center glyph (E/W runs).
fn prefix_before_center(
    parts: &LabelParts,
    side: LabelSide,
    font_px: f64,
    style: FaceStyle,
) -> f64 {
    match side {
        LabelSide::East | LabelSide::North | LabelSide::South => 0.0,
        LabelSide::West => {
            if parts.traveling.is_empty() && parts.charge == 0 {
                let center = center_glyphs(parts, style);
                let normals: Vec<_> = center
                    .iter()
                    .enumerate()
                    .filter(|(_, g)| g.role == ScriptRole::Normal)
                    .collect();
                let Some(&(idx, _)) = normals.last() else {
                    return 0.0;
                };
                return advance_glyphs_px(&center[..idx], font_px, style);
            }
            let mut prefix = charge_glyphs(parts.charge, style);
            prefix.extend(h_travel_glyphs(parts, style));
            advance_glyphs_px(&prefix, font_px, style)
        }
    }
}

/// Full oriented label ink AABB relative to the atom (SVG +Y down).
///
/// Used for bond shortening so traveling H and charge superscripts clear
/// bond ends when a neighbor approaches from that side. Atom-center
/// isotropic [`PlacedLabel::clearance`] still uses [`center_glyph_ink_rel`].
fn label_clearance_ink_rel(
    parts: &LabelParts,
    side: LabelSide,
    font_px: f64,
    style: FaceStyle,
) -> (f64, Option<(f64, f64, f64, f64)>) {
    let (center_adv, _) = center_glyph_ink_rel(parts, side, font_px, style);
    let prefix = prefix_before_center(parts, side, font_px, style);
    let origin_x = -prefix - 0.5 * center_adv;
    let baseline = baseline_offset(font_px, style);
    // Atom at (0, 0); outline_placed uses atom_x for N/S centering.
    let Some(shape) = outline_placed(parts, side, origin_x, baseline, 0.0, 0.0, font_px, style)
    else {
        return (center_adv, None);
    };
    let ink = shape.bounds().map(|(x0, y0, x1, y1)| (x0, x1, y0, y1));
    (center_adv, ink)
}

/// Ink AABB of the center glyph relative to the atom (SVG +Y down).
fn center_glyph_ink_rel(
    parts: &LabelParts,
    side: LabelSide,
    font_px: f64,
    style: FaceStyle,
) -> (f64, Option<(f64, f64, f64, f64)>) {
    let anchor = center_anchor_glyphs(parts, side, style);
    if anchor.is_empty() {
        return (0.0, None);
    }
    // ``*`` uses the custom star metrics.
    if anchor.len() == 1 && anchor[0].ch == '*' && anchor[0].role == ScriptRole::Normal {
        let face = font::face_metrics(style);
        let scale = font_px / face.upem;
        let (shape, adv_em) = font::outline_star_em(style);
        let advance = adv_em * scale;
        let half = 0.5 * advance;
        let base = baseline_offset(font_px, style);
        let ink = shape.bounds().map(|(x0, y0, x1, y1)| {
            (
                x0 * scale - half,
                x1 * scale - half,
                base - y1 * scale,
                base - y0 * scale,
            )
        });
        return (advance, ink);
    }
    let face = font::face_metrics(style);
    let scale = font_px / face.upem;
    let (shape, adv_em) = font::outline_chem_run_em(&anchor, style);
    let advance = adv_em * scale;
    let half = 0.5 * advance;
    let base = baseline_offset(font_px, style);
    let ink = shape.and_then(|s| s.bounds()).map(|(x0, y0, x1, y1)| {
        (
            x0 * scale - half,
            x1 * scale - half,
            base - y1 * scale,
            base - y0 * scale,
        )
    });
    (advance, ink)
}

fn aabb_support(xmin: f64, xmax: f64, ymin: f64, ymax: f64, ux: f64, uy: f64) -> f64 {
    let sx = if ux >= 0.0 { xmax } else { xmin };
    let sy = if uy >= 0.0 { ymax } else { ymin };
    sx * ux + sy * uy
}

fn clearance_toward(
    advance: f64,
    ink: Option<(f64, f64, f64, f64)>,
    ux: f64,
    uy: f64,
    standoff: f64,
) -> f64 {
    let half = 0.5 * advance;
    let ink_reach = ink
        .map(|(x0, x1, y0, y1)| aabb_support(x0, x1, y0, y1, ux, uy).max(0.0))
        .unwrap_or(0.0);
    half.max(ink_reach) + LABEL_GAP_PX + standoff
}

fn clearance_isotropic(advance: f64, ink: Option<(f64, f64, f64, f64)>, standoff: f64) -> f64 {
    let mut half = 0.5 * advance;
    if let Some((x0, x1, _, _)) = ink {
        half = half.max(0.5 * (x1 - x0));
    }
    half + LABEL_GAP_PX + standoff
}

/// Outline chem label glyphs at the placed origin (SVG +Y down).
fn outline_placed(
    parts: &LabelParts,
    side: LabelSide,
    origin_x: f64,
    baseline_y: f64,
    atom_x: f64,
    _atom_y: f64,
    font_px: f64,
    style: FaceStyle,
) -> Option<Shape> {
    let face = font::face_metrics(style);
    let scale = font_px / face.upem;
    let line_gap = 1.1 * (face.cap_height / face.upem) * font_px;

    match side {
        LabelSide::East | LabelSide::West => {
            let glyphs = glyphs_for_side(parts, side, style);
            font::compile_chem_shapes(
                &glyphs,
                origin_x,
                baseline_y,
                font_px,
                "start",
                style,
            )
        }
        LabelSide::North | LabelSide::South => {
            // Stack: center on atom; travel (+ charge) above (N) or below (S).
            let center = center_glyphs(parts, style);
            let mut travel = h_travel_glyphs(parts, style);
            travel.extend(charge_glyphs(parts.charge, style));
            let (c_shape, c_adv_em) = font::outline_chem_run_em(&center, style);
            let c_adv = c_adv_em * scale;
            let c_origin_x = atom_x - 0.5 * c_adv;
            let mut acc = c_shape.map(|s| {
                s.scale(scale, -scale, 0.0, 0.0)
                    .translate(c_origin_x, baseline_y)
            });
            if !travel.is_empty() {
                let (t_shape, t_adv_em) = font::outline_chem_run_em(&travel, style);
                let t_adv = t_adv_em * scale;
                let t_origin_x = atom_x - 0.5 * t_adv;
                let t_y = if side == LabelSide::North {
                    baseline_y - line_gap
                } else {
                    baseline_y + line_gap
                };
                if let Some(ts) = t_shape {
                    let placed = ts
                        .scale(scale, -scale, 0.0, 0.0)
                        .translate(t_origin_x, t_y);
                    acc = Some(match acc.take() {
                        Some(a) => a.union(&placed),
                        None => placed,
                    });
                }
            }
            acc.filter(|s| !s.is_empty())
        }
    }
}

/// Place one label so the atom-center glyph sits on `(atom_x, atom_y)`.
pub fn place_label(
    raw: &str,
    atom_x: f64,
    atom_y: f64,
    side: LabelSide,
    font_px: f64,
    style: FaceStyle,
) -> PlacedLabel {
    place_label_weighted(
        raw,
        atom_x,
        atom_y,
        side,
        font_px,
        style,
        crate::metrics::WEIGHT_MIN,
    )
}

/// Like [`place_label`], with mol ``weight`` increasing bond↔label clearance.
pub fn place_label_weighted(
    raw: &str,
    atom_x: f64,
    atom_y: f64,
    side: LabelSide,
    font_px: f64,
    style: FaceStyle,
    weight: f64,
) -> PlacedLabel {
    let parts = split_label(raw);
    let text = compose_label(&parts, side);
    let (center_adv, ink) = center_glyph_ink_rel(&parts, side, font_px, style);
    let prefix = prefix_before_center(&parts, side, font_px, style);
    let origin_x = atom_x - prefix - 0.5 * center_adv;
    let y = atom_y + baseline_offset(font_px, style);
    let standoff = label_weight_standoff_px(weight);
    let clearance = clearance_isotropic(center_adv, ink, standoff);
    let path_d = outline_placed(&parts, side, origin_x, y, atom_x, atom_y, font_px, style)
        .map(|s| s.to_svg_d())
        .unwrap_or_default();
    PlacedLabel {
        raw: raw.to_string(),
        text,
        origin_x,
        y,
        atom_x,
        atom_y,
        side,
        clearance,
        path_d,
    }
}

/// Glyph ink for a placed label (halo / occupancy).
pub fn label_ink_shape(pl: &PlacedLabel, font_px: f64, style: FaceStyle) -> Option<Shape> {
    let parts = split_label(&pl.raw);
    outline_placed(
        &parts,
        pl.side,
        pl.origin_x,
        pl.y,
        pl.atom_x,
        pl.atom_y,
        font_px,
        style,
    )
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
/// ``weight`` (≥ 1) thickens bond↔label standoff so fat ink clears fat stems.
pub fn place_backbone(
    atoms: &[AtomIn],
    bonds: &[BondIn],
    font_px: f64,
    style: FaceStyle,
    weight: f64,
) -> (Vec<BondOut>, Vec<Option<PlacedLabel>>) {
    let standoff = label_weight_standoff_px(weight);
    let n = atoms.len();
    let mut nbrs: Vec<Vec<(f64, f64)>> = vec![Vec::new(); n];
    for b in bonds {
        if b.begin < n && b.end < n {
            nbrs[b.begin].push((atoms[b.end].x, atoms[b.end].y));
            nbrs[b.end].push((atoms[b.begin].x, atoms[b.begin].y));
        }
    }

    let mut metrics: Vec<Option<(f64, Option<(f64, f64, f64, f64)>)>> = Vec::with_capacity(n);
    let mut labels: Vec<Option<PlacedLabel>> = Vec::with_capacity(n);
    for (i, atom) in atoms.iter().enumerate() {
        let raw = atom.label.as_deref().unwrap_or("").trim();
        if raw.is_empty() {
            labels.push(None);
            metrics.push(None);
            continue;
        }
        let parts = split_label(raw);
        let elem = parts.center.as_str();
        let side = label_side_for((atom.x, atom.y), &nbrs[i], Some(elem));
        // Full label ink (H + charge) so bonds do not clip superscripts.
        metrics.push(Some(label_clearance_ink_rel(&parts, side, font_px, style)));
        labels.push(Some(place_label_weighted(
            raw, atom.x, atom.y, side, font_px, style, weight,
        )));
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
            Some((adv, ink)) => clearance_toward(adv, ink, ux, uy, standoff),
            None => 0.0,
        };
        let gap2 = match metrics[b.end] {
            Some((adv, ink)) => clearance_toward(adv, ink, -ux, -uy, standoff),
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
    fn atom_display_label_appends_implicit_h() {
        assert_eq!(atom_display_label("O", 1, 0).as_deref(), Some("OH"));
        assert_eq!(atom_display_label("N", 2, 0).as_deref(), Some("NH2"));
        assert_eq!(atom_display_label("N", 4, 1).as_deref(), Some("NH4+"));
        assert_eq!(atom_display_label("O", 0, -1).as_deref(), Some("O−"));
        assert_eq!(atom_display_label("C", 3, 0), None);
        // Charged carbon keeps H (RDKit): [CH3+] → CH3+.
        assert_eq!(atom_display_label("C", 3, 1).as_deref(), Some("CH3+"));
        assert_eq!(atom_display_label("*", 0, 0).as_deref(), Some("*"));
    }

    #[test]
    fn split_charge_and_ammonium() {
        let n = split_label("NH4+");
        assert_eq!(n.center, "N");
        assert_eq!(n.traveling, "H4");
        assert_eq!(n.charge, 1);
        let o = split_label("O−");
        assert_eq!(o.center, "O");
        assert_eq!(o.charge, -1);
        let star = split_label("*");
        assert_eq!(star.center, "*");
    }

    #[test]
    fn oh_flips_on_west() {
        let parts = split_label("OH");
        assert_eq!(compose_label(&parts, LabelSide::East), "OH");
        assert_eq!(compose_label(&parts, LabelSide::West), "HO");
    }

    #[test]
    fn nh2_uses_subscript() {
        let parts = split_label("NH2");
        assert_eq!(compose_label(&parts, LabelSide::East), "NH₂");
        assert_eq!(compose_label(&parts, LabelSide::West), "H₂N");
        // Same pathway as braced markup (bare ``H_2`` is literal outside ``$``).
        let via_markup = markup::parse_label_markup("H_{2}", FaceStyle::Regular);
        assert_eq!(via_markup.len(), 2);
        assert_eq!(via_markup[0].ch, 'H');
        assert_eq!(via_markup[1].role, ScriptRole::Subscript);
    }

    #[test]
    fn star_label_scripts_via_markup() {
        // Bare underscore is literal (``my_name``-safe).
        assert_eq!(
            compose_label(&split_label("R_1"), LabelSide::East),
            "R_1"
        );
        assert_eq!(
            compose_label(&split_label("my_name"), LabelSide::East),
            "my_name"
        );
        assert_eq!(
            compose_label(&split_label("$R_1$"), LabelSide::East),
            "R₁"
        );
        assert_eq!(
            compose_label(&split_label("R^2"), LabelSide::East),
            "R²"
        );
        assert_eq!(
            compose_label(&split_label("R^{2+}"), LabelSide::East),
            "R²⁺"
        );
        assert_eq!(
            compose_label(&split_label(r"$\alpha$"), LabelSide::East),
            "α"
        );
        assert_eq!(
            compose_label(&split_label("**R**"), LabelSide::East),
            "R"
        );
        let pl = place_label("$R_1$", 0.0, 0.0, LabelSide::East, FONT_PX, FaceStyle::Regular);
        assert_eq!(pl.text, "R₁");
        assert!(!pl.path_d.is_empty());
        // West anchors on R (not the subscript digit).
        let west = place_label("$R_1$", 0.0, 0.0, LabelSide::West, FONT_PX, FaceStyle::Regular);
        assert_eq!(west.text, "R₁");
        assert!(west.origin_x < 0.0 || west.origin_x.abs() < FONT_PX);
    }

    #[test]
    fn charge_is_plain_not_superscript() {
        // Formal charge rides the same baseline as the element (not a raised
        // script). N/S stacking already moves the charge off the atom; a
        // second superscript lift made top too high and bottom crowded.
        let parts = split_label("NH4+");
        assert_eq!(compose_label(&parts, LabelSide::East), "NH₄+");
        assert_eq!(
            compose_label(&split_label("N+"), LabelSide::East),
            "N+"
        );
        assert_eq!(
            compose_label(&split_label("Fe2+"), LabelSide::East),
            "Fe2+"
        );
        // Explicit caret markup still superscripts.
        assert_eq!(
            compose_label(&split_label("R^{2+}"), LabelSide::East),
            "R²⁺"
        );
    }

    #[test]
    fn charge_shares_element_baseline_east() {
        let style = FaceStyle::Regular;
        let n = place_label("N", 0.0, 0.0, LabelSide::East, FONT_PX, style);
        let np = place_label("N+", 0.0, 0.0, LabelSide::East, FONT_PX, style);
        let (_nx0, ny0, _nx1, _ny1) = label_ink_shape(&n, FONT_PX, style)
            .expect("N")
            .bounds()
            .expect("N ink");
        let (_px0, py0, _px1, _py1) = label_ink_shape(&np, FONT_PX, style)
            .expect("N+")
            .bounds()
            .expect("N+ ink");
        // Superscript would lift the label top by ~0.5×cap (~0.35×FONT_PX).
        assert!(
            py0 > ny0 - 0.12 * FONT_PX,
            "N+ top {py0} rose too far above N top {ny0} (still superscript?)"
        );
    }

    #[test]
    fn north_south_charge_stack_without_script_lift() {
        let style = FaceStyle::Regular;
        let face = font::face_metrics(style);
        let line_gap = 1.1 * (face.cap_height / face.upem) * FONT_PX;

        let north_n = place_label("N", 0.0, 0.0, LabelSide::North, FONT_PX, style);
        let north_np = place_label("N+", 0.0, 0.0, LabelSide::North, FONT_PX, style);
        let (_, n_top, _, _) = label_ink_shape(&north_n, FONT_PX, style)
            .unwrap()
            .bounds()
            .unwrap();
        let (_, np_top, _, _) = label_ink_shape(&north_np, FONT_PX, style)
            .unwrap()
            .bounds()
            .unwrap();
        let rise = n_top - np_top; // SVG +Y down: smaller y is higher
        assert!(
            rise > 0.45 * line_gap,
            "north charge should stack above N; rise={rise}, line_gap={line_gap}"
        );
        assert!(
            rise < line_gap + 0.55 * FONT_PX,
            "north charge rose {rise} (line_gap={line_gap}); script lift on top of stack?"
        );

        let south_n = place_label("N", 0.0, 0.0, LabelSide::South, FONT_PX, style);
        let south_np = place_label("N+", 0.0, 0.0, LabelSide::South, FONT_PX, style);
        let (_, _, _, n_bot) = label_ink_shape(&south_n, FONT_PX, style)
            .unwrap()
            .bounds()
            .unwrap();
        let (_, _, _, np_bot) = label_ink_shape(&south_np, FONT_PX, style)
            .unwrap()
            .bounds()
            .unwrap();
        let drop = np_bot - n_bot;
        assert!(
            drop > 0.45 * line_gap,
            "south charge should stack below N; drop={drop}, line_gap={line_gap}"
        );
        // Without script lift the charge sits fully on the lower baseline —
        // drop reaches ~line_gap + charge height, not a cramped half-step.
        assert!(
            drop > line_gap * 0.85,
            "south charge crowded against N: drop={drop}, line_gap={line_gap}"
        );
    }

    #[test]
    fn charge_extends_east_bond_clearance() {
        // Center-only clearance ignores the superscript; full-label ink must
        // push the bond end past the charge when approached from the east.
        let style = FaceStyle::Regular;
        let bare = split_label("O");
        let charged = split_label("O+");
        let (adv_o, ink_o) = label_clearance_ink_rel(&bare, LabelSide::East, FONT_PX, style);
        let (adv_op, ink_op) = label_clearance_ink_rel(&charged, LabelSide::East, FONT_PX, style);
        let east_o = clearance_toward(adv_o, ink_o, 1.0, 0.0, 0.0);
        let east_op = clearance_toward(adv_op, ink_op, 1.0, 0.0, 0.0);
        assert!(
            east_op > east_o + 0.8,
            "O+ east clearance {east_op} should exceed bare O {east_o} by charge width"
        );
        // West approach still clears the element (charge is on the east).
        let west_o = clearance_toward(adv_o, ink_o, -1.0, 0.0, 0.0);
        let west_op = clearance_toward(adv_op, ink_op, -1.0, 0.0, 0.0);
        assert!(
            (west_op - west_o).abs() < 0.5,
            "west clearance should stay near the element: O={west_o} O+={west_op}"
        );
    }

    #[test]
    fn backbone_shortens_into_charge_from_charge_side() {
        // Stronger west neighbor → East label (charge to the right); weaker
        // east neighbor so a bond still approaches from the charge side.
        let atoms = vec![
            AtomIn {
                x: -40.0,
                y: 0.0,
                label: None,
            },
            AtomIn {
                x: 0.0,
                y: 0.0,
                label: Some("O+".into()),
            },
            AtomIn {
                x: 25.0,
                y: 0.0,
                label: None,
            },
        ];
        let bonds = vec![
            BondIn { begin: 0, end: 1 },
            BondIn { begin: 1, end: 2 },
        ];
        let (out, labels) = place_backbone(&atoms, &bonds, FONT_PX, FaceStyle::Regular, 1.0);
        let lab = labels[1].as_ref().expect("O+");
        assert_eq!(lab.side, LabelSide::East, "expected East charge side, got {:?}", lab.side);
        assert!(
            lab.text.contains('⁺') || lab.text.contains('+'),
            "expected charge in label text, got {}",
            lab.text
        );
        // Right-hand bond must stop short of the atom past the charge ink.
        let o_adv = advance_px("O", FONT_PX, FaceStyle::Regular);
        let gap_from_o = out[1].x1 - 0.0;
        assert!(
            gap_from_o > 0.5 * o_adv + LABEL_GAP_PX + 0.8,
            "right bond must clear charge: gap={gap_from_o}, half_O={}",
            0.5 * o_adv
        );
        // Bare O with the same skeleton shortens less on the charge side.
        let atoms_bare = vec![
            AtomIn {
                x: -40.0,
                y: 0.0,
                label: None,
            },
            AtomIn {
                x: 0.0,
                y: 0.0,
                label: Some("O".into()),
            },
            AtomIn {
                x: 25.0,
                y: 0.0,
                label: None,
            },
        ];
        let (out_bare, _) = place_backbone(&atoms_bare, &bonds, FONT_PX, FaceStyle::Regular, 1.0);
        assert!(
            gap_from_o > out_bare[1].x1 + 0.5,
            "O+ gap {gap_from_o} should exceed bare O gap {}",
            out_bare[1].x1
        );
    }

    #[test]
    fn glca_does_not_flip() {
        let parts = split_label("GlcA");
        assert_eq!(compose_label(&parts, LabelSide::East), "GlcA");
        assert_eq!(compose_label(&parts, LabelSide::West), "GlcA");
    }

    #[test]
    fn side_from_neighbors() {
        assert_eq!(
            label_side((0.0, 0.0), &[(10.0, 0.0)]),
            LabelSide::West
        );
        assert_eq!(
            label_side((0.0, 0.0), &[(-10.0, 0.0)]),
            LabelSide::East
        );
        // Steep vertical, degree > 1 → N/S.
        assert_eq!(
            label_side((0.0, 0.0), &[(1.0, 10.0), (-1.0, 10.0)]),
            LabelSide::North
        );
    }

    #[test]
    fn oh_center_stays_on_atom() {
        let style = FaceStyle::Regular;
        let east = place_label("OH", 100.0, 50.0, LabelSide::East, FONT_PX, style);
        let west = place_label("OH", 100.0, 50.0, LabelSide::West, FONT_PX, style);
        assert_eq!(east.text, "OH");
        assert_eq!(west.text, "HO");
        assert!(!east.path_d.is_empty());
        assert!(east.path_d.contains('M'));
        let o_adv = advance_px("O", FONT_PX, style);
        assert!((east.origin_x + 0.5 * o_adv - 100.0).abs() < 1e-6);
        let h_adv = advance_px("H", FONT_PX, style);
        assert!((west.origin_x + h_adv + 0.5 * o_adv - 100.0).abs() < 1e-6);
        assert!((east.clearance - (0.5 * o_adv + LABEL_GAP_PX)).abs() < 1e-6);
        assert!(east.clearance < advance_px("OH", FONT_PX, style) * 0.5 + LABEL_GAP_PX - 0.1);
    }

    #[test]
    fn n_diagonal_clearance_uses_ink_support() {
        let parts = split_label("N");
        let (adv, ink) = center_glyph_ink_rel(&parts, LabelSide::East, FONT_PX, FaceStyle::Regular);
        let horiz = clearance_toward(adv, ink, 1.0, 0.0, 0.0);
        let diag = clearance_toward(
            adv,
            ink,
            std::f64::consts::FRAC_1_SQRT_2,
            std::f64::consts::FRAC_1_SQRT_2,
            0.0,
        );
        assert!((horiz - (0.5 * adv + LABEL_GAP_PX)).abs() < 1e-6);
        assert!(diag > horiz + 0.3);
    }

    #[test]
    fn backbone_weight_increases_bond_label_standoff() {
        use crate::metrics::label_weight_standoff_px;
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
        let (out1, labs1) = place_backbone(&atoms, &bonds, FONT_PX, FaceStyle::Regular, 1.0);
        let (out2, labs2) = place_backbone(&atoms, &bonds, FONT_PX, FaceStyle::Regular, 2.0);
        let c1 = labs1[1].as_ref().unwrap().clearance;
        let c2 = labs2[1].as_ref().unwrap().clearance;
        let extra = label_weight_standoff_px(2.0) - label_weight_standoff_px(1.0);
        assert!((c2 - c1 - extra).abs() < 1e-6);
        assert!((40.0 - out1[0].x2) < (40.0 - out2[0].x2) - 0.5);
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
        let (out, labels) = place_backbone(
            &atoms,
            &bonds,
            FONT_PX,
            FaceStyle::Regular,
            crate::metrics::WEIGHT_MIN,
        );
        assert!(labels[1].is_some());
        let lab = labels[1].as_ref().unwrap();
        assert_eq!(lab.text, "OH");
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
        let (out, labels) = place_backbone(
            &atoms,
            &bonds,
            FONT_PX,
            FaceStyle::Regular,
            crate::metrics::WEIGHT_MIN,
        );
        let lab = labels[1].as_ref().unwrap();
        let n_adv = advance_px("N", FONT_PX, FaceStyle::Regular);
        let gap = ((out[0].x2 - 30.0).powi(2) + (out[0].y2 - 30.0).powi(2)).sqrt();
        assert!(gap > 0.5 * n_adv + LABEL_GAP_PX + 0.3);
        assert!((lab.clearance - (0.5 * n_adv + LABEL_GAP_PX)).abs() < 1e-6);
    }

    #[test]
    fn star_label_emits_path() {
        let pl = place_label("*", 10.0, 10.0, LabelSide::East, FONT_PX, FaceStyle::Regular);
        assert_eq!(pl.text, "*");
        assert!(!pl.path_d.is_empty());
    }

    #[test]
    fn split_empty_charges_and_non_h_suffix() {
        let empty = split_label("  ");
        assert!(empty.center.is_empty());
        assert_eq!(empty.charge, 0);

        let fe = split_label("Fe3+");
        assert_eq!(fe.center, "Fe");
        assert_eq!(fe.charge, 3);
        assert!(fe.traveling.is_empty());

        let n2 = split_label("N2+");
        assert_eq!(n2.center, "N");
        assert_eq!(n2.charge, 2);

        let multi = split_label("O++");
        assert_eq!(multi.center, "O");
        assert_eq!(multi.charge, 2);

        // Trailing junk after H → whole body is center (NHAc).
        let nhac = split_label("NHAc");
        assert_eq!(nhac.center, "NHAc");
        assert!(nhac.traveling.is_empty());
    }

    #[test]
    fn label_side_degree0_and_vertical_degree1() {
        assert_eq!(
            label_side_for((0.0, 0.0), &[], Some("O")),
            LabelSide::West
        );
        assert_eq!(
            label_side_for((0.0, 0.0), &[], Some("C")),
            LabelSide::East
        );
        // Degree-1 steep vertical → forced East (RDKit).
        assert_eq!(
            label_side((0.0, 0.0), &[(0.0, 20.0)]),
            LabelSide::East
        );
    }

    #[test]
    fn label_side_degree3_keeps_north_south() {
        // Three neighbors with a near-vertical bond.
        let side = label_side(
            (0.0, 0.0),
            &[(10.0, 2.0), (-10.0, 2.0), (0.0, 15.0)],
        );
        assert_eq!(side, LabelSide::North);

        let south = label_side(
            (0.0, 0.0),
            &[(10.0, -2.0), (-10.0, -2.0), (0.0, -15.0)],
        );
        assert_eq!(south, LabelSide::South);
    }

    #[test]
    fn north_south_nh2_stacks_and_compose_charge() {
        let north = place_label("NH2", 50.0, 50.0, LabelSide::North, FONT_PX, FaceStyle::Regular);
        let south = place_label("NH2", 50.0, 50.0, LabelSide::South, FONT_PX, FaceStyle::Regular);
        assert!(north.path_d.contains('M'));
        assert!(south.path_d.contains('M'));
        assert_eq!(north.text, "NH₂");
        let ink = label_ink_shape(&north, FONT_PX, FaceStyle::Regular);
        assert!(ink.is_some());

        let parts = split_label("Fe2+");
        assert_eq!(compose_label(&parts, LabelSide::East), "Fe2+");
        let west_abbr = place_label("GlcA", 0.0, 0.0, LabelSide::West, FONT_PX, FaceStyle::Regular);
        assert!(!west_abbr.path_d.is_empty());
    }

    #[test]
    fn shorten_bond_collapse_and_oob_backbone() {
        let (x1, y1, x2, y2) = shorten_bond(0.0, 0.0, 5.0, 0.0, 4.0, 4.0);
        assert!((x1 - 2.5).abs() < 1e-9);
        assert!((x2 - 2.5).abs() < 1e-9);
        assert!((y1 - y2).abs() < 1e-9);

        let atoms = vec![AtomIn {
            x: 0.0,
            y: 0.0,
            label: Some("O".into()),
        }];
        let bonds = vec![BondIn { begin: 0, end: 9 }];
        let (out, labels) = place_backbone(&atoms, &bonds, FONT_PX, FaceStyle::Regular, 1.0);
        assert!(labels[0].is_some());
        assert_eq!(out[0].x1, 0.0);
        assert_eq!(out[0].x2, 0.0);
    }

    #[test]
    fn charged_west_oh_and_empty_label_skipped() {
        let pl = place_label("OH+", 10.0, 10.0, LabelSide::West, FONT_PX, FaceStyle::Regular);
        assert!(pl.text.contains('⁺') || pl.text.contains('+'));
        assert!(!pl.path_d.is_empty());

        let atoms = vec![
            AtomIn {
                x: 0.0,
                y: 0.0,
                label: Some("   ".into()),
            },
            AtomIn {
                x: 20.0,
                y: 0.0,
                label: None,
            },
        ];
        let (_, labels) = place_backbone(
            &atoms,
            &[BondIn { begin: 0, end: 1 }],
            FONT_PX,
            FaceStyle::Regular,
            1.0,
        );
        assert!(labels[0].is_none());
        assert!(labels[1].is_none());
    }

    #[test]
    fn script_digits_and_empty_place() {
        // Charge magnitude is plain digits + sign; H-counts stay subscript.
        let fe = split_label("Fe10+");
        assert_eq!(fe.charge, 10);
        assert_eq!(compose_label(&fe, LabelSide::East), "Fe10+");

        let nh9 = compose_label(&split_label("NH9"), LabelSide::East);
        assert!(nh9.contains('₉'));

        let nh3 = compose_label(&split_label("NH3"), LabelSide::East);
        assert!(nh3.contains('₃'));
        assert_eq!(compose_label(&split_label("Fe4+"), LabelSide::East), "Fe4+");

        // H-count subscripts (charges are plain, not raised digits).
        for (raw, needle) in [
            ("NH4", '₄'),
            ("NH5", '₅'),
            ("NH6", '₆'),
            ("NH7", '₇'),
            ("NH8", '₈'),
        ] {
            let t = compose_label(&split_label(raw), LabelSide::East);
            assert!(t.contains(needle), "{raw} → {t}");
        }
        for mag in 5..=9 {
            let raw = format!("Fe{mag}+");
            assert_eq!(
                compose_label(&split_label(&raw), LabelSide::East),
                format!("Fe{mag}+")
            );
        }

        let empty = place_label("", 0.0, 0.0, LabelSide::East, FONT_PX, FaceStyle::Regular);
        assert!(empty.path_d.is_empty());
        assert!(empty.text.is_empty());
    }

    #[test]
    fn degree3_near_vertical_bond_angles() {
        // ~85° and ~-85° bonds exercise the atan degree windows.
        let north = label_side(
            (0.0, 0.0),
            &[
                (1.0, 10.0),  // near vertical south neighbor sum → North
                (8.0, 1.0),
                (-8.0, 1.0),
            ],
        );
        assert_eq!(north, LabelSide::North);
        let south = label_side(
            (0.0, 0.0),
            &[
                (1.0, -10.0),
                (8.0, -1.0),
                (-8.0, -1.0),
            ],
        );
        assert_eq!(south, LabelSide::South);
    }
}
