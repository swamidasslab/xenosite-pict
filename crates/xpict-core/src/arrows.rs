//! Reaction / network edge shafts in document space.
//!
//! Ports the Python `draw/arrows.py` route cleanup + paint helpers so hosts
//! share one implementation (orthogonal stays axis-aligned; polyline fillets).

use crate::doc::{EdgeArrow, EdgeRouting};

/// Flatten tiny orthogonal/polyline jogs under this length (px).
pub const KINK_PX: f64 = 10.0;
/// Corner fillet radius for non-orthogonal (polyline) bends (px).
pub const TURN_RADIUS: f64 = 36.0;

const DEFAULT_COLOR: &str = "#222";
const DEFAULT_WIDTH: f64 = 1.6;
const HEAD: f64 = 9.0;
const DASH: &str = "6 4";
const EQ_SEP: f64 = 3.2;

/// Document-space point.
pub type Pt = (f64, f64);

fn hypot(dx: f64, dy: f64) -> f64 {
    dx.hypot(dy)
}

fn point_line_distance(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let l2 = dx * dx + dy * dy;
    if l2 < 1e-12 {
        return hypot(px - ax, py - ay);
    }
    let t = ((px - ax) * dx + (py - ay) * dy) / l2;
    let qx = ax + t * dx;
    let qy = ay + t * dy;
    hypot(px - qx, py - qy)
}

/// Drop near-duplicates and flatten kinks under ``kink_px``.
pub fn simplify_route(pts: &[Pt], kink_px: f64) -> Vec<Pt> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let mut cleaned: Vec<Pt> = Vec::with_capacity(pts.len());
    for &(x, y) in pts {
        if cleaned
            .last()
            .map(|&(lx, ly)| hypot(x - lx, y - ly) > 1e-6)
            .unwrap_or(true)
        {
            cleaned.push((x, y));
        }
    }
    if cleaned.len() <= 2 {
        return cleaned;
    }

    // Collapse short middle segments (classic orthogonal micro-jog).
    let mut changed = true;
    while changed && cleaned.len() > 2 {
        changed = false;
        let mut out: Vec<Pt> = vec![cleaned[0]];
        let mut i = 1;
        while i + 1 < cleaned.len() {
            let (x0, y0) = *out.last().unwrap();
            let (x1, y1) = cleaned[i];
            let (x2, y2) = cleaned[i + 1];
            let seg = hypot(x1 - x0, y1 - y0);
            let nxt = hypot(x2 - x1, y2 - y1);
            if seg < kink_px || nxt < kink_px {
                i += 1;
                changed = true;
                continue;
            }
            out.push(cleaned[i]);
            i += 1;
        }
        out.push(*cleaned.last().unwrap());
        cleaned = out;
    }

    // Remove vertices nearly collinear with neighbors.
    changed = true;
    while changed && cleaned.len() > 2 {
        changed = false;
        let mut out: Vec<Pt> = vec![cleaned[0]];
        for i in 1..cleaned.len() - 1 {
            let (ax, ay) = *out.last().unwrap();
            let (px, py) = cleaned[i];
            let (bx, by) = cleaned[i + 1];
            if point_line_distance(px, py, ax, ay, bx, by) < kink_px {
                changed = true;
                continue;
            }
            out.push(cleaned[i]);
        }
        out.push(*cleaned.last().unwrap());
        cleaned = out;
    }
    cleaned
}

/// True when every segment is horizontal or vertical.
pub fn axis_aligned(pts: &[Pt], tol: f64) -> bool {
    if pts.len() < 2 {
        return true;
    }
    for i in 1..pts.len() {
        let dx = (pts[i].0 - pts[i - 1].0).abs();
        let dy = (pts[i].1 - pts[i - 1].1).abs();
        if dx > tol && dy > tol {
            return false;
        }
    }
    true
}

/// Fillet soft polyline turns; keep orthogonal shafts axis-aligned.
pub fn should_fillet(pts: &[Pt], routing: Option<EdgeRouting>) -> bool {
    match routing {
        Some(EdgeRouting::Orthogonal) => false,
        Some(EdgeRouting::Polyline | EdgeRouting::Splines) => !axis_aligned(pts, 1e-3),
        None => pts.len() > 2 && !axis_aligned(pts, 1e-3),
    }
}

fn polyline_d(pts: &[Pt]) -> String {
    if pts.is_empty() {
        return String::new();
    }
    let mut bits = Vec::with_capacity(pts.len());
    bits.push(format!("M {:.2} {:.2}", pts[0].0, pts[0].1));
    for &(x, y) in &pts[1..] {
        bits.push(format!("L {x:.2} {y:.2}"));
    }
    bits.join(" ")
}

