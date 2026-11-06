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

/// House SVG export tolerance (px): drop vertices within this of the chord.
///
/// ~½ px is invisible under bond/halo strokes and chem labels, and collapses
/// the 0.02‑px stair-steps `i_overlay` round joins used to emit.
pub const SVG_PATH_EPS_PX: f64 = 0.5;

/// Douglas–Peucker for an open polyline. Keeps endpoints.
pub fn douglas_peucker(pts: &[(f64, f64)], eps: f64) -> Vec<(f64, f64)> {
    if pts.len() < 3 || eps <= 0.0 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    douglas_peucker_rec(pts, 0, pts.len() - 1, eps * eps, &mut keep);
    pts.iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, p)| *p)
        .collect()
}

fn douglas_peucker_rec(
    pts: &[(f64, f64)],
    i0: usize,
    i1: usize,
    eps2: f64,
    keep: &mut [bool],
) {
    if i1 <= i0 + 1 {
        return;
    }
    let (ax, ay) = pts[i0];
    let (bx, by) = pts[i1];
    let dx = bx - ax;
    let dy = by - ay;
    let lab2 = dx * dx + dy * dy;
    let mut best_i = i0;
    let mut best_d2 = 0.0_f64;
    for i in (i0 + 1)..i1 {
        let (px, py) = pts[i];
        let d2 = if lab2 < 1e-24 {
            (px - ax).hypot(py - ay).powi(2)
        } else {
            let t = ((px - ax) * dx + (py - ay) * dy) / lab2;
            let qx = ax + t * dx;
            let qy = ay + t * dy;
            (px - qx).hypot(py - qy).powi(2)
        };
        if d2 > best_d2 {
            best_d2 = d2;
            best_i = i;
        }
    }
    if best_d2 <= eps2 || best_i == i0 {
        return;
    }
    keep[best_i] = true;
    douglas_peucker_rec(pts, i0, best_i, eps2, keep);
    douglas_peucker_rec(pts, best_i, i1, eps2, keep);
}

/// Closed-ring Douglas–Peucker (does not repeat the first point at the end).
pub fn douglas_peucker_closed(ring: &[(f64, f64)], eps: f64) -> Vec<(f64, f64)> {
    let mut pts = ring.to_vec();
    if pts.len() >= 2 {
        let a = pts[0];
        let b = *pts.last().unwrap();
        if (a.0 - b.0).hypot(a.1 - b.1) <= 1e-9 {
            pts.pop();
        }
    }
    if pts.len() < 3 || eps <= 0.0 {
        return pts;
    }
    // Diameter anchors: farthest from pts[0], then farthest from that — both
    // arcs of the loop get simplified (open DP alone would pin neighbors).
    let mut a = 0usize;
    let mut b = 1usize;
    let mut best = 0.0_f64;
    for i in 1..pts.len() {
        let d = (pts[i].0 - pts[0].0).hypot(pts[i].1 - pts[0].1);
        if d > best {
            best = d;
            b = i;
        }
    }
    best = 0.0;
    for (i, p) in pts.iter().enumerate() {
        let d = (p.0 - pts[b].0).hypot(p.1 - pts[b].1);
        if d > best {
            best = d;
            a = i;
        }
    }
    if a == b {
        return douglas_peucker(&pts, eps);
    }
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    let mut keep = vec![false; pts.len()];
    keep[lo] = true;
    keep[hi] = true;
    let eps2 = eps * eps;
    douglas_peucker_rec(&pts, lo, hi, eps2, &mut keep);
    // Wrap-around chain hi → … → n-1 → 0 → … → lo
    let mut chain_idx: Vec<usize> = (hi..pts.len()).collect();
    chain_idx.extend(0..=lo);
    let chain: Vec<(f64, f64)> = chain_idx.iter().map(|&i| pts[i]).collect();
    let mut chain_keep = vec![false; chain.len()];
    chain_keep[0] = true;
    *chain_keep.last_mut().unwrap() = true;
    douglas_peucker_rec(&chain, 0, chain.len() - 1, eps2, &mut chain_keep);
    for (k, &i) in chain_idx.iter().enumerate() {
        if chain_keep[k] {
            keep[i] = true;
        }
    }
    let out: Vec<_> = pts
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, p)| *p)
        .collect();
    if out.len() >= 3 {
        out
    } else {
        pts
    }
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
    let ring = capsule_polygon(x1, y1, x2, y2, total, 4);
    if ring.len() < 3 {
        return None;
    }
    let ring = douglas_peucker_closed(&ring, SVG_PATH_EPS_PX);
    Some(polygon_to_svg_d(&ring))
}

