//! Thin wrappers over `i_overlay` (boolean ops, simplify, outline / stroke).

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::i_shape::float::area::Area;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::stroke::offset::StrokeOffset;
use i_overlay::mesh::float::style::{LineCap, LineJoin, OutlineStyle, StrokeStyle};

use super::{capsule_polygon, circle_polygon, polygon_to_svg_d};

type Contour = Vec<[f64; 2]>;
type OverlayShape = Vec<Contour>;
type Shapes = Vec<OverlayShape>;

/// Multipolygon ink / halo geometry.
#[derive(Clone, Debug, Default)]
pub struct Shape {
    shapes: Shapes,
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
        Self::from_ring(&capsule_polygon(x1, y1, x2, y2, radius, 8))
    }

    pub fn polyline_buffer(pts: &[(f64, f64)], radius: f64) -> Self {
        if pts.len() < 2 || radius <= 0.0 {
            return Self::empty();
        }
        if pts.len() == 2 {
            return Self::capsule(pts[0].0, pts[0].1, pts[1].0, pts[1].1, radius);
        }
        let path: Contour = pts.iter().copied().map(pt).collect();
        let r = radius.max(0.05) * 0.2;
        let style = StrokeStyle::new(radius * 2.0)
            .line_join(LineJoin::Round(r))
            .start_cap(LineCap::Round(r))
            .end_cap(LineCap::Round(r));
        Self::wrap(path.stroke(style, false))
    }

    pub fn multipoint_buffer(pts: &[(f64, f64)], radius: f64) -> Self {
        pts.iter()
            .fold(Self::empty(), |acc, &(x, y)| acc.union(&Self::disk(x, y, radius, 8)))
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
        Self::wrap(
            self.shapes
                .overlay(&other.shapes, rule, FillRule::NonZero),
        )
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
        let join = LineJoin::Round((dist.abs() * 0.35).max(0.25));
        Self::wrap(
            self.shapes
                .outline(&OutlineStyle::new(dist).line_join(join)),
        )
    }

    /// `i_overlay::SimplifyShape` (collinear cleanup; holes stay valid).
    pub fn simplify(&self) -> Self {
        if self.is_empty() {
            return Self::empty();
        }
        Self::wrap(self.shapes.simplify_shape(FillRule::NonZero))
    }

    pub fn point_count(&self) -> usize {
        self.shapes.iter().flat_map(|s| s.iter()).map(|c| c.len()).sum()
    }

    /// White knockout: simplify, then outline offset.
    pub fn halo(&self, dist: f64) -> Self {
        if self.is_empty() || dist <= 0.0 {
            return Self::empty();
        }
        self.simplify().buffer(dist)
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
        self.shapes
            .iter()
            .flatten()
            .filter(|c| c.len() >= 3)
            .map(|c| {
                let ring: Vec<(f64, f64)> = c.iter().copied().map(xy).collect();
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
