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
/// Head length at [`DEFAULT_WIDTH`] (px). Thicker strokes scale this.
const HEAD: f64 = 9.0;

/// Filled/open head length for this stroke. House 1.6 px → 9 px.
pub fn head_px(stroke_width: Option<f64>) -> f64 {
    let w = stroke_width.unwrap_or(DEFAULT_WIDTH).max(0.5);
    HEAD * (w / DEFAULT_WIDTH)
}

/// Shortest painted shaft that still shows a head and a bit of stroke.
pub fn min_shaft_px(stroke_width: Option<f64>) -> f64 {
    head_px(stroke_width) * 2.5
}
const DASH: &str = "6 4";
const EQ_SEP: f64 = 3.2;
/// Gap from molecule viewport edge to shaft anchors (document px).
pub const ANCHOR_GAP: f64 = 6.0;

/// Axis-aligned box around ``(cx,cy)`` clipped toward ``(tx,ty)``.
pub fn clip_box_edge(cx: f64, cy: f64, tx: f64, ty: f64, w: f64, h: f64, pad: f64) -> Pt {
    let dx = tx - cx;
    let dy = ty - cy;
    if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
        return (cx, cy);
    }
    let hw = w * 0.5 + pad;
    let hh = h * 0.5 + pad;
    let sx = if dx.abs() > 1e-9 {
        hw / dx.abs()
    } else {
        f64::INFINITY
    };
    let sy = if dy.abs() > 1e-9 {
        hh / dy.abs()
    } else {
        f64::INFINITY
    };
    let t = sx.min(sy);
    (cx + dx * t, cy + dy * t)
}

/// Viewport-boundary anchors between two molecule boxes.
///
/// Each box is ``(x, y, width, height)`` in document space.
pub fn edge_anchors(src: (f64, f64, f64, f64), tgt: (f64, f64, f64, f64), pad: f64) -> (Pt, Pt) {
    let (sx, sy, sw, sh) = src;
    let (tx, ty, tw, th) = tgt;
    let scx = sx + sw * 0.5;
    let scy = sy + sh * 0.5;
    let tcx = tx + tw * 0.5;
    let tcy = ty + th * 0.5;
    let p0 = clip_box_edge(scx, scy, tcx, tcy, sw, sh, pad);
    let p1 = clip_box_edge(tcx, tcy, scx, scy, tw, th, pad);
    (p0, p1)
}

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

