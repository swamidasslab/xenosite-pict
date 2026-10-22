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

/// Last interpolating vertex and unit end-tangent of the open Catmull–Rom.
///
/// The filled arrow tip is this point; the shaft is trimmed back along the
/// same cubics so the head cannot sit off the curve.
pub fn open_catmull_end_frame(pts: &[(f64, f64)]) -> ((f64, f64), (f64, f64)) {
    let pts = dedupe_poly_pts_open(pts);
    match pts.as_slice() {
        [] => ((0.0, 0.0), (1.0, 0.0)),
        [p] => (*p, (1.0, 0.0)),
        [a, b] => {
            let l = (b.0 - a.0).hypot(b.1 - a.1).max(1e-9);
            (*b, ((b.0 - a.0) / l, (b.1 - a.1) / l))
        }
        _ => {
            let tip = *pts.last().unwrap();
            let mut tan = (1.0, 0.0);
            for_open_cubics(&pts, |_p1, _c1, c2, p2| {
                let dx = p2.0 - c2.0;
                let dy = p2.1 - c2.1;
                let l = dx.hypot(dy).max(1e-9);
                tan = (dx / l, dy / l);
            });
            (tip, tan)
        }
    }
}

/// Open Catmull–Rom cubics stopped `trim` px short of the last vertex.
pub fn centripetal_catmull_rom_open_path_trim_end(pts: &[(f64, f64)], trim: f64) -> String {
    let pts = dedupe_poly_pts_open(pts);
    if pts.len() < 2 {
        return String::new();
    }
    if pts.len() == 2 {
        let (x0, y0) = pts[0];
        let (x1, y1) = pts[1];
        let l = (x1 - x0).hypot(y1 - y0).max(1e-9);
        let keep = (l - trim.max(0.0)).max(0.0);
        let t = keep / l;
        return format!(
            "M {:.2} {:.2} L {:.2} {:.2}",
            x0,
            y0,
            x0 + t * (x1 - x0),
            y0 + t * (y1 - y0)
        );
    }
    let mut cubics = Vec::new();
    for_open_cubics(&pts, |p1, c1, c2, p2| cubics.push((p1, c1, c2, p2)));
    let lens: Vec<f64> = cubics
        .iter()
        .map(|&(a, b, c, d)| cubic_len(a, b, c, d))
        .collect();
    let total: f64 = lens.iter().sum();
    let keep = (total - trim.max(0.0)).max(0.0);
    if keep < 1e-6 {
        return String::new();
    }
    let mut d = format!("M {:.2} {:.2}", cubics[0].0 .0, cubics[0].0 .1);
    let mut acc = 0.0;
    for (i, &(p1, c1, c2, p2)) in cubics.iter().enumerate() {
        let len = lens[i];
        if acc + len <= keep + 1e-9 {
            d.push_str(&format!(
                " C {:.2} {:.2} {:.2} {:.2} {:.2} {:.2}",
                c1.0, c1.1, c2.0, c2.1, p2.0, p2.1
            ));
            acc += len;
            continue;
        }
        let t = cubic_t_for_length(p1, c1, c2, p2, (keep - acc).max(0.0));
        let (lc1, lc2, end) = split_cubic_left(p1, c1, c2, p2, t);
        d.push_str(&format!(
            " C {:.2} {:.2} {:.2} {:.2} {:.2} {:.2}",
            lc1.0, lc1.1, lc2.0, lc2.1, end.0, end.1
        ));
        break;
    }
    d
}

fn lerp_pt(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))
}

fn split_cubic_left(
    p0: (f64, f64),
    c1: (f64, f64),
    c2: (f64, f64),
    p3: (f64, f64),
    t: f64,
) -> ((f64, f64), (f64, f64), (f64, f64)) {
    let p01 = lerp_pt(p0, c1, t);
    let p12 = lerp_pt(c1, c2, t);
    let p23 = lerp_pt(c2, p3, t);
    let p012 = lerp_pt(p01, p12, t);
    let p123 = lerp_pt(p12, p23, t);
    (p01, p012, lerp_pt(p012, p123, t))
}

fn cubic_len(p0: (f64, f64), c1: (f64, f64), c2: (f64, f64), p3: (f64, f64)) -> f64 {
    const N: usize = 8;
    let mut prev = p0;
    let mut len = 0.0;
    for k in 1..=N {
        let q = bezier_cubic(p0, c1, c2, p3, k as f64 / N as f64);
        len += (q.0 - prev.0).hypot(q.1 - prev.1);
        prev = q;
    }
    len
}

fn cubic_prefix_len(p0: (f64, f64), c1: (f64, f64), c2: (f64, f64), p3: (f64, f64), t: f64) -> f64 {
    const N: usize = 8;
    let t = t.clamp(0.0, 1.0);
    let mut prev = p0;
    let mut len = 0.0;
    for k in 1..=N {
        let q = bezier_cubic(p0, c1, c2, p3, t * k as f64 / N as f64);
        len += (q.0 - prev.0).hypot(q.1 - prev.1);
        prev = q;
    }
    len
}

fn cubic_t_for_length(
    p0: (f64, f64),
    c1: (f64, f64),
    c2: (f64, f64),
    p3: (f64, f64),
    need: f64,
) -> f64 {
    let len = cubic_len(p0, c1, c2, p3);
    if need <= 0.0 {
        return 0.0;
    }
    if need >= len {
        return 1.0;
    }
    let mut lo = 0.0;
    let mut hi = 1.0;
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if cubic_prefix_len(p0, c1, c2, p3, mid) < need {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
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
    fn trim_end_stops_short_of_the_tip() {
        let pts = [(0.0, 0.0), (80.0, 0.0), (80.0, 80.0)];
        let full = centripetal_catmull_rom_open_path(&pts);
        assert!(full.trim_end().ends_with("80.00 80.00"), "{full}");
        let trim = centripetal_catmull_rom_open_path_trim_end(&pts, 9.0);
        assert!(
            !trim.trim_end().ends_with("80.00 80.00"),
            "trimmed shaft still reaches the tip: {trim}"
        );
        assert!(trim.contains('C'), "{trim}");
    }
}
