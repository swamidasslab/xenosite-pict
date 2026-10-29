//! Occupancy + greedy constraint placer for mol chrome text.
//!
//! Chem ink (bonds, atom disks) is fixed obstacles. Callouts / region labels /
//! similar annotations register a preferred side + size and grow gap until clear.

use crate::metrics::BOND_PX;
use crate::scene::{AtomIn, MoleculeIn};

/// Default “no clash” air: 0.1 CC-bond.
pub const CLASH_MARGIN_PX: f64 = 0.1 * BOND_PX;
/// Extra layout width on every label so neighboring text is not jammed.
///
/// Paint is unchanged; only the clash / occupancy box grows in x.
pub const LABEL_PAD_X_PX: f64 = 0.2 * BOND_PX;
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

    /// Grow independently in x and y (label horizontal air uses `my = 0`).
    pub fn inflate_xy(self, mx: f64, my: f64) -> Self {
        Self::new(self.x0 - mx, self.y0 - my, self.x1 + mx, self.y1 + my)
    }

    pub fn overlaps(self, other: Aabb) -> bool {
        self.x0 < other.x1 && self.x1 > other.x0 && self.y0 < other.y1 && self.y1 > other.y0
    }

    /// True when `other` lies entirely inside this box (edges count as inside).
    pub fn contains(self, other: Aabb) -> bool {
        self.x0 <= other.x0 && self.y0 <= other.y0 && self.x1 >= other.x1 && self.y1 >= other.y1
    }

    /// True when `p` lies inside this box, expanded by `pad`.
    pub fn covers_point(self, p: (f64, f64), pad: f64) -> bool {
        self.x0 - pad <= p.0 && p.0 <= self.x1 + pad && self.y0 - pad <= p.1 && p.1 <= self.y1 + pad
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
///
/// Four cardinals plus four 45° diagonals (SVG +y down).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Above,
    Below,
    AboveLeft,
    AboveRight,
    BelowLeft,
    BelowRight,
}

impl Side {
    /// Clockwise from Right, including 45° diagonals.
    pub const ALL_8: [Side; 8] = [
        Side::Right,
        Side::BelowRight,
        Side::Below,
        Side::BelowLeft,
        Side::Left,
        Side::AboveLeft,
        Side::Above,
        Side::AboveRight,
    ];

    pub fn is_diagonal(self) -> bool {
        matches!(
            self,
            Side::AboveLeft | Side::AboveRight | Side::BelowLeft | Side::BelowRight
        )
    }

    /// Rotate through alternates. Cardinals stay on the four axis slots so a
    /// locked `prefer: above` does not jump to a 45° seed; diagonals walk all 8.
    pub fn all_cw_from(self) -> Vec<Side> {
        if self.is_diagonal() {
            let i = Self::ALL_8.iter().position(|&s| s == self).unwrap_or(0);
            let mut out = Self::ALL_8.to_vec();
            out.rotate_left(i);
            out
        } else {
            match self {
                Side::Right => vec![Side::Right, Side::Above, Side::Left, Side::Below],
                Side::Left => vec![Side::Left, Side::Below, Side::Right, Side::Above],
                Side::Above => vec![Side::Above, Side::Right, Side::Below, Side::Left],
                Side::Below => vec![Side::Below, Side::Left, Side::Above, Side::Right],
                _ => Self::ALL_8.to_vec(),
            }
        }
    }

    /// Unit vector from the anchor toward the label (SVG +y down).
    pub fn unit(self) -> (f64, f64) {
        const S: f64 = std::f64::consts::FRAC_1_SQRT_2;
        match self {
            Side::Right => (1.0, 0.0),
            Side::BelowRight => (S, S),
            Side::Below => (0.0, 1.0),
            Side::BelowLeft => (-S, S),
            Side::Left => (-1.0, 0.0),
            Side::AboveLeft => (-S, -S),
            Side::Above => (0.0, -1.0),
            Side::AboveRight => (S, -S),
        }
    }

    /// 8-way compass of a SVG vector (+y down). `None` if the vector is ~0.
    pub fn from_vec(dx: f64, dy: f64) -> Option<Self> {
        if !dx.is_finite() || !dy.is_finite() {
            return None;
        }
        if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
            return None;
        }
        let mut d = dy.atan2(dx).to_degrees() + 22.5;
        if d < 0.0 {
            d += 360.0;
        }
        if d >= 360.0 {
            d -= 360.0;
        }
        Some(Self::ALL_8[(d / 45.0).floor() as usize])
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
    pub fn add_aabb(&mut self, box_: Aabb) {
        self.aabbs.push(box_);
    }

