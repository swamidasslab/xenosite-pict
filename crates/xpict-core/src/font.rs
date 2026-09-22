//! Bundled Liberation Sans outlines (`ttf-parser`).
//!
//! Replaces Python fontTools for advances, ink bounds, and glyph contours.
//! Contours are sampled (8 steps per curve) to match the old ContourPen, then
//! assembled with even-odd fill into [`crate::geom::Shape`].
//!
//! Chem scripts (H-counts, charges) are **fake** sub/superscripts: scale by
//! [`crate::metrics::SCRIPT_SCALE`], buffer the outline to restore stem weight,
//! and cache the result. Dummy ``*`` uses a custom star at [`STAR_FRAC`].

#![cfg(feature = "font")]

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ttf_parser::{Face, GlyphId, OutlineBuilder, Rect};

use crate::geom::Shape;
use crate::metrics::{FONT_STEM_EM, SCRIPT_SCALE, STAR_FRAC};

const BEZIER_STEPS: usize = 8;

const FONT_REGULAR: &[u8] =
    include_bytes!("../../../src/xpict/data/fonts/LiberationSans-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../../../src/xpict/data/fonts/LiberationSans-Bold.ttf");
const FONT_ITALIC: &[u8] =
    include_bytes!("../../../src/xpict/data/fonts/LiberationSans-Italic.ttf");
const FONT_BOLD_ITALIC: &[u8] =
    include_bytes!("../../../src/xpict/data/fonts/LiberationSans-BoldItalic.ttf");

/// Liberation Sans face variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FaceStyle {
    #[default]
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FaceStyle {
    pub fn from_flags(bold: bool, italic: bool) -> Self {
        match (bold, italic) {
            (true, true) => Self::BoldItalic,
            (true, false) => Self::Bold,
            (false, true) => Self::Italic,
            (false, false) => Self::Regular,
        }
    }

    fn bytes(self) -> &'static [u8] {
        match self {
            Self::Regular => FONT_REGULAR,
            Self::Bold => FONT_BOLD,
            Self::Italic => FONT_ITALIC,
            Self::BoldItalic => FONT_BOLD_ITALIC,
        }
    }
}

/// Typographic constants in em units (font +Y up).
#[derive(Debug, Clone, Copy)]
pub struct FaceMetrics {
    pub upem: f64,
    pub ascent: f64,
    pub descent: f64,
    pub line_gap: f64,
    pub cap_height: f64,
    pub x_height: f64,
    pub stem_em: f64,
}

/// One character metrics in em units (font +Y up).
#[derive(Debug, Clone, Copy)]
pub struct GlyphMetrics {
    pub advance: f64,
    pub ink_xmin: Option<f64>,
    pub ink_ymin: Option<f64>,
    pub ink_xmax: Option<f64>,
    pub ink_ymax: Option<f64>,
}

impl GlyphMetrics {
    pub fn has_ink(&self) -> bool {
        self.ink_xmin.is_some()
    }

    pub fn ink_width(&self) -> f64 {
        match (self.ink_xmin, self.ink_xmax) {
            (Some(a), Some(b)) => b - a,
            _ => 0.0,
        }
    }

    pub fn ink_height(&self) -> f64 {
        match (self.ink_ymin, self.ink_ymax) {
            (Some(a), Some(b)) => b - a,
            _ => 0.0,
        }
    }
}

fn face(style: FaceStyle) -> Face<'static> {
    Face::parse(style.bytes(), 0).expect("bundled Liberation Sans TTF")
}

