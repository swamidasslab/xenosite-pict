//! Centripetal Catmull–Rom → cubic Bézier SVG `d`.
//!
//! Shared by region hulls (closed) and scheme arrows (open). Lives outside
//! `doc` so [`crate::arrows`] can paint shafts without a module cycle.

/// Centripetal Catmull–Rom (α = 0.5) closed loop as cubic Bézier SVG `d`.
pub fn centripetal_catmull_rom_closed_path(pts: &[(f64, f64)]) -> String {
    let pts = dedupe_poly_pts(pts);
    let n = pts.len();
    if n < 3 {
        return String::new();
    }
    const ALPHA: f64 = 0.5;
    let mut d = format!("M {:.2} {:.2}", pts[0].0, pts[0].1);
    for i in 0..n {
        let p0 = pts[(i + n - 1) % n];
        let p1 = pts[i];
        let p2 = pts[(i + 1) % n];
        let p3 = pts[(i + 2) % n];
        let (c1, c2) = catmull_rom_segment_cubic(p0, p1, p2, p3, ALPHA);
        d.push_str(&format!(
            " C {:.2} {:.2} {:.2} {:.2} {:.2} {:.2}",
            c1.0, c1.1, c2.0, c2.1, p2.0, p2.1
        ));
    }
    d.push_str(" Z");
    d
}

/// Open centripetal Catmull–Rom (α = 0.5) as cubic Bézier SVG `d`.
pub fn centripetal_catmull_rom_open_path(pts: &[(f64, f64)]) -> String {
    let pts = dedupe_poly_pts_open(pts);
    if pts.len() < 2 {
        return String::new();
    }
    if pts.len() == 2 {
        return format!(
            "M {:.2} {:.2} L {:.2} {:.2}",
            pts[0].0, pts[0].1, pts[1].0, pts[1].1
        );
    }
    let mut d = format!("M {:.2} {:.2}", pts[0].0, pts[0].1);
    for_open_cubics(&pts, |_p1, c1, c2, p2| {
        d.push_str(&format!(
            " C {:.2} {:.2} {:.2} {:.2} {:.2} {:.2}",
            c1.0, c1.1, c2.0, c2.1, p2.0, p2.1
        ));
    });
    d
}

/// Last interpolating vertex and unit last-chord of the open controls.
pub fn open_catmull_end_frame(pts: &[(f64, f64)]) -> ((f64, f64), (f64, f64)) {
    let pts = dedupe_poly_pts_open(pts);
    match pts.as_slice() {
        [] => ((0.0, 0.0), (1.0, 0.0)),
        [p] => (*p, (1.0, 0.0)),
        rest => {
            let tip = *rest.last().unwrap();
            let prev = rest[rest.len() - 2];
            let l = (tip.0 - prev.0).hypot(tip.1 - prev.1).max(1e-9);
            (tip, ((tip.0 - prev.0) / l, (tip.1 - prev.1) / l))
        }
    }
}

/// Shaft + arrow from the same controls: tip is the last vertex, base is that
/// point pulled back along the last chord. The shaft interpolates `base`.
///
/// Head and shaft share those two points. No SVG `d` is consulted.
pub fn open_catmull_with_arrow(
    pts: &[(f64, f64)],
    head: f64,
) -> (String, (f64, f64), (f64, f64), (f64, f64)) {
    let mut shaft = dedupe_poly_pts_open(pts);
    if shaft.len() < 2 {
        let tip = shaft.first().copied().unwrap_or((0.0, 0.0));
        return (String::new(), tip, tip, (1.0, 0.0));
    }
    let (tip, (ux, uy)) = open_catmull_end_frame(&shaft);
    let prev = shaft[shaft.len() - 2];
    let chord = (tip.0 - prev.0).hypot(tip.1 - prev.1).max(1e-9);
    let h = head.max(0.0).min(chord * 0.9);
    let base = (tip.0 - ux * h, tip.1 - uy * h);
    let last = shaft.len() - 1;
    shaft[last] = base;
    (
        centripetal_catmull_rom_open_path(&shaft),
        tip,
        base,
        (ux, uy),
    )
}