/// Disk ink halo (point buffer then grow → single circle at `r + grow`).
pub fn disk_halo_path_d(cx: f64, cy: f64, ink_radius: f64, grow: f64) -> Option<String> {
    if grow <= 0.0 || ink_radius < 0.0 {
        return None;
    }
    let ring = circle_polygon(cx, cy, ink_radius + grow, 8);
    if ring.is_empty() {
        return None;
    }
    let ring = douglas_peucker_closed(&ring, SVG_PATH_EPS_PX);
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
        // Center stays open…
        assert!(!halo.contains(10.0, 10.0));
        // …but the offset inner rim is covered (just inside the original hole).
        assert!(halo.contains(10.0 + 3.2, 10.0));
        assert!(halo.area() > 0.0);
    }

    #[cfg(feature = "geom")]
    #[test]
    fn lib_simplify_drops_collinear_then_halo_keeps_o_hole() {
        // Mid-edge points are collinear — from_ring runs lib simplify.
        let ring = vec![
            (0.0, 0.0),
            (5.0, 0.0),
            (10.0, 0.0),
            (10.0, 5.0),
            (10.0, 10.0),
            (0.0, 10.0),
            (0.0, 5.0),
        ];
        let ink = Shape::from_ring(&ring);
        assert!(ink.point_count() < ring.len());

        let outer: Vec<(f64, f64)> = (0..24)
            .map(|i| {
                let t = TAU * i as f64 / 24.0;
                (20.0 + 10.0 * t.cos(), 20.0 + 10.0 * t.sin())
            })
            .collect();
        let inner: Vec<(f64, f64)> = (0..24)
            .map(|i| {
                let t = TAU * i as f64 / 24.0;
                (20.0 + 5.0 * t.cos(), 20.0 + 5.0 * t.sin())
            })
            .collect();
        let ring = Shape::from_contours_evenodd(&[outer, inner]);
        assert!(ring.has_holes());
        assert!(ring.simplify().has_holes());
        let halo = ring.halo(1.5);
        assert!(!halo.is_empty());
        assert!(!halo.contains(20.0, 20.0));
        assert!(halo.contains(20.0 + 4.0, 20.0)); // inner rim
        assert!(halo.area() > ring.area());
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

    #[test]
    fn polygon_and_halo_guards() {
        assert!(polygon_to_svg_d(&[(0.0, 0.0), (1.0, 0.0)]).is_empty());
        assert!(circle_polygon(0.0, 0.0, 0.0, 8).is_empty());
        assert!(circle_polygon(0.0, 0.0, -1.0, 8).is_empty());
        assert!(capsule_polygon(0.0, 0.0, 10.0, 0.0, 0.0, 8).is_empty());
        // Degenerate segment → disk at the tip.
        let disk = capsule_polygon(1.0, 2.0, 1.0, 2.0, 3.0, 8);
        assert!(disk.len() >= 3);
        assert!(capsule_halo_path_d(0.0, 0.0, 10.0, 0.0, 1.0, 0.0).is_none());
        assert!(capsule_halo_path_d(0.0, 0.0, 10.0, 0.0, -1.0, 1.0).is_none());
        assert!(disk_halo_path_d(0.0, 0.0, 1.0, 0.0).is_none());
        assert!(disk_halo_path_d(0.0, 0.0, -0.1, 1.0).is_none());
        let d = disk_halo_path_d(0.0, 0.0, 1.0, 2.0).unwrap();
        assert!(d.ends_with('Z'));
    }

    /// Parity with Python ``test_shape_evenodd_and_halo`` (square annulus).
    #[cfg(feature = "geom")]
    #[test]
    fn shape_evenodd_square_annulus_and_halo() {
        let outer = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)];
        let inner = vec![(5.0, 5.0), (15.0, 5.0), (15.0, 15.0), (5.0, 15.0)];
        let ink = Shape::from_contours_evenodd(&[outer, inner]);
        assert!(!ink.is_empty());
        assert!(ink.has_holes());
        assert!(!ink.contains(10.0, 10.0));
        let halo = ink.halo(1.0);
        assert!(!halo.is_empty());
        assert!(!halo.contains(10.0, 10.0));
        assert!(halo.area() > ink.area());
        assert!(ink.to_svg_d().contains('M'));
    }

    /// Parity with Python ``test_capsule_halo_path_closed`` / OFFSET_PX constant.
    #[test]
    fn capsule_halo_closed_and_offset_px() {
        use crate::metrics::{LABEL_GAP_PX, OFFSET_PX};
        let d = capsule_halo_path_d(0.0, 0.0, 20.0, 0.0, 0.56, LABEL_GAP_PX).unwrap();
        assert!(d.ends_with('Z'));
        assert!((OFFSET_PX - 4.5).abs() < 1e-9);
    }

    #[test]
    fn rdp_collapses_0_02px_stairsteps() {
        let mut ring = Vec::new();
        for i in 0..500 {
            ring.push((35.63, 100.0 + i as f64 * 0.02));
        }
        let y1 = ring.last().unwrap().1;
        ring.push((40.0, y1));
        ring.push((40.0, 100.0));
        let out = douglas_peucker_closed(&ring, SVG_PATH_EPS_PX);
        assert!(
            out.len() < 20,
            "expected dramatic collapse, got {} pts from {}",
            out.len(),
            ring.len()
        );
    }

    #[cfg(feature = "geom")]
    #[test]
    fn halo_svg_not_megabytes_for_grid_of_capsules() {
        let mut ink = Shape::empty();
        for i in 0..40 {
            let x = i as f64 * 12.0;
            let cap = Shape::capsule(x, 0.0, x + 10.0, 8.0, 1.2);
            ink = if ink.is_empty() { cap } else { ink.union(&cap) };
        }
        let halo = ink.halo(1.5);
        let d = halo.to_svg_d();
        let n_l = d.matches(" L ").count();
        assert!(
            d.len() < 50_000,
            "halo d too big: {} chars, {} L cmds, {} pts",
            d.len(),
            n_l,
            halo.point_count()
        );
        assert!(n_l < 2_000, "too many L commands: {n_l}");
    }

    #[cfg(feature = "geom")]
    #[test]
    fn shape_ops_cover_buffers_booleans_and_empty() {
        assert!(Shape::empty().is_empty());
        assert!(Shape::from_ring(&[(0.0, 0.0), (1.0, 0.0)]).is_empty());
        assert!(Shape::from_contours_evenodd(&[vec![(0.0, 0.0), (1.0, 0.0)]]).is_empty());
        assert!(Shape::from_contours_evenodd(&[]).is_empty());

        let a = Shape::disk(0.0, 0.0, 5.0, 12);
        let b = Shape::disk(8.0, 0.0, 5.0, 12);
        assert!(!a.union(&Shape::empty()).is_empty());
        assert!(!Shape::empty().union(&a).is_empty());
        assert!(Shape::empty().difference(&a).is_empty());
        assert!(!a.difference(&Shape::empty()).is_empty());
        let diff = a.difference(&b);
        assert!(!diff.is_empty());
        let x = a.xor(&b);
        assert!(!x.is_empty());
        assert_eq!(a.buffer(0.0).point_count(), a.point_count());
        assert!(Shape::empty().simplify().is_empty());
        assert!(Shape::empty().halo(1.0).is_empty());
        assert!(a.halo(0.0).is_empty());

        assert!(Shape::polyline_buffer(&[], 1.0).is_empty());
        assert!(Shape::polyline_buffer(&[(0.0, 0.0)], 1.0).is_empty());
        assert!(Shape::polyline_buffer(&[(0.0, 0.0), (10.0, 0.0)], 0.0).is_empty());
        let cap = Shape::polyline_buffer(&[(0.0, 0.0), (10.0, 0.0)], 1.0);
        assert!(!cap.is_empty());
        let poly = Shape::polyline_buffer(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], 1.5);
        assert!(!poly.is_empty());

        let multi = Shape::multipoint_buffer(&[(0.0, 0.0), (20.0, 0.0)], 2.0);
        assert!(!multi.is_empty());

        assert!(Shape::annular(0.0, 0.0, 0.0, 1.0).is_empty());
        assert!(Shape::annular(0.0, 0.0, 5.0, 0.0).is_empty());
        let ring = Shape::annular(0.0, 0.0, 10.0, 2.0);
        assert!(!ring.is_empty());
        assert!(ring.has_holes() || ring.area() > 0.0);
        // Stroke wider than diameter → filled disk (no hole).
        let fat = Shape::annular(0.0, 0.0, 1.0, 4.0);
        assert!(!fat.is_empty());

        let moved = a.translate(3.0, -2.0);
        let c = moved.centroid().unwrap();
        assert!((c.0 - 3.0).abs() < 0.5);
        assert!((c.1 + 2.0).abs() < 0.5);
        let flipped = a.scale(-1.0, 1.0, 0.0, 0.0);
        assert!(!flipped.is_empty());
        assert!(a.contains(0.0, 0.0));
        assert!(!a.contains(100.0, 100.0));

        // horizontal_span_at: disk mid-height has positive width; miss → 0.
        let span = a.horizontal_span_at(0.0);
        assert!(span > 5.0);
        assert_eq!(a.horizontal_span_at(1000.0), 0.0);
    }
}