/// Face metrics for a Liberation Sans style.
pub fn face_metrics(style: FaceStyle) -> FaceMetrics {
    let f = face(style);
    let upem = f64::from(f.units_per_em());
    let mut cap = f.capital_height().map(f64::from).unwrap_or(0.0);
    let mut xh = f.x_height().map(f64::from).unwrap_or(0.0);
    if cap <= 0.0 {
        if let Some(g) = glyph_metrics('H', style) {
            cap = g.ink_ymax.unwrap_or(0.0);
        }
    }
    if xh <= 0.0 {
        if let Some(g) = glyph_metrics('x', style) {
            xh = g.ink_ymax.unwrap_or(0.0);
        }
    }
    FaceMetrics {
        upem,
        ascent: f64::from(f.ascender()),
        descent: f64::from(f.descender()),
        line_gap: f64::from(f.line_gap()),
        cap_height: cap,
        x_height: xh,
        stem_em: measure_stem_em(style),
    }
}

/// Metrics for a single Unicode scalar.
pub fn glyph_metrics(ch: char, style: FaceStyle) -> Option<GlyphMetrics> {
    let f = face(style);
    let upem = f64::from(f.units_per_em());
    let Some(gid) = f.glyph_index(ch) else {
        return Some(GlyphMetrics {
            advance: 0.5 * upem,
            ink_xmin: None,
            ink_ymin: None,
            ink_xmax: None,
            ink_ymax: None,
        });
    };
    let advance = f64::from(f.glyph_hor_advance(gid).unwrap_or(0));
    let ink = f.glyph_bounding_box(gid).map(rect_to_ink);
    Some(GlyphMetrics {
        advance,
        ink_xmin: ink.map(|r| r.0),
        ink_ymin: ink.map(|r| r.1),
        ink_xmax: ink.map(|r| r.2),
        ink_ymax: ink.map(|r| r.3),
    })
}

fn rect_to_ink(r: Rect) -> (f64, f64, f64, f64) {
    (
        f64::from(r.x_min),
        f64::from(r.y_min),
        f64::from(r.x_max),
        f64::from(r.y_max),
    )
}

struct ContourCollector {
    contours: Vec<Vec<(f64, f64)>>,
    cur: Vec<(f64, f64)>,
}

impl ContourCollector {
    fn new() -> Self {
        Self {
            contours: Vec::new(),
            cur: Vec::new(),
        }
    }

    fn finish_contour(&mut self) {
        if self.cur.len() >= 3 {
            self.contours.push(std::mem::take(&mut self.cur));
        } else {
            self.cur.clear();
        }
    }
}

impl OutlineBuilder for ContourCollector {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish_contour();
        self.cur = vec![(f64::from(x), f64::from(y))];
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.cur.push((f64::from(x), f64::from(y)));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x0, y0) = *self.cur.last().expect("quad_to without move_to");
        let (x1, y1, x, y) = (f64::from(x1), f64::from(y1), f64::from(x), f64::from(y));
        for i in 1..=BEZIER_STEPS {
            let t = i as f64 / BEZIER_STEPS as f64;
            let mt = 1.0 - t;
            self.cur.push((
                mt * mt * x0 + 2.0 * mt * t * x1 + t * t * x,
                mt * mt * y0 + 2.0 * mt * t * y1 + t * t * y,
            ));
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x0, y0) = *self.cur.last().expect("curve_to without move_to");
        let (x1, y1, x2, y2, x, y) = (
            f64::from(x1),
            f64::from(y1),
            f64::from(x2),
            f64::from(y2),
            f64::from(x),
            f64::from(y),
        );
        for i in 1..=BEZIER_STEPS {
            let t = i as f64 / BEZIER_STEPS as f64;
            let mt = 1.0 - t;
            self.cur.push((
                mt * mt * mt * x0
                    + 3.0 * mt * mt * t * x1
                    + 3.0 * mt * t * t * x2
                    + t * t * t * x,
                mt * mt * mt * y0
                    + 3.0 * mt * mt * t * y1
                    + 3.0 * mt * t * t * y2
                    + t * t * t * y,
            ));
        }
    }

    fn close(&mut self) {
        self.finish_contour();
    }
}

fn outline_glyph(face: &Face<'_>, gid: GlyphId) -> Vec<Vec<(f64, f64)>> {
    let mut collector = ContourCollector::new();
    let _ = face.outline_glyph(gid, &mut collector);
    collector.finish_contour();
    collector.contours
}