    pub fn clashes(&self, cand: Aabb, margin: f64) -> bool {
        self.clash_penalty(cand, margin) > 0.0
    }

    /// Center of mass of AABB centers and segment midpoints (the ink bulk).
    pub fn centroid(&self) -> Option<(f64, f64)> {
        self.centroid_where(|_| true)
    }

    /// Centroid of obstacles whose centers sit **outside** `inside`.
    ///
    /// Used so a region label aims away from neighboring chrome (OH, callout)
    /// rather than from its own enclosed atoms (those already sit in the hull).
    pub fn centroid_outside(&self, inside: Aabb) -> Option<(f64, f64)> {
        self.centroid_where(|(cx, cy)| {
            cx < inside.x0 || cx > inside.x1 || cy < inside.y0 || cy > inside.y1
        })
    }

    fn centroid_where(&self, keep: impl Fn((f64, f64)) -> bool) -> Option<(f64, f64)> {
        let mut sx = 0.0;
        let mut sy = 0.0;
        let mut n = 0.0;
        for a in &self.aabbs {
            let (cx, cy) = a.center();
            if !keep((cx, cy)) {
                continue;
            }
            sx += cx;
            sy += cy;
            n += 1.0;
        }
        for s in &self.segs {
            let c = (0.5 * (s.x0 + s.x1), 0.5 * (s.y0 + s.y1));
            if !keep(c) {
                continue;
            }
            sx += c.0;
            sy += c.1;
            n += 1.0;
        }
        if n < 1.0 {
            None
        } else {
            Some((sx / n, sy / n))
        }
    }

    /// How badly `cand` overlaps obstacles (0 = clear). Bond crossings weigh
    /// more than AABB overlaps so greedy fallbacks avoid straddling ink.
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

