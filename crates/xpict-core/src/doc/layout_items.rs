//! Occupancy + greedy constraint placer for mol chrome text.
//!
//! Chem ink (bonds, atom disks) is fixed obstacles. Callouts / region labels /
//! similar annotations register a preferred side + size and grow gap until clear.

use crate::metrics::BOND_PX;
use crate::scene::{AtomIn, MoleculeIn};

/// Default “no clash” air: 0.1 CC-bond.
pub const CLASH_MARGIN_PX: f64 = 0.1 * BOND_PX;
/// Name / outside-region hug: 0.5 CC-bond.
pub const HALF_BOND_PX: f64 = 0.5 * BOND_PX;
/// Atom treated as a disk for contain / clash (not a dimensionless point).
pub const ATOM_DISK_R_PX: f64 = 0.3 * BOND_PX;
/// Extra pad when a shape must **contain** another (on top of the contained size).
pub const CONTAIN_MARGIN_PX: f64 = CLASH_MARGIN_PX;

/// Axis-aligned obstacle or candidate label box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Aabb {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self {
            x0: x0.min(x1),
            y0: y0.min(y1),
            x1: x0.max(x1),
            y1: y0.max(y1),
        }
    }

    pub fn width(self) -> f64 {
        self.x1 - self.x0
    }

    pub fn height(self) -> f64 {
        self.y1 - self.y0
    }

    pub fn inflate(self, m: f64) -> Self {
        Self::new(self.x0 - m, self.y0 - m, self.x1 + m, self.y1 + m)
    }

    pub fn overlaps(self, other: Aabb) -> bool {
        self.x0 < other.x1 && self.x1 > other.x0 && self.y0 < other.y1 && self.y1 > other.y0
    }

    pub fn center(self) -> (f64, f64) {
        ((self.x0 + self.x1) * 0.5, (self.y0 + self.y1) * 0.5)
    }

    pub fn translated(self, dx: f64, dy: f64) -> Self {
        Self::new(self.x0 + dx, self.y0 + dy, self.x1 + dx, self.y1 + dy)
    }
}

/// Bond (or other) line-segment obstacle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seg {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Seg {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// Approximate thick segment as a stadium AABB (cheap clash).
    pub fn thick_aabb(self, half_w: f64) -> Aabb {
        Aabb::new(self.x0, self.y0, self.x1, self.y1).inflate(half_w)
    }
}

/// Preferred placement side relative to an anchor / region edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Above,
    Below,
}

impl Side {
    pub fn all_cw_from(self) -> [Side; 4] {
        match self {
            Side::Right => [Side::Right, Side::Above, Side::Left, Side::Below],
            Side::Left => [Side::Left, Side::Below, Side::Right, Side::Above],
            Side::Above => [Side::Above, Side::Right, Side::Below, Side::Left],
            Side::Below => [Side::Below, Side::Left, Side::Above, Side::Right],
        }
    }
}

/// Fixed chem obstacles in SVG / document space.
#[derive(Debug, Clone, Default)]
pub struct Occupancy {
    pub aabbs: Vec<Aabb>,
    pub segs: Vec<Seg>,
    /// Half-width used when expanding segments for clash tests.
    pub seg_half: f64,
}

impl Occupancy {
    pub fn new() -> Self {
        Self {
            aabbs: Vec::new(),
            segs: Vec::new(),
            seg_half: 1.5,
        }
    }

    /// Bond segments only (region labels sit among atoms — avoid bond ink).
    pub fn from_bonds(mol: &MoleculeIn, dx: f64, dy: f64, seg_half: f64) -> Self {
        let mut occ = Self::new();
        occ.seg_half = seg_half.max(0.5);
        for b in &mol.bonds {
            let Some(a) = find_atom(mol, b.begin) else {
                continue;
            };
            let Some(c) = find_atom(mol, b.end) else {
                continue;
            };
            occ.segs
                .push(Seg::new(a.x + dx, a.y + dy, c.x + dx, c.y + dy));
        }
        occ
    }