/// Outline plain Unicode in font space (+Y up); return (shape, advance_em).
pub fn outline_run_em(text: &str, style: FaceStyle) -> (Option<Shape>, f64) {
    if text.is_empty() {
        return (None, 0.0);
    }
    let f = face(style);
    let upem = f64::from(f.units_per_em());
    let mut contours: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut pen_x = 0.0;
    for ch in text.chars() {
        let Some(gid) = f.glyph_index(ch) else {
            pen_x += 0.5 * upem;
            continue;
        };
        for mut contour in outline_glyph(&f, gid) {
            for p in &mut contour {
                p.0 += pen_x;
            }
            contours.push(contour);
        }
        pen_x += f64::from(f.glyph_hor_advance(gid).unwrap_or(0));
    }
    if contours.is_empty() {
        return (None, pen_x);
    }
    let shape = Shape::from_contours_evenodd(&contours);
    if shape.is_empty() {
        (None, pen_x)
    } else {
        (Some(shape), pen_x)
    }
}

/// Sub / super / normal draw role for chem label glyphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptRole {
    Normal,
    Subscript,
    Superscript,
}

/// One chem-label glyph with an optional script role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChemGlyph {
    pub ch: char,
    pub role: ScriptRole,
}

fn script_cache() -> &'static Mutex<HashMap<(char, FaceStyle, ScriptRole), (Option<Shape>, f64)>> {
    static CACHE: OnceLock<Mutex<HashMap<(char, FaceStyle, ScriptRole), (Option<Shape>, f64)>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Outline one character in em space (+Y up), optionally as a fake script.
///
/// Scripts: scale by [`SCRIPT_SCALE`], then buffer so stem weight stays close
/// to the full-size glyph. Results are memoized.
pub fn outline_glyph_em(ch: char, style: FaceStyle, role: ScriptRole) -> (Option<Shape>, f64) {
    if let Ok(guard) = script_cache().lock() {
        if let Some(hit) = guard.get(&(ch, style, role)) {
            return hit.clone();
        }
    }
    let built = outline_glyph_em_uncached(ch, style, role);
    if let Ok(mut guard) = script_cache().lock() {
        guard.insert((ch, style, role), built.clone());
    }
    built
}

fn outline_glyph_em_uncached(
    ch: char,
    style: FaceStyle,
    role: ScriptRole,
) -> (Option<Shape>, f64) {
    let f = face(style);
    let upem = f64::from(f.units_per_em());
    let Some(gid) = f.glyph_index(ch) else {
        return (None, 0.5 * upem);
    };
    let advance = f64::from(f.glyph_hor_advance(gid).unwrap_or(0));
    let contours = outline_glyph(&f, gid);
    if contours.is_empty() {
        return (None, advance);
    }
    let shape = Shape::from_contours_evenodd(&contours);
    if shape.is_empty() {
        return (None, advance);
    }
    match role {
        ScriptRole::Normal => (Some(shape), advance),
        ScriptRole::Subscript | ScriptRole::Superscript => {
            // Scale about the glyph origin; advance tracks the smaller width.
            let scaled = shape.scale(SCRIPT_SCALE, SCRIPT_SCALE, 0.0, 0.0);
            // Inflate so 0.66× glyphs keep roughly full stem weight.
            let grow = 0.5 * (1.0 - SCRIPT_SCALE) * FONT_STEM_EM * upem;
            let thick = if grow > 0.05 {
                scaled.buffer(grow)
            } else {
                scaled
            };
            (Some(thick), advance * SCRIPT_SCALE)
        }
    }
}

