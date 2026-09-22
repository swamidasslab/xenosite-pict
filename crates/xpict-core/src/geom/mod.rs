//! Geometry for halos and ink (Shapely stand-in).
//!
//! Capsule/disk helpers always available. Full multipolygon buffer / boolean
//! ops live behind the `geom` feature (`i_overlay`) — enable for Python so
//! Shapely can leave the runtime dependency set.

use std::f64::consts::{PI, TAU};

/// Closed polygon as SVG path `d` (absolute M/L/Z).
pub fn polygon_to_svg_d(ring: &[(f64, f64)]) -> String {
    if ring.len() < 3 {
        return String::new();
    }
    let mut out = format!("M {:.2} {:.2}", ring[0].0, ring[0].1);
    for &(x, y) in &ring[1..] {
        out.push_str(&format!(" L {:.2} {:.2}", x, y));
    }
    out.push_str(" Z");
    out
}

/// Approximate a circle as a closed polygon (Shapely `quad_segs` style).
pub fn circle_polygon(cx: f64, cy: f64, radius: f64, quad_segs: u32) -> Vec<(f64, f64)> {
    if radius <= 0.0 {
        return Vec::new();
    }
    let segs = quad_segs.max(3) as usize;
    (0..segs)
        .map(|i| {
            let t = TAU * i as f64 / segs as f64;
            (cx + radius * t.cos(), cy + radius * t.sin())
        })
        .collect()
}

/// Semicircle on a round cap at `tip`; `u` points along the segment into the cap.
#[allow(clippy::too_many_arguments)]
fn cap_arc(
    tip_x: f64,
    tip_y: f64,
    nx: f64,
    ny: f64,
    ux: f64,
    uy: f64,
    radius: f64,
    steps: usize,
) -> Vec<(f64, f64)> {
    let mut pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = PI * i as f64 / steps as f64;
        let px = tip_x + radius * (nx * t.cos() + ux * t.sin());
        let py = tip_y + radius * (ny * t.cos() + uy * t.sin());
        pts.push((px, py));
    }
    pts
}

/// Filled capsule: segment `(x1,y1)–(x2,y2)` thickened by `radius`, round caps.
pub fn capsule_polygon(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    radius: f64,
    quad_segs: u32,
) -> Vec<(f64, f64)> {
    if radius <= 0.0 {
        return Vec::new();
    }
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-12 {
        return circle_polygon(x1, y1, radius, quad_segs);
    }
    let ux = dx / len;
    let uy = dy / len;
    let nx = -uy;
    let ny = ux;
    let steps = quad_segs.max(3) as usize;
    let half = steps / 2;

    let mut ring = Vec::new();
    ring.push((x1 + nx * radius, y1 + ny * radius));
    ring.push((x2 + nx * radius, y2 + ny * radius));
    ring.extend(cap_arc(x2, y2, nx, ny, ux, uy, radius, half));
    ring.push((x2 - nx * radius, y2 - ny * radius));
    ring.push((x1 - nx * radius, y1 - ny * radius));
    ring.extend(cap_arc(x1, y1, -nx, -ny, -ux, -uy, radius, half));
    ring
}

/// White knockout path for a stroked capsule (ink radius + grow distance).
pub fn capsule_halo_path_d(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    ink_radius: f64,
    grow: f64,
) -> Option<String> {
    if grow <= 0.0 || ink_radius < 0.0 {
        return None;
    }
    let total = ink_radius + grow;
    let ring = capsule_polygon(x1, y1, x2, y2, total, 8);
    if ring.len() < 3 {
        return None;
    }
    Some(polygon_to_svg_d(&ring))
}

/// Disk ink halo (point buffer then grow → single circle at `r + grow`).
pub fn disk_halo_path_d(cx: f64, cy: f64, ink_radius: f64, grow: f64) -> Option<String> {
    if grow <= 0.0 || ink_radius < 0.0 {
        return None;
    }
    let ring = circle_polygon(cx, cy, ink_radius + grow, 12);
    if ring.is_empty() {
        return None;
    }
    Some(polygon_to_svg_d(&ring))
}

#[cfg(feature = "geom")]
mod ops;

#[cfg(feature = "geom")]
pub use ops::Shape;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capsule_path_is_closed() {
        let d = capsule_halo_path_d(0.0, 0.0, 20.0, 0.0, 1.0, 2.0).unwrap();
        assert!(d.starts_with('M'));
        assert!(d.ends_with('Z'));
    }

    #[cfg(feature = "geom")]
    #[test]
    fn evenodd_ring_has_hole_and_halo_keeps_center_empty() {
        let outer: Vec<(f64, f64)> = (0..16)
            .map(|i| {
                let t = TAU * i as f64 / 16.0;
                (10.0 + 8.0 * t.cos(), 10.0 + 8.0 * t.sin())
            })
            .collect();
        let inner: Vec<(f64, f64)> = (0..16)
            .map(|i| {
                let t = TAU * i as f64 / 16.0;
                (10.0 + 4.0 * t.cos(), 10.0 + 4.0 * t.sin())
            })
            .collect();
        let ink = Shape::from_contours_evenodd(&[outer, inner]);
        assert!(!ink.is_empty());
        assert!(ink.has_holes());
        assert!(!ink.contains(10.0, 10.0));
        let halo = ink.halo(1.0);
        assert!(!halo.is_empty());
        assert!(!halo.contains(10.0, 10.0));
        assert!(halo.area() > 0.0);
    }

    #[cfg(feature = "geom")]
    #[test]
    fn simplify_before_halo_drops_dense_vertices() {
        let dense: Vec<(f64, f64)> = (0..120)
            .map(|i| {
                let t = TAU * i as f64 / 120.0;
                (20.0 + 10.0 * t.cos(), 20.0 + 10.0 * t.sin())
            })
            .collect();
        let ink = Shape::from_ring(&dense);
        assert!(ink.point_count() >= 100);
        let simplified = ink.simplify(0.5);
        assert!(simplified.point_count() < ink.point_count() / 2);
        let halo = ink.halo(2.0);
        assert!(!halo.is_empty());
        assert!(halo.area() > ink.area());
        // Halo ring should not cover the disk center.
        assert!(!halo.contains(20.0, 20.0) || !ink.has_holes());
    }

    #[cfg(feature = "geom")]
    #[test]
    fn capsule_union_and_svg() {
        let a = Shape::capsule(0.0, 0.0, 20.0, 0.0, 1.0);
        let b = Shape::disk(10.0, 0.0, 2.0, 12);
        let u = a.union(&b);
        let d = u.to_svg_d();
        assert!(d.contains('M') && d.contains('Z'));
    }

    #[cfg(feature = "geom")]
    #[test]
    fn evenodd_orients_exterior_ccw() {
        let outer: Vec<(f64, f64)> = vec![(0., 0.), (0., 10.), (10., 10.), (10., 0.)]; // CW
        let inner: Vec<(f64, f64)> = vec![(2., 2.), (8., 2.), (8., 8.), (2., 8.)]; // CCW
        let s = Shape::from_contours_evenodd(&[outer, inner]);
        let polys = s.polygons_rings();
        assert_eq!(polys.len(), 1);
        let ext = &polys[0].0;
        let mut a = 0.0;
        for i in 0..ext.len() {
            let j = (i + 1) % ext.len();
            a += ext[i].0 * ext[j].1 - ext[j].0 * ext[i].1;
        }
        assert!(a > 0.0, "exterior should be CCW, got {a}");
        assert!(s.buffer(1.0).area() > s.area());
    }
}
