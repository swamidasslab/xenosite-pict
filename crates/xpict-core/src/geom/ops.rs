//! Thin wrappers over `i_overlay` (boolean ops, simplify, outline / stroke).

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::i_shape::float::area::Area;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::stroke::offset::StrokeOffset;
use i_overlay::mesh::float::style::{LineCap, LineJoin, OutlineStyle, StrokeStyle};

use super::{
    capsule_polygon, circle_polygon, douglas_peucker_closed, polygon_to_svg_d, SVG_PATH_EPS_PX,
};
use std::f64::consts::FRAC_PI_6;

/// Max turn (radians) for outline/`buffer` round joins.
///
/// `π/6` (30°) left chem-label bowls faceted after weight grow; ~8° keeps O/C/G
/// curved without ballooning path size the way sub-degree joins do.
const OUTLINE_ROUND_RAD: f64 = 0.14;

type Contour = Vec<[f64; 2]>;
type OverlayShape = Vec<Contour>;
type Shapes = Vec<OverlayShape>;

/// Multipolygon ink / halo geometry.
#[derive(Clone, Debug, Default)]
pub struct Shape {
    shapes: Shapes,
}

/// Output outline tolerance: points closer than this to the drawn outline
/// are below the 0.01 path precision after a 2× zoom, so they only cost bytes.
const SVG_THIN_PX: f64 = 0.02;

/// Halo outline tolerance: well under a hairline on a half-opacity knockout.
const HALO_THIN_PX: f64 = 0.2;

/// Douglas–Peucker on a closed ring: split at the point farthest from the
/// first, thin each half, and rejoin.
fn thin_ring(c: &Contour, tol: f64) -> Contour {
    if c.len() <= 4 {
        return c.clone();
    }
    let d2 = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2);
    let far = (1..c.len())
        .max_by(|&i, &j| d2(c[0], c[i]).total_cmp(&d2(c[0], c[j])))
        .unwrap_or(c.len() / 2);
    let mut a: Contour = c[..=far].to_vec();
    let mut b: Contour = c[far..].to_vec();
    b.push(c[0]);
    a = thin_open(&a, tol);
    b = thin_open(&b, tol);
    a.pop();
    b.pop();
    a.extend(b);
    a
}