fn painted_shaft_d(pts: &[Pt], routing: Option<EdgeRouting>, curve_catmull: bool) -> String {
    if curve_catmull {
        crate::catmull::centripetal_catmull_rom_open_path(pts)
    } else {
        shaft_path_d(pts, routing)
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

/// Shaft stroke and arrow head from the route controls: tip is the last
/// vertex, base is that point pulled back along the last chord. Both pieces
/// are those two points — never a second read of a painted `d`.
fn shaft_and_head(
    pts: &[Pt],
    head_size: f64,
    routing: Option<EdgeRouting>,
    curve_catmull: bool,
) -> (String, Pt, Pt, (f64, f64)) {
    if curve_catmull {
        crate::catmull::open_catmull_with_arrow(pts, head_size)
    } else {
        let (shaft, tip, (ux, uy)) = shorten_polyline_end(pts, head_size);
        let base = *shaft.last().unwrap_or(&tip);
        (shaft_path_d(&shaft, routing), tip, base, (ux, uy))
    }
}

fn filled_arrow_head_d(tip: Pt, base: Pt, size: f64) -> String {
    let (ux, uy) = unit(tip.0 - base.0, tip.1 - base.1);
    let (px, py) = perp(ux, uy);
    let half = size * 0.55;
    format!(
        "M {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2} Z",
        base.0 + px * half,
        base.1 + py * half,
        tip.0,
        tip.1,
        base.0 - px * half,
        base.1 - py * half
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
    /// Paint the shaft as an open centripetal Catmull–Rom (scheme energy refine).
    pub curve_catmull: bool,
    /// Override label position (document px). `None` → longest-segment midpoint.
    pub label_xy: Option<Pt>,
    /// Label rotation in degrees (SVG, about the glyph origin).
    pub label_rotate: Option<f64>,
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
        /// Degrees; `0` omits the SVG transform.
        rotate: f64,
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
        stroke_dasharray: if dashed { Some(DASH.to_string()) } else { None },
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

/// Parallel-curve offset of a polyline by `dist` along the left normal.
///
/// Interior vertices use a miter so every segment stays exactly `dist`
/// from its source (constant lane width, including at corners). A 180°
/// fold falls back to the unit bisector.
pub(crate) fn offset_polyline(pts: &[Pt], dist: f64) -> Vec<Pt> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let n = pts.len();
    let seg_n = |i: usize| {
        let (ux, uy) = unit(pts[i + 1].0 - pts[i].0, pts[i + 1].1 - pts[i].1);
        perp(ux, uy)
    };
    let mut out = Vec::with_capacity(n);
    for (i, &(x, y)) in pts.iter().enumerate() {
        if i == 0 {
            let (px, py) = seg_n(0);
            out.push((x + px * dist, y + py * dist));
            continue;
        }
        if i + 1 == n {
            let (px, py) = seg_n(n - 2);
            out.push((x + px * dist, y + py * dist));
            continue;
        }
        let (n1x, n1y) = seg_n(i - 1);
        let (n2x, n2y) = seg_n(i);
        let denom = 1.0 + n1x * n2x + n1y * n2y;
        if denom.abs() < 1e-6 {
            let mut px = n1x + n2x;
            let mut py = n1y + n2y;
            let l = hypot(px, py).max(1e-9);
            px /= l;
            py /= l;
            out.push((x + px * dist, y + py * dist));
        } else {
            out.push((
                x + dist * (n1x + n2x) / denom,
                y + dist * (n1y + n2y) / denom,
            ));
        }
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
    curve_catmull: bool,
) -> Vec<EdgePrim> {
    let (shaft_d, tip, base, (ux, uy)) = shaft_and_head(pts, head_size, routing, curve_catmull);
    let (px, py) = perp(ux, uy);
    let barb = head_size * 0.55;
    let mut out = Vec::new();
    if !shaft_d.is_empty() {
        out.push(path_prim(shaft_d, color, "none", width, dashed, class));
    }
    out.push(path_prim(
        format!(
            "M {:.2} {:.2} L {:.2} {:.2}",
            tip.0,
            tip.1,
            base.0 + px * barb,
            base.1 + py * barb
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
    let head = head_px(edge.stroke_width).min((polyline_length(&pts) * 0.45).max(HEAD * 0.5));
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
                head * 0.85,
                &format!("{cls} eq-fwd"),
                routing,
                edge.curve_catmull,
            ));
            out.extend(harpoon_poly(
                &rev,
                color,
                width,
                dashed,
                head * 0.85,
                &format!("{cls} eq-rev"),
                routing,
                edge.curve_catmull,
            ));
        }
        EdgeArrow::Line => {
            out.push(path_prim(
                painted_shaft_d(&pts, routing, edge.curve_catmull),
                color,
                "none",
                width,
                dashed,
                &cls,
            ));
        }
        EdgeArrow::Open => {
            let (shaft_d, tip, base, (ux, uy)) =
                shaft_and_head(&pts, head, routing, edge.curve_catmull);
            if !shaft_d.is_empty() {
                out.push(path_prim(shaft_d, color, "none", width, dashed, &cls));
            }
            let (px, py) = perp(ux, uy);
            let half = head * 0.55;
            out.push(path_prim(
                format!(
                    "M {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2}",
                    base.0 + px * half,
                    base.1 + py * half,
                    tip.0,
                    tip.1,
                    base.0 - px * half,
                    base.1 - py * half
                ),
                color,
                "none",
                width,
                false,
                &format!("{cls} head"),
            ));
        }
        EdgeArrow::Forward => {
            let (shaft_d, tip, base, _) = shaft_and_head(&pts, head, routing, edge.curve_catmull);
            if !shaft_d.is_empty() {
                out.push(path_prim(shaft_d, color, "none", width, dashed, &cls));
            }
            out.push(path_prim(
                filled_arrow_head_d(tip, base, head),
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
        let (x, y) = edge
            .label_xy
            .unwrap_or((mx + px * 10.0, my + py * 10.0 + 4.0));
        let rotate = edge.label_rotate.unwrap_or(0.0);
        out.push(EdgePrim::Text {
            x,
            y,
            text: label.clone(),
            fill: color.to_string(),
            font_size: 11.0,
            anchor: "middle".into(),
            class: format!("{cls} label"),
            rotate,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint(pts: Vec<Pt>, arrow: EdgeArrow, routing: Option<EdgeRouting>) -> EdgePaintIn {
        EdgePaintIn {
            pts,
            arrow,
            routing,
            color: Some("#06c".into()),
            stroke_width: Some(2.0),
            dashed: true,
            label: Some("lab".into()),
            label_pos: Some("below".into()),
            index: 1,
            curve_catmull: false,
            label_xy: None,
            label_rotate: None,
        }
    }

    #[test]
    fn head_scales_with_stroke() {
        assert!((head_px(None) - 9.0).abs() < 1e-9);
        assert!((head_px(Some(1.6)) - 9.0).abs() < 1e-9);
        assert!((head_px(Some(4.8)) - 27.0).abs() < 1e-9);
        assert!(min_shaft_px(Some(4.8)) > head_px(Some(4.8)) * 2.0);
    }

    #[test]
    fn simplify_snaps_micro_kink() {
        let jog = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 5.0), (200.0, 5.0)];
        let flat = simplify_route(&jog, KINK_PX);
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0], (0.0, 0.0));
        assert_eq!(flat[1], (200.0, 5.0));
    }

    #[test]
    fn simplify_handles_short_and_duplicates() {
        assert!(simplify_route(&[], KINK_PX).is_empty());
        assert_eq!(simplify_route(&[(1.0, 2.0)], KINK_PX), vec![(1.0, 2.0)]);
        let dups = vec![(0.0, 0.0), (0.0, 0.0), (10.0, 0.0)];
        assert_eq!(simplify_route(&dups, KINK_PX).len(), 2);
        // Real elbow stays.
        let elbow = vec![(0.0, 0.0), (80.0, 0.0), (80.0, 60.0), (140.0, 60.0)];
        assert_eq!(simplify_route(&elbow, KINK_PX).len(), 4);
    }

    #[test]
    fn orthogonal_keeps_sharp_corners() {
        let pts = vec![(0.0, 0.0), (80.0, 0.0), (80.0, 60.0), (140.0, 60.0)];
        assert!(!should_fillet(&pts, Some(EdgeRouting::Orthogonal)));
        assert!(!should_fillet(&pts, Some(EdgeRouting::Polyline))); // axis-aligned
        assert!(!should_fillet(&pts, None));
        let d = shaft_path_d(&pts, Some(EdgeRouting::Orthogonal));
        assert!(!d.contains('Q'));
        assert!(d.matches('L').count() >= 2);
    }

    #[test]
    fn polyline_diagonal_gets_fillet() {
        let pts = vec![(0.0, 0.0), (80.0, 0.0), (120.0, 60.0), (180.0, 60.0)];
        assert!(should_fillet(&pts, Some(EdgeRouting::Polyline)));
        assert!(should_fillet(&pts, Some(EdgeRouting::Splines)));
        assert!(should_fillet(&pts, None));
        let d = shaft_path_d(&pts, Some(EdgeRouting::Polyline));
        assert!(d.contains('Q'));
    }

    #[test]
    fn filleted_path_edge_cases() {
        assert!(filleted_path_d(&[], TURN_RADIUS).is_empty());
        assert!(!filleted_path_d(&[(0.0, 0.0), (10.0, 0.0)], TURN_RADIUS).contains('Q'));
        assert!(!filleted_path_d(&[(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)], 0.0).contains('Q'));
        // Near-collinear corner → L, not Q.
        let almost = vec![(0.0, 0.0), (50.0, 0.0), (100.0, 0.1)];
        let d = filleted_path_d(&almost, TURN_RADIUS);
        assert!(d.contains('L'));
        // Tiny arms skip fillet.
        let tiny = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)];
        assert!(!filleted_path_d(&tiny, TURN_RADIUS).contains('Q'));
    }

    #[test]
    fn axis_aligned_and_point_line() {
        assert!(axis_aligned(&[(0.0, 0.0)], 1e-3));
        assert!(axis_aligned(&[(0.0, 0.0), (1.0, 0.0), (1.0, 2.0)], 1e-3));
        assert!(!axis_aligned(&[(0.0, 0.0), (1.0, 1.0)], 1e-3));
        assert!(point_line_distance(0.0, 1.0, 0.0, 0.0, 0.0, 0.0) >= 0.0);
        assert!((point_line_distance(1.0, 1.0, 0.0, 0.0, 2.0, 0.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn shorten_polyline_edge_cases() {
        let (s, tip, _) = shorten_polyline_end(&[], 5.0);
        assert_eq!(s.len(), 1);
        assert_eq!(tip, (0.0, 0.0));
        let (s, tip, dir) = shorten_polyline_end(&[(1.0, 2.0)], 5.0);
        assert_eq!(tip, (1.0, 2.0));
        assert_eq!(dir, (1.0, 0.0));
        assert_eq!(s, vec![(1.0, 2.0)]);
        // Zero-length segment skipped.
        let pts = vec![(0.0, 0.0), (0.0, 0.0), (100.0, 0.0)];
        let (s, _, (ux, _)) = shorten_polyline_end(&pts, 10.0);
        assert!(s.len() >= 2);
        assert!(ux > 0.0);
        // Trim longer than path.
        let (s, _, _) = shorten_polyline_end(&[(0.0, 0.0), (5.0, 0.0)], 20.0);
        assert!(s.len() <= 2);
    }

    fn painted_shaft_and_head(prims: &[EdgePrim]) -> (&str, &str) {
        let shaft = prims.iter().find_map(|p| match p {
            EdgePrim::Path { d, class, .. }
                if class.contains("edge")
                    && !class.contains("head")
                    && !class.contains("harpoon") =>
            {
                Some(d.as_str())
            }
            _ => None,
        });
        let head = prims.iter().find_map(|p| match p {
            EdgePrim::Path { d, class, .. } if class.contains("head") => Some(d.as_str()),
            _ => None,
        });
        (shaft.expect("shaft"), head.expect("head"))
    }

    fn assert_forward_head_locked_to_route(pts: Vec<Pt>, curve_catmull: bool) {
        let mut edge = paint(pts.clone(), EdgeArrow::Forward, None);
        edge.curve_catmull = curve_catmull;
        edge.label = None;
        edge.stroke_width = Some(4.8);
        let simplified = simplify_route(&pts, KINK_PX);
        let tip = *simplified.last().expect("route");
        let prev = simplified[simplified.len() - 2];
        let head_size =
            head_px(edge.stroke_width).min((polyline_length(&simplified) * 0.45).max(HEAD * 0.5));
        let (expected_shaft, base) = if curve_catmull {
            let chord = (tip.0 - prev.0).hypot(tip.1 - prev.1).max(1e-9);
            let h = head_size.min(chord * 0.9);
            let (ux, uy) = ((tip.0 - prev.0) / chord, (tip.1 - prev.1) / chord);
            let base = (tip.0 - ux * h, tip.1 - uy * h);
            let mut shaft_pts = simplified.clone();
            let last = shaft_pts.len() - 1;
            shaft_pts[last] = base;
            (
                crate::catmull::centripetal_catmull_rom_open_path(&shaft_pts),
                base,
            )
        } else {
            let (shaft, _, _) = shorten_polyline_end(&simplified, head_size);
            let base = *shaft.last().unwrap_or(&tip);
            (shaft_path_d(&shaft, None), base)
        };
        let prims = edge_primitives(&edge);
        let (shaft, head) = painted_shaft_and_head(&prims);
        assert_eq!(
            shaft, expected_shaft,
            "shaft must interpolate the last routing points and end at the head base"
        );
        assert_eq!(
            head,
            filled_arrow_head_d(tip, base, head_size),
            "head must be the triangle of the last control and that base"
        );
    }

    #[test]
    fn catmull_forward_head_meets_shaft() {
        assert_forward_head_locked_to_route(vec![(0.0, 0.0), (80.0, 0.0), (80.0, 80.0)], true);
        assert_forward_head_locked_to_route(
            vec![(0.0, 0.0), (40.0, 50.0), (90.0, 10.0), (160.0, 80.0)],
            true,
        );
        assert_forward_head_locked_to_route(vec![(0.0, 0.0), (140.0, 20.0)], true);
        assert_forward_head_locked_to_route(vec![(0.0, 0.0), (80.0, 0.0), (80.0, 80.0)], false);
        assert_forward_head_locked_to_route(
            vec![(0.0, 0.0), (40.0, 50.0), (90.0, 10.0), (160.0, 80.0)],
            false,
        );
    }

    #[test]
    fn open_head_uses_the_same_base_as_the_shaft() {
        let pts = vec![(0.0, 0.0), (50.0, 40.0), (120.0, 15.0), (180.0, 90.0)];
        let mut edge = paint(pts.clone(), EdgeArrow::Open, None);
        edge.curve_catmull = true;
        edge.label = None;
        edge.stroke_width = Some(4.8);
        let simplified = simplify_route(&pts, KINK_PX);
        let tip = *simplified.last().unwrap();
        let prev = simplified[simplified.len() - 2];
        let head_size =
            head_px(edge.stroke_width).min((polyline_length(&simplified) * 0.45).max(HEAD * 0.5));
        let chord = (tip.0 - prev.0).hypot(tip.1 - prev.1).max(1e-9);
        let h = head_size.min(chord * 0.9);
        let (ux, uy) = ((tip.0 - prev.0) / chord, (tip.1 - prev.1) / chord);
        let base = (tip.0 - ux * h, tip.1 - uy * h);
        let mut shaft_pts = simplified.clone();
        let last = shaft_pts.len() - 1;
        shaft_pts[last] = base;
        let expected_shaft = crate::catmull::centripetal_catmull_rom_open_path(&shaft_pts);
        let (px, py) = perp(ux, uy);
        let half = head_size * 0.55;
        let expected_head = format!(
            "M {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2}",
            base.0 + px * half,
            base.1 + py * half,
            tip.0,
            tip.1,
            base.0 - px * half,
            base.1 - py * half
        );
        let prims = edge_primitives(&edge);
        let (shaft, head) = painted_shaft_and_head(&prims);
        assert_eq!(shaft, expected_shaft);
        assert_eq!(head, expected_head);
    }

    #[test]
    fn catmull_shaft_is_shared_open_path() {
        let pts = vec![(0.0, 0.0), (80.0, 0.0), (80.0, 80.0)];
        let mut edge = paint(pts.clone(), EdgeArrow::Line, None);
        edge.curve_catmull = true;
        edge.label = None;
        let prims = edge_primitives(&edge);
        let d = prims.iter().find_map(|p| match p {
            EdgePrim::Path { d, class, .. }
                if class.contains("shaft") || class.contains("edge") =>
            {
                Some(d.as_str())
            }
            _ => None,
        });
        let d = d.expect("shaft path");
        let expected =
            crate::catmull::centripetal_catmull_rom_open_path(&simplify_route(&pts, KINK_PX));
        assert_eq!(d, expected);
        assert!(d.contains('C'), "{d}");
        assert!(!d.contains('Z'), "{d}");
    }

    #[test]
    fn forward_edge_emits_shaft_and_head() {
        let edge = paint(vec![(0.0, 0.0), (100.0, 0.0)], EdgeArrow::Forward, None);
        let prims = edge_primitives(&edge);
        assert!(prims.len() >= 2);
        assert!(prims.iter().any(|p| matches!(p, EdgePrim::Text { .. })));
        assert!(prims.iter().any(|p| matches!(
            p,
            EdgePrim::Path {
                stroke_dasharray: Some(_),
                ..
            }
        )));
    }

    #[test]
    fn line_open_equilibrium_arrows() {
        let pts = vec![(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (80.0, 40.0)];
        for arrow in [EdgeArrow::Line, EdgeArrow::Open, EdgeArrow::Equilibrium] {
            let mut edge = paint(pts.clone(), arrow, Some(EdgeRouting::Orthogonal));
            edge.label_pos = Some("above".into());
            let prims = edge_primitives(&edge);
            assert!(!prims.is_empty(), "{arrow:?}");
        }
        // Short path → no paint.
        let short = paint(vec![(0.0, 0.0), (1.0, 0.0)], EdgeArrow::Line, None);
        assert!(edge_primitives(&short).is_empty());
    }

    #[test]
    fn edge_anchors_on_box_boundary() {
        let (p0, p1) = edge_anchors((0.0, 0.0, 100.0, 80.0), (200.0, 0.0, 100.0, 80.0), 0.0);
        assert!((p0.0 - 100.0).abs() < 1e-6);
        assert!((p1.0 - 200.0).abs() < 1e-6);
        assert!((p0.1 - 40.0).abs() < 1e-6);
        let same = clip_box_edge(0.0, 0.0, 0.0, 0.0, 10.0, 10.0, 0.0);
        assert_eq!(same, (0.0, 0.0));
    }

    #[test]
    fn offset_and_label_helpers() {
        assert_eq!(offset_polyline(&[(0.0, 0.0)], 2.0).len(), 1);
        let off = offset_polyline(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], 2.0);
        assert_eq!(off.len(), 3);
        // Miter keeps both segments exactly `dist` away (90° corner).
        assert!((off[0].1 - 2.0).abs() < 1e-9, "{off:?}");
        assert!(
            (off[1].0 - 8.0).abs() < 1e-9 && (off[1].1 - 2.0).abs() < 1e-9,
            "{off:?}"
        );
        assert!((off[2].0 - 8.0).abs() < 1e-9, "{off:?}");
        let (x, y, _, _) = label_point(&[]);
        assert_eq!((x, y), (0.0, 0.0));
        let _ = label_point(&[(5.0, 5.0)]);
    }
}