/// Polyline with quadratic fillets at corners (larger ``radius`` → softer turns).
pub fn filleted_path_d(pts: &[Pt], radius: f64) -> String {
    if pts.len() < 2 {
        return String::new();
    }
    if pts.len() == 2 || radius <= 0.0 {
        return polyline_d(pts);
    }
    let mut bits: Vec<String> = vec![format!("M {:.2} {:.2}", pts[0].0, pts[0].1)];
    for i in 1..pts.len() - 1 {
        let (ax, ay) = pts[i - 1];
        let (bx, by) = pts[i];
        let (cx, cy) = pts[i + 1];
        let v1x = ax - bx;
        let v1y = ay - by;
        let v2x = cx - bx;
        let v2y = cy - by;
        let len1 = hypot(v1x, v1y).max(1e-9);
        let len2 = hypot(v2x, v2y).max(1e-9);
        let dot = (v1x * v2x + v1y * v2y) / (len1 * len2);
        if dot < -0.98 {
            bits.push(format!("L {bx:.2} {by:.2}"));
            continue;
        }
        let r = radius.min(0.45 * len1).min(0.45 * len2);
        if r < 1.0 {
            bits.push(format!("L {bx:.2} {by:.2}"));
            continue;
        }
        let u1x = v1x / len1;
        let u1y = v1y / len1;
        let u2x = v2x / len2;
        let u2y = v2y / len2;
        let p1x = bx + u1x * r;
        let p1y = by + u1y * r;
        let p2x = bx + u2x * r;
        let p2y = by + u2y * r;
        bits.push(format!("L {p1x:.2} {p1y:.2}"));
        bits.push(format!("Q {bx:.2} {by:.2} {p2x:.2} {p2y:.2}"));
    }
    let (lx, ly) = *pts.last().unwrap();
    bits.push(format!("L {lx:.2} {ly:.2}"));
    bits.join(" ")
}

/// Path `d` for a shaft: fillet polyline turns; keep orthogonal H/V sharp.
pub fn shaft_path_d(pts: &[Pt], routing: Option<EdgeRouting>) -> String {
    if should_fillet(pts, routing) {
        filleted_path_d(pts, TURN_RADIUS)
    } else {
        polyline_d(pts)
    }
}

fn unit(dx: f64, dy: f64) -> (f64, f64) {
    let l = hypot(dx, dy).max(1e-9);
    (dx / l, dy / l)
}

fn perp(ux: f64, uy: f64) -> (f64, f64) {
    (-uy, ux)
}

fn polyline_length(pts: &[Pt]) -> f64 {
    let mut total = 0.0;
    for i in 1..pts.len() {
        total += hypot(pts[i].0 - pts[i - 1].0, pts[i].1 - pts[i - 1].1);
    }
    total
}

fn shorten_polyline_end(pts: &[Pt], amount: f64) -> (Vec<Pt>, Pt, (f64, f64)) {
    if pts.len() < 2 {
        let p = pts.first().copied().unwrap_or((0.0, 0.0));
        return (vec![p], p, (1.0, 0.0));
    }
    let tip = *pts.last().unwrap();
    let mut pts = pts.to_vec();
    let mut remaining = amount;
    while pts.len() >= 2 && remaining > 0.0 {
        let (x1, y1) = pts[pts.len() - 2];
        let (x2, y2) = pts[pts.len() - 1];
        let seg = hypot(x2 - x1, y2 - y1);
        if seg <= 1e-9 {
            pts.pop();
            continue;
        }
        if seg > remaining {
            let (ux, uy) = unit(x2 - x1, y2 - y1);
            let nx = x2 - ux * remaining;
            let ny = y2 - uy * remaining;
            let last = pts.len() - 1;
            pts[last] = (nx, ny);
            return (pts, tip, (ux, uy));
        }
        remaining -= seg;
        pts.pop();
    }
    let (ux, uy) = if pts.len() >= 2 {
        unit(
            pts[pts.len() - 1].0 - pts[pts.len() - 2].0,
            pts[pts.len() - 1].1 - pts[pts.len() - 2].1,
        )
    } else {
        (1.0, 0.0)
    };
    (pts, tip, (ux, uy))
}

fn filled_arrow_head_d(tip_x: f64, tip_y: f64, ux: f64, uy: f64, size: f64) -> String {
    let (px, py) = perp(ux, uy);
    let bx = tip_x - ux * size;
    let by = tip_y - uy * size;
    let half = size * 0.55;
    format!(
        "M {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2} Z",
        bx + px * half,
        by + py * half,
        tip_x,
        tip_y,
        bx - px * half,
        by - py * half
    )
}