fn thin_open(pts: &[[f64; 2]], tol: f64) -> Contour {
    let n = pts.len();
    if n <= 2 {
        return pts.to_vec();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((i, j)) = stack.pop() {
        let (a, b) = (pts[i], pts[j]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx.hypot(dy);
        let mut best = (0.0, i);
        for (k, p) in pts.iter().enumerate().take(j).skip(i + 1) {
            let d = if len < 1e-12 {
                (p[0] - a[0]).hypot(p[1] - a[1])
            } else {
                (dx * (a[1] - p[1]) - dy * (a[0] - p[0])).abs() / len
            };
            if d > best.0 {
                best = (d, k);
            }
        }
        if best.0 > tol {
            keep[best.1] = true;
            stack.push((i, best.1));
            stack.push((best.1, j));
        }
    }
    pts.iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| *p)
        .collect()
}

fn pt(p: (f64, f64)) -> [f64; 2] {
    [p.0, p.1]
}

fn xy(p: [f64; 2]) -> (f64, f64) {
    (p[0], p[1])
}

impl Shape {
    fn wrap(shapes: Shapes) -> Self {
        Self { shapes }
    }

    pub fn empty() -> Self {
        Self::wrap(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty() || self.shapes.iter().all(|s| s.is_empty() || s[0].len() < 3)
    }

    /// Single exterior ring (lib-simplify cleans winding / collinear).
    pub fn from_ring(ring: &[(f64, f64)]) -> Self {
        if ring.len() < 3 {
            return Self::empty();
        }
        let c: Contour = ring.iter().copied().map(pt).collect();
        Self::wrap(c.simplify_shape(FillRule::NonZero))
    }

    /// Independent contours → even-odd / XOR shape (glyph counters).
    pub fn from_contours_evenodd(contours: &[Vec<(f64, f64)>]) -> Self {
        let parts: Vec<Contour> = contours
            .iter()
            .filter(|r| r.len() >= 3)
            .map(|r| r.iter().copied().map(pt).collect())
            .collect();
        if parts.is_empty() {
            return Self::empty();
        }
        Self::wrap(parts.simplify_shape(FillRule::EvenOdd))
    }

    pub fn disk(cx: f64, cy: f64, radius: f64, quad_segs: u32) -> Self {
        Self::from_ring(&circle_polygon(cx, cy, radius, quad_segs))
    }

    pub fn capsule(x1: f64, y1: f64, x2: f64, y2: f64, radius: f64) -> Self {
        // 4 quad-segs (~8 arc samples / cap) is plenty for bond-width capsules.
        Self::from_ring(&capsule_polygon(x1, y1, x2, y2, radius, 4))
    }

    pub fn polyline_buffer(pts: &[(f64, f64)], radius: f64) -> Self {
        if pts.len() < 2 || radius <= 0.0 {
            return Self::empty();
        }
        if pts.len() == 2 {
            return Self::capsule(pts[0].0, pts[0].1, pts[1].0, pts[1].1, radius);
        }
        let path: Contour = pts.iter().copied().map(pt).collect();
        // `Round` takes a max turn angle (radians), not a distance — ~30° keeps
        // round caps smooth without emitting dozens of micro-segments.
        let ang = FRAC_PI_6;
        let style = StrokeStyle::new(radius * 2.0)
            .line_join(LineJoin::Round(ang))
            .start_cap(LineCap::Round(ang))
            .end_cap(LineCap::Round(ang));
        Self::wrap(path.stroke(style, false)).decimate(SVG_PATH_EPS_PX)
    }

    pub fn multipoint_buffer(pts: &[(f64, f64)], radius: f64) -> Self {
        pts.iter().fold(Self::empty(), |acc, &(x, y)| {
            acc.union(&Self::disk(x, y, radius, 8))
        })
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
        Self::wrap(self.shapes.overlay(&other.shapes, rule, FillRule::NonZero))
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

    /// Outline offset (`dist` may be negative).
    pub fn buffer(&self, dist: f64) -> Self {
        if self.is_empty() || dist.abs() < 1e-15 {
            return self.clone();
        }
        // Angle (radians), not px. Finer than stroke joins: label weight-grow
        // buffers glyph outlines, and 30° turns painted O as a polygon.
        let join = LineJoin::Round(OUTLINE_ROUND_RAD);
        Self::wrap(
            self.shapes
                .outline(&OutlineStyle::new(dist).line_join(join)),
        )
        .simplify()
        .decimate(SVG_PATH_EPS_PX)
    }

    /// `i_overlay::SimplifyShape` (collinear cleanup; holes stay valid).
    pub fn simplify(&self) -> Self {
        if self.is_empty() {
            return Self::empty();
        }
        Self::wrap(self.shapes.simplify_shape(FillRule::NonZero))
    }

    /// Drop near-collinear vertices for SVG / paint (topology via closed RDP).
    pub fn decimate(&self, eps_px: f64) -> Self {
        if self.is_empty() || eps_px <= 0.0 {
            return self.clone();
        }
        let mut shapes = Vec::with_capacity(self.shapes.len());
        for shape in &self.shapes {
            let mut parts = Vec::with_capacity(shape.len());
            for contour in shape {
                if contour.len() < 3 {
                    continue;
                }
                let ring: Vec<(f64, f64)> = contour.iter().copied().map(xy).collect();
                let simplified = douglas_peucker_closed(&ring, eps_px);
                if simplified.len() >= 3 {
                    parts.push(simplified.into_iter().map(pt).collect());
                }
            }
            if !parts.is_empty() {
                shapes.push(parts);
            }
        }
        Self::wrap(shapes)
    }

    pub fn point_count(&self) -> usize {
        self.shapes
            .iter()
            .flat_map(|s| s.iter())
            .map(|c| c.len())
            .sum()
    }

    /// White knockout: simplify, then outline offset, then thin the round
    /// joins (they come back with sub-0.1px steps that only cost bytes).
    pub fn halo(&self, dist: f64) -> Self {
        if self.is_empty() || dist <= 0.0 {
            return Self::empty();
        }
        self.simplify().buffer(dist).thin(HALO_THIN_PX)
    }

    /// Drop contour points within `tol` of the outline (Douglas–Peucker).
    pub fn thin(&self, tol: f64) -> Self {
        Self::wrap(
            self.shapes
                .iter()
                .map(|s| {
                    s.iter()
                        .map(|c| thin_ring(c, tol))
                        .filter(|c| c.len() >= 3)
                        .collect::<OverlayShape>()
                })
                .filter(|s: &OverlayShape| !s.is_empty())
                .collect(),
        )
    }

    pub fn has_holes(&self) -> bool {
        self.shapes.iter().any(|s| s.len() > 1)
    }

    pub fn translate(&self, dx: f64, dy: f64) -> Self {
        let mut shapes = self.shapes.clone();
        for p in shapes.iter_mut().flatten().flatten() {
            p[0] += dx;
            p[1] += dy;
        }
        Self::wrap(shapes)
    }

    pub fn scale(&self, sx: f64, sy: f64, ox: f64, oy: f64) -> Self {
        let mut shapes = self.shapes.clone();
        for p in shapes.iter_mut().flatten().flatten() {
            p[0] = ox + (p[0] - ox) * sx;
            p[1] = oy + (p[1] - oy) * sy;
        }
        // Negative scale flips winding — lib simplify restores topology.
        Self::wrap(shapes).simplify()
    }

    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut iter = self.shapes.iter().flatten().flatten();
        let first = iter.next()?;
        let (mut minx, mut miny, mut maxx, mut maxy) = (first[0], first[1], first[0], first[1]);
        for p in iter {
            minx = minx.min(p[0]);
            miny = miny.min(p[1]);
            maxx = maxx.max(p[0]);
            maxy = maxy.max(p[1]);
        }
        Some((minx, miny, maxx, maxy))
    }

    pub fn area(&self) -> f64 {
        self.shapes.area().abs()
    }

    pub fn centroid(&self) -> Option<(f64, f64)> {
        let (minx, miny, maxx, maxy) = self.bounds()?;
        Some(((minx + maxx) * 0.5, (miny + maxy) * 0.5))
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        // Even-odd over all contours in each shape (holes flip).
        self.shapes.iter().any(|shape| {
            shape
                .iter()
                .filter(|c| c.len() >= 3)
                .fold(false, |inside, c| inside ^ point_in_ring(x, y, c))
        })
    }

    pub fn to_svg_d(&self) -> String {
        // Decimate at export so callers that skip [`Self::decimate`] still
        // emit compact path data (halo / glyph outlines were multi-MB).
        self.decimate(SVG_PATH_EPS_PX)
            .shapes
            .iter()
            .flatten()
            .filter(|c| c.len() >= 3)
            .map(|c| {
                let ring: Vec<(f64, f64)> = thin_ring(c, SVG_THIN_PX).into_iter().map(xy).collect();
                polygon_to_svg_d(&ring)
            })
            .filter(|d| !d.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Leftmost horizontal chord length at `y` (stem-width helper).
    pub fn horizontal_span_at(&self, y: f64) -> f64 {
        let mut xs = Vec::new();
        for contour in self.shapes.iter().flatten() {
            let n = contour.len();
            for i in 0..n {
                let a = contour[i];
                let b = contour[(i + 1) % n];
                if (a[1] - y) * (b[1] - y) > 0.0 || (a[1] - b[1]).abs() < 1e-15 {
                    continue;
                }
                let t = (y - a[1]) / (b[1] - a[1]);
                if (0.0..=1.0).contains(&t) {
                    xs.push(a[0] + t * (b[0] - a[0]));
                }
            }
        }
        if xs.len() < 2 {
            return 0.0;
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        xs.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
        if xs.len() < 2 {
            0.0
        } else {
            xs[1] - xs[0]
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn polygons_rings(&self) -> Vec<(Vec<(f64, f64)>, Vec<Vec<(f64, f64)>>)> {
        self.shapes
            .iter()
            .filter(|s| !s.is_empty())
            .map(|shape| {
                let exterior: Vec<(f64, f64)> = shape[0].iter().copied().map(xy).collect();
                let holes: Vec<Vec<(f64, f64)>> = shape[1..]
                    .iter()
                    .map(|c| c.iter().copied().map(xy).collect())
                    .collect();
                (exterior, holes)
            })
            .collect()
    }
}

fn point_in_ring(x: f64, y: f64, ring: &Contour) -> bool {
    let n = ring.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (ring[i][0], ring[i][1]);
        let (xj, yj) = (ring[j][0], ring[j][1]);
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi + f64::EPSILON) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod thin_tests {
    use super::*;

    #[test]
    fn thin_ring_drops_near_collinear_points_keeps_corners() {
        // A square with a dense, slightly wobbly top edge.
        let mut c: Contour = (0..=100)
            .map(|i| [i as f64 * 0.1, (i % 2) as f64 * 0.01])
            .collect();
        c.extend([[10.0, 10.0], [0.0, 10.0]]);
        let t = thin_ring(&c, 0.02);
        assert_eq!(t.len(), 4, "{t:?}");
        for corner in [[0.0, 0.0], [10.0, 10.0], [0.0, 10.0]] {
            assert!(t.contains(&corner), "lost {corner:?}: {t:?}");
        }
    }
}