/// Custom five-point star in em units (centered on advance, mid cap-height).
///
/// Liberation's asterisk is too small / high for R-group markers. Size is
/// driven by [`STAR_FRAC`] × cap-height (outer diameter).
pub fn outline_star_em(style: FaceStyle) -> (Shape, f64) {
    let face_m = face_metrics(style);
    let height = STAR_FRAC * face_m.cap_height;
    let outer = 0.5 * height;
    let inner = outer * 0.38;
    let pad = 0.12 * face_m.cap_height;
    let advance = height + pad;
    let cx = 0.5 * advance;
    let cy = 0.5 * face_m.cap_height;
    let mut ring = Vec::with_capacity(10);
    for i in 0..10 {
        let a = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::PI / 5.0;
        let r = if i % 2 == 0 { outer } else { inner };
        ring.push((cx + r * a.cos(), cy + r * a.sin()));
    }
    let shape = Shape::from_ring(&ring);
    (shape, advance)
}

/// Outline a chem glyph run in em space (+Y up). Returns `(shape, advance_em)`.
///
/// Subscripts drop by ~⅓ cap-height; superscripts rise by ~½ (RDKit-ish).
pub fn outline_chem_run_em(glyphs: &[ChemGlyph], style: FaceStyle) -> (Option<Shape>, f64) {
    if glyphs.is_empty() {
        return (None, 0.0);
    }
    // Lone ``*`` → custom star.
    if glyphs.len() == 1 && glyphs[0].ch == '*' && glyphs[0].role == ScriptRole::Normal {
        let (s, adv) = outline_star_em(style);
        return (if s.is_empty() { None } else { Some(s) }, adv);
    }
    let face_m = face_metrics(style);
    let sub_dy = -0.33 * face_m.cap_height;
    let super_dy = 0.50 * face_m.cap_height;
    let mut pen_x = 0.0;
    let mut acc: Option<Shape> = None;
    for g in glyphs {
        let (piece, adv) = if g.ch == '*' && g.role == ScriptRole::Normal {
            let (s, a) = outline_star_em(style);
            (Some(s), a)
        } else {
            outline_glyph_em(g.ch, style, g.role)
        };
        let dy = match g.role {
            ScriptRole::Normal => 0.0,
            ScriptRole::Subscript => sub_dy,
            ScriptRole::Superscript => super_dy,
        };
        if let Some(p) = piece.filter(|s| !s.is_empty()) {
            let placed = p.translate(pen_x, dy);
            acc = Some(match acc.take() {
                Some(a) => a.union(&placed),
                None => placed,
            });
        }
        pen_x += adv;
    }
    (acc, pen_x)
}

/// Place a chem glyph run in SVG space (+Y down) with baseline at `y`.
///
/// `anchor`: ``start`` / ``middle`` / ``end`` on the advance box.
pub fn compile_chem_shapes(
    glyphs: &[ChemGlyph],
    x: f64,
    y: f64,
    font_size: f64,
    anchor: &str,
    style: FaceStyle,
) -> Option<Shape> {
    if glyphs.is_empty() {
        return None;
    }
    let face_m = face_metrics(style);
    let scale = font_size / face_m.upem;
    let (geom_em, advance_em) = outline_chem_run_em(glyphs, style);
    let advance = advance_em * scale;
    let origin_x = match anchor {
        "end" => x - advance,
        "start" | "left" => x,
        _ => x - 0.5 * advance,
    };
    let geom_em = geom_em?;
    // Scripts: RDKit shifts sub down / super up by ~½ of the preceding glyph height.
    // We bake a vertical nudge per glyph when assembling horizontal runs in labels.
    let geom = geom_em
        .scale(scale, -scale, 0.0, 0.0)
        .translate(origin_x, y);
    if geom.is_empty() {
        None
    } else {
        Some(geom)
    }
}

/// Compile plain text to SVG-space glyph geometry (+Y down).
pub fn compile_text_shapes(
    text: &str,
    x: f64,
    y: f64,
    font_size: f64,
    anchor: &str,
    style: FaceStyle,
) -> Option<Shape> {
    if text.is_empty() {
        return None;
    }
    let glyphs: Vec<ChemGlyph> = text
        .chars()
        .map(|ch| ChemGlyph {
            ch,
            role: ScriptRole::Normal,
        })
        .collect();
    compile_chem_shapes(&glyphs, x, y, font_size, anchor, style)
}