/// Minimal shaft/head paint request (host supplies resolved route points).
#[derive(Debug, Clone)]
pub struct EdgePaintIn {
    pub pts: Vec<Pt>,
    pub arrow: EdgeArrow,
    pub routing: Option<EdgeRouting>,
    pub color: Option<String>,
    pub stroke_width: Option<f64>,
    pub dashed: bool,
    pub label: Option<String>,
    pub label_pos: Option<String>,
    pub index: usize,
}

/// One SVG-ish path or text fragment for overlays.
#[derive(Debug, Clone, PartialEq)]
pub enum EdgePrim {
    Path {
        d: String,
        stroke: String,
        fill: String,
        stroke_width: f64,
        stroke_dasharray: Option<String>,
        class: String,
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        fill: String,
        font_size: f64,
        anchor: String,
        class: String,
    },
}

fn path_prim(
    d: String,
    stroke: &str,
    fill: &str,
    width: f64,
    dashed: bool,
    class: &str,
) -> EdgePrim {
    EdgePrim::Path {
        d,
        stroke: stroke.to_string(),
        fill: fill.to_string(),
        stroke_width: width,
        stroke_dasharray: if dashed {
            Some(DASH.to_string())
        } else {
            None
        },
        class: class.to_string(),
    }
}

fn label_point(pts: &[Pt]) -> (f64, f64, f64, f64) {
    if pts.len() < 2 {
        let (x, y) = pts.first().copied().unwrap_or((0.0, 0.0));
        return (x, y, 0.0, -1.0);
    }
    let mut best_i = 1;
    let mut best_len = -1.0;
    for i in 1..pts.len() {
        let l = hypot(pts[i].0 - pts[i - 1].0, pts[i].1 - pts[i - 1].1);
        if l > best_len {
            best_len = l;
            best_i = i;
        }
    }
    let (x1, y1) = pts[best_i - 1];
    let (x2, y2) = pts[best_i];
    let (ux, uy) = unit(x2 - x1, y2 - y1);
    let (px, py) = perp(ux, uy);
    ((x1 + x2) * 0.5, (y1 + y2) * 0.5, px, py)
}

fn offset_polyline(pts: &[Pt], dist: f64) -> Vec<Pt> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let n = pts.len();
    let mut out = Vec::with_capacity(n);
    for (i, &(x, y)) in pts.iter().enumerate() {
        let (px, py) = if i == 0 {
            let (ux, uy) = unit(pts[1].0 - x, pts[1].1 - y);
            perp(ux, uy)
        } else if i + 1 == n {
            let (ux, uy) = unit(x - pts[i - 1].0, y - pts[i - 1].1);
            perp(ux, uy)
        } else {
            let (u1x, u1y) = unit(x - pts[i - 1].0, y - pts[i - 1].1);
            let (u2x, u2y) = unit(pts[i + 1].0 - x, pts[i + 1].1 - y);
            let (p1x, p1y) = perp(u1x, u1y);
            let (p2x, p2y) = perp(u2x, u2y);
            let mut px = p1x + p2x;
            let mut py = p1y + p2y;
            let l = hypot(px, py).max(1e-9);
            px /= l;
            py /= l;
            (px, py)
        };
        out.push((x + px * dist, y + py * dist));
    }
    out
}

fn harpoon_poly(
    pts: &[Pt],
    color: &str,
    width: f64,
    dashed: bool,
    head_size: f64,
    class: &str,
    routing: Option<EdgeRouting>,
) -> Vec<EdgePrim> {
    let (shaft, tip, (ux, uy)) = shorten_polyline_end(pts, head_size);
    let (px, py) = perp(ux, uy);
    let barb = head_size * 0.55;
    let bx = tip.0 - ux * head_size;
    let by = tip.1 - uy * head_size;
    let mut out = Vec::new();
    if shaft.len() >= 2 {
        out.push(path_prim(
            shaft_path_d(&shaft, routing),
            color,
            "none",
            width,
            dashed,
            class,
        ));
    }
    out.push(path_prim(
        format!(
            "M {:.2} {:.2} L {:.2} {:.2}",
            tip.0,
            tip.1,
            bx + px * barb,
            by + py * barb
        ),
        color,
        "none",
        width,
        false,
        &format!("{class} harpoon"),
    ));
    out
}

