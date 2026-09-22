//! Multipolygon ops via `i_overlay` (buffer, union, difference, xor).

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::stroke::offset::StrokeOffset;
use i_overlay::mesh::float::style::{LineCap, LineJoin, OutlineStyle, StrokeStyle};

use super::{capsule_polygon, circle_polygon, polygon_to_svg_d};

type Contour = Vec<[f64; 2]>;
/// One polygon: exterior first, then holes (i_overlay shape).
type OverlayShape = Vec<Contour>;

/// Multipolygon ink / halo geometry (Shapely stand-in).
#[derive(Clone, Debug, Default)]
pub struct Shape {
    shapes: Vec<OverlayShape>,
}

fn to_pt(p: (f64, f64)) -> [f64; 2] {
    [p.0, p.1]
}

fn from_pt(p: [f64; 2]) -> (f64, f64) {
    (p[0], p[1])
}

fn ring_to_contour(ring: &[(f64, f64)]) -> Contour {
    ring.iter().copied().map(to_pt).collect()
}

fn close_dedupe(contour: &mut Contour) {
    if contour.len() >= 2 {
        let a = contour[0];
        let b = *contour.last().unwrap();
        if (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9 {
            contour.pop();
        }
    }
}

fn signed_area(contour: &Contour) -> f64 {
    if contour.len() < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..contour.len() {
        let j = (i + 1) % contour.len();
        a += contour[i][0] * contour[j][1] - contour[j][0] * contour[i][1];
    }
    0.5 * a
}

/// Ensure exterior is CCW and holes CW (i_overlay convention).
fn orient_shape(mut shape: OverlayShape) -> OverlayShape {
    for (i, contour) in shape.iter_mut().enumerate() {
        close_dedupe(contour);
        let area = signed_area(contour);
        let want_ccw = i == 0;
        if want_ccw && area < 0.0 {
            contour.reverse();
        } else if !want_ccw && area > 0.0 {
            contour.reverse();
        }
    }
    shape
}

fn normalize(shapes: Vec<OverlayShape>) -> Vec<OverlayShape> {
    shapes.into_iter().map(orient_shape).filter(|s| !s.is_empty() && s[0].len() >= 3).collect()
}

impl Shape {
    pub fn empty() -> Self {
        Self { shapes: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
            || self
                .shapes
                .iter()
                .all(|s| s.is_empty() || s.iter().all(|c| c.len() < 3))
    }

    /// Single exterior ring (no holes).
    pub fn from_ring(ring: &[(f64, f64)]) -> Self {
        if ring.len() < 3 {
            return Self::empty();
        }
        let mut c = ring_to_contour(ring);
        close_dedupe(&mut c);
        Self {
            shapes: normalize(vec![vec![c]]),
        }
    }

    /// Even-odd / XOR fill of independent contours (font glyph counters).
    pub fn from_contours_evenodd(contours: &[Vec<(f64, f64)>]) -> Self {
        let mut parts: Vec<OverlayShape> = Vec::new();
        for ring in contours {
            if ring.len() < 3 {
                continue;
            }
            let mut c = ring_to_contour(ring);
            close_dedupe(&mut c);
            if c.len() < 3 {
                continue;
            }
            parts.push(vec![c]);
        }
        if parts.is_empty() {
            return Self::empty();
        }
        let empty: Vec<OverlayShape> = Vec::new();
        let out = parts.overlay(&empty, OverlayRule::Subject, FillRule::EvenOdd);
        Self {
            shapes: normalize(out),
        }
    }

    pub fn disk(cx: f64, cy: f64, radius: f64, quad_segs: u32) -> Self {
        let ring = circle_polygon(cx, cy, radius, quad_segs);
        Self::from_ring(&ring)
    }

    pub fn capsule(x1: f64, y1: f64, x2: f64, y2: f64, radius: f64) -> Self {
        let ring = capsule_polygon(x1, y1, x2, y2, radius, 8);
        Self::from_ring(&ring)
    }

    pub fn polyline_buffer(pts: &[(f64, f64)], radius: f64) -> Self {
        if pts.len() < 2 || radius <= 0.0 {
            return Self::empty();
        }
        if pts.len() == 2 {
            return Self::capsule(pts[0].0, pts[0].1, pts[1].0, pts[1].1, radius);
        }
        let path: Contour = pts.iter().copied().map(to_pt).collect();
        let style = StrokeStyle::new(radius * 2.0)
            .line_join(LineJoin::Round(radius.max(0.05) * 0.2))
            .start_cap(LineCap::Round(radius.max(0.05) * 0.2))
            .end_cap(LineCap::Round(radius.max(0.05) * 0.2));
        let shapes = path.stroke(style, false);
        Self {
            shapes: normalize(shapes),
        }
    }

    /// Shapely `MultiPoint(pts).buffer(radius)`.
    pub fn multipoint_buffer(pts: &[(f64, f64)], radius: f64) -> Self {
        if pts.is_empty() || radius <= 0.0 {
            return Self::empty();
        }
        let mut acc = Self::empty();
        for &(x, y) in pts {
            acc = acc.union(&Self::disk(x, y, radius, 8));
        }
        acc
    }

    pub fn annular(cx: f64, cy: f64, r: f64, stroke_width: f64) -> Self {
        if r <= 0.0 || stroke_width <= 0.0 {
            return Self::empty();
        }
        let outer = Self::disk(cx, cy, r + 0.5 * stroke_width, 16);
        let inner_r = r - 0.5 * stroke_width;
        if inner_r > 1e-6 {
            outer.difference(&Self::disk(cx, cy, inner_r, 16))
        } else {
            outer
        }
    }

    fn overlay_with(&self, other: &Self, rule: OverlayRule) -> Self {
        if self.is_empty() && other.is_empty() {
            return Self::empty();
        }
        let out = self
            .shapes
            .overlay(&other.shapes, rule, FillRule::EvenOdd);
        Self {
            shapes: normalize(out),
        }
    }

    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return other.clone();
        }
        if other.is_empty() {
            return self.clone();
        }
        self.overlay_with(other, OverlayRule::Union)
    }

    pub fn difference(&self, other: &Self) -> Self {
        if self.is_empty() {
            return Self::empty();
        }
        if other.is_empty() {
            return self.clone();
        }
        self.overlay_with(other, OverlayRule::Difference)
    }

    pub fn xor(&self, other: &Self) -> Self {
        self.overlay_with(other, OverlayRule::Xor)
    }

    /// Grow / shrink outline (`dist` may be negative).
    pub fn buffer(&self, dist: f64) -> Self {
        if self.is_empty() || dist.abs() < 1e-15 {
            return self.clone();
        }
        let join = LineJoin::Round(dist.abs().max(0.05) * 0.2);
        let style = OutlineStyle::new(dist).line_join(join);
        let mut out: Vec<OverlayShape> = Vec::new();
        for shape in &self.shapes {
            let grown = shape.outline(&style);
            out.extend(grown);
        }
        // Merge fragments.
        if out.len() <= 1 {
            return Self {
                shapes: normalize(out),
            };
        }
        let empty: Vec<OverlayShape> = Vec::new();
        let merged = out.overlay(&empty, OverlayRule::Subject, FillRule::NonZero);
        Self {
            shapes: normalize(merged),
        }
    }

    /// Punch interior rings back out of a grown halo (Shapely halo_from_shapes).
    pub fn halo(&self, dist: f64) -> Self {
        if self.is_empty() || dist <= 0.0 {
            return Self::empty();
        }
        let grown = self.buffer(dist);
        let counters = self.holes_as_shape();
        if counters.is_empty() {
            grown
        } else {
            grown.difference(&counters)
        }
    }

    fn holes_as_shape(&self) -> Self {
        let mut holes: Vec<OverlayShape> = Vec::new();
        for shape in &self.shapes {
            for hole in shape.iter().skip(1) {
                if hole.len() >= 3 {
                    holes.push(orient_shape(vec![hole.clone()]));
                }
            }
        }
        if holes.is_empty() {
            return Self::empty();
        }
        let empty: Vec<OverlayShape> = Vec::new();
        let out = holes.overlay(&empty, OverlayRule::Subject, FillRule::NonZero);
        Self {
            shapes: normalize(out),
        }
    }

    pub fn has_holes(&self) -> bool {
        self.shapes.iter().any(|s| s.len() > 1)
    }

    pub fn translate(&self, dx: f64, dy: f64) -> Self {
        let mut shapes = self.shapes.clone();
        for shape in &mut shapes {
            for contour in shape {
                for p in contour {
                    p[0] += dx;
                    p[1] += dy;
                }
            }
        }
        Self { shapes }
    }

    pub fn scale(&self, sx: f64, sy: f64, ox: f64, oy: f64) -> Self {
        let mut shapes = self.shapes.clone();
        for shape in &mut shapes {
            for contour in shape {
                for p in contour {
                    p[0] = ox + (p[0] - ox) * sx;
                    p[1] = oy + (p[1] - oy) * sy;
                }
            }
        }
        // Negative scale flips winding; restore exterior CCW / holes CW.
        Self {
            shapes: normalize(shapes),
        }
    }

    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut minx = f64::INFINITY;
        let mut miny = f64::INFINITY;
        let mut maxx = f64::NEG_INFINITY;
        let mut maxy = f64::NEG_INFINITY;
        let mut any = false;
        for shape in &self.shapes {
            for contour in shape {
                for p in contour {
                    any = true;
                    minx = minx.min(p[0]);
                    miny = miny.min(p[1]);
                    maxx = maxx.max(p[0]);
                    maxy = maxy.max(p[1]);
                }
            }
        }
        if any {
            Some((minx, miny, maxx, maxy))
        } else {
            None
        }
    }

    pub fn area(&self) -> f64 {
        let mut total = 0.0;
        for shape in &self.shapes {
            for (i, contour) in shape.iter().enumerate() {
                let a = signed_area(contour).abs();
                if i == 0 {
                    total += a;
                } else {
                    total -= a;
                }
            }
        }
        total.max(0.0)
    }

    pub fn centroid(&self) -> Option<(f64, f64)> {
        // Average of exterior ring centroids weighted by area.
        let mut cx = 0.0;
        let mut cy = 0.0;
        let mut w = 0.0;
        for shape in &self.shapes {
            if let Some(ext) = shape.first() {
                if ext.len() < 3 {
                    continue;
                }
                let a = signed_area(ext).abs();
                if a < 1e-18 {
                    continue;
                }
                let mut sx = 0.0;
                let mut sy = 0.0;
                for p in ext {
                    sx += p[0];
                    sy += p[1];
                }
                let n = ext.len() as f64;
                cx += (sx / n) * a;
                cy += (sy / n) * a;
                w += a;
            }
        }
        if w > 0.0 {
            Some((cx / w, cy / w))
        } else {
            None
        }
    }

    /// Point-in-polygon including holes (even-odd per shape, then any).
    pub fn contains(&self, x: f64, y: f64) -> bool {
        for shape in &self.shapes {
            let mut inside = false;
            for contour in shape {
                if point_in_ring(x, y, contour) {
                    inside = !inside;
                }
            }
            if inside {
                return true;
            }
        }
        false
    }

    pub fn to_svg_d(&self) -> String {
        let mut chunks = Vec::new();
        for shape in &self.shapes {
            for contour in shape {
                if contour.len() < 3 {
                    continue;
                }
                let ring: Vec<(f64, f64)> = contour.iter().copied().map(from_pt).collect();
                let d = polygon_to_svg_d(&ring);
                if !d.is_empty() {
                    chunks.push(d);
                }
            }
        }
        chunks.join(" ")
    }

    /// Length of the leftmost horizontal chord at `y` (stem-width helper).
    pub fn horizontal_span_at(&self, y: f64) -> f64 {
        let mut segs: Vec<(f64, f64)> = Vec::new();
        for shape in &self.shapes {
            // Only exterior for stem; approximate filled region via even-odd edges.
            for contour in shape {
                let n = contour.len();
                for i in 0..n {
                    let a = contour[i];
                    let b = contour[(i + 1) % n];
                    if (a[1] - y) * (b[1] - y) > 0.0 {
                        continue;
                    }
                    if (a[1] - b[1]).abs() < 1e-15 {
                        continue;
                    }
                    let t = (y - a[1]) / (b[1] - a[1]);
                    if !(0.0..=1.0).contains(&t) {
                        continue;
                    }
                    let x = a[0] + t * (b[0] - a[0]);
                    segs.push((x, x));
                }
            }
        }
        if segs.is_empty() {
            return 0.0;
        }
        // Collect crossing xs, sort, pair as intervals (even-odd).
        let mut xs: Vec<f64> = segs.iter().map(|(x, _)| *x).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // Unique near-duplicates
        let mut uniq = Vec::new();
        for x in xs {
            if uniq.last().map(|u: &f64| (x - *u).abs() > 1e-9).unwrap_or(true) {
                uniq.push(x);
            }
        }
        if uniq.len() < 2 {
            return 0.0;
        }
        // Leftmost filled interval
        uniq[1] - uniq[0]
    }

    /// Flatten polygons for Python: list of (exterior, holes...).
    pub fn polygons_rings(&self) -> Vec<(Vec<(f64, f64)>, Vec<Vec<(f64, f64)>>)> {
        let mut out = Vec::new();
        for shape in &self.shapes {
            if shape.is_empty() {
                continue;
            }
            let exterior: Vec<(f64, f64)> = shape[0].iter().copied().map(from_pt).collect();
            let holes: Vec<Vec<(f64, f64)>> = shape[1..]
                .iter()
                .map(|c| c.iter().copied().map(from_pt).collect())
                .collect();
            out.push((exterior, holes));
        }
        out
    }
}

fn point_in_ring(x: f64, y: f64, ring: &Contour) -> bool {
    // Ray casting
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let xi = ring[i][0];
        let yi = ring[i][1];
        let xj = ring[j][0];
        let yj = ring[j][1];
        let intersect = ((yi > y) != (yj > y))
            && (x < (xj - xi) * (y - yi) / (yj - yi + f64::EPSILON) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}