fn measure_stem_em(style: FaceStyle) -> f64 {
    let f = face(style);
    let upem = f64::from(f.units_per_em());
    let Some(gid) = f.glyph_index('H') else {
        return 0.0933;
    };
    let contours = outline_glyph(&f, gid);
    if contours.is_empty() {
        return 0.0933;
    }
    let geom = Shape::from_contours_evenodd(&contours);
    let Some((_minx, miny, _maxx, maxy)) = geom.bounds() else {
        return 0.0933;
    };
    let y = miny + 0.75 * (maxy - miny);
    let span = geom.horizontal_span_at(y);
    if span <= 0.0 {
        0.0933
    } else {
        span / upem
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{FONT_PX, FONT_STEM_EM, SCRIPT_SCALE, STAR_FRAC};

    #[test]
    fn liberation_face_metrics() {
        let m = face_metrics(FaceStyle::Regular);
        assert_eq!(m.upem, 2048.0);
        assert!((m.ascent - 1854.0).abs() < 1e-6);
        assert!((m.descent - -434.0).abs() < 1e-6);
        assert!((m.stem_em - FONT_STEM_EM).abs() / FONT_STEM_EM < 0.05);
        assert!(m.cap_height > 0.5 * m.upem);
    }

    #[test]
    fn glyph_h_and_o() {
        let h = glyph_metrics('H', FaceStyle::Regular).unwrap();
        assert!((h.advance - 1479.0).abs() < 1e-6);
        assert!(h.has_ink());
        assert!(h.ink_height() > 1000.0);
        let o = glyph_metrics('O', FaceStyle::Regular).unwrap();
        assert!(o.ink_width() > o.advance * 0.5);
    }

    #[test]
    fn outline_o_has_hole() {
        let (shape, adv) = outline_run_em("O", FaceStyle::Regular);
        let shape = shape.expect("O outline");
        assert!(shape.has_holes());
        assert!(adv > 0.0);
        assert!(!shape.contains(
            shape.centroid().unwrap().0,
            shape.centroid().unwrap().1
        ));
    }

    #[test]
    fn compile_svg_space_label() {
        let shape = compile_text_shapes("OH", 100.0, 50.0, FONT_PX, "middle", FaceStyle::Regular)
            .expect("OH");
        assert!(!shape.is_empty());
        let d = shape.to_svg_d();
        assert!(d.contains('M') && d.contains('Z'));
    }

    #[test]
    fn script_glyph_is_smaller_but_thickened() {
        let (full, adv_full) = outline_glyph_em('2', FaceStyle::Regular, ScriptRole::Normal);
        let (sub, adv_sub) = outline_glyph_em('2', FaceStyle::Regular, ScriptRole::Subscript);
        let full = full.expect("2");
        let sub = sub.expect("2 sub");
        assert!(adv_sub < adv_full * 0.75);
        assert!(sub.area() > 0.0);
        // Buffered script should not collapse to a speck.
        assert!(sub.area() > full.area() * SCRIPT_SCALE * SCRIPT_SCALE * 0.5);
    }

    #[test]
    fn star_is_larger_than_asterisk_advance() {
        let (_ast, adv_ast) = outline_run_em("*", FaceStyle::Regular);
        let (star, adv_star) = outline_star_em(FaceStyle::Regular);
        assert!(adv_star > adv_ast * 1.2);
        let face = face_metrics(FaceStyle::Regular);
        let (_x0, y0, _x1, y1) = star.bounds().expect("star ink");
        let height = y1 - y0;
        // Custom star targets STAR_FRAC × cap-height; allow simplify slack.
        assert!(height > 1.4 * face.cap_height);
        assert!(height < 1.9 * face.cap_height);
        let _ = STAR_FRAC;
    }
}
