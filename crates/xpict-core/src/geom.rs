//! Geometry for halos and ink (Shapely stand-in).
//!
//! **Partial port.** Capsule/disk halo SVG paths are live (see below). Still in
//! Python/Shapely: glyph contour XOR, `halo_from_shapes` counter punch,
//! annotate buffers, multi-point polylines.
//!
//! Next: polygon `buffer` + `difference`/`union` via `geo` + `i_overlay` (or
//! Clipper) so label knockouts and `O` counters share one implementation across
//! PyO3 and WASM — then Shapely can leave the runtime dependency set.

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
            let t = std::f64::consts::TAU * i as f64 / segs as f64;
            (cx + radius * t.cos(), cy + radius * t.sin())
        })
        .collect()
}

/// Semicircle on a round cap at `tip`; `u` points along the segment into the cap.
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
        let t = std::f64::consts::PI * i as f64 / steps as f64;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capsule_path_is_closed() {
        let d = capsule_halo_path_d(0.0, 0.0, 20.0, 0.0, 1.0, 2.0).unwrap();
        assert!(d.starts_with('M'));
        assert!(d.ends_with('Z'));
    }
}