/// Dense polyline along the open Catmull–Rom (same cubics as the SVG shaft).
pub fn sample_open_catmull(pts: &[(f64, f64)], n_per_seg: usize) -> Vec<(f64, f64)> {
    let pts = dedupe_poly_pts_open(pts);
    if pts.len() <= 2 {
        return pts;
    }
    let n_per = n_per_seg.max(2);
    let mut out = vec![pts[0]];
    for_open_cubics(&pts, |p1, c1, c2, p2| {
        for k in 1..=n_per {
            let t = k as f64 / n_per as f64;
            out.push(bezier_cubic(p1, c1, c2, p2, t));
        }
    });
    out
}

fn for_open_cubics(
    pts: &[(f64, f64)],
    mut each: impl FnMut((f64, f64), (f64, f64), (f64, f64), (f64, f64)),
) {
    let n = pts.len();
    const ALPHA: f64 = 0.5;
    for i in 0..n - 1 {
        // Phantom neighbors so the end tangents follow the last chord. Duplicating
        // the endpoint (p3 = p2) made the last cubic degenerate and left arrow
        // heads sitting off the painted shaft.
        let p0 = if i == 0 {
            (2.0 * pts[0].0 - pts[1].0, 2.0 * pts[0].1 - pts[1].1)
        } else {
            pts[i - 1]
        };
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = if i + 2 < n {
            pts[i + 2]
        } else {
            (
                2.0 * pts[n - 1].0 - pts[n - 2].0,
                2.0 * pts[n - 1].1 - pts[n - 2].1,
            )
        };
        let (c1, c2) = catmull_rom_segment_cubic(p0, p1, p2, p3, ALPHA);
        each(p1, c1, c2, p2);
    }
}

fn bezier_cubic(
    p0: (f64, f64),
    c1: (f64, f64),
    c2: (f64, f64),
    p3: (f64, f64),
    t: f64,
) -> (f64, f64) {
    let u = 1.0 - t;
    let uu = u * u;
    let tt = t * t;
    (
        uu * u * p0.0 + 3.0 * uu * t * c1.0 + 3.0 * u * tt * c2.0 + tt * t * p3.0,
        uu * u * p0.1 + 3.0 * uu * t * c1.1 + 3.0 * u * tt * c2.1 + tt * t * p3.1,
    )
}

fn dedupe_poly_pts_open(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    for &p in pts {
        if out
            .last()
            .is_none_or(|&q| (q.0 - p.0).hypot(q.1 - p.1) > 1e-6)
        {
            out.push(p);
        }
    }
    out
}

fn dedupe_poly_pts(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = dedupe_poly_pts_open(pts);
    if out.len() >= 2 {
        let a = out[0];
        let b = *out.last().unwrap();
        if (a.0 - b.0).hypot(a.1 - b.1) <= 1e-6 {
            out.pop();
        }
    }
    out
}

fn knot_span(a: (f64, f64), b: (f64, f64), alpha: f64) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1).powf(alpha).max(1e-9)
}

/// Non-uniform CR tangent at `p` given neighbors `pm`, `pp` (Yuksel).
fn catmull_rom_tangent(pm: (f64, f64), p: (f64, f64), pp: (f64, f64), alpha: f64) -> (f64, f64) {
    let d0 = knot_span(pm, p, alpha);
    let d1 = knot_span(p, pp, alpha);
    (
        (p.0 - pm.0) / d0 - (pp.0 - pm.0) / (d0 + d1) + (pp.0 - p.0) / d1,
        (p.1 - pm.1) / d0 - (pp.1 - pm.1) / (d0 + d1) + (pp.1 - p.1) / d1,
    )
}