    /// Bond segments + disks at atoms (callout tip / free annotations).
    ///
    /// When `atom_r` is `None`, uses [`ATOM_DISK_R_PX`] (~0.3 CC-bond).
    pub fn from_molecule(mol: &MoleculeIn, dx: f64, dy: f64, atom_r: Option<f64>) -> Self {
        let r = atom_r.unwrap_or(ATOM_DISK_R_PX).max(1.0);
        let mut occ = Self::from_bonds(mol, dx, dy, (r * 0.35).max(0.5));
        for a in &mol.atoms {
            occ.aabbs.push(Aabb::new(
                a.x + dx - r,
                a.y + dy - r,
                a.x + dx + r,
                a.y + dy + r,
            ));
        }
        occ
    }

    /// Register an extra obstacle (placed label, chem glyph AABB, …).
    #[allow(dead_code)] // public IR; chrome callers will register labels next
    pub fn add_aabb(&mut self, box_: Aabb) {
        self.aabbs.push(box_);
    }

    pub fn clashes(&self, cand: Aabb, margin: f64) -> bool {
        self.clash_penalty(cand, margin) > 0.0
    }

    /// How badly `cand` overlaps obstacles (0 = clear). Bond crossings weigh
    /// more than soft AABB overlaps so greedy fallbacks avoid straddling ink.
    pub fn clash_penalty(&self, cand: Aabb, margin: f64) -> f64 {
        let mut pen = 0.0;
        let padded = cand.inflate(margin);
        for &a in &self.aabbs {
            if padded.overlaps(a) {
                let ox = (padded.x1.min(a.x1) - padded.x0.max(a.x0)).max(0.0);
                let oy = (padded.y1.min(a.y1) - padded.y0.max(a.y0)).max(0.0);
                pen += ox * oy;
            }
        }
        let need = self.seg_half + margin;
        for &s in &self.segs {
            // Edge distance to the segment (not a loose stadium AABB) so diagonal
            // bonds don't false-clear labels that sit on the stroke.
            let d = seg_aabb_min_dist(s.x0, s.y0, s.x1, s.y1, cand);
            if d < need {
                let depth = need - d;
                // Quadratic: deep crossings (label on a bond) dominate shallow
                // grazes so the least-clash seed won't pick a straddling side.
                pen += 200.0 * depth * depth;
            }
        }
        pen
    }
}

/// Minimum distance from segment to AABB (≥0 outside; 0 if they intersect).
fn seg_aabb_min_dist(x0: f64, y0: f64, x1: f64, y1: f64, a: Aabb) -> f64 {
    let n = 16;
    let mut best = f64::INFINITY;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let px = x0 + t * (x1 - x0);
        let py = y0 + t * (y1 - y0);
        if px >= a.x0 && px <= a.x1 && py >= a.y0 && py <= a.y1 {
            return 0.0;
        }
        let cx = px.clamp(a.x0, a.x1);
        let cy = py.clamp(a.y0, a.y1);
        best = best.min((px - cx).hypot(py - cy));
    }
    for &(cx, cy) in &[(a.x0, a.y0), (a.x1, a.y0), (a.x1, a.y1), (a.x0, a.y1)] {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len2 = dx * dx + dy * dy;
        let t = if len2 < 1e-18 {
            0.0
        } else {
            ((cx - x0) * dx + (cy - y0) * dy) / len2
        }
        .clamp(0.0, 1.0);
        let qx = x0 + t * dx;
        let qy = y0 + t * dy;
        best = best.min((cx - qx).hypot(cy - qy));
    }
    best
}

fn find_atom(mol: &MoleculeIn, index: i32) -> Option<&AtomIn> {
    mol.atoms.iter().find(|a| a.index == index)
}