/// Build overlay primitives for one edge given a resolved route.
pub fn edge_primitives(edge: &EdgePaintIn) -> Vec<EdgePrim> {
    let pts = simplify_route(&edge.pts, KINK_PX);
    if polyline_length(&pts) < 4.0 {
        return Vec::new();
    }
    let color = edge.color.as_deref().unwrap_or(DEFAULT_COLOR);
    let width = edge.stroke_width.unwrap_or(DEFAULT_WIDTH);
    let dashed = edge.dashed;
    let routing = edge.routing;
    let cls = format!("edge edge-{}", edge.index);
    let mut out: Vec<EdgePrim> = Vec::new();

    match edge.arrow {
        EdgeArrow::Equilibrium => {
            let fwd = offset_polyline(&pts, EQ_SEP);
            let mut rev = offset_polyline(&pts, -EQ_SEP);
            rev.reverse();
            out.extend(harpoon_poly(
                &fwd,
                color,
                width,
                dashed,
                HEAD * 0.85,
                &format!("{cls} eq-fwd"),
                routing,
            ));
            out.extend(harpoon_poly(
                &rev,
                color,
                width,
                dashed,
                HEAD * 0.85,
                &format!("{cls} eq-rev"),
                routing,
            ));
        }
        EdgeArrow::Line => {
            out.push(path_prim(
                shaft_path_d(&pts, routing),
                color,
                "none",
                width,
                dashed,
                &cls,
            ));
        }
        EdgeArrow::Open => {
            let (shaft, tip, (ux, uy)) = shorten_polyline_end(&pts, HEAD);
            if shaft.len() >= 2 {
                out.push(path_prim(
                    shaft_path_d(&shaft, routing),
                    color,
                    "none",
                    width,
                    dashed,
                    &cls,
                ));
            }
            let (px, py) = perp(ux, uy);
            let bx = tip.0 - ux * HEAD;
            let by = tip.1 - uy * HEAD;
            let half = HEAD * 0.55;
            out.push(path_prim(
                format!(
                    "M {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2}",
                    bx + px * half,
                    by + py * half,
                    tip.0,
                    tip.1,
                    bx - px * half,
                    by - py * half
                ),
                color,
                "none",
                width,
                false,
                &format!("{cls} head"),
            ));
        }
        EdgeArrow::Forward => {
            let (shaft, tip, (ux, uy)) = shorten_polyline_end(&pts, HEAD);
            if shaft.len() >= 2 {
                out.push(path_prim(
                    shaft_path_d(&shaft, routing),
                    color,
                    "none",
                    width,
                    dashed,
                    &cls,
                ));
            }
            out.push(path_prim(
                filled_arrow_head_d(tip.0, tip.1, ux, uy, HEAD),
                "none",
                color,
                0.0,
                false,
                &format!("{cls} head"),
            ));
        }
    }

    if let Some(ref label) = edge.label {
        let (mx, my, mut px, mut py) = label_point(&pts);
        let pos = edge
            .label_pos
            .as_deref()
            .unwrap_or("above")
            .to_ascii_lowercase();
        if pos == "below" || pos == "right" {
            px = -px;
            py = -py;
        }
        out.push(EdgePrim::Text {
            x: mx + px * 10.0,
            y: my + py * 10.0 + 4.0,
            text: label.clone(),
            fill: color.to_string(),
            font_size: 11.0,
            anchor: "middle".into(),
            class: format!("{cls} label"),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplify_snaps_micro_kink() {
        let jog = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 5.0), (200.0, 5.0)];
        let flat = simplify_route(&jog, KINK_PX);
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0], (0.0, 0.0));
        assert_eq!(flat[1], (200.0, 5.0));
    }

    #[test]
    fn orthogonal_keeps_sharp_corners() {
        let pts = vec![(0.0, 0.0), (80.0, 0.0), (80.0, 60.0), (140.0, 60.0)];
        assert!(!should_fillet(&pts, Some(EdgeRouting::Orthogonal)));
        let d = shaft_path_d(&pts, Some(EdgeRouting::Orthogonal));
        assert!(!d.contains('Q'));
        assert!(d.matches('L').count() >= 2);
    }

    #[test]
    fn polyline_diagonal_gets_fillet() {
        let pts = vec![(0.0, 0.0), (80.0, 0.0), (120.0, 60.0), (180.0, 60.0)];
        assert!(should_fillet(&pts, Some(EdgeRouting::Polyline)));
        let d = shaft_path_d(&pts, Some(EdgeRouting::Polyline));
        assert!(d.contains('Q'));
    }

    #[test]
    fn forward_edge_emits_shaft_and_head() {
        let edge = EdgePaintIn {
            pts: vec![(0.0, 0.0), (100.0, 0.0)],
            arrow: EdgeArrow::Forward,
            routing: Some(EdgeRouting::Polyline),
            color: None,
            stroke_width: None,
            dashed: false,
            label: Some("ox".into()),
            label_pos: None,
            index: 0,
        };
        let prims = edge_primitives(&edge);
        assert!(prims.len() >= 2);
        assert!(prims.iter().any(|p| matches!(p, EdgePrim::Text { .. })));
    }
}