/// Cubic Bézier controls for the CR segment from `p1` → `p2`.
fn catmull_rom_segment_cubic(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    alpha: f64,
) -> ((f64, f64), (f64, f64)) {
    let m1 = catmull_rom_tangent(p0, p1, p2, alpha);
    let m2 = catmull_rom_tangent(p1, p2, p3, alpha);
    let dt = knot_span(p1, p2, alpha);
    (
        (p1.0 + m1.0 * dt / 3.0, p1.1 + m1.1 * dt / 3.0),
        (p2.0 - m2.0 * dt / 3.0, p2.1 - m2.1 * dt / 3.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_interpolates_square() {
        let sq = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let d = centripetal_catmull_rom_closed_path(&sq);
        assert!(d.starts_with("M 0.00 0.00"), "d={d}");
        assert!(d.contains("C "), "expected cubic segments: {d}");
        for (x, y) in sq {
            let needle = format!("{x:.2} {y:.2}");
            assert!(
                d.contains(&needle),
                "missing interpolating vertex {needle} in {d}"
            );
        }
        let m1 = catmull_rom_tangent(sq[3], sq[0], sq[1], 0.0);
        assert!((m1.0 - 0.5 * (sq[1].0 - sq[3].0)).abs() < 1e-9);
        assert!((m1.1 - 0.5 * (sq[1].1 - sq[3].1)).abs() < 1e-9);
    }

    #[test]
    fn open_emits_cubics() {
        let d = centripetal_catmull_rom_open_path(&[(0.0, 0.0), (40.0, 0.0), (80.0, 20.0)]);
        assert!(d.contains('C'), "{d}");
        assert!(!d.contains('Z'), "{d}");
        let line = centripetal_catmull_rom_open_path(&[(0.0, 0.0), (10.0, 0.0)]);
        assert!(line.contains('L'), "{line}");
        assert!(centripetal_catmull_rom_open_path(&[(0.0, 0.0)]).is_empty());
        assert!(centripetal_catmull_rom_closed_path(&[(0.0, 0.0), (1.0, 0.0)]).is_empty());
    }

    #[test]
    fn open_last_cubic_follows_end_chord() {
        let d = centripetal_catmull_rom_open_path(&[(0.0, 0.0), (80.0, 0.0), (80.0, 80.0)]);
        assert!(
            !d.contains("80.00 80.00 80.00 80.00"),
            "p3=p2 collapsed the last cubic; head would detach: {d}"
        );
        assert!(d.trim_end().ends_with("80.00 80.00"), "{d}");
    }

    #[test]
    fn sample_open_follows_control_vertices() {
        let pts = [(0.0, 0.0), (80.0, 0.0), (80.0, 80.0)];
        let s = sample_open_catmull(&pts, 4);
        assert!(s.len() > pts.len(), "{}", s.len());
        assert!((s[0].0 - 0.0).abs() < 1e-9 && (s[0].1 - 0.0).abs() < 1e-9);
        let last = *s.last().unwrap();
        assert!((last.0 - 80.0).abs() < 1e-6 && (last.1 - 80.0).abs() < 1e-6);
    }

    #[test]
    fn end_frame_is_the_last_control() {
        let pts = [(0.0, 0.0), (80.0, 0.0), (80.0, 80.0)];
        let (tip, (ux, uy)) = open_catmull_end_frame(&pts);
        assert!((tip.0 - 80.0).abs() < 1e-9 && (tip.1 - 80.0).abs() < 1e-9);
        assert!(
            uy > 0.5 && ux.abs() < 0.3,
            "end tangent should follow last chord: {ux},{uy}"
        );
    }

    #[test]
    fn arrow_base_is_last_chord_not_a_path_parse() {
        let pts = [(0.0, 0.0), (40.0, 50.0), (90.0, 10.0), (160.0, 80.0)];
        let head = 18.0;
        let (d, tip, base, (ux, uy)) = open_catmull_with_arrow(&pts, head);
        assert!((tip.0 - 160.0).abs() < 1e-9 && (tip.1 - 80.0).abs() < 1e-9);
        let chord = 70.0_f64.hypot(70.0);
        assert!((ux - (160.0 - 90.0) / chord).abs() < 1e-9);
        assert!((uy - (80.0 - 10.0) / chord).abs() < 1e-9);
        assert!(((tip.0 - base.0).hypot(tip.1 - base.1) - head).abs() < 1e-9);
        let mut shaft = pts.to_vec();
        *shaft.last_mut().unwrap() = base;
        assert_eq!(d, centripetal_catmull_rom_open_path(&shaft));
        assert!(
            (base.0 - tip.0 + ux * head).abs() < 1e-9 && (base.1 - tip.1 + uy * head).abs() < 1e-9,
            "base must sit on the last chord"
        );
        let needle = format!("{:.2} {:.2}", base.0, base.1);
        assert!(
            d.trim_end().ends_with(&needle),
            "cubic shaft must interpolate the head base, not a trimmed curve: d={d} base={needle}"
        );
    }
}