/// Label ink relative to a trial origin (from `markup_bounds_at_origin`).
#[derive(Debug, Clone, Copy)]
pub struct InkAtOrigin {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl InkAtOrigin {
    pub fn translated(self, x: f64, y: f64) -> Aabb {
        Aabb::new(self.x0 + x, self.y0 + y, self.x1 + x, self.y1 + y)
    }
}

/// Request to place a text item near an anchor.
#[derive(Debug, Clone)]
pub struct PlaceRequest {
    /// Point the annotation refers to (atom, region edge mid, …).
    pub anchor: (f64, f64),
    pub ink: InkAtOrigin,
    pub preferred: Side,
    /// Gap from anchor to nearest label edge (start); grown on clash.
    pub min_gap: f64,
    pub max_gap: f64,
    pub gap_step: f64,
    /// Extra air between label AABB and obstacles.
    pub margin: f64,
    /// If set, arrow tip stops this far short of the anchor (callout standoff).
    pub tip_standoff: Option<f64>,
    /// Pull arrow start off the label edge toward the tip (air before ink).
    pub label_pullback: f64,
}

/// Resolved placement for paint.
#[derive(Debug, Clone)]
pub struct Placement {
    /// Origin passed to outline_markup (baseline / anchor point).
    pub x: f64,
    pub y: f64,
    pub bounds: Aabb,
    #[allow(dead_code)] // useful for tests / next chrome migrants
    pub side: Side,
    /// Arrow from label edge → tip (callouts).
    #[allow(dead_code)] // paint may rebuild the segment after refine
    pub arrow_from: Option<(f64, f64)>,
    pub arrow_to: Option<(f64, f64)>,
}

/// Greedy place: try preferred side, grow gap, then rotate through alternates.
///
/// Returns the first **clear** pose. If every (side, gap) in range still hits
/// ink, returns the candidate with the **lowest** [`Occupancy::clash_penalty`]
/// (preferred side / smaller gap as tiny tie-breaks) — never the last CW slot
/// by accident, which could seed a label across a bond.
pub fn place_greedy(occ: &Occupancy, req: &PlaceRequest) -> Placement {
    let sides = req.preferred.all_cw_from();
    let mut best_clash: Option<(f64, Placement)> = None;
    for (si, side) in sides.into_iter().enumerate() {
        let mut gap = req.min_gap;
        while gap <= req.max_gap + 1e-9 {
            let p = force_place(req, side, gap);
            let pen = occ.clash_penalty(p.bounds, req.margin);
            if pen <= 1e-12 {
                return p;
            }
            // Prefer lower penalty; then earlier side (preferred first); then
            // smaller gap (snugger seed for the AL refine).
            let score = pen + si as f64 * 1e-3 + (gap - req.min_gap) * 1e-6;
            let replace = match &best_clash {
                None => true,
                Some((s, _)) => score < *s,
            };
            if replace {
                best_clash = Some((score, p));
            }
            gap += req.gap_step;
        }
    }
    best_clash
        .map(|(_, p)| p)
        .unwrap_or_else(|| force_place(req, req.preferred, req.min_gap))
}

/// Side with the lowest clash at `min_gap` (preferred as tie-break).
///
/// Used for wide `atom_label` text so the seed does not start on a bond just
/// because the chem-travel alternate pointed that way.
pub fn least_clash_side(occ: &Occupancy, req: &PlaceRequest) -> Side {
    let mut best = req.preferred;
    let mut best_score = f64::INFINITY;
    for (si, side) in req.preferred.all_cw_from().into_iter().enumerate() {
        let p = force_place(req, side, req.min_gap);
        let pen = occ.clash_penalty(p.bounds, req.margin);
        let score = pen + si as f64 * 1e-6;
        if score < best_score {
            best_score = score;
            best = side;
        }
    }
    best
}

fn force_place(req: &PlaceRequest, side: Side, gap: f64) -> Placement {
    let (ax, ay) = req.anchor;
    let ink = req.ink;
    let (x, y) = match side {
        Side::Right => {
            // Label left edge at ax + gap.
            let x = ax + gap - ink.x0;
            let y = ay - (ink.y0 + ink.y1) * 0.5;
            (x, y)
        }
        Side::Left => {
            // Label right edge at ax - gap.
            let x = ax - gap - ink.x1;
            let y = ay - (ink.y0 + ink.y1) * 0.5;
            (x, y)
        }
        Side::Above => {
            // Label bottom at ay - gap.
            let x = ax - (ink.x0 + ink.x1) * 0.5;
            let y = ay - gap - ink.y1;
            (x, y)
        }
        Side::Below => {
            // Label top at ay + gap.
            let x = ax - (ink.x0 + ink.x1) * 0.5;
            let y = ay + gap - ink.y0;
            (x, y)
        }
    };
    let bounds = ink.translated(x, y);
    let (arrow_from, arrow_to) = match req.tip_standoff {
        Some(st) => callout_arrow(bounds, (ax, ay), side, st, req.label_pullback),
        None => (None, None),
    };
    Placement {
        x,
        y,
        bounds,
        side,
        arrow_from,
        arrow_to,
    }
}

/// Arrow segment pulled back from the label edge **and** short of the anchor.
///
/// `label_pullback`: start moves off the label AABB toward the tip.
/// `tip_standoff`: tip stops this far short of the atom/target (clears chem).
pub fn callout_arrow(
    bounds: Aabb,
    anchor: (f64, f64),
    side: Side,
    tip_standoff: f64,
    label_pullback: f64,
) -> (Option<(f64, f64)>, Option<(f64, f64)>) {
    let (ax, ay) = anchor;
    let edge = match side {
        Side::Right => (bounds.x0, bounds.center().1),
        Side::Left => (bounds.x1, bounds.center().1),
        Side::Above => (bounds.center().0, bounds.y1),
        Side::Below => (bounds.center().0, bounds.y0),
    };
    let dx = ax - edge.0;
    let dy = ay - edge.1;
    let len = dx.hypot(dy).max(1e-6);
    let ux = dx / len;
    let uy = dy / len;
    let pull = label_pullback.max(0.0).min(len * 0.4);
    let from = (edge.0 + ux * pull, edge.1 + uy * pull);
    // Tip short of anchor; leave room for the label pullback.
    let max_st = (len - pull).max(0.0) * 0.9;
    let st = tip_standoff.max(0.0).min(max_st);
    let to = (ax - ux * st, ay - uy * st);
    // Degenerate: keep a short forward stub.
    let span = (to.0 - from.0) * ux + (to.1 - from.1) * uy;
    if span < 1.0 {
        let mid = pull + (len - pull) * 0.5;
        let from = (
            edge.0 + ux * (mid - 0.5).max(pull),
            edge.1 + uy * (mid - 0.5).max(pull),
        );
        let to = (
            edge.0 + ux * (mid + 0.5).min(len - 0.5),
            edge.1 + uy * (mid + 0.5).min(len - 0.5),
        );
        return (Some(from), Some(to));
    }
    (Some(from), Some(to))
}

/// Tip point on the preferred side of `anchor`, `standoff` px away.
pub fn tip_point_from_anchor(anchor: (f64, f64), side: Side, standoff: f64) -> (f64, f64) {
    let (ax, ay) = anchor;
    match side {
        Side::Right => (ax + standoff, ay),
        Side::Left => (ax - standoff, ay),
        Side::Above => (ax, ay - standoff),
        Side::Below => (ax, ay + standoff),
    }
}

/// Grow tip standoff until the tip clears every obstacle AABB (+ margin).
///
/// Used so a left callout tip does not land in the H of a west-facing OH.
pub fn tip_standoff_clearing(
    anchor: (f64, f64),
    side: Side,
    base: f64,
    obstacles: &[Aabb],
    margin: f64,
    max_standoff: f64,
) -> f64 {
    let mut st = base.max(0.0);
    while st <= max_standoff + 1e-9 {
        let (tx, ty) = tip_point_from_anchor(anchor, side, st);
        let tip_box = Aabb::new(tx, ty, tx, ty).inflate(margin.max(0.0));
        let hit = obstacles.iter().any(|&o| tip_box.overlaps(o));
        if !hit {
            return st;
        }
        st += 2.0;
    }
    max_standoff.max(base)
}

/// Place region text against an edge of the region AABB (inside or outside).
///
/// Does **not** mutate region geometry — only returns a text origin.
/// Exhausts slide/bump on the preferred side before trying alternates so an
/// explicit `pos: above` stays near the top when possible.
pub fn place_region_edge(
    occ: &Occupancy,
    region: Aabb,
    ink: InkAtOrigin,
    inside: bool,
    preferred: Side,
    edge_margin: f64,
    clash_margin: f64,
    // `about`: reference for `pos` (Left/Right/Above/Below) — mean of contained atoms.
    about: (f64, f64),
    // Outside labels may rotate CW when the preferred side is blocked; inside
    // seeds stay on `pos` so a grown hull is not parked on the wrong edge.
    flip_sides: bool,
) -> Placement {
    let sides: Vec<Side> = if flip_sides {
        preferred.all_cw_from().to_vec()
    } else {
        vec![preferred]
    };
    let best = force_region(region, ink, inside, preferred, edge_margin, 0.0, about);
    // Outside / clash-free inside seeds may bump out before the hull grows.
    let max_bump = BOND_PX.max(24.0);
    let bumps = [
        0.0, 2.0, 4.0, 6.0, 8.0, 12.0, 16.0, 20.0, 24.0, 28.0, 32.0,
    ];
    // Prefer slide=0 (aligned with `about`) before skirting up/down the edge —
    // early negative slides parked Left seeds above the ring.
    let slides = [
        0.0, 0.15, -0.15, 0.3, -0.3, 0.45, -0.45, 0.55, -0.55, 0.7, -0.7,
    ];
    let mut least_bad = best;
    let mut least_pen = f64::INFINITY;
    for side in sides {
        for &bump in &bumps {
            if bump > max_bump + 1e-9 {
                continue;
            }
            for &t in &slides {
                let p = force_region(region, ink, inside, side, edge_margin + bump, t, about);
                if !occ.clashes(p.bounds, clash_margin) {
                    return p;
                }
                let pen = clash_penalty(occ, p.bounds, clash_margin);
                if pen < least_pen {
                    least_pen = pen;
                    least_bad = p;
                }
            }
        }
        // Inside: never leave the requested side.
        if !flip_sides {
            return least_bad;
        }
    }
    least_bad
}

fn clash_penalty(occ: &Occupancy, cand: Aabb, margin: f64) -> f64 {
    let padded = cand.inflate(margin);
    let mut pen = 0.0;
    for &a in &occ.aabbs {
        if padded.overlaps(a) {
            let ox = (padded.x1.min(a.x1) - padded.x0.max(a.x0)).max(0.0);
            let oy = (padded.y1.min(a.y1) - padded.y0.max(a.y0)).max(0.0);
            pen += ox * oy;
        }
    }
    for &s in &occ.segs {
        let thick = s.thick_aabb(occ.seg_half);
        if padded.overlaps(thick) {
            let ox = (padded.x1.min(thick.x1) - padded.x0.max(thick.x0)).max(0.0);
            let oy = (padded.y1.min(thick.y1) - padded.y0.max(thick.y0)).max(0.0);
            pen += ox * oy;
        }
    }
    pen
}

fn force_region(
    region: Aabb,
    ink: InkAtOrigin,
    inside: bool,
    side: Side,
    margin: f64,
    slide_frac: f64,
    about: (f64, f64),
) -> Placement {
    // Slide along the preferred edge about the atom-mean, not the AABB center —
    // grown hulls / asymmetric includes shift the box while `pos` stays on atoms.
    let (cx, cy) = about;
    let hw = region.width() * 0.5;
    let hh = region.height() * 0.5;
    let (x, y) = if inside {
        match side {
            Side::Above => {
                // Near top inner edge.
                let x = cx + slide_frac * hw * 1.6 - (ink.x0 + ink.x1) * 0.5;
                let y = region.y0 + margin - ink.y0;
                (x, y)
            }
            Side::Below => {
                let x = cx + slide_frac * hw * 1.6 - (ink.x0 + ink.x1) * 0.5;
                let y = region.y1 - margin - ink.y1;
                (x, y)
            }
            Side::Left => {
                let y = cy + slide_frac * hh * 1.6 - (ink.y0 + ink.y1) * 0.5;
                let x = region.x0 + margin - ink.x0;
                (x, y)
            }
            Side::Right => {
                let y = cy + slide_frac * hh * 1.6 - (ink.y0 + ink.y1) * 0.5;
                let x = region.x1 - margin - ink.x1;
                (x, y)
            }
        }
    } else {
        match side {
            Side::Above => {
                let x = cx + slide_frac * hw * 1.6 - (ink.x0 + ink.x1) * 0.5;
                let y = region.y0 - margin - ink.y1;
                (x, y)
            }
            Side::Below => {
                let x = cx + slide_frac * hw * 1.6 - (ink.x0 + ink.x1) * 0.5;
                let y = region.y1 + margin - ink.y0;
                (x, y)
            }
            Side::Left => {
                let y = cy + slide_frac * hh * 1.6 - (ink.y0 + ink.y1) * 0.5;
                let x = region.x0 - margin - ink.x1;
                (x, y)
            }
            Side::Right => {
                let y = cy + slide_frac * hh * 1.6 - (ink.y0 + ink.y1) * 0.5;
                let x = region.x1 + margin - ink.x0;
                (x, y)
            }
        }
    };
    Placement {
        x,
        y,
        bounds: ink.translated(x, y),
        side,
        arrow_from: None,
        arrow_to: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greedy_grows_gap_past_obstacle() {
        let mut occ = Occupancy::new();
        // Block the near-right slot.
        occ.add_aabb(Aabb::new(10.0, -5.0, 25.0, 5.0));
        let ink = InkAtOrigin {
            x0: 0.0,
            y0: -4.0,
            x1: 20.0,
            y1: 4.0,
        };
        let req = PlaceRequest {
            anchor: (0.0, 0.0),
            ink,
            preferred: Side::Right,
            min_gap: 4.0,
            max_gap: 80.0,
            gap_step: 4.0,
            margin: 1.0,
            tip_standoff: None,
            label_pullback: 0.0,
        };
        let p = place_greedy(&occ, &req);
        assert!(
            p.bounds.x0 >= 25.0 - 1.0,
            "should clear obstacle: {:?}",
            p.bounds
        );
        assert_eq!(p.side, Side::Right);
    }

    #[test]
    fn greedy_fallback_avoids_bond_straddle() {
        // Horizontal bond through the anchor: left/right seeds sit on the stroke.
        // With a short max_gap, preferred Right cannot clear — pick Above/Below.
        let mut occ = Occupancy::new();
        occ.seg_half = 1.5;
        occ.segs.push(Seg::new(-50.0, 0.0, 50.0, 0.0));
        let ink = InkAtOrigin {
            x0: -12.0,
            y0: -5.0,
            x1: 12.0,
            y1: 5.0,
        };
        let req = PlaceRequest {
            anchor: (0.0, 0.0),
            ink,
            preferred: Side::Right,
            min_gap: 2.0,
            max_gap: 5.0,
            gap_step: 1.0,
            margin: 1.0,
            tip_standoff: None,
            label_pullback: 0.0,
        };
        let p = place_greedy(&occ, &req);
        assert!(
            matches!(p.side, Side::Above | Side::Below),
            "least-clash fallback should leave the bond, got {:?}",
            p.side
        );
        assert!(
            !occ.clashes(p.bounds, req.margin),
            "vertical seed should clear the horizontal bond: {:?}",
            p.bounds
        );
        assert_eq!(least_clash_side(&occ, &req), p.side);
    }

    #[test]
    fn region_inside_above_hugs_top_not_center() {
        let occ = Occupancy::new();
        let region = Aabb::new(0.0, 0.0, 100.0, 80.0);
        let ink = InkAtOrigin {
            x0: -10.0,
            y0: -5.0,
            x1: 10.0,
            y1: 5.0,
        };
        let p = place_region_edge(
            &occ,
            region,
            ink,
            true,
            Side::Above,
            3.0,
            1.0,
            region.center(),
            false,
        );
        // Label top near region.y0 + margin.
        assert!(
            p.bounds.y0 < 20.0,
            "inside-above should hug top, got y0={}",
            p.bounds.y0
        );
        assert!(p.bounds.y1 < 40.0, "should not sit at center");
    }

    #[test]
    fn tip_standoff_grows_past_chem_box() {
        let anchor = (40.0, 0.0);
        // West-facing OH occupying space left of the atom.
        let oh = Aabb::new(20.0, -5.0, 38.0, 5.0);
        let st = tip_standoff_clearing(anchor, Side::Left, 6.0, &[oh], 2.0, 80.0);
        let tip = tip_point_from_anchor(anchor, Side::Left, st);
        assert!(
            tip.0 <= oh.x0 - 2.0 + 1e-6,
            "tip {:?} should clear past oh.x0={} (st={st})",
            tip,
            oh.x0
        );
        assert!(st > 6.0, "should grow beyond base standoff");
    }

    #[test]
    fn tip_standoff_short_of_anchor() {
        let occ = Occupancy::new();
        let ink = InkAtOrigin {
            x0: 0.0,
            y0: -4.0,
            x1: 16.0,
            y1: 4.0,
        };
        let standoff = 6.0;
        let req = PlaceRequest {
            anchor: (0.0, 0.0),
            ink,
            preferred: Side::Left,
            min_gap: 20.0,
            max_gap: 40.0,
            gap_step: 4.0,
            margin: 0.0,
            tip_standoff: Some(standoff),
            label_pullback: 0.0,
        };
        let p = place_greedy(&occ, &req);
        let to = p.arrow_to.expect("tip");
        let dist = to.0.hypot(to.1);
        assert!(
            (dist - standoff).abs() < 0.5,
            "tip dist {dist} vs standoff {standoff}"
        );
    }
}