    /// True when a callout shaft clips an obstacle that is not the target mark.
    ///
    /// Label-box clash stays primary (`1e9`). A blocked shaft is a milder Auto
    /// veto so Above-through-exo loses to a 45° pose still past the atom,
    /// without empty-page Right winning.
    pub fn shaft_hits(&self, p: &Placement, margin: f64, anchor: (f64, f64)) -> bool {
        let (Some(from), Some(to)) = (p.arrow_from, p.arrow_to) else {
            return false;
        };
        shaft_hits_obstacles(from, to, &self.aabbs, margin, anchor)
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

    /// Widen the layout box in x so clash / occupancy keep neighboring text apart.
    pub fn pad_x(self, pad: f64) -> Self {
        Self {
            x0: self.x0 - pad,
            x1: self.x1 + pad,
            ..self
        }
    }
}

/// Layout ink for chrome / caption text: painted glyphs plus horizontal air.
pub fn padded_label_ink(ink: InkAtOrigin) -> InkAtOrigin {
    ink.pad_x(LABEL_PAD_X_PX)
}

/// Point on `ink` (origin-relative) nearest the incoming `unit` direction.
fn facing_hit(ink: InkAtOrigin, ux: f64, uy: f64) -> (f64, f64) {
    let hx = if ux > 1e-9 {
        ink.x0
    } else if ux < -1e-9 {
        ink.x1
    } else {
        (ink.x0 + ink.x1) * 0.5
    };
    let hy = if uy > 1e-9 {
        ink.y0
    } else if uy < -1e-9 {
        ink.y1
    } else {
        (ink.y0 + ink.y1) * 0.5
    };
    (hx, hy)
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
    pub side: Side,
    /// Arrow from label edge → tip (callouts). Paint may rebuild after refine.
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

/// Seed with no side bias: min clash, then **radially out** from the bulk.
///
/// Each side grows gap until clear (same as [`place_greedy`]). Among those
/// poses, slightly prefer compass directions aligned with `anchor − bulk`
/// (a top oxygen seeds above / above-left / above-right). Empty-page Right
/// is not radial-out from a ring below the OH. A blocked shaft may nudge
/// one slot (above → above-right) but cannot jump to the empty page.
/// `req.preferred` is ignored.
pub fn place_auto(occ: &Occupancy, req: &PlaceRequest) -> Placement {
    let bulk = occ.centroid().unwrap_or(req.anchor);
    let mut best: Option<(f64, Placement)> = None;
    for side in Side::ALL_8 {
        let mut gap = req.min_gap;
        let mut side_best: Option<(f64, Placement)> = None;
        while gap <= req.max_gap + 1e-9 {
            let p = force_place(req, side, gap);
            let pen = occ.clash_penalty(p.bounds, req.margin);
            if pen <= 1e-12 {
                side_best = Some((0.0, p));
                break;
            }
            let score = pen + (gap - req.min_gap) * 1e-6;
            let replace = match &side_best {
                None => true,
                Some((s, _)) => score < *s,
            };
            if replace {
                side_best = Some((score, p));
            }
            gap += req.gap_step;
        }
        let Some((_, p)) = side_best else {
            continue;
        };
        let pen = occ.clash_penalty(p.bounds, req.margin);
        let shaft = if occ.shaft_hits(&p, req.margin, req.anchor) {
            1.0
        } else {
            0.0
        };
        let score = auto_score(&p, bulk, req.anchor, pen, shaft);
        let replace = match &best {
            None => true,
            Some((s, _)) => score < *s,
        };
        if replace {
            best = Some((score, p));
        }
    }
    best.map(|(_, p)| p)
        .unwrap_or_else(|| force_place(req, Side::Right, req.min_gap))
}

/// Lower is better. Label-box clash first; then a mild radial-out bias.
///
/// `radial` is the cosine of the angle between (label − anchor) and
/// (anchor − bulk): 1 = straight out from the molecule center through the
/// atom. That is a slight Auto preference, not a hard side lock. A blocked
/// callout shaft costs one compass step: above-through-exo loses to a
/// far-side 45° (the intended tradeoff), not to empty-page Right.
fn auto_score(p: &Placement, bulk: (f64, f64), anchor: (f64, f64), pen: f64, shaft: f64) -> f64 {
    let (cx, cy) = p.bounds.center();
    let (lx, ly) = (cx - anchor.0, cy - anchor.1);
    let llen = lx.hypot(ly).max(1e-6);
    let (rx, ry) = (anchor.0 - bulk.0, anchor.1 - bulk.1);
    let rlen = rx.hypot(ry).max(1e-6);
    let radial = (lx * rx + ly * ry) / (llen * rlen);
    let along = lx * rx + ly * ry;
    let sep = (cx - bulk.0).hypot(cy - bulk.1);
    // Clash 1e9. Radial ~1e4 (slight far-side bias). A blocked shaft is
    // one compass step: above-through-exo loses to above-right, not to
    // empty-page Right (radial ~ 0). Along/sep only tie-break.
    pen * 1e9 + shaft * 1.2e4 - radial * 1e4 - along * 10.0 - sep
}

fn force_place(req: &PlaceRequest, side: Side, gap: f64) -> Placement {
    let (ax, ay) = req.anchor;
    let ink = req.ink;
    let (ux, uy) = side.unit();
    let (hx, hy) = facing_hit(ink, ux, uy);
    let x = ax + gap * ux - hx;
    let y = ay + gap * uy - hy;
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
    arrow_from_edge(
        facing_edge_mid(bounds, side),
        anchor,
        tip_standoff,
        label_pullback,
    )
}

/// Like [`callout_arrow`], but slide the start along the facing edge so the
/// shaft misses other labels while still aiming at `anchor`.
///
/// Center attachment is tried first (radial). Corners are fallbacks. The
/// destination mark (highlight circle / chem disk covering `anchor`) is skipped.
pub fn callout_arrow_clearing(
    bounds: Aabb,
    anchor: (f64, f64),
    side: Side,
    tip_standoff: f64,
    label_pullback: f64,
    obstacles: &[Aabb],
    margin: f64,
) -> (Option<(f64, f64)>, Option<(f64, f64)>) {
    let mut fallback = callout_arrow(bounds, anchor, side, tip_standoff, label_pullback);
    for edge in facing_edge_samples(bounds, side) {
        let arrow = arrow_from_edge(edge, anchor, tip_standoff, label_pullback);
        let (Some(from), Some(to)) = arrow else {
            continue;
        };
        if !shaft_hits_obstacles(from, to, obstacles, margin, anchor) {
            return arrow;
        }
        if fallback.0.is_none() {
            fallback = arrow;
        }
    }
    fallback
}

fn facing_edge_mid(bounds: Aabb, side: Side) -> (f64, f64) {
    let (ux, uy) = side.unit();
    let hx = if ux > 1e-9 {
        bounds.x0
    } else if ux < -1e-9 {
        bounds.x1
    } else {
        bounds.center().0
    };
    let hy = if uy > 1e-9 {
        bounds.y0
    } else if uy < -1e-9 {
        bounds.y1
    } else {
        bounds.center().1
    };
    (hx, hy)
}

fn facing_edge_samples(bounds: Aabb, side: Side) -> Vec<(f64, f64)> {
    let (ux, uy) = side.unit();
    let mut edges: Vec<((f64, f64), (f64, f64))> = Vec::new();
    if ux > 1e-9 {
        edges.push(((bounds.x0, bounds.y0), (bounds.x0, bounds.y1)));
    } else if ux < -1e-9 {
        edges.push(((bounds.x1, bounds.y0), (bounds.x1, bounds.y1)));
    }
    if uy > 1e-9 {
        edges.push(((bounds.x0, bounds.y0), (bounds.x1, bounds.y0)));
    } else if uy < -1e-9 {
        edges.push(((bounds.x0, bounds.y1), (bounds.x1, bounds.y1)));
    }
    if edges.is_empty() {
        return vec![bounds.center()];
    }
    let mut out = vec![facing_edge_mid(bounds, side)];
    for &(a, b) in &edges {
        for k in 0..=8 {
            let t = k as f64 / 8.0;
            let p = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            if out.iter().any(|q| (q.0 - p.0).hypot(q.1 - p.1) < 0.5) {
                continue;
            }
            out.push(p);
        }
    }
    out
}

fn arrow_from_edge(
    edge: (f64, f64),
    anchor: (f64, f64),
    tip_standoff: f64,
    label_pullback: f64,
) -> (Option<(f64, f64)>, Option<(f64, f64)>) {
    let (ax, ay) = anchor;
    let dx = ax - edge.0;
    let dy = ay - edge.1;
    let len = dx.hypot(dy).max(1e-6);
    let ux = dx / len;
    let uy = dy / len;
    let pull = label_pullback.max(0.0).min(len * 0.4);
    let from = (edge.0 + ux * pull, edge.1 + uy * pull);
    let max_st = (len - pull).max(0.0) * 0.9;
    let st = tip_standoff.max(0.0).min(max_st);
    let to = (ax - ux * st, ay - uy * st);
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

fn shaft_hits_obstacles(
    from: (f64, f64),
    to: (f64, f64),
    obstacles: &[Aabb],
    margin: f64,
    anchor: (f64, f64),
) -> bool {
    for &a in obstacles {
        // Destination mark covering the target is the tip's home, not a miss.
        if a.covers_point(anchor, 1.0) {
            continue;
        }
        if seg_aabb_min_dist(from.0, from.1, to.0, to.1, a) < margin {
            return true;
        }
    }
    false
}

/// Tip point on the preferred side of `anchor`, `standoff` px away.
pub fn tip_point_from_anchor(anchor: (f64, f64), side: Side, standoff: f64) -> (f64, f64) {
    let (ax, ay) = anchor;
    let (ux, uy) = side.unit();
    (ax + standoff * ux, ay + standoff * uy)
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
        preferred.all_cw_from()
    } else {
        vec![preferred]
    };
    let best = force_region(region, ink, inside, preferred, edge_margin, 0.0, about);
    // Outside / clash-free inside seeds may bump out before the hull grows.
    let max_bump = BOND_PX.max(24.0);
    let bumps = [0.0, 2.0, 4.0, 6.0, 8.0, 12.0, 16.0, 20.0, 24.0, 28.0, 32.0];
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

/// Auto region seed: sit on the hull side facing away from *other* chrome.
///
/// Interior atoms are already inside the hull, so they do not pick the side —
/// a landscape box would otherwise always slide to empty-page Right. Neighbor
/// chrome (OH, callout, …) does: an obstacle on top seeds Below. Grows on the
/// chosen side until clear, then rotates if that edge is blocked.
pub fn place_region_auto(
    occ: &Occupancy,
    region: Aabb,
    ink: InkAtOrigin,
    edge_margin: f64,
    clash_margin: f64,
    about: (f64, f64),
) -> Placement {
    let far = occ
        .centroid_outside(region)
        .and_then(|(bx, by)| Side::from_vec(about.0 - bx, about.1 - by));
    let sides: Vec<Side> = match far {
        Some(s) => s.all_cw_from(),
        None => Side::ALL_8.to_vec(),
    };
    let max_bump = BOND_PX.max(24.0) * 2.0;
    let bumps = [
        0.0, 2.0, 4.0, 6.0, 8.0, 12.0, 16.0, 20.0, 24.0, 28.0, 32.0, 40.0, 48.0, 56.0, 64.0,
    ];
    let mut least_bad: Option<(f64, Placement)> = None;
    for side in sides {
        for &bump in &bumps {
            if bump > max_bump + 1e-9 {
                continue;
            }
            let p = force_region(region, ink, false, side, edge_margin + bump, 0.0, about);
            let pen = occ.clash_penalty(p.bounds, clash_margin);
            if pen <= 1e-12 {
                return p;
            }
            let score = pen + bump * 1e-6;
            let replace = match &least_bad {
                None => true,
                Some((s, _)) => score < *s,
            };
            if replace {
                least_bad = Some((score, p));
            }
        }
    }
    least_bad
        .map(|(_, p)| p)
        .unwrap_or_else(|| force_region(region, ink, false, Side::Right, edge_margin, 0.0, about))
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
            Side::AboveRight => (region.x1 - margin - ink.x1, region.y0 + margin - ink.y0),
            Side::AboveLeft => (region.x0 + margin - ink.x0, region.y0 + margin - ink.y0),
            Side::BelowRight => (region.x1 - margin - ink.x1, region.y1 - margin - ink.y1),
            Side::BelowLeft => (region.x0 + margin - ink.x0, region.y1 - margin - ink.y1),
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
            Side::AboveRight => (region.x1 + margin - ink.x0, region.y0 - margin - ink.y1),
            Side::AboveLeft => (region.x0 - margin - ink.x1, region.y0 - margin - ink.y1),
            Side::BelowRight => (region.x1 + margin - ink.x0, region.y1 + margin - ink.y0),
            Side::BelowLeft => (region.x0 - margin - ink.x1, region.y1 + margin - ink.y0),
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
            !matches!(p.side, Side::Left | Side::Right),
            "least-clash fallback should leave the bond, got {:?}",
            p.side
        );
        assert!(
            !occ.clashes(p.bounds, req.margin),
            "vertical seed should clear the horizontal bond: {:?}",
            p.bounds
        );
        assert!(
            !matches!(least_clash_side(&occ, &req), Side::Left | Side::Right),
            "least-clash side should leave the bond"
        );
    }

    #[test]
    fn place_auto_picks_the_open_side() {
        let mut occ = Occupancy::new();
        occ.add_aabb(Aabb::new(12.0, -6.0, 48.0, 6.0)); // block right
        occ.add_aabb(Aabb::new(-6.0, 12.0, 6.0, 48.0)); // block below
        occ.add_aabb(Aabb::new(-48.0, -6.0, -12.0, 6.0)); // block left
        let ink = InkAtOrigin {
            x0: -10.0,
            y0: -4.0,
            x1: 10.0,
            y1: 4.0,
        };
        let req = PlaceRequest {
            anchor: (0.0, 0.0),
            ink,
            preferred: Side::Right,
            min_gap: 8.0,
            max_gap: 16.0,
            gap_step: 2.0,
            margin: 1.0,
            tip_standoff: None,
            label_pullback: 0.0,
        };
        let p = place_auto(&occ, &req);
        assert!(
            matches!(p.side, Side::Above | Side::AboveLeft | Side::AboveRight),
            "open air is above: {:?}",
            p.side
        );
        assert!(!occ.clashes(p.bounds, req.margin));
    }

    #[test]
    fn place_auto_empty_page_on_the_right_still_picks_away_from_bulk() {
        // Ring of ink below the anchor; Right is clash-free empty canvas.
        let mut occ = Occupancy::new();
        for i in 0..6 {
            let t = std::f64::consts::TAU * i as f64 / 6.0 + std::f64::consts::FRAC_PI_2;
            let x = 30.0 * t.cos();
            let y = 40.0 + 30.0 * t.sin();
            occ.add_aabb(Aabb::new(x - 4.0, y - 4.0, x + 4.0, y + 4.0));
        }
        let ink = InkAtOrigin {
            x0: -20.0,
            y0: -4.0,
            x1: 20.0,
            y1: 4.0,
        };
        let req = PlaceRequest {
            anchor: (0.0, 0.0),
            ink,
            preferred: Side::Right,
            min_gap: 8.0,
            max_gap: 40.0,
            gap_step: 4.0,
            margin: 1.0,
            tip_standoff: None,
            label_pullback: 0.0,
        };
        let p = place_auto(&occ, &req);
        assert!(
            matches!(p.side, Side::Above | Side::AboveLeft | Side::AboveRight),
            "bulk is below; empty Right is next to the ring: {:?}",
            p.side
        );
        assert!(
            p.bounds.center().1 < 0.0,
            "should sit above the bulk: {:?}",
            p.bounds
        );
    }

    #[test]
    fn place_auto_is_not_always_above() {
        // Bulk to the left of the anchor: the far side is Right.
        let mut occ = Occupancy::new();
        occ.add_aabb(Aabb::new(-48.0, -8.0, -8.0, 8.0));
        occ.add_aabb(Aabb::new(-48.0, -24.0, -8.0, -10.0));
        occ.add_aabb(Aabb::new(-48.0, 10.0, -8.0, 24.0));
        let ink = InkAtOrigin {
            x0: -8.0,
            y0: -4.0,
            x1: 8.0,
            y1: 4.0,
        };
        let req = PlaceRequest {
            anchor: (0.0, 0.0),
            ink,
            preferred: Side::Above,
            min_gap: 8.0,
            max_gap: 24.0,
            gap_step: 4.0,
            margin: 1.0,
            tip_standoff: None,
            label_pullback: 0.0,
        };
        let p = place_auto(&occ, &req);
        assert!(
            matches!(p.side, Side::Right | Side::AboveRight | Side::BelowRight),
            "bulk is left: {:?}",
            p.side
        );
        assert!(
            p.bounds.center().0 > 0.0,
            "should sit to the right of the bulk: {:?}",
            p.bounds
        );
    }

    #[test]
    fn place_auto_picks_a_45_degree_when_that_is_farthest() {
        // Bulk in the lower-left quadrant: farthest clash-free slot is above-right.
        let mut occ = Occupancy::new();
        occ.add_aabb(Aabb::new(-40.0, 8.0, -8.0, 40.0));
        occ.add_aabb(Aabb::new(-40.0, 40.0, 8.0, 56.0));
        let ink = InkAtOrigin {
            x0: -6.0,
            y0: -4.0,
            x1: 6.0,
            y1: 4.0,
        };
        let req = PlaceRequest {
            anchor: (0.0, 0.0),
            ink,
            preferred: Side::Above,
            min_gap: 8.0,
            max_gap: 24.0,
            gap_step: 4.0,
            margin: 1.0,
            tip_standoff: None,
            label_pullback: 0.0,
        };
        let p = place_auto(&occ, &req);
        assert_eq!(
            p.side,
            Side::AboveRight,
            "bulk is lower-left: {:?}",
            p.bounds
        );
        assert!(p.bounds.center().0 > 0.0 && p.bounds.center().1 < 0.0);
    }

    #[test]
    fn place_auto_callout_is_radial_when_shaft_is_clear() {
        // No exo on the above ray: Auto stays vertical / radial-out.
        let mut occ = Occupancy::new();
        occ.add_aabb(Aabb::new(-40.0, 10.0, 40.0, 50.0));
        let ink = InkAtOrigin {
            x0: -16.0,
            y0: -5.0,
            x1: 16.0,
            y1: 5.0,
        };
        let req = PlaceRequest {
            anchor: (0.0, 0.0),
            ink,
            preferred: Side::Right,
            min_gap: 28.0,
            max_gap: 72.0,
            gap_step: 4.0,
            margin: 2.0,
            tip_standoff: Some(8.0),
            label_pullback: 2.0,
        };
        let p = place_auto(&occ, &req);
        assert_eq!(
            p.side,
            Side::Above,
            "clear above-ray should stay radial-out, not empty Right: {:?}",
            p.side
        );
    }

    #[test]
    fn place_auto_callout_stays_radial_when_exo_blocks_the_ray() {
        // Target at origin; bulk below; a label sits on the above ray (kitchen-sink
        // exo). Auto may stay vertical — offset around exo is paint-time
        // (`callout_arrow_clearing`), not empty-page Right.
        let mut occ = Occupancy::new();
        occ.add_aabb(Aabb::new(-8.0, -22.0, 8.0, -10.0));
        occ.add_aabb(Aabb::new(-40.0, 10.0, 40.0, 50.0));
        let ink = InkAtOrigin {
            x0: -16.0,
            y0: -5.0,
            x1: 16.0,
            y1: 5.0,
        };
        let req = PlaceRequest {
            anchor: (0.0, 0.0),
            ink,
            preferred: Side::Above,
            min_gap: 28.0,
            max_gap: 72.0,
            gap_step: 4.0,
            margin: 2.0,
            tip_standoff: Some(8.0),
            label_pullback: 2.0,
        };
        let p = place_auto(&occ, &req);
        assert!(
            matches!(p.side, Side::Above | Side::AboveLeft | Side::AboveRight),
            "must stay past the OH, not empty-page Right: {:?}",
            p.side
        );
        assert!(
            p.bounds.center().1 < 0.0,
            "must sit above the bulk: {:?}",
            p.bounds
        );
        assert!(
            !matches!(p.side, Side::Right | Side::Left),
            "empty-page side is not radial-out: {:?}",
            p.side
        );
    }

    #[test]
    fn callout_arrow_clearing_slides_off_blocker() {
        // Wide label above a small exo, tip on a highlight-circle standoff —
        // same geometry as kitchen-sink phenol / exo.
        let bounds = Aabb::new(-28.0, -48.0, 28.0, -36.0);
        let exo = Aabb::new(-8.0, -24.0, 8.0, -16.0);
        let (from, to) =
            callout_arrow_clearing(bounds, (0.0, 0.0), Side::Above, 14.0, 2.0, &[exo], 2.0);
        let (from, to) = (from.expect("from"), to.expect("to"));
        let d = super::seg_aabb_min_dist(from.0, from.1, to.0, to.1, exo);
        assert!(
            d + 1e-6 >= 2.0,
            "shaft {from:?}→{to:?} still hits exo (d={d})"
        );
        assert!(
            to.1 < -10.0,
            "tip should stay above the origin on the callout ray: {to:?}"
        );
    }

    #[test]
    fn side_from_vec_is_eight_way() {
        assert_eq!(Side::from_vec(10.0, 0.0), Some(Side::Right));
        assert_eq!(Side::from_vec(10.0, 10.0), Some(Side::BelowRight));
        assert_eq!(Side::from_vec(0.0, 10.0), Some(Side::Below));
        assert_eq!(Side::from_vec(-10.0, 10.0), Some(Side::BelowLeft));
        assert_eq!(Side::from_vec(-10.0, 0.0), Some(Side::Left));
        assert_eq!(Side::from_vec(-10.0, -10.0), Some(Side::AboveLeft));
        assert_eq!(Side::from_vec(0.0, -10.0), Some(Side::Above));
        assert_eq!(Side::from_vec(10.0, -10.0), Some(Side::AboveRight));
    }

    #[test]
    fn place_region_auto_picks_away_from_top_obstacle_not_empty_right() {
        let mut occ = Occupancy::new();
        // Obstacle on the top edge (like an OH); Right of the box is empty.
        occ.add_aabb(Aabb::new(40.0, -18.0, 60.0, -2.0));
        let region = Aabb::new(0.0, 0.0, 100.0, 80.0);
        let ink = InkAtOrigin {
            x0: -10.0,
            y0: -5.0,
            x1: 10.0,
            y1: 5.0,
        };
        let p = place_region_auto(&occ, region, ink, 4.0, 1.0, (50.0, 40.0));
        assert_eq!(
            p.side,
            Side::Below,
            "OH is on top; auto sits on the far side, not empty Right or above the OH: {:?}",
            p.bounds
        );
        assert!(
            p.bounds.y0 >= 80.0 - 1.0,
            "should sit below the box: {:?}",
            p.bounds
        );
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
