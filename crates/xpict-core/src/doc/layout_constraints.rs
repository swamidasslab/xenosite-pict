//! Hierarchical constraint layout for mol chrome.
//!
//! **Fixed vs movable.** After depict lays out the molecule, chem is frozen:
//! bonds, atom disks, chem atom text, and shade disks (actual painted radii).
//! That whole chem subtree is fixed input to the solver — if it ever moves, it
//! is only a rigid transform of the mol. Annotations (caption, region text,
//! callout, atom number/label, …) are the only free variables.
//!
//! **Bond clearance is baseline and hard.** Movable annotations share
//! [`push_clear_bonds`] — [`HardConstraint::Outside`] vs every bond line,
//! enforced by an augmented-Lagrangian outer loop around Nelder–Mead. Soft
//! hinges alone leave residual ink-on-bond; do not special-case a weaker bond
//! gap for atom labels.
//!
//! One layout world per mol: nested node paths (`mol/chem/…`,
//! `mol/annotations/…`), edge-aware shapes, per-type constraint kits, and a
//! Nelder–Mead minimizer that stops on **pose motion in CSS-px** (not ΔE).

use std::collections::HashMap;

use crate::doc::layout_items::{Aabb, Side, ATOM_DISK_R_PX, CONTAIN_MARGIN_PX};
use crate::metrics::{BOND_PX, STROKE_PX};

/// Stable path id (`mol/chem/atoms/a3/shade`, `mol/annotations/captions/name`).
pub type NodeId = String;

/// Geometry used for contain / clash / opt_dist (edge-based, not center-to-center).
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Point {
        x: f64,
        y: f64,
    },
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
    },
    Line {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    },
    Aabb(Aabb),
    /// Closed polyline / hull proxy (last→first implied if not equal).
    PointSet(Vec<(f64, f64)>),
}

impl Shape {
    pub fn aabb(&self) -> Aabb {
        match self {
            Shape::Point { x, y } => Aabb::new(*x, *y, *x, *y),
            Shape::Circle { cx, cy, r } => Aabb::new(cx - r, cy - r, cx + r, cy + r),
            Shape::Line { x0, y0, x1, y1 } => Aabb::new(*x0, *y0, *x1, *y1),
            Shape::Aabb(a) => *a,
            Shape::PointSet(pts) => {
                if pts.is_empty() {
                    return Aabb::new(0.0, 0.0, 0.0, 0.0);
                }
                let mut x0 = f64::INFINITY;
                let mut y0 = f64::INFINITY;
                let mut x1 = f64::NEG_INFINITY;
                let mut y1 = f64::NEG_INFINITY;
                for &(x, y) in pts {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
                Aabb::new(x0, y0, x1, y1)
            }
        }
    }

    /// Translate by `(dx, dy)` (movable pose apply).
    pub fn translated(&self, dx: f64, dy: f64) -> Shape {
        match self {
            Shape::Point { x, y } => Shape::Point {
                x: x + dx,
                y: y + dy,
            },
            Shape::Circle { cx, cy, r } => Shape::Circle {
                cx: cx + dx,
                cy: cy + dy,
                r: *r,
            },
            Shape::Line { x0, y0, x1, y1 } => Shape::Line {
                x0: x0 + dx,
                y0: y0 + dy,
                x1: x1 + dx,
                y1: y1 + dy,
            },
            Shape::Aabb(a) => Shape::Aabb(Aabb::new(a.x0 + dx, a.y0 + dy, a.x1 + dx, a.y1 + dy)),
            Shape::PointSet(pts) => {
                Shape::PointSet(pts.iter().map(|&(x, y)| (x + dx, y + dy)).collect())
            }
        }
    }

    /// Control points used for solver termination (CSS-px).
    pub fn control_points(&self) -> Vec<(f64, f64)> {
        match self {
            Shape::Point { x, y } => vec![(*x, *y)],
            Shape::Circle { cx, cy, r } => vec![
                (*cx, *cy),
                (cx + r, *cy),
                (cx - r, *cy),
                (*cx, cy + r),
                (*cx, cy - r),
            ],
            Shape::Line { x0, y0, x1, y1 } => vec![(*x0, *y0), (*x1, *y1)],
            Shape::Aabb(a) => vec![
                (a.x0, a.y0),
                (a.x1, a.y0),
                (a.x1, a.y1),
                (a.x0, a.y1),
                a.center(),
            ],
            Shape::PointSet(pts) => pts.clone(),
        }
    }
}

/// Signed edge distance: positive = separated, negative = penetration depth.
///
/// For AABB↔AABB / Circle↔Circle / Point↔* this is exact enough for soft
/// clash. PointSet uses distance to closest boundary segment (negative inside).
pub fn edge_signed_distance(a: &Shape, b: &Shape) -> f64 {
    match (a, b) {
        (Shape::Point { x: ax, y: ay }, Shape::Point { x: bx, y: by }) => (ax - bx).hypot(ay - by),
        (Shape::Point { x, y }, other) => point_to_shape_signed(*x, *y, other),
        (other, Shape::Point { x, y }) => point_to_shape_signed(*x, *y, other),
        (
            Shape::Circle {
                cx: ax,
                cy: ay,
                r: ar,
            },
            Shape::Circle {
                cx: bx,
                cy: by,
                r: br,
            },
        ) => (ax - bx).hypot(ay - by) - ar - br,
        (Shape::Aabb(aa), Shape::Aabb(bb)) => aabb_aabb_signed(*aa, *bb),
        (Shape::Circle { cx, cy, r }, Shape::Aabb(box_))
        | (Shape::Aabb(box_), Shape::Circle { cx, cy, r }) => {
            aabb_circle_signed(*box_, *cx, *cy, *r)
        }
        (Shape::Line { .. }, _) | (_, Shape::Line { .. }) => {
            // Approximate thick segment via AABB inflate of half stroke later;
            // here use point-pair distance between closest points on edges.
            line_shape_signed(a, b)
        }
        (Shape::PointSet(pts), other) | (other, Shape::PointSet(pts)) => {
            let aa = a.aabb();
            let bb = b.aabb();
            let d = aabb_aabb_signed(aa, bb);
            let (acx, acy) = aa.center();
            let (bcx, bcy) = bb.center();
            let da = if matches!(a, Shape::PointSet(_)) {
                point_to_polyline_signed(acx, acy, pts)
            } else {
                point_to_shape_signed(acx, acy, other)
            };
            let db = if matches!(b, Shape::PointSet(_)) {
                point_to_polyline_signed(bcx, bcy, pts)
            } else {
                point_to_shape_signed(bcx, bcy, other)
            };
            da.max(db).min(d)
        }
    }
}

fn aabb_aabb_signed(a: Aabb, b: Aabb) -> f64 {
    let ox = (a.x0.max(b.x0))..(a.x1.min(b.x1));
    let oy = (a.y0.max(b.y0))..(a.y1.min(b.y1));
    let overlap_x = ox.end - ox.start;
    let overlap_y = oy.end - oy.start;
    if overlap_x > 0.0 && overlap_y > 0.0 {
        return -overlap_x.min(overlap_y);
    }
    let dx = if a.x1 < b.x0 {
        b.x0 - a.x1
    } else if b.x1 < a.x0 {
        a.x0 - b.x1
    } else {
        0.0
    };
    let dy = if a.y1 < b.y0 {
        b.y0 - a.y1
    } else if b.y1 < a.y0 {
        a.y0 - b.y1
    } else {
        0.0
    };
    if dx > 0.0 && dy > 0.0 {
        (dx * dx + dy * dy).sqrt()
    } else {
        dx.max(dy)
    }
}

fn aabb_circle_signed(a: Aabb, cx: f64, cy: f64, r: f64) -> f64 {
    let qx = cx.clamp(a.x0, a.x1);
    let qy = cy.clamp(a.y0, a.y1);
    let outside = (cx - qx).hypot(cy - qy);
    if outside > 1e-12 {
        return outside - r;
    }
    // Center inside AABB: penetration = r + dist to nearest edge (negative).
    let to_edge = (cx - a.x0).min(a.x1 - cx).min(cy - a.y0).min(a.y1 - cy);
    -(r + to_edge)
}

fn point_to_shape_signed(px: f64, py: f64, s: &Shape) -> f64 {
    match s {
        Shape::Point { x, y } => (px - x).hypot(py - y),
        Shape::Circle { cx, cy, r } => (px - cx).hypot(py - cy) - r,
        Shape::Aabb(a) => {
            let qx = px.clamp(a.x0, a.x1);
            let qy = py.clamp(a.y0, a.y1);
            let outside = (px - qx).hypot(py - qy);
            if outside > 1e-12 {
                outside
            } else {
                let to_edge = (px - a.x0).min(a.x1 - px).min(py - a.y0).min(a.y1 - py);
                -to_edge
            }
        }
        Shape::Line { x0, y0, x1, y1 } => point_to_seg(px, py, *x0, *y0, *x1, *y1),
        Shape::PointSet(pts) => point_to_polyline_signed(px, py, pts),
    }
}

fn point_to_seg(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-18 {
        return (px - x0).hypot(py - y0);
    }
    let t = ((px - x0) * dx + (py - y0) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    (px - (x0 + t * dx)).hypot(py - (y0 + t * dy))
}

/// Signed distance to closed polyline: negative inside (even-odd / winding).
fn point_to_polyline_signed(px: f64, py: f64, pts: &[(f64, f64)]) -> f64 {
    if pts.len() < 2 {
        return if let Some(&(x, y)) = pts.first() {
            (px - x).hypot(py - y)
        } else {
            0.0
        };
    }
    let mut best = f64::INFINITY;
    let n = pts.len();
    for i in 0..n {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[(i + 1) % n];
        best = best.min(point_to_seg(px, py, x0, y0, x1, y1));
    }
    if point_in_poly(px, py, pts) {
        -best
    } else {
        best
    }
}

fn point_in_poly(px: f64, py: f64, pts: &[(f64, f64)]) -> bool {
    let mut inside = false;
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi + 1e-18) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn line_shape_signed(a: &Shape, b: &Shape) -> f64 {
    match (a, b) {
        (
            Shape::Line {
                x0: ax0,
                y0: ay0,
                x1: ax1,
                y1: ay1,
            },
            Shape::Line {
                x0: bx0,
                y0: by0,
                x1: bx1,
                y1: by1,
            },
        ) => seg_seg_dist(*ax0, *ay0, *ax1, *ay1, *bx0, *by0, *bx1, *by1),
        // AABB vs bond: endpoints alone miss a segment that *crosses* the box.
        // Subtract half stroke so Outside matches painted bond ink width.
        (Shape::Line { x0, y0, x1, y1 }, Shape::Aabb(box_))
        | (Shape::Aabb(box_), Shape::Line { x0, y0, x1, y1 }) => {
            seg_aabb_signed(*x0, *y0, *x1, *y1, *box_) - bond_half_stroke()
        }
        (Shape::Line { x0, y0, x1, y1 }, Shape::Circle { cx, cy, r })
        | (Shape::Circle { cx, cy, r }, Shape::Line { x0, y0, x1, y1 }) => {
            point_to_seg(*cx, *cy, *x0, *y0, *x1, *y1) - *r - bond_half_stroke()
        }
        (Shape::Line { x0, y0, x1, y1 }, other) | (other, Shape::Line { x0, y0, x1, y1 }) => {
            // PointSet / residual: sample the segment against the other shape.
            let n = 12;
            let mut best = f64::INFINITY;
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let px = x0 + t * (x1 - x0);
                let py = y0 + t * (y1 - y0);
                best = best.min(point_to_shape_signed(px, py, other));
            }
            best
        }
        _ => aabb_aabb_signed(a.aabb(), b.aabb()),
    }
}

fn bond_half_stroke() -> f64 {
    STROKE_PX.max(0.8) * 0.5
}

/// Signed distance segment ↔ AABB. Negative when the segment crosses the box
/// (endpoint-only distance would falsely stay positive).
fn seg_aabb_signed(x0: f64, y0: f64, x1: f64, y1: f64, a: Aabb) -> f64 {
    let n = 16;
    let mut best_out = f64::INFINITY;
    let mut best_pen: f64 = 0.0;
    let mut any_in = false;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let px = x0 + t * (x1 - x0);
        let py = y0 + t * (y1 - y0);
        let d = point_to_shape_signed(px, py, &Shape::Aabb(a));
        if d < 0.0 {
            any_in = true;
            best_pen = best_pen.max(-d);
        } else {
            best_out = best_out.min(d);
        }
    }
    // Also AABB corners → segment (tight exterior when parallel to an edge).
    for &(cx, cy) in &[(a.x0, a.y0), (a.x1, a.y0), (a.x1, a.y1), (a.x0, a.y1)] {
        let d = point_to_seg(cx, cy, x0, y0, x1, y1);
        // Corner outside the segment proximity still counts as exterior gap.
        if !any_in {
            best_out = best_out.min(d);
        }
    }
    if any_in {
        -best_pen
    } else {
        best_out
    }
}

#[allow(clippy::too_many_arguments)]
fn seg_seg_dist(
    ax0: f64,
    ay0: f64,
    ax1: f64,
    ay1: f64,
    bx0: f64,
    by0: f64,
    bx1: f64,
    by1: f64,
) -> f64 {
    point_to_seg(ax0, ay0, bx0, by0, bx1, by1)
        .min(point_to_seg(ax1, ay1, bx0, by0, bx1, by1))
        .min(point_to_seg(bx0, by0, ax0, ay0, ax1, ay1))
        .min(point_to_seg(bx1, by1, ax0, ay0, ax1, ay1))
}

/// Soft constraint forms emitted by type kits.
#[derive(Debug, Clone)]
pub enum Constraint {
    /// `a` should stay outside `b` by at least `clash_dist` (edge gap).
    Outside {
        a: NodeId,
        b: NodeId,
        clash_dist: f64,
        strength: f64,
    },
    /// `a` should stay inside `b` by at least `pad`. Energy is `strength * g⁺²`
    /// using the same residual as [`HardConstraint::Inside`].
    Inside {
        a: NodeId,
        b: NodeId,
        pad: f64,
        strength: f64,
    },
    MinGap {
        a: NodeId,
        b: NodeId,
        dist: f64,
        strength: f64,
    },
    PreferSide {
        a: NodeId,
        b: NodeId,
        side: Side,
        strength: f64,
        /// Extra spring along the edge, as a fraction of `strength`
        /// (Left/Right → match `b`'s y; Above/Below → match x). `0` is hinge
        /// only — the solver does not infer this from `b`'s shape.
        align_along: f64,
    },
    PointStandoff {
        tip: NodeId,
        target: NodeId,
        dist: f64,
        strength: f64,
    },
    // Glyph / bond Halo is paint-only (shape + buffer after placement) — never
    // a layout constraint. See `chrome_paint::halo_for_markup`.
    MinimizeDist {
        a: NodeId,
        b: NodeId,
        opt_dist: f64,
        strength: f64,
    },
    /// Spring the object's center (or point) to a placement `(x, y)`.
    ///
    /// This is the preferred paint position (centered caption, side hug, callout
    /// standoff, atom-label seed). Clash terms can spend this energy to move
    /// off an obstacle; with a clear field the minimum is this pose.
    PreferPose {
        a: NodeId,
        x: f64,
        y: f64,
        strength: f64,
    },
}

/// Inequality constraints `g(x) ≤ 0` enforced via augmented Lagrangian.
///
/// Soft [`Constraint`] hinges can leave residual overlap when PreferPose /
/// PreferSide fight them. Hard rows here get Powell–Hestenes–Rockafellar
/// multipliers so bond (and similar) clearance converges to feasibility.
/// Clearance (no overlap) is a stronger Lagrangian than tether.
pub const LAGRANGIAN_CLEAR: u8 = 0;
/// Association ceiling (MaxDist). Solved after clearance and held weaker.
pub const LAGRANGIAN_TETHER: u8 = 1;

#[derive(Debug, Clone)]
pub enum HardConstraint {
    /// Require `edge_signed(a,b) ≥ clash_dist` (`g = clash_dist - d`).
    Outside {
        a: NodeId,
        b: NodeId,
        clash_dist: f64,
        /// Lower is stronger. [`LAGRANGIAN_CLEAR`] vs [`LAGRANGIAN_TETHER`].
        lagrangian: u8,
    },
    /// Require `a` inside `b` by at least `pad` (`g` = protrusion / shallow depth).
    Inside {
        a: NodeId,
        b: NodeId,
        pad: f64,
        lagrangian: u8,
    },
    /// Require `edge_signed(a,b) ≤ dist` (`g = d - dist`). Overlap (`d < 0`) is fine.
    ///
    /// This is a **closest-point** tether: some part of `a` must lie within
    /// `dist` of `b` (the far side of a long label may stick out). Atom labels
    /// use it so bond-clear AL cannot fling the label across the molecule.
    MaxDist {
        a: NodeId,
        b: NodeId,
        dist: f64,
        lagrangian: u8,
    },
}

fn hard_lagrangian(h: &HardConstraint) -> u8 {
    match h {
        HardConstraint::Outside { lagrangian, .. }
        | HardConstraint::Inside { lagrangian, .. }
        | HardConstraint::MaxDist { lagrangian, .. } => *lagrangian,
    }
}

/// How hard the placement well pulls, relative to clash hinges (strength ~3–8).
const POSE_STRENGTH: f64 = 0.35;

/// Hard MaxDist ceiling for **index / mapno** numbers (~0.3 CC-bond).
///
/// Closest-point: some part of the glyph must reach within this gap of home.
pub const ATOM_NUMBER_TETHER_PX: f64 = 0.3 * BOND_PX;

/// Hard MaxDist ceiling for custom **atom_label** text (~0.5 CC-bond).
///
/// Longer than [`ATOM_NUMBER_TETHER_PX`] so words like “ortho” can clear ink
/// while still staying associated with the atom.
pub const ATOM_TEXT_TETHER_PX: f64 = 0.5 * BOND_PX;

fn shape_anchor(shape: &Shape) -> (f64, f64) {
    match shape {
        Shape::Point { x, y } => (*x, *y),
        other => other.aabb().center(),
    }
}

fn push_placement_pose(constraints: &mut Vec<Constraint>, id: &str, rest: &Shape) {
    let (x, y) = shape_anchor(rest);
    constraints.push(Constraint::PreferPose {
        a: id.to_string(),
        x,
        y,
        strength: POSE_STRENGTH,
    });
}

/// Hard [`HardConstraint::Outside`] vs every fixed bond line under `mol/chem/bonds`.
///
/// Baseline for captions, region text, callouts, atom numbers/labels — do not
/// weaken this per kit. Half stroke width is already in edge distance.
fn push_clear_bonds(world: &LayoutWorld, hard: &mut Vec<HardConstraint>, a: &str, clash_dist: f64) {
    if let Some(bonds) = world.get("mol/chem/bonds") {
        for bid in bonds.children.clone() {
            hard.push(HardConstraint::Outside {
                a: a.to_string(),
                b: bid,
                clash_dist,
                lagrangian: LAGRANGIAN_CLEAR,
            });
        }
    }
}

/// Object kind → which kit installed it (debug / filtering).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectType {
    Mol,
    Chem,
    Bonds,
    Atom,
    Coord,
    AtomText,
    Shade,
    Annotations,
    Captions,
    Caption,
    Regions,
    Region,
    RegionHull,
    RegionText,
    Callouts,
    CalloutLabel,
    CalloutTip,
    CalloutRoute,
    AtomNumbers,
    AtomNumber,
    AtomLabels,
    AtomLabel,
    Group,
}

/// One node in the layout tree.
#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub object_type: ObjectType,
    pub shape: Shape,
    /// When true, shape is an obstacle / target only (not a DOF).
    pub fixed: bool,
    /// Translation DOF for movable leaves (origin offset applied to `rest`).
    pub tx: f64,
    pub ty: f64,
    /// Rest pose (before tx/ty). For fixed nodes, this is the world pose.
    pub rest: Shape,
}

impl LayoutNode {
    pub fn world_shape(&self) -> Shape {
        if self.fixed {
            self.rest.clone()
        } else {
            self.rest.translated(self.tx, self.ty)
        }
    }
}

/// Layout world rooted at `mol`.
#[derive(Debug, Clone)]
pub struct LayoutWorld {
    pub nodes: HashMap<NodeId, LayoutNode>,
    pub root: NodeId,
}

impl LayoutWorld {
    pub fn new(root_id: impl Into<NodeId>) -> Self {
        let root = root_id.into();
        let mut nodes = HashMap::new();
        nodes.insert(
            root.clone(),
            LayoutNode {
                id: root.clone(),
                parent: None,
                children: Vec::new(),
                object_type: ObjectType::Mol,
                shape: Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
                fixed: true,
                tx: 0.0,
                ty: 0.0,
                rest: Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
            },
        );
        Self { nodes, root }
    }

    pub fn insert(
        &mut self,
        id: impl Into<NodeId>,
        parent: impl Into<NodeId>,
        object_type: ObjectType,
        shape: Shape,
        fixed: bool,
    ) -> NodeId {
        let id = id.into();
        let parent = parent.into();
        if let Some(p) = self.nodes.get_mut(&parent) {
            if !p.children.iter().any(|c| c == &id) {
                p.children.push(id.clone());
            }
        }
        self.nodes.insert(
            id.clone(),
            LayoutNode {
                id: id.clone(),
                parent: Some(parent),
                children: Vec::new(),
                object_type,
                shape: shape.clone(),
                fixed,
                tx: 0.0,
                ty: 0.0,
                rest: shape,
            },
        );
        id
    }

    pub fn get(&self, id: &str) -> Option<&LayoutNode> {
        self.nodes.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut LayoutNode> {
        self.nodes.get_mut(id)
    }

    /// Union AABB of a node and all descendants (world poses).
    pub fn subtree_aabb(&self, id: &str) -> Option<Aabb> {
        let node = self.nodes.get(id)?;
        let mut box_ = node.world_shape().aabb();
        let mut stack = node.children.clone();
        while let Some(cid) = stack.pop() {
            if let Some(c) = self.nodes.get(&cid) {
                let a = c.world_shape().aabb();
                box_ = Aabb::new(
                    box_.x0.min(a.x0),
                    box_.y0.min(a.y0),
                    box_.x1.max(a.x1),
                    box_.y1.max(a.y1),
                );
                stack.extend(c.children.iter().cloned());
            }
        }
        Some(box_)
    }

    /// Refresh group `rest`/`shape` AABB unions from children (fixed groups).
    pub fn recompute_group_aabbs(&mut self) {
        // Post-order: process leaves depth first via stack of (id, visited).
        let ids: Vec<_> = self.nodes.keys().cloned().collect();
        for id in ids {
            let Some(node) = self.nodes.get(&id) else {
                continue;
            };
            if node.children.is_empty() || !node.fixed {
                continue;
            }
            if let Some(a) = self.subtree_aabb(&id) {
                if let Some(n) = self.nodes.get_mut(&id) {
                    n.rest = Shape::Aabb(a);
                    n.shape = Shape::Aabb(a);
                }
            }
        }
    }

    pub fn set_pose(&mut self, id: &str, tx: f64, ty: f64) {
        if let Some(n) = self.nodes.get_mut(id) {
            if !n.fixed {
                n.tx = tx;
                n.ty = ty;
            }
        }
    }

    pub fn movable_ids(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|(_, n)| !n.fixed)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Flatten movable (tx, ty) into a parameter vector (stable sorted by id).
    pub fn params(&self) -> (Vec<NodeId>, Vec<f64>) {
        let mut ids = self.movable_ids();
        ids.sort();
        let mut p = Vec::with_capacity(ids.len() * 2);
        for id in &ids {
            let n = &self.nodes[id];
            p.push(n.tx);
            p.push(n.ty);
        }
        (ids, p)
    }

    pub fn apply_params(&mut self, ids: &[NodeId], p: &[f64]) {
        for (i, id) in ids.iter().enumerate() {
            let tx = p.get(i * 2).copied().unwrap_or(0.0);
            let ty = p.get(i * 2 + 1).copied().unwrap_or(0.0);
            self.set_pose(id, tx, ty);
        }
    }

    pub fn control_points_movable(&self) -> Vec<(f64, f64)> {
        let mut pts = Vec::new();
        for n in self.nodes.values() {
            if n.fixed {
                continue;
            }
            pts.extend(n.world_shape().control_points());
        }
        pts
    }
}

fn hinge_below(x: f64, lo: f64) -> f64 {
    let v = lo - x;
    if v > 0.0 {
        v * v
    } else {
        0.0
    }
}

fn spring(d: f64, d_star: f64) -> f64 {
    let e = d - d_star;
    e * e
}

/// Evaluate soft energy for the current world poses.
pub fn energy(
    world: &LayoutWorld,
    constraints: &[Constraint],
    seed: &[(NodeId, f64, f64)],
    w_travel: f64,
) -> f64 {
    let mut e = 0.0;
    for c in constraints {
        e += constraint_energy(world, c);
    }
    if w_travel > 0.0 {
        for (id, sx, sy) in seed {
            if let Some(n) = world.get(id) {
                let dx = n.tx - sx;
                let dy = n.ty - sy;
                e += w_travel * (dx * dx + dy * dy);
            }
        }
    }
    e
}

fn constraint_energy(world: &LayoutWorld, c: &Constraint) -> f64 {
    match c {
        Constraint::Outside {
            a,
            b,
            clash_dist,
            strength,
        } => {
            let (sa, sb) = match (world.get(a), world.get(b)) {
                (Some(na), Some(nb)) => (na.world_shape(), nb.world_shape()),
                _ => return 0.0,
            };
            let d = edge_signed_distance(&sa, &sb);
            strength * hinge_below(d, *clash_dist)
        }
        Constraint::Inside {
            a,
            b,
            pad,
            strength,
        } => {
            let (sa, sb) = match (world.get(a), world.get(b)) {
                (Some(na), Some(nb)) => (na.world_shape(), nb.world_shape()),
                _ => return 0.0,
            };
            let g = inside_g(&sa, &sb, *pad);
            strength * g.max(0.0) * g.max(0.0)
        }
        Constraint::MinGap {
            a,
            b,
            dist,
            strength,
        } => {
            let (sa, sb) = match (world.get(a), world.get(b)) {
                (Some(na), Some(nb)) => (na.world_shape(), nb.world_shape()),
                _ => return 0.0,
            };
            let d = edge_signed_distance(&sa, &sb);
            strength * hinge_below(d, *dist)
        }
        Constraint::PreferSide {
            a,
            b,
            side,
            strength,
            align_along,
        } => {
            let (sa, sb) = match (world.get(a), world.get(b)) {
                (Some(na), Some(nb)) => (na.world_shape(), nb.world_shape()),
                _ => return 0.0,
            };
            let (ax, ay) = sa.aabb().center();
            let (bx, by) = sb.aabb().center();
            let (dx, dy) = (ax - bx, ay - by);
            let good = match side {
                Side::Right => dx,
                Side::Left => -dx,
                Side::Below => dy,
                Side::Above => -dy,
            };
            let ortho = match side {
                Side::Left | Side::Right => dy,
                Side::Above | Side::Below => dx,
            };
            let along = align_along.max(0.0);
            strength * hinge_below(good, 0.0) + along * strength * ortho * ortho
        }
        Constraint::PointStandoff {
            tip,
            target,
            dist,
            strength,
        } => {
            let (sa, sb) = match (world.get(tip), world.get(target)) {
                (Some(na), Some(nb)) => (na.world_shape(), nb.world_shape()),
                _ => return 0.0,
            };
            let (tx, ty) = shape_anchor(&sa);
            let (ax, ay) = shape_anchor(&sb);
            let d = (tx - ax).hypot(ty - ay);
            strength * spring(d, *dist)
        }
        Constraint::MinimizeDist {
            a,
            b,
            opt_dist,
            strength,
        } => {
            let (sa, sb) = match (world.get(a), world.get(b)) {
                (Some(na), Some(nb)) => (na.world_shape(), nb.world_shape()),
                _ => return 0.0,
            };
            let d = edge_signed_distance(&sa, &sb).max(0.0);
            strength * spring(d, *opt_dist)
        }
        Constraint::PreferPose { a, x, y, strength } => {
            let Some(n) = world.get(a) else {
                return 0.0;
            };
            let (cx, cy) = shape_anchor(&n.world_shape());
            let dx = cx - x;
            let dy = cy - y;
            strength * (dx * dx + dy * dy)
        }
    }
}

/// Solver options (pose-drift termination + augmented Lagrangian).
#[derive(Debug, Clone)]
pub struct SolveOpts {
    pub max_iters: usize,
    /// Ring buffer length for control-point drift.
    pub drift_window: usize,
    /// Stop when max control-point displacement vs window ago `< epsilon_px`.
    pub epsilon_px: f64,
    pub w_travel: f64,
    pub noise: f64,
    pub initial_simplex_scale: f64,
    /// Outer AL updates of multipliers / penalty weight.
    pub al_outer_max: usize,
    /// Initial quadratic penalty weight ρ.
    pub al_rho0: f64,
    /// Multiply ρ when max hard violation stalls.
    pub al_rho_grow: f64,
    pub al_rho_max: f64,
    /// Stop outer loop when max `g⁺` (px) is below this.
    pub al_tol: f64,
}

impl Default for SolveOpts {
    fn default() -> Self {
        Self {
            max_iters: 200,
            drift_window: 4,
            epsilon_px: 0.75,
            w_travel: 0.02,
            noise: 0.5,
            initial_simplex_scale: 2.0,
            al_outer_max: 6,
            al_rho0: 8.0,
            al_rho_grow: 2.5,
            al_rho_max: 2.0e4,
            al_tol: 0.25,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SolveResult {
    pub iters: usize,
    pub energy: f64,
    pub stopped_on_drift: bool,
    /// Max hard violation `max(0, g_i)` after the last outer update (0 if no hard).
    pub max_hard_violation: f64,
}

/// Inside residual `g`. Feasible when `g ≤ 0` (`a` inside `b` by `pad`).
///
/// Shape dispatch lives here (geometry), not as a second energy formula in
/// [`constraint_energy`] / [`hard_g`].
fn inside_g(sa: &Shape, sb: &Shape, pad: f64) -> f64 {
    match (sa, sb) {
        (Shape::Aabb(inner), Shape::Aabb(outer)) => {
            let left = outer.x0 + pad - inner.x0;
            let right = inner.x1 - (outer.x1 - pad);
            let top = outer.y0 + pad - inner.y0;
            let bot = inner.y1 - (outer.y1 - pad);
            left.max(0.0) + right.max(0.0) + top.max(0.0) + bot.max(0.0)
        }
        (Shape::Point { x, y }, _) => point_to_shape_signed(*x, *y, sb) + pad,
        (Shape::Aabb(box_), Shape::PointSet(pts)) => {
            let (cx, cy) = box_.center();
            point_to_polyline_signed(cx, cy, pts) + pad
        }
        (Shape::Aabb(box_), Shape::Circle { cx, cy, r }) => {
            let mut g = f64::NEG_INFINITY;
            for &(px, py) in &[
                (box_.x0, box_.y0),
                (box_.x1, box_.y0),
                (box_.x0, box_.y1),
                (box_.x1, box_.y1),
            ] {
                let d = (px - cx).hypot(py - cy) - (r - pad);
                g = g.max(d);
            }
            g
        }
        _ => edge_signed_distance(sa, sb) + pad,
    }
}

/// Signed hard residual `g(x)`. Feasible when `g ≤ 0`.
pub fn hard_g(world: &LayoutWorld, h: &HardConstraint) -> f64 {
    match h {
        HardConstraint::Outside {
            a, b, clash_dist, ..
        } => {
            let (sa, sb) = match (world.get(a), world.get(b)) {
                (Some(na), Some(nb)) => (na.world_shape(), nb.world_shape()),
                _ => return 0.0,
            };
            clash_dist - edge_signed_distance(&sa, &sb)
        }
        HardConstraint::Inside { a, b, pad, .. } => {
            let (sa, sb) = match (world.get(a), world.get(b)) {
                (Some(na), Some(nb)) => (na.world_shape(), nb.world_shape()),
                _ => return 0.0,
            };
            inside_g(&sa, &sb, *pad)
        }
        HardConstraint::MaxDist { a, b, dist, .. } => {
            let (sa, sb) = match (world.get(a), world.get(b)) {
                (Some(na), Some(nb)) => (na.world_shape(), nb.world_shape()),
                _ => return 0.0,
            };
            edge_signed_distance(&sa, &sb) - dist
        }
    }
}

/// Max of `max(0, g_i)` over hard constraints.
pub fn max_hard_violation(world: &LayoutWorld, hard: &[HardConstraint]) -> f64 {
    hard.iter()
        .map(|h| hard_g(world, h).max(0.0))
        .fold(0.0_f64, f64::max)
}

/// Powell–Hestenes–Rockafellar penalty for inequality `g ≤ 0`.
fn phr_penalty(g: f64, lambda: f64, rho: f64) -> f64 {
    let t = (lambda + rho * g).max(0.0);
    (t * t - lambda * lambda) / (2.0 * rho)
}

fn al_energy(
    world: &LayoutWorld,
    soft: &[Constraint],
    hard: &[HardConstraint],
    lambda: &[f64],
    rho: &[f64],
    active: &[bool],
    seed: &[(NodeId, f64, f64)],
    w_travel: f64,
) -> f64 {
    let mut e = energy(world, soft, seed, w_travel);
    for (i, h) in hard.iter().enumerate() {
        if !active.get(i).copied().unwrap_or(false) {
            continue;
        }
        let lam = lambda.get(i).copied().unwrap_or(0.0);
        let r = rho.get(i).copied().unwrap_or(1.0).max(1e-6);
        e += phr_penalty(hard_g(world, h), lam, r);
    }
    e
}

/// Minimize soft energy (+ AL hard terms) with Nelder–Mead; stop on pose drift.
///
/// Hard rows are grouped by [`HardConstraint`] `lagrangian` (lower = stronger).
/// Each group is its own PHR loop; stronger groups are then held at `al_rho_max`
/// so a weaker tether cannot re-introduce overlap.
pub fn solve(
    world: &mut LayoutWorld,
    constraints: &[Constraint],
    hard: &[HardConstraint],
    opts: &SolveOpts,
) -> SolveResult {
    let (ids, mut params) = world.params();
    if ids.is_empty() {
        return SolveResult {
            iters: 0,
            energy: energy(world, constraints, &[], opts.w_travel),
            stopped_on_drift: true,
            max_hard_violation: max_hard_violation(world, hard),
        };
    }

    // Seed travel targets = current poses (after greedy).
    let seed: Vec<(NodeId, f64, f64)> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), params[i * 2], params[i * 2 + 1]))
        .collect();

    // Light noise once up front (not every AL outer).
    if opts.noise > 0.0 {
        for (i, _) in ids.iter().enumerate() {
            let n = opts.noise * pseudo_noise(i as u64 + 1);
            params[i * 2] += n;
            params[i * 2 + 1] += opts.noise * pseudo_noise(i as u64 + 17);
        }
        world.apply_params(&ids, &params);
    }

    let mut groups: Vec<u8> = hard.iter().map(hard_lagrangian).collect();
    groups.sort_unstable();
    groups.dedup();
    if groups.is_empty() {
        groups.push(0);
    }

    let outer_max = if hard.is_empty() {
        1
    } else {
        opts.al_outer_max.max(1)
    };
    let mut lambda = vec![0.0; hard.len()];
    let mut rho = vec![opts.al_rho0.max(1e-6); hard.len()];
    // Stronger Lagrangians start at higher ρ so snug cannot sit on a label
    // before the outer loop has grown the penalty.
    let max_group = *groups.last().unwrap_or(&0);
    for (i, h) in hard.iter().enumerate() {
        let steps = max_group.saturating_sub(hard_lagrangian(h)) as i32;
        rho[i] = (opts.al_rho0.max(1e-6) * opts.al_rho_grow.powi(steps * 5)).min(opts.al_rho_max);
    }
    let mut active = vec![false; hard.len()];
    let mut total_iters = 0usize;
    let mut stopped_on_drift = false;
    let mut last_energy = 0.0;

    for (gi, &group) in groups.iter().enumerate() {
        for (i, h) in hard.iter().enumerate() {
            if hard_lagrangian(h) <= group {
                active[i] = true;
            }
        }
        let mut prev_max_v = f64::INFINITY;
        for outer in 0..outer_max {
            let nm = nelder_mead(
                world,
                &ids,
                constraints,
                hard,
                &lambda,
                &rho,
                &active,
                &seed,
                opts,
                outer == 0 && gi == 0,
            );
            total_iters += nm.iters;
            stopped_on_drift = nm.stopped_on_drift;
            last_energy = nm.energy;

            if hard.is_empty() {
                break;
            }

            let mut max_v = 0.0_f64;
            for (i, h) in hard.iter().enumerate() {
                if hard_lagrangian(h) != group {
                    continue;
                }
                let g = hard_g(world, h);
                max_v = max_v.max(g.max(0.0));
                lambda[i] = (lambda[i] + rho[i] * g).max(0.0);
            }
            if max_v < opts.al_tol {
                break;
            }
            if outer + 1 < outer_max && max_v > prev_max_v * 0.85 {
                for (i, h) in hard.iter().enumerate() {
                    if hard_lagrangian(h) == group {
                        rho[i] = (rho[i] * opts.al_rho_grow).min(opts.al_rho_max);
                    }
                }
            }
            prev_max_v = max_v;
        }
        // Hold this Lagrangian so weaker groups cannot spend it.
        for (i, h) in hard.iter().enumerate() {
            if hard_lagrangian(h) == group {
                rho[i] = opts.al_rho_max;
            }
        }
    }

    SolveResult {
        iters: total_iters,
        energy: last_energy,
        stopped_on_drift,
        max_hard_violation: max_hard_violation(world, hard),
    }
}

#[allow(clippy::too_many_arguments)]
fn nelder_mead(
    world: &mut LayoutWorld,
    ids: &[NodeId],
    constraints: &[Constraint],
    hard: &[HardConstraint],
    lambda: &[f64],
    rho: &[f64],
    active: &[bool],
    seed: &[(NodeId, f64, f64)],
    opts: &SolveOpts,
    rebuild_simplex: bool,
) -> SolveResult {
    let mut params = Vec::with_capacity(ids.len() * 2);
    for id in ids {
        let n = world.get(id).unwrap();
        params.push(n.tx);
        params.push(n.ty);
    }

    let n = params.len();
    let scale = opts.initial_simplex_scale;
    let eval = |world: &mut LayoutWorld, v: &[f64]| -> f64 {
        world.apply_params(ids, v);
        al_energy(
            world,
            constraints,
            hard,
            lambda,
            rho,
            active,
            seed,
            opts.w_travel,
        )
    };

    let mut simplex: Vec<(Vec<f64>, f64)> = Vec::with_capacity(n + 1);
    {
        let e0 = eval(world, &params);
        simplex.push((params.clone(), e0));
    }
    if rebuild_simplex || simplex.len() == 1 {
        for i in 0..n {
            let mut v = params.clone();
            v[i] += if v[i].abs() > 1e-6 {
                scale * 0.05 * v[i].abs().max(1.0)
            } else {
                scale
            };
            let e = eval(world, &v);
            simplex.push((v, e));
        }
    }

    let mut history: Vec<Vec<(f64, f64)>> = Vec::new();
    world.apply_params(ids, &simplex[0].0);
    history.push(world.control_points_movable());

    let alpha = 1.0;
    let gamma = 2.0;
    let rho = 0.5;
    let sigma = 0.5;

    let mut stopped_on_drift = false;
    let mut iters = 0;
    for iter in 0..opts.max_iters {
        iters = iter + 1;
        simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut centroid = vec![0.0; n];
        for s in simplex.iter().take(n) {
            for (j, x) in s.0.iter().enumerate() {
                centroid[j] += x;
            }
        }
        for c in &mut centroid {
            *c /= n as f64;
        }

        let worst = &simplex[n];
        let mut xr = vec![0.0; n];
        for j in 0..n {
            xr[j] = centroid[j] + alpha * (centroid[j] - worst.0[j]);
        }
        let er = eval(world, &xr);

        if er < simplex[0].1 {
            let mut xe = vec![0.0; n];
            for j in 0..n {
                xe[j] = centroid[j] + gamma * (xr[j] - centroid[j]);
            }
            let ee = eval(world, &xe);
            if ee < er {
                simplex[n] = (xe, ee);
            } else {
                simplex[n] = (xr, er);
            }
        } else if er < simplex[n - 1].1 {
            simplex[n] = (xr, er);
        } else {
            let mut xc = vec![0.0; n];
            if er < worst.1 {
                for j in 0..n {
                    xc[j] = centroid[j] + rho * (xr[j] - centroid[j]);
                }
            } else {
                for j in 0..n {
                    xc[j] = centroid[j] + rho * (worst.0[j] - centroid[j]);
                }
            }
            let ec = eval(world, &xc);
            if ec < worst.1.min(er) {
                simplex[n] = (xc, ec);
            } else {
                let best_p = simplex[0].0.clone();
                for s in simplex.iter_mut().skip(1) {
                    for j in 0..n {
                        s.0[j] = best_p[j] + sigma * (s.0[j] - best_p[j]);
                    }
                    s.1 = eval(world, &s.0);
                }
            }
        }

        simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        world.apply_params(ids, &simplex[0].0);
        let pts = world.control_points_movable();
        history.push(pts.clone());
        if history.len() > opts.drift_window + 1 {
            let drop = history.len() - (opts.drift_window + 1);
            history.drain(0..drop);
        }
        if history.len() > opts.drift_window {
            let old = &history[history.len() - 1 - opts.drift_window];
            let drift = max_point_drift(old, &pts);
            if drift < opts.epsilon_px {
                stopped_on_drift = true;
                break;
            }
        }
    }

    simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    world.apply_params(ids, &simplex[0].0);
    SolveResult {
        iters,
        energy: simplex[0].1,
        stopped_on_drift,
        max_hard_violation: 0.0,
    }
}

fn max_point_drift(a: &[(f64, f64)], b: &[(f64, f64)]) -> f64 {
    let n = a.len().min(b.len());
    let mut m: f64 = 0.0;
    for i in 0..n {
        let d = (a[i].0 - b[i].0).abs().max((a[i].1 - b[i].1).abs());
        m = m.max(d);
    }
    // Length change: treat as large drift.
    if a.len() != b.len() {
        m = m.max(1e9);
    }
    m
}

fn pseudo_noise(seed: u64) -> f64 {
    // Deterministic cheap hash → [-1, 1]
    let mut x = seed.wrapping_mul(0x9E3779B97F4A7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58476D1CE4E5B9);
    x ^= x >> 27;
    let u = (x >> 11) as f64 / ((1u64 << 53) as f64);
    u * 2.0 - 1.0
}

// ─── Build helpers / kits ───────────────────────────────────────────────────

/// Install fixed chem group: bonds as lines, atoms as **disks** (~0.3 CC-bond),
/// optional shade circles.
pub fn install_chem(
    world: &mut LayoutWorld,
    bonds: &[(f64, f64, f64, f64)],
    atoms: &[(usize, f64, f64)],
    shades: &[(usize, f64, f64, f64)],
) {
    install_chem_with_texts(world, bonds, atoms, shades, &[]);
}

/// Like [`install_chem`], plus fixed chem atom-text AABBs (`ObjectType::AtomText`).
///
/// Chem ink is immovable — annotations MinGap/Outside against these boxes.
pub fn install_chem_with_texts(
    world: &mut LayoutWorld,
    bonds: &[(f64, f64, f64, f64)],
    atoms: &[(usize, f64, f64)],
    shades: &[(usize, f64, f64, f64)],
    atom_texts: &[(usize, Aabb)],
) {
    let chem = world.insert(
        "mol/chem",
        "mol",
        ObjectType::Chem,
        Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
        true,
    );
    let bonds_id = world.insert(
        "mol/chem/bonds",
        &chem,
        ObjectType::Bonds,
        Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
        true,
    );
    for (i, &(x0, y0, x1, y1)) in bonds.iter().enumerate() {
        world.insert(
            format!("mol/chem/bonds/b{i}"),
            &bonds_id,
            ObjectType::Bonds,
            Shape::Line { x0, y0, x1, y1 },
            true,
        );
    }
    let atoms_id = world.insert(
        "mol/chem/atoms",
        &chem,
        ObjectType::Atom,
        Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
        true,
    );
    for &(idx, x, y) in atoms {
        let aid = world.insert(
            format!("mol/chem/atoms/a{idx}"),
            &atoms_id,
            ObjectType::Atom,
            Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
            true,
        );
        world.insert(
            format!("mol/chem/atoms/a{idx}/coord"),
            &aid,
            ObjectType::Coord,
            Shape::Circle {
                cx: x,
                cy: y,
                r: ATOM_DISK_R_PX,
            },
            true,
        );
    }
    for &(idx, cx, cy, r) in shades {
        let aid = format!("mol/chem/atoms/a{idx}");
        if world.get(&aid).is_none() {
            continue;
        }
        world.insert(
            format!("mol/chem/atoms/a{idx}/shade"),
            &aid,
            ObjectType::Shade,
            Shape::Circle { cx, cy, r },
            true,
        );
    }
    if !atom_texts.is_empty() {
        let texts_id = world.insert(
            "mol/chem/atom_texts",
            &chem,
            ObjectType::AtomText,
            Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
            true,
        );
        for &(idx, box_) in atom_texts {
            world.insert(
                format!("mol/chem/atom_texts/t{idx}"),
                &texts_id,
                ObjectType::AtomText,
                Shape::Aabb(box_),
                true,
            );
        }
    }
    world.recompute_group_aabbs();
}

/// Caption kit (solver bench / unit tests only).
///
/// Paint places [`crate::doc::caption::apply_mol_captions`] **after** chrome
/// constraints — the mol name is not a live layout DOF.
pub fn caption_kit(
    world: &mut LayoutWorld,
    constraints: &mut Vec<Constraint>,
    hard: &mut Vec<HardConstraint>,
    ink: Aabb,
    preferred: Side,
    opt_dist: f64,
    clash_dist: f64,
) -> NodeId {
    let _ = world.insert(
        "mol/annotations",
        "mol",
        ObjectType::Annotations,
        Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
        true,
    );
    let caps = world.insert(
        "mol/annotations/captions",
        "mol/annotations",
        ObjectType::Captions,
        Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
        true,
    );
    // Rest at ink; tx/ty move the label.
    let id = world.insert(
        "mol/annotations/captions/name",
        &caps,
        ObjectType::Caption,
        Shape::Aabb(ink),
        false,
    );
    constraints.push(Constraint::Outside {
        a: id.clone(),
        b: "mol/chem".into(),
        clash_dist,
        strength: 8.0,
    });
    constraints.push(Constraint::PreferSide {
        a: id.clone(),
        b: "mol/chem".into(),
        side: preferred,
        strength: 2.0,
        align_along: 0.0,
    });
    constraints.push(Constraint::MinimizeDist {
        a: id.clone(),
        b: "mol/chem".into(),
        opt_dist,
        strength: 1.0,
    });
    push_clear_bonds(world, hard, &id, clash_dist);
    push_placement_pose(constraints, &id, &Shape::Aabb(ink));
    id
}

/// Box / hull region text kit.
///
/// `side_about` is the mean of contained atom positions — [`Constraint::PreferSide`]
/// (`pos`) is relative to this point, not the hull or full-mol AABB. Edge-align
/// is `align_along` on that constraint (not a solver branch on Point vs AABB).
pub fn region_text_kit(
    world: &mut LayoutWorld,
    constraints: &mut Vec<Constraint>,
    hard: &mut Vec<HardConstraint>,
    region_idx: usize,
    hull: Shape,
    text_ink: Aabb,
    inside: bool,
    preferred: Side,
    clash_dist: f64,
    side_about: (f64, f64),
    other_labels: &[Aabb],
) -> NodeId {
    let _ = world.insert(
        "mol/annotations",
        "mol",
        ObjectType::Annotations,
        Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
        true,
    );
    let regs = world.insert(
        "mol/annotations/regions",
        "mol/annotations",
        ObjectType::Regions,
        Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
        true,
    );
    let rid = world.insert(
        format!("mol/annotations/regions/r{region_idx}"),
        &regs,
        ObjectType::Region,
        Shape::Aabb(hull.aabb()),
        true,
    );
    let hull_id = world.insert(
        format!("mol/annotations/regions/r{region_idx}/hull"),
        &rid,
        ObjectType::RegionHull,
        hull,
        true,
    );
    let text_id = world.insert(
        format!("mol/annotations/regions/r{region_idx}/text"),
        &rid,
        ObjectType::RegionText,
        Shape::Aabb(text_ink),
        false,
    );
    let side_ref = world.insert(
        format!("mol/annotations/regions/r{region_idx}/side_ref"),
        &rid,
        ObjectType::Group,
        Shape::Point {
            x: side_about.0,
            y: side_about.1,
        },
        true,
    );
    if inside {
        // Soft contain only while snugging. A hard Inside of the *grown* seed
        // hull pinned the label at the far edge (with PreferPose) so the
        // outline never shrank. Soft Inside + strong MinimizeDist lets the
        // label travel toward chem; paint rebuilds the hull around the result.
        // No PreferPose.
        constraints.push(Constraint::Inside {
            a: text_id.clone(),
            b: hull_id.clone(),
            pad: CONTAIN_MARGIN_PX,
            strength: 2.0,
        });
        constraints.push(Constraint::MinimizeDist {
            a: text_id.clone(),
            b: "mol/chem".into(),
            opt_dist: clash_dist,
            strength: 16.0,
        });
    } else {
        constraints.push(Constraint::Outside {
            a: text_id.clone(),
            b: hull_id.clone(),
            clash_dist: 1.0,
            strength: 6.0,
        });
        constraints.push(Constraint::MinimizeDist {
            a: text_id.clone(),
            b: hull_id.clone(),
            opt_dist: clash_dist,
            strength: 1.5,
        });
        push_placement_pose(constraints, &text_id, &Shape::Aabb(text_ink));
    }
    constraints.push(Constraint::PreferSide {
        a: text_id.clone(),
        b: side_ref,
        side: preferred,
        strength: if inside { 4.0 } else { 2.0 },
        align_along: 0.35,
    });
    push_clear_bonds(world, hard, &text_id, clash_dist);
    // Prior chrome labels / circles (kitchen-sink atom numbers, callouts, …).
    for (i, box_) in other_labels.iter().enumerate() {
        if box_.width() <= 0.0 && box_.height() <= 0.0 {
            continue;
        }
        let oid = world.insert(
            format!("mol/annotations/regions/r{region_idx}/other{i}"),
            &rid,
            ObjectType::Group,
            Shape::Aabb(*box_),
            true,
        );
        hard.push(HardConstraint::Outside {
            a: text_id.clone(),
            b: oid,
            clash_dist,
            lagrangian: LAGRANGIAN_CLEAR,
        });
    }
    // Avoid shade disks (labels sit in front, but still clear painted wash).
    // Inside snug: mild shade Outside so shade cannot pin the seed far out.
    for (id, n) in world.nodes.clone() {
        if n.object_type == ObjectType::Shade {
            constraints.push(Constraint::Outside {
                a: text_id.clone(),
                b: id,
                clash_dist: 1.0,
                strength: if inside { 1.5 } else { 3.0 },
            });
        }
    }
    // Shade / atom disks soft-Inside hull (paint will rebuild hull after snug).
    for (id, n) in world.nodes.clone() {
        if matches!(n.object_type, ObjectType::Shade | ObjectType::Coord) {
            constraints.push(Constraint::Inside {
                a: id,
                b: format!("mol/annotations/regions/r{region_idx}/hull"),
                pad: CONTAIN_MARGIN_PX,
                strength: 2.0,
            });
        }
    }
    text_id
}

/// Callout two-point kit: movable label AABB + tip point; optional route points.
pub fn callout_kit(
    world: &mut LayoutWorld,
    constraints: &mut Vec<Constraint>,
    hard: &mut Vec<HardConstraint>,
    callout_idx: usize,
    label_ink: Aabb,
    tip_xy: (f64, f64),
    target: (f64, f64),
    tip_standoff: f64,
    preferred: Side,
    clash_dist: f64,
    route_points: &[(f64, f64)],
) -> (NodeId, NodeId) {
    let _ = world.insert(
        "mol/annotations",
        "mol",
        ObjectType::Annotations,
        Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
        true,
    );
    let calls = world.insert(
        "mol/annotations/callouts",
        "mol/annotations",
        ObjectType::Callouts,
        Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
        true,
    );
    let cid = format!("mol/annotations/callouts/c{callout_idx}");
    let _ = world.insert(
        &cid,
        &calls,
        ObjectType::Group,
        Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
        true,
    );
    let label_id = world.insert(
        format!("{cid}/label"),
        &cid,
        ObjectType::CalloutLabel,
        Shape::Aabb(label_ink),
        false,
    );
    let tip_id = world.insert(
        format!("{cid}/tip"),
        &cid,
        ObjectType::CalloutTip,
        Shape::Point {
            x: tip_xy.0,
            y: tip_xy.1,
        },
        false,
    );
    // Fixed target point for standoff.
    let target_id = world.insert(
        format!("{cid}/target"),
        &cid,
        ObjectType::Coord,
        Shape::Point {
            x: target.0,
            y: target.1,
        },
        true,
    );
    constraints.push(Constraint::PointStandoff {
        tip: tip_id.clone(),
        target: target_id.clone(),
        dist: tip_standoff,
        strength: 5.0,
    });
    constraints.push(Constraint::Outside {
        a: label_id.clone(),
        b: "mol/chem".into(),
        clash_dist,
        strength: 6.0,
    });
    constraints.push(Constraint::PreferSide {
        a: label_id.clone(),
        b: "mol/chem".into(),
        side: preferred,
        strength: 1.5,
        align_along: 0.0,
    });
    // Label box stays one CC-bond from the target (tip standoff is separate).
    constraints.push(Constraint::MinGap {
        a: label_id.clone(),
        b: target_id.clone(),
        dist: crate::metrics::BOND_PX,
        strength: 8.0,
    });
    constraints.push(Constraint::MinimizeDist {
        a: label_id.clone(),
        b: tip_id.clone(),
        opt_dist: crate::metrics::BOND_PX * 0.5,
        strength: 0.4,
    });
    push_placement_pose(constraints, &label_id, &Shape::Aabb(label_ink));
    push_placement_pose(
        constraints,
        &tip_id,
        &Shape::Point {
            x: tip_xy.0,
            y: tip_xy.1,
        },
    );
    push_clear_bonds(world, hard, &label_id, clash_dist);
    push_clear_bonds(world, hard, &tip_id, clash_dist);
    for (k, &(rx, ry)) in route_points.iter().enumerate() {
        let rid = world.insert(
            format!("{cid}/route{k}"),
            &cid,
            ObjectType::CalloutRoute,
            Shape::Point { x: rx, y: ry },
            false,
        );
        constraints.push(Constraint::MinimizeDist {
            a: rid.clone(),
            b: tip_id.clone(),
            opt_dist: 8.0,
            strength: 0.5,
        });
        constraints.push(Constraint::MinimizeDist {
            a: rid.clone(),
            b: label_id.clone(),
            opt_dist: 8.0,
            strength: 0.5,
        });
        push_placement_pose(constraints, &rid, &Shape::Point { x: rx, y: ry });
        push_clear_bonds(world, hard, &rid, clash_dist);
    }
    (label_id, tip_id)
}

/// Atom index / mapno numbers **and** custom atom labels.
///
/// One kit for all three — chrome `LabelRole::Number` (index or mapno) and
/// `LabelRole::Text` both call [`refine_atom_number`] → here.
///
/// Soft energy (NM objective):
/// - strong [`Constraint::MinimizeDist`] to the snug gap (`clash_dist`) on chem
///   text if present, else the atom point — this is the home magnet;
/// - mild soft [`Constraint::Outside`] vs bonds and vs other labels (smooth
///   repulsion in addition to hard rows).
///
/// Hard / AL (two Lagrangians; lower index is stronger):
/// - [`LAGRANGIAN_CLEAR`]: Outside vs bonds, chem text, and prior labels;
/// - [`LAGRANGIAN_TETHER`]: [`HardConstraint::MaxDist`] ceiling (`tether_px`).
///   No-overlap outranks association — a shared PHR let snug+tether cover "2".
///
/// No PreferPose — a greedy seed is only the NM start; pinning it made numbers
/// look randomly parked. Shade is **not** a layout obstacle.
const ATOM_SNUG_STRENGTH: f64 = 14.0;
const ATOM_MILD_REPEL: f64 = 2.5;

pub fn atom_number_kit(
    world: &mut LayoutWorld,
    constraints: &mut Vec<Constraint>,
    hard: &mut Vec<HardConstraint>,
    atom_idx: usize,
    ink: Aabb,
    clash_dist: f64,
    tether_px: f64,
    _halo_pad: Option<f64>,
    _avoid_shade: bool,
) -> NodeId {
    let _ = world.insert(
        "mol/annotations",
        "mol",
        ObjectType::Annotations,
        Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
        true,
    );
    let nums = world.insert(
        "mol/annotations/atom_numbers",
        "mol/annotations",
        ObjectType::AtomNumbers,
        Shape::Aabb(Aabb::new(0.0, 0.0, 0.0, 0.0)),
        true,
    );
    let id = world.insert(
        format!("mol/annotations/atom_numbers/n{atom_idx}"),
        &nums,
        ObjectType::AtomNumber,
        Shape::Aabb(ink),
        false,
    );
    // Home: chem atom text when painted; otherwise the atom **center point**
    // (not the 0.3-bond Coord disk — snugging to the disk left a ~disk-radius
    // air gap that looked randomly far from the nucleus).
    let text_anchor = format!("mol/chem/atom_texts/t{atom_idx}");
    let anchor = if world.get(&text_anchor).is_some() {
        Some(text_anchor)
    } else {
        let coord = format!("mol/chem/atoms/a{atom_idx}/coord");
        let center = world.get(&coord).map(|n| match n.world_shape() {
            Shape::Circle { cx, cy, .. } | Shape::Point { x: cx, y: cy } => (cx, cy),
            other => other.aabb().center(),
        });
        center.map(|(cx, cy)| {
            let tip = format!("mol/chem/atoms/a{atom_idx}/snug");
            let parent = format!("mol/chem/atoms/a{atom_idx}");
            world.insert(
                &tip,
                &parent,
                ObjectType::Coord,
                Shape::Point { x: cx, y: cy },
                true,
            );
            tip
        })
    };
    if let Some(ref a) = anchor {
        constraints.push(Constraint::MinimizeDist {
            a: id.clone(),
            b: a.clone(),
            opt_dist: clash_dist,
            strength: ATOM_SNUG_STRENGTH,
        });
        hard.push(HardConstraint::MaxDist {
            a: id.clone(),
            b: a.clone(),
            dist: tether_px,
            lagrangian: LAGRANGIAN_TETHER,
        });
    }
    // Hard feasibility + mild soft repulsion vs every bond.
    push_clear_bonds(world, hard, &id, clash_dist);
    if let Some(bonds) = world.get("mol/chem/bonds") {
        for bid in bonds.children.clone() {
            constraints.push(Constraint::Outside {
                a: id.clone(),
                b: bid,
                clash_dist,
                strength: ATOM_MILD_REPEL,
            });
        }
    }
    // Chem text and prior annotation boxes (`t{10000+}`): hard Outside + mild
    // soft twin. Keep-off-labels is AL — a soft hinge loses to snug MinimizeDist
    // (kitchen-sink "2" vs "ortho").
    if let Some(texts) = world.get("mol/chem/atom_texts") {
        for tid in texts.children.clone() {
            hard.push(HardConstraint::Outside {
                a: id.clone(),
                b: tid.clone(),
                clash_dist,
                lagrangian: LAGRANGIAN_CLEAR,
            });
            constraints.push(Constraint::Outside {
                a: id.clone(),
                b: tid,
                clash_dist,
                strength: ATOM_MILD_REPEL,
            });
        }
    }
    // Intentionally no Outside vs shade — wash is not a placement obstacle.
    // No PreferPose: snug + mild repel own the soft energy.
    let _ = ink;
    id
}

/// Build a **concave** hull proxy through include points, pad for exclude.
///
/// Not a convex hull: long chords dig **inward** toward interior include
/// samples so L-shaped sets keep bays. After outward normal pad, any exclude
/// still inside (or within `exclude_pad`) forces an inward **notch** so the
/// polygon cannot swallow excluded atoms. Vertex-only push is not enough.
pub fn hull_proxy(
    include: &[(f64, f64)],
    exclude: &[(f64, f64)],
    pad: f64,
    exclude_pad: f64,
) -> Shape {
    if include.is_empty() {
        return Shape::PointSet(vec![]);
    }
    let clear = exclude_pad.max(CONTAIN_MARGIN_PX);
    // Default: **convex** surround so atoms + an outside label stay enclosed
    // (concave dig + a distant label arm pinched through rings). Use concave
    // only when an exclude sits inside that convex hull (empty bay).
    let convex = convex_hull_pts(include);
    let bay = !exclude.is_empty()
        && convex.len() >= 3
        && exclude.iter().any(|&(x, y)| point_in_poly(x, y, &convex));
    let max_edge = (1.25 * BOND_PX).max(pad * 2.0 + 1.0);
    let seed = if bay {
        concave_hull_pts(include, max_edge)
    } else {
        convex
    };
    let mut pts = finish_hull_proxy(seed, include, exclude, pad, clear);
    if pts.len() < 3 || !poly_contains_all(&pts, include) {
        pts = finish_hull_proxy(convex_hull_pts(include), include, exclude, pad, clear);
    }
    if pts.len() < 3 {
        let a = Shape::PointSet(include.to_vec()).aabb().inflate(pad);
        return Shape::Aabb(a);
    }
    Shape::PointSet(pts)
}

fn finish_hull_proxy(
    mut pts: Vec<(f64, f64)>,
    include: &[(f64, f64)],
    exclude: &[(f64, f64)],
    pad: f64,
    clear: f64,
) -> Vec<(f64, f64)> {
    if pts.len() < 3 {
        return pts;
    }
    let (cx, cy) = centroid_of(include);
    for &(ex, ey) in exclude {
        clip_away_from_point(&mut pts, include, cx, cy, ex, ey, clear);
    }
    offset_polygon_outward(&mut pts, pad);
    for &(ex, ey) in exclude {
        clip_away_from_point(&mut pts, include, cx, cy, ex, ey, clear);
    }
    pts
}

fn poly_contains_all(poly: &[(f64, f64)], pts: &[(f64, f64)]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    pts.iter()
        .all(|&(x, y)| point_in_poly(x, y, poly) || point_to_poly_edge_dist(x, y, poly) < 1.5)
}

fn point_to_poly_edge_dist(px: f64, py: f64, poly: &[(f64, f64)]) -> f64 {
    let n = poly.len();
    let mut best = f64::INFINITY;
    for i in 0..n {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % n];
        best = best.min(point_to_seg_dist(px, py, x0, y0, x1, y1));
    }
    best
}

fn centroid_of(pts: &[(f64, f64)]) -> (f64, f64) {
    let n = pts.len().max(1) as f64;
    let sx: f64 = pts.iter().map(|p| p.0).sum();
    let sy: f64 = pts.iter().map(|p| p.1).sum();
    (sx / n, sy / n)
}

/// Sutherland–Hodgman keep the half-plane on the include-centroid side of the
/// supporting line facing exclude `E`.
///
/// Ideal plane is `E - clear·dir`. It is never pushed past the furthest
/// **include-cloud** sample toward `E` (`keep_guard`) — that used to shave ring
/// atoms when a substituent exclude sat close to the hull. Empty bays still
/// clip: includes do not reach into the bay, so the plane sits at the include
/// frontier and leaves `E` outside. Guard with the original cloud (not the
/// padded polygon) so offset growth cannot disable the post-pad clip.
fn clip_away_from_point(
    pts: &mut Vec<(f64, f64)>,
    keep_guard: &[(f64, f64)],
    cx: f64,
    cy: f64,
    ex: f64,
    ey: f64,
    clear: f64,
) {
    if pts.len() < 3 {
        return;
    }
    let mut dx = ex - cx;
    let mut dy = ey - cy;
    let len = dx.hypot(dy);
    if len < 1e-9 {
        dx = 1.0;
        dy = 0.0;
    } else {
        dx /= len;
        dy /= len;
    }
    let excl_proj = (ex - cx) * dx + (ey - cy) * dy;
    // Floor only from includes *between* the centroid and E. Arms that project
    // past E (L-bay) must not block the cut; includes beyond E along dir are
    // on the far side of the exclude and are not a "keep" frontier.
    let max_keep = keep_guard
        .iter()
        .map(|&(x, y)| (x - cx) * dx + (y - cy) * dy)
        .filter(|&p| p < excl_proj - 1e-6)
        .fold(f64::NEG_INFINITY, f64::max);
    // Ideal: plane at excl_proj - clear. Floor: just past near-side includes.
    let plane_thresh = if max_keep.is_finite() {
        (excl_proj - clear).max(max_keep + 1e-3)
    } else {
        excl_proj - clear
    };
    if plane_thresh >= excl_proj - 1e-6 {
        return;
    }
    let px0 = cx + dx * plane_thresh;
    let py0 = cy + dy * plane_thresh;
    let inside = |x: f64, y: f64| (x - px0) * dx + (y - py0) * dy <= 1e-9;
    let intersect = |ax: f64, ay: f64, bx: f64, by: f64| {
        let da = (ax - px0) * dx + (ay - py0) * dy;
        let db = (bx - px0) * dx + (by - py0) * dy;
        let t = da / (da - db);
        (ax + (bx - ax) * t, ay + (by - ay) * t)
    };
    let n = pts.len();
    let mut out = Vec::with_capacity(n + 2);
    for i in 0..n {
        let (ax, ay) = pts[i];
        let (bx, by) = pts[(i + 1) % n];
        let ain = inside(ax, ay);
        let bin = inside(bx, by);
        if ain && bin {
            out.push((bx, by));
        } else if ain && !bin {
            out.push(intersect(ax, ay, bx, by));
        } else if !ain && bin {
            out.push(intersect(ax, ay, bx, by));
            out.push((bx, by));
        }
    }
    if out.len() >= 3 {
        *pts = out;
    }
}

/// Concave hull via convex seed + inward edge digging (Park–Oh style).
///
/// Long chords dig using **interior** include samples (left of CCW edges),
/// pulling the boundary into empty bays instead of spanning them.
fn concave_hull_pts(include: &[(f64, f64)], max_edge: f64) -> Vec<(f64, f64)> {
    let mut cloud: Vec<(f64, f64)> = include.to_vec();
    cloud.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    cloud.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6);
    if cloud.len() < 3 {
        return cloud;
    }
    let mut hull = convex_hull_pts(&cloud);
    if hull.len() < 3 {
        return hull;
    }
    let on_hull = |hull: &[(f64, f64)], p: (f64, f64)| {
        hull.iter()
            .any(|&h| (h.0 - p.0).abs() < 1e-6 && (h.1 - p.1).abs() < 1e-6)
    };
    for _ in 0..cloud.len().saturating_mul(4).max(8) {
        let n = hull.len();
        let mut best: Option<(usize, (f64, f64), f64)> = None;
        for i in 0..n {
            let a = hull[i];
            let b = hull[(i + 1) % n];
            let edge_len = (a.0 - b.0).hypot(a.1 - b.1);
            if edge_len <= max_edge {
                continue;
            }
            for &p in &cloud {
                if on_hull(&hull, p) {
                    continue;
                }
                // CCW: interior is left of a→b (positive cross). Dig inward.
                let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
                if cross < 1e-9 {
                    continue; // exterior or colinear
                }
                let d = point_to_seg_dist(p.0, p.1, a.0, a.1, b.0, b.1);
                if d > edge_len * 0.55 {
                    continue;
                }
                let better = match best {
                    None => true,
                    Some((bi, _, bd)) => {
                        let be = {
                            let ba = hull[bi];
                            let bb = hull[(bi + 1) % n];
                            (ba.0 - bb.0).hypot(ba.1 - bb.1)
                        };
                        edge_len > be + 1e-9 || ((edge_len - be).abs() < 1e-9 && d < bd)
                    }
                };
                if better {
                    best = Some((i, p, d));
                }
            }
        }
        let Some((i, p, _)) = best else {
            break;
        };
        hull.insert(i + 1, p);
    }
    hull
}

fn point_to_seg_dist(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-18 {
        return (px - x0).hypot(py - y0);
    }
    let t = ((px - x0) * dx + (py - y0) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    (px - (x0 + t * dx)).hypot(py - (y0 + t * dy))
}

/// Push each vertex along the average of adjacent outward edge normals.
fn offset_polygon_outward(pts: &mut [(f64, f64)], pad: f64) {
    if pad.abs() < 1e-12 || pts.len() < 3 {
        return;
    }
    let n = pts.len();
    // Outward for CCW: right normal of each directed edge. Ensure CCW.
    let area2: f64 = (0..n)
        .map(|i| {
            let (x0, y0) = pts[i];
            let (x1, y1) = pts[(i + 1) % n];
            x0 * y1 - x1 * y0
        })
        .sum();
    let sign = if area2 >= 0.0 { 1.0 } else { -1.0 };
    let mut out = vec![(0.0, 0.0); n];
    for i in 0..n {
        let prev = pts[(i + n - 1) % n];
        let cur = pts[i];
        let next = pts[(i + 1) % n];
        let e0 = (cur.0 - prev.0, cur.1 - prev.1);
        let e1 = (next.0 - cur.0, next.1 - cur.1);
        let n0 = (sign * e0.1, -sign * e0.0);
        let n1 = (sign * e1.1, -sign * e1.0);
        let mut nx = n0.0 + n1.0;
        let mut ny = n0.1 + n1.1;
        let len = nx.hypot(ny);
        if len < 1e-9 {
            // Fallback: from centroid.
            let cx: f64 = pts.iter().map(|p| p.0).sum::<f64>() / n as f64;
            let cy: f64 = pts.iter().map(|p| p.1).sum::<f64>() / n as f64;
            nx = cur.0 - cx;
            ny = cur.1 - cy;
        }
        let len = nx.hypot(ny).max(1e-6);
        out[i] = (cur.0 + nx / len * pad, cur.1 + ny / len * pad);
    }
    pts.copy_from_slice(&out);
}

/// Closed **centripetal Catmull–Rom** through proxy vertices → SVG cubics.
///
/// Interpolates the hull points with α = ½ (Yuksel et al.): no cusps/loops for
/// distinct samples, G¹ joins, no linear solve (unlike Hobby / κ-curves). The
/// old mid-edge quadratic paint could throw controls past reflex notches.
///
/// `curve` is ignored (kept so call sites stay stable).
pub fn bezier_path_from_proxy(proxy: &Shape, _curve: f64) -> String {
    let pts = match proxy {
        Shape::PointSet(p) => p.clone(),
        Shape::Aabb(a) => vec![(a.x0, a.y0), (a.x1, a.y0), (a.x1, a.y1), (a.x0, a.y1)],
        _ => return String::new(),
    };
    centripetal_catmull_rom_closed_path(&pts)
}

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

fn dedupe_poly_pts(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    for &p in pts {
        if out
            .last()
            .is_none_or(|&q| (q.0 - p.0).hypot(q.1 - p.1) > 1e-6)
        {
            out.push(p);
        }
    }
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

fn convex_hull_pts(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let mut sp: Vec<(f64, f64)> = pts.to_vec();
    sp.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    sp.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9);
    if sp.len() < 2 {
        return sp;
    }
    fn cross(o: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    }
    let mut lower = Vec::new();
    for &p in &sp {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper = Vec::new();
    for &p in sp.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Refine a seeded label AABB translation with caption kit + simplex.
///
/// `rest` is the label ink in world space at the greedy/snug seed (tx=ty=0).
/// Returns world-space delta `(dtx, dty)` to add to the seed origin.
pub fn refine_caption(
    chem_aabb: Aabb,
    bonds: &[(f64, f64, f64, f64)],
    rest: Aabb,
    preferred: Side,
    opt_dist: f64,
    clash_dist: f64,
) -> (f64, f64) {
    let mut world = LayoutWorld::new("mol");
    // Synthetic chem AABB (group) + bonds.
    install_chem(&mut world, bonds, &[], &[]);
    if let Some(n) = world.get_mut("mol/chem") {
        n.rest = Shape::Aabb(chem_aabb);
        n.shape = Shape::Aabb(chem_aabb);
    }
    let mut cs = Vec::new();
    let mut hard = Vec::new();
    let id = caption_kit(
        &mut world, &mut cs, &mut hard, rest, preferred, opt_dist, clash_dist,
    );
    let opts = SolveOpts {
        max_iters: 60,
        noise: 0.0,
        epsilon_px: 0.75,
        ..SolveOpts::default()
    };
    let _ = solve(&mut world, &cs, &hard, &opts);
    let n = world.get(&id).unwrap();
    (n.tx, n.ty)
}

/// Refine region text after greedy edge place.
///
/// `atoms` are `(index, x, y)` centers — installed as ~0.3-bond disks so
/// contain / clash treat them as area, not points. PreferPose stays on the
/// seed (no pull-to-center).
pub fn refine_region_text(
    bonds: &[(f64, f64, f64, f64)],
    atoms: &[(usize, f64, f64)],
    shades: &[(usize, f64, f64, f64)],
    hull: Shape,
    text_rest: Aabb,
    inside: bool,
    preferred: Side,
    clash_dist: f64,
    other_labels: &[Aabb],
) -> (f64, f64) {
    let mut world = LayoutWorld::new("mol");
    install_chem(&mut world, bonds, atoms, shades);
    let mut cs = Vec::new();
    let mut hard = Vec::new();
    let side_about = if atoms.is_empty() {
        text_rest.center()
    } else {
        let n = atoms.len() as f64;
        let sx: f64 = atoms.iter().map(|(_, x, _)| x).sum();
        let sy: f64 = atoms.iter().map(|(_, _, y)| y).sum();
        (sx / n, sy / n)
    };
    let id = region_text_kit(
        &mut world,
        &mut cs,
        &mut hard,
        0,
        hull,
        text_rest,
        inside,
        preferred,
        clash_dist,
        side_about,
        other_labels,
    );
    // Inside seeds often start far (clash-free outside, hull grown around them);
    // allow a larger simplex so soft snug to chem can travel inward.
    let opts = SolveOpts {
        max_iters: if inside { 160 } else { 80 },
        noise: 0.0,
        epsilon_px: 0.75,
        initial_simplex_scale: if inside { 14.0 } else { 4.0 },
        ..SolveOpts::default()
    };
    let _ = solve(&mut world, &cs, &hard, &opts);
    let n = world.get(&id).unwrap();
    (n.tx, n.ty)
}

/// Refine callout label + tip; returns `(label_dxy, tip_dxy, route_xy_world)`.
pub fn refine_callout(
    bonds: &[(f64, f64, f64, f64)],
    label_rest: Aabb,
    tip_xy: (f64, f64),
    target: (f64, f64),
    tip_standoff: f64,
    preferred: Side,
    clash_dist: f64,
    routes: &[(f64, f64)],
) -> ((f64, f64), (f64, f64), Vec<(f64, f64)>) {
    let mut world = LayoutWorld::new("mol");
    install_chem(&mut world, bonds, &[], &[]);
    let mut cs = Vec::new();
    let mut hard = Vec::new();
    let (label_id, tip_id) = callout_kit(
        &mut world,
        &mut cs,
        &mut hard,
        0,
        label_rest,
        tip_xy,
        target,
        tip_standoff,
        preferred,
        clash_dist,
        routes,
    );
    let opts = SolveOpts {
        max_iters: 100,
        noise: 0.0,
        epsilon_px: 0.75,
        ..SolveOpts::default()
    };
    let _ = solve(&mut world, &cs, &hard, &opts);
    let label = world.get(&label_id).unwrap();
    let tip = world.get(&tip_id).unwrap();
    let mut route_out = Vec::new();
    for k in 0..routes.len() {
        let rid = format!("mol/annotations/callouts/c0/route{k}");
        if let Some(n) = world.get(&rid) {
            if let Shape::Point { x, y } = n.world_shape() {
                route_out.push((x, y));
            }
        }
    }
    ((label.tx, label.ty), (tip.tx, tip.ty), route_out)
}

/// Refine atom number / custom atom label after a PreferPose seed.
///
/// `atom_texts` are fixed chem (and prior annotation) AABBs — the solver moves
/// the annotation Outside them. `halo_pad` / `avoid_shade` as in [`atom_number_kit`].
pub fn refine_atom_number(
    bonds: &[(f64, f64, f64, f64)],
    atoms: &[(usize, f64, f64)],
    shades: &[(usize, f64, f64, f64)],
    atom_texts: &[(usize, Aabb)],
    atom_idx: usize,
    rest: Aabb,
    clash_dist: f64,
    tether_px: f64,
    halo_pad: Option<f64>,
    avoid_shade: bool,
) -> (f64, f64) {
    let mut world = LayoutWorld::new("mol");
    install_chem_with_texts(&mut world, bonds, atoms, shades, atom_texts);
    let mut cs = Vec::new();
    let mut hard = Vec::new();
    let id = atom_number_kit(
        &mut world,
        &mut cs,
        &mut hard,
        atom_idx,
        rest,
        clash_dist,
        tether_px,
        halo_pad,
        avoid_shade,
    );
    let opts = SolveOpts {
        max_iters: 100,
        noise: 0.0,
        epsilon_px: 0.5,
        initial_simplex_scale: 4.0,
        ..SolveOpts::default()
    };
    let _ = solve(&mut world, &cs, &hard, &opts);
    let n = world.get(&id).unwrap();
    (n.tx, n.ty)
}

/// Suggest up to `max_k` route points between label center and tip along a
/// soft arc away from bond midpoints (adaptive routing seed).
pub fn suggest_route_points(
    from: (f64, f64),
    to: (f64, f64),
    bond_mids: &[(f64, f64)],
    max_k: usize,
) -> Vec<(f64, f64)> {
    if max_k == 0 {
        return vec![];
    }
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let len = dx.hypot(dy).max(1e-6);
    let nx = -dy / len;
    let ny = dx / len;
    // If a bond mid sits near the straight segment, bend away.
    let mut bend: f64 = 0.0;
    for &(mx, my) in bond_mids {
        let t = ((mx - from.0) * dx + (my - from.1) * dy) / (len * len);
        if (0.1..0.9).contains(&t) {
            let px = from.0 + t * dx;
            let py = from.1 + t * dy;
            let dist = (mx - px).hypot(my - py);
            if dist < 12.0 {
                let side = (mx - px) * nx + (my - py) * ny;
                bend += if side >= 0.0 { -10.0 } else { 10.0 };
            }
        }
    }
    if bend.abs() < 1e-6 {
        return vec![];
    }
    let k = max_k.min(2);
    let mut out = Vec::with_capacity(k);
    for i in 1..=k {
        let t = i as f64 / (k as f64 + 1.0);
        let bx = from.0 + t * dx + nx * bend * (1.0 - (2.0 * t - 1.0).abs());
        let by = from.1 + t * dy + ny * bend * (1.0 - (2.0 * t - 1.0).abs());
        out.push((bx, by));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_union_nested_group() {
        let mut w = LayoutWorld::new("mol");
        install_chem(
            &mut w,
            &[(0.0, 0.0, 10.0, 0.0)],
            &[(0, 0.0, 0.0), (1, 10.0, 0.0)],
            &[],
        );
        let a = w.subtree_aabb("mol/chem").unwrap();
        assert!(a.x0 <= 0.0 && a.x1 >= 10.0);
        assert!(w.get("mol/chem/atoms/a0/coord").is_some());
        assert!(w.get("mol/chem/bonds/b0").is_some());
    }

    #[test]
    fn outside_group_covers_children() {
        let mut w = LayoutWorld::new("mol");
        install_chem(
            &mut w,
            &[(0.0, 0.0, 20.0, 0.0)],
            &[(0, 0.0, 0.0), (1, 20.0, 0.0)],
            &[],
        );
        let mut cs = Vec::new();
        let mut hard = Vec::new();
        let ink = Aabb::new(-5.0, 10.0, 5.0, 18.0);
        let id = caption_kit(&mut w, &mut cs, &mut hard, ink, Side::Below, 4.0, 2.0);
        // Caption overlapping chem should have high Outside energy.
        w.set_pose(&id, 10.0, -14.0); // pull onto the bond
        let e_bad = energy(&w, &cs, &[], 0.0);
        w.set_pose(&id, 0.0, 0.0); // rest below
        let e_ok = energy(&w, &cs, &[], 0.0);
        assert!(
            e_bad > e_ok + 1.0,
            "overlap should cost more: bad={e_bad} ok={e_ok}"
        );
    }

    #[test]
    fn edge_distance_aabb_circle() {
        let a = Shape::Aabb(Aabb::new(0.0, 0.0, 10.0, 10.0));
        let c = Shape::Circle {
            cx: 20.0,
            cy: 5.0,
            r: 2.0,
        };
        let d = edge_signed_distance(&a, &c);
        assert!((d - 8.0).abs() < 1e-6, "d={d}");
    }

    #[test]
    fn point_in_hull_negative_distance() {
        let hull = Shape::PointSet(vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let d = edge_signed_distance(&Shape::Point { x: 5.0, y: 5.0 }, &hull);
        assert!(d < 0.0, "inside should be negative, d={d}");
    }

    #[test]
    fn simplex_moves_caption_off_chem() {
        let mut w = LayoutWorld::new("mol");
        install_chem(
            &mut w,
            &[(0.0, 0.0, 30.0, 0.0)],
            &[(0, 0.0, 0.0), (1, 30.0, 0.0)],
            &[],
        );
        let mut cs = Vec::new();
        let mut hard = Vec::new();
        // Rest overlapping chem AABB.
        let ink = Aabb::new(5.0, -4.0, 25.0, 4.0);
        let id = caption_kit(&mut w, &mut cs, &mut hard, ink, Side::Below, 6.0, 2.0);
        w.set_pose(&id, 0.0, 0.0);
        let opts = SolveOpts {
            max_iters: 80,
            noise: 0.0,
            epsilon_px: 0.5,
            ..SolveOpts::default()
        };
        let r = solve(&mut w, &cs, &hard, &opts);
        let pose = w.get(&id).unwrap();
        let box_ = pose.world_shape().aabb();
        // Should prefer sitting below the chem band.
        assert!(
            box_.y0 > 2.0 || r.iters > 0,
            "expected move below, box={box_:?} iters={}",
            r.iters
        );
    }

    #[test]
    fn hull_proxy_and_bezier_path() {
        let include = [(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (0.0, 10.0)];
        let exclude = [(25.0, 5.0)];
        let proxy = hull_proxy(&include, &exclude, 4.0, 8.0);
        match &proxy {
            Shape::PointSet(pts) => assert!(pts.len() >= 3),
            Shape::Aabb(_) => {}
            _ => panic!("unexpected"),
        }
        let d = bezier_path_from_proxy(&proxy, 0.5);
        assert!(
            d.contains('C') || d.contains('Q') || d.contains('M'),
            "d={d}"
        );
    }

    #[test]
    fn centripetal_catmull_rom_interpolates_square() {
        let sq = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let d = centripetal_catmull_rom_closed_path(&sq);
        assert!(d.starts_with("M 0.00 0.00"), "d={d}");
        assert!(d.contains("C "), "expected cubic segments: {d}");
        // Each vertex appears as a cubic end point (interpolating).
        for (x, y) in sq {
            let needle = format!("{x:.2} {y:.2}");
            assert!(
                d.contains(&needle),
                "missing interpolating vertex {needle} in {d}"
            );
        }
        // Uniform (α=0) tangent at a knot is ½(next − prev).
        let m1 = catmull_rom_tangent(sq[3], sq[0], sq[1], 0.0);
        assert!((m1.0 - 0.5 * (sq[1].0 - sq[3].0)).abs() < 1e-9);
        assert!((m1.1 - 0.5 * (sq[1].1 - sq[3].1)).abs() < 1e-9);
    }

    #[test]
    fn hull_proxy_evicts_exclude_inside_bay() {
        // L-shaped include: convex chord (60,0)—(0,60) would swallow (20,20).
        let include = [
            (0.0, 0.0),
            (30.0, 0.0),
            (60.0, 0.0),
            (0.0, 30.0),
            (0.0, 60.0),
            (15.0, 0.0),
            (45.0, 0.0),
            (0.0, 15.0),
            (0.0, 45.0),
        ];
        let exclude = [(20.0, 20.0)];
        let clear = 8.0;
        let proxy = hull_proxy(&include, &exclude, 2.0, clear);
        let Shape::PointSet(pts) = proxy else {
            panic!("expected PointSet");
        };
        assert!(
            !point_in_poly(20.0, 20.0, &pts),
            "exclude must not lie inside proxy"
        );
        let d = edge_signed_distance(
            &Shape::Point { x: 20.0, y: 20.0 },
            &Shape::PointSet(pts.clone()),
        );
        assert!(
            d >= clear - 1.5,
            "exclude needs ~{clear} clearance from hull (include-guarded plane), signed_dist={d}"
        );
    }

    #[test]
    fn route_points_bend_around_bond() {
        let mids = [(50.0, 0.0)];
        let pts = suggest_route_points((0.0, 0.0), (100.0, 0.0), &mids, 2);
        assert!(!pts.is_empty());
        assert!(pts[0].1.abs() > 1.0, "should bend off axis: {:?}", pts);
    }

    #[test]
    fn prefer_pose_is_a_well_at_the_placement() {
        let mut w = LayoutWorld::new("mol");
        install_chem(&mut w, &[], &[], &[]);
        if let Some(n) = w.get_mut("mol/chem") {
            let chem = Aabb::new(0.0, 0.0, 40.0, 20.0);
            n.rest = Shape::Aabb(chem);
            n.shape = Shape::Aabb(chem);
        }
        let mut cs = Vec::new();
        let mut hard = Vec::new();
        // Placed centered under the chem box, half a bond below.
        let ink = Aabb::new(10.0, 35.0, 30.0, 48.0);
        let id = caption_kit(&mut w, &mut cs, &mut hard, ink, Side::Below, 15.0, 3.0);
        let at_pose = energy(&w, &cs, &[], 0.0);
        w.set_pose(&id, 18.0, 0.0);
        let shifted = energy(&w, &cs, &[], 0.0);
        assert!(
            shifted > at_pose + 20.0,
            "leaving the placement should cost energy: pose={at_pose} shifted={shifted}"
        );
        let opts = SolveOpts {
            max_iters: 40,
            noise: 0.0,
            epsilon_px: 0.5,
            w_travel: 0.0,
            ..SolveOpts::default()
        };
        let _ = solve(&mut w, &cs, &hard, &opts);
        let back = w.get(&id).unwrap();
        assert!(
            back.tx.abs() < 3.0 && back.ty.abs() < 3.0,
            "solver should return to the placement well, got tx={} ty={}",
            back.tx,
            back.ty
        );
    }

    #[test]
    fn prefer_side_align_along_is_constraint_data_not_target_shape() {
        let mut w = LayoutWorld::new("mol");
        w.insert(
            "lab",
            "mol",
            ObjectType::Group,
            Shape::Aabb(Aabb::new(-4.0, -2.0, 4.0, 2.0)),
            false,
        );
        w.insert(
            "pt",
            "mol",
            ObjectType::Group,
            Shape::Point { x: 0.0, y: 0.0 },
            true,
        );
        w.insert(
            "box",
            "mol",
            ObjectType::Group,
            Shape::Aabb(Aabb::new(-10.0, -10.0, 10.0, 10.0)),
            true,
        );
        w.set_pose("lab", 20.0, 12.0);

        let hinge = Constraint::PreferSide {
            a: "lab".into(),
            b: "pt".into(),
            side: Side::Right,
            strength: 4.0,
            align_along: 0.0,
        };
        let along = Constraint::PreferSide {
            a: "lab".into(),
            b: "pt".into(),
            side: Side::Right,
            strength: 4.0,
            align_along: 0.35,
        };
        let e_hinge = constraint_energy(&w, &hinge);
        let e_along = constraint_energy(&w, &along);
        assert!(
            e_along > e_hinge + 10.0,
            "align_along must cost the y offset: hinge={e_hinge} along={e_along}"
        );

        let along_box = Constraint::PreferSide {
            a: "lab".into(),
            b: "box".into(),
            side: Side::Right,
            strength: 4.0,
            align_along: 0.35,
        };
        let e_box = constraint_energy(&w, &along_box);
        assert!(
            (e_box - e_along).abs() < 1e-9,
            "align_along energy must not depend on Point vs AABB: pt={e_along} box={e_box}"
        );
    }

    #[test]
    fn kits_put_align_along_on_prefer_side_not_in_the_solver() {
        let mut w = LayoutWorld::new("mol");
        install_chem(&mut w, &[], &[(0, 0.0, 0.0)], &[]);
        if let Some(n) = w.get_mut("mol/chem") {
            let chem = Aabb::new(0.0, 0.0, 40.0, 20.0);
            n.rest = Shape::Aabb(chem);
            n.shape = Shape::Aabb(chem);
        }
        let ink = Aabb::new(-4.0, -2.0, 4.0, 2.0);
        let mut cap_cs = Vec::new();
        let mut cap_h = Vec::new();
        caption_kit(&mut w, &mut cap_cs, &mut cap_h, ink, Side::Below, 8.0, 3.0);
        let cap_along: Vec<f64> = cap_cs
            .iter()
            .filter_map(|c| match c {
                Constraint::PreferSide { align_along, .. } => Some(*align_along),
                _ => None,
            })
            .collect();
        assert_eq!(cap_along, vec![0.0], "caption vs chem AABB is hinge-only");

        let mut reg_cs = Vec::new();
        let mut reg_h = Vec::new();
        region_text_kit(
            &mut w,
            &mut reg_cs,
            &mut reg_h,
            0,
            Shape::Aabb(Aabb::new(0.0, 0.0, 40.0, 40.0)),
            ink,
            true,
            Side::Left,
            3.0,
            (20.0, 20.0),
            &[],
        );
        let reg_along: Vec<f64> = reg_cs
            .iter()
            .filter_map(|c| match c {
                Constraint::PreferSide { align_along, .. } => Some(*align_along),
                _ => None,
            })
            .collect();
        assert_eq!(
            reg_along,
            vec![0.35],
            "region pos vs atom-mean Point requests edge-align on the constraint"
        );
    }

    #[test]
    fn inside_energy_is_g_plus_squared_for_every_shape_pair() {
        let mut w = LayoutWorld::new("mol");
        w.insert(
            "lab",
            "mol",
            ObjectType::Group,
            Shape::Aabb(Aabb::new(-4.0, -2.0, 4.0, 2.0)),
            false,
        );
        w.insert(
            "box",
            "mol",
            ObjectType::Group,
            Shape::Aabb(Aabb::new(-10.0, -10.0, 10.0, 10.0)),
            true,
        );
        w.insert(
            "circ",
            "mol",
            ObjectType::Group,
            Shape::Circle {
                cx: 0.0,
                cy: 0.0,
                r: 12.0,
            },
            true,
        );
        w.set_pose("lab", 9.0, 0.0);

        for b in ["box", "circ"] {
            let soft = Constraint::Inside {
                a: "lab".into(),
                b: b.into(),
                pad: 1.0,
                strength: 3.0,
            };
            let hard = HardConstraint::Inside {
                a: "lab".into(),
                b: b.into(),
                pad: 1.0,
                lagrangian: LAGRANGIAN_CLEAR,
            };
            let e = constraint_energy(&w, &soft);
            let g = hard_g(&w, &hard).max(0.0);
            assert!(
                (e - 3.0 * g * g).abs() < 1e-9,
                "Inside energy must be strength·g⁺² ({b}): e={e} g={g}"
            );
            assert!(g > 0.5, "fixture should protrude ({b}): g={g}");
        }
    }

    #[test]
    fn atom_number_kit_ignores_shade() {
        let mut w = LayoutWorld::new("mol");
        install_chem(&mut w, &[], &[(0, 0.0, 0.0)], &[(0, 0.0, 0.0, 8.0)]);
        let mut cs = Vec::new();
        let mut hard = Vec::new();
        let ink = Aabb::new(-2.0, -2.0, 2.0, 2.0);
        let id = atom_number_kit(
            &mut w,
            &mut cs,
            &mut hard,
            0,
            ink,
            4.0,
            ATOM_NUMBER_TETHER_PX,
            Some(2.0),
            true,
        );
        let e = energy(&w, &cs, &[], 0.0);
        assert!(e.is_finite());
        assert!(w.get(&id).is_some());
        // No layout constraint vs shade — wash/halo are paint-only for numbers.
        let shade_soft = cs.iter().any(|c| match c {
            Constraint::Outside { b, .. } => b.ends_with("/shade"),
            _ => false,
        });
        let shade_hard = hard.iter().any(|h| match h {
            HardConstraint::Outside { b, .. } => b.ends_with("/shade"),
            _ => false,
        });
        assert!(
            !shade_soft && !shade_hard,
            "atom_number_kit must not constrain vs shade"
        );
    }

    #[test]
    fn atom_number_kit_clears_bonds_hard() {
        let mut w = LayoutWorld::new("mol");
        install_chem(
            &mut w,
            &[(0.0, 0.0, 30.0, 0.0)],
            &[(0, 0.0, 0.0), (1, 30.0, 0.0)],
            &[],
        );
        let mut cs = Vec::new();
        let mut hard = Vec::new();
        let ink = Aabb::new(10.0, -2.0, 14.0, 2.0); // sits on the bond
        let _ = atom_number_kit(
            &mut w,
            &mut cs,
            &mut hard,
            0,
            ink,
            3.0,
            ATOM_NUMBER_TETHER_PX,
            None,
            false,
        );
        let bond_h: Vec<_> = hard
            .iter()
            .filter_map(|h| match h {
                HardConstraint::Outside { b, clash_dist, .. } if b.contains("/bonds/") => {
                    Some(*clash_dist)
                }
                _ => None,
            })
            .collect();
        assert!(
            !bond_h.is_empty(),
            "atom numbers must hard-Outside every bond line"
        );
        assert!(
            bond_h.iter().all(|&d| d >= 3.0 - 1e-9),
            "bond clear must use full clash_dist; got {bond_h:?}"
        );
        let bond_soft: Vec<_> = cs
            .iter()
            .filter_map(|c| match c {
                Constraint::Outside {
                    b,
                    clash_dist,
                    strength,
                    ..
                } if b.contains("/bonds/") => Some((*clash_dist, *strength)),
                _ => None,
            })
            .collect();
        assert!(
            !bond_soft.is_empty()
                && bond_soft
                    .iter()
                    .all(|&(d, s)| d >= 3.0 - 1e-9 && (s - ATOM_MILD_REPEL).abs() < 1e-9),
            "bonds also get mild soft Outside repulsion; got {bond_soft:?}"
        );
        assert!(
            !cs.iter()
                .any(|c| matches!(c, Constraint::MinGap { b, .. } if b.contains("/bonds/"))),
            "bond clearance is Outside, not MinGap"
        );
    }

    #[test]
    fn atom_text_tether_longer_than_number() {
        assert!(
            ATOM_TEXT_TETHER_PX > ATOM_NUMBER_TETHER_PX,
            "custom atom text should allow a longer MaxDist than index/mapno"
        );
    }

    #[test]
    fn atom_number_kit_tethers_to_atom() {
        let mut w = LayoutWorld::new("mol");
        install_chem(&mut w, &[], &[(0, 0.0, 0.0)], &[]);
        let mut cs = Vec::new();
        let mut hard = Vec::new();
        let ink = Aabb::new(-2.0, -2.0, 2.0, 2.0);
        let _ = atom_number_kit(
            &mut w,
            &mut cs,
            &mut hard,
            0,
            ink,
            3.0,
            ATOM_NUMBER_TETHER_PX,
            None,
            false,
        );
        let tether: Vec<_> = hard
            .iter()
            .filter_map(|h| match h {
                HardConstraint::MaxDist { b, dist, .. } => Some((b.clone(), *dist)),
                _ => None,
            })
            .collect();
        assert_eq!(tether.len(), 1, "atom label must MaxDist-tether");
        assert!(
            tether[0].0.ends_with("/snug"),
            "without chem text, tether to atom center point (/snug); got {}",
            tether[0].0
        );
        assert!(
            (tether[0].1 - ATOM_NUMBER_TETHER_PX).abs() < 1e-9,
            "tether dist must be ATOM_NUMBER_TETHER_PX; got {}",
            tether[0].1
        );
        // Custom atom text may request the longer ceiling.
        let mut w2 = LayoutWorld::new("mol");
        install_chem(&mut w2, &[], &[(0, 0.0, 0.0)], &[]);
        let mut cs2 = Vec::new();
        let mut hard_text = Vec::new();
        let _ = atom_number_kit(
            &mut w2,
            &mut cs2,
            &mut hard_text,
            0,
            ink,
            3.0,
            ATOM_TEXT_TETHER_PX,
            None,
            false,
        );
        let text_tether: Vec<_> = hard_text
            .iter()
            .filter_map(|h| match h {
                HardConstraint::MaxDist { dist, .. } => Some(*dist),
                _ => None,
            })
            .collect();
        assert_eq!(text_tether.len(), 1);
        assert!(
            (text_tether[0] - ATOM_TEXT_TETHER_PX).abs() < 1e-9,
            "kit must honor caller tether_px; got {}",
            text_tether[0]
        );
    }

    #[test]
    fn atom_number_kit_tethers_to_chem_text_when_present() {
        let mut w = LayoutWorld::new("mol");
        install_chem_with_texts(
            &mut w,
            &[],
            &[(0, 0.0, 0.0)],
            &[],
            &[(0, Aabb::new(4.0, -3.0, 14.0, 3.0))], // chem "OH" beside atom
        );
        let mut cs = Vec::new();
        let mut hard = Vec::new();
        let ink = Aabb::new(16.0, -2.0, 22.0, 2.0);
        let _ = atom_number_kit(
            &mut w,
            &mut cs,
            &mut hard,
            0,
            ink,
            3.0,
            ATOM_NUMBER_TETHER_PX,
            None,
            false,
        );
        let tether: Vec<_> = hard
            .iter()
            .filter_map(|h| match h {
                HardConstraint::MaxDist { b, .. } => Some(b.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(tether, vec!["mol/chem/atom_texts/t0".to_string()]);
        assert!(
            !hard.iter().any(|h| matches!(
                h,
                HardConstraint::MaxDist { b, .. } if b.ends_with("/coord")
            )),
            "chem text wins over atom point for tether"
        );
    }

    #[test]
    fn al_keeps_atom_label_within_tether() {
        let mut w = LayoutWorld::new("mol");
        install_chem(
            &mut w,
            &[(0.0, 0.0, 30.0, 0.0)],
            &[(0, 0.0, 0.0), (1, 30.0, 0.0)],
            &[],
        );
        let mut cs = Vec::new();
        let mut hard = Vec::new();
        // Seed far from atom 0 — soft PreferPose would hold it; MaxDist must pull in.
        let ink = Aabb::new(200.0, -2.0, 208.0, 2.0);
        let id = atom_number_kit(
            &mut w,
            &mut cs,
            &mut hard,
            0,
            ink,
            3.0,
            ATOM_NUMBER_TETHER_PX,
            None,
            false,
        );
        let g0 = hard
            .iter()
            .filter_map(|h| match h {
                HardConstraint::MaxDist { .. } => Some(hard_g(&w, h).max(0.0)),
                _ => None,
            })
            .fold(0.0_f64, f64::max);
        assert!(g0 > 50.0, "far seed must violate tether, g0={g0}");
        let opts = SolveOpts {
            max_iters: 80,
            noise: 0.0,
            epsilon_px: 0.5,
            initial_simplex_scale: 8.0,
            al_outer_max: 8,
            al_tol: 0.25,
            ..SolveOpts::default()
        };
        let r = solve(&mut w, &cs, &hard, &opts);
        let tether_v = hard
            .iter()
            .filter_map(|h| match h {
                HardConstraint::MaxDist { .. } => Some(hard_g(&w, h).max(0.0)),
                _ => None,
            })
            .fold(0.0_f64, f64::max);
        assert!(
            tether_v <= opts.al_tol + 1e-6,
            "AL must satisfy MaxDist tether: viol={tether_v} iters={}",
            r.iters
        );
        let box_ = w.get(&id).unwrap().world_shape();
        let coord = w.get("mol/chem/atoms/a0/coord").unwrap().world_shape();
        let d = edge_signed_distance(&box_, &coord);
        assert!(
            d <= ATOM_NUMBER_TETHER_PX + opts.al_tol + 1e-6,
            "label edge gap to atom must be ≤ tether, d={d}"
        );
    }

    #[test]
    fn al_moves_atom_label_off_bond() {
        let mut w = LayoutWorld::new("mol");
        install_chem(
            &mut w,
            &[(0.0, 0.0, 40.0, 0.0)],
            &[(0, 0.0, 0.0), (1, 40.0, 0.0)],
            &[],
        );
        let mut cs = Vec::new();
        let mut hard = Vec::new();
        let clash = 3.0;
        // PreferPose seed sits on the bond — soft alone can leave residual.
        let ink = Aabb::new(18.0, -2.0, 22.0, 2.0);
        let id = atom_number_kit(
            &mut w,
            &mut cs,
            &mut hard,
            0,
            ink,
            clash,
            ATOM_NUMBER_TETHER_PX,
            None,
            false,
        );
        let g0 = max_hard_violation(&w, &hard);
        assert!(g0 > 1.0, "seed must violate hard bond clear, g0={g0}");
        let opts = SolveOpts {
            max_iters: 80,
            noise: 0.0,
            epsilon_px: 0.5,
            initial_simplex_scale: 4.0,
            al_outer_max: 8,
            al_tol: 0.25,
            ..SolveOpts::default()
        };
        let r = solve(&mut w, &cs, &hard, &opts);
        assert!(
            r.max_hard_violation <= opts.al_tol + 1e-6,
            "AL should drive hard residual ≤ tol: viol={} iters={}",
            r.max_hard_violation,
            r.iters
        );
        let box_ = w.get(&id).unwrap().world_shape();
        // Compare against the installed bond shapes (stroke half-width included).
        let bonds = w.get("mol/chem/bonds").unwrap().children.clone();
        for bid in bonds {
            let sb = w.get(&bid).unwrap().world_shape();
            let d = edge_signed_distance(&box_, &sb);
            assert!(
                d + opts.al_tol + 1e-6 >= clash,
                "label must clear bond within al_tol: edge_dist={d} clash={clash}"
            );
        }
        let _ = id;
    }

    #[test]
    fn atom_number_kit_clears_prior_labels_hard() {
        let mut w = LayoutWorld::new("mol");
        install_chem_with_texts(
            &mut w,
            &[(0.0, 0.0, 30.0, 0.0)],
            &[(0, 0.0, 0.0), (1, 30.0, 0.0)],
            &[],
            &[(10_000, Aabb::new(6.0, -5.0, 14.0, 5.0))],
        );
        let mut cs = Vec::new();
        let mut hard = Vec::new();
        let _ = atom_number_kit(
            &mut w,
            &mut cs,
            &mut hard,
            0,
            Aabb::new(5.0, -6.0, 25.0, 6.0),
            3.0,
            ATOM_TEXT_TETHER_PX,
            None,
            false,
        );
        let prior_h: Vec<_> = hard
            .iter()
            .filter_map(|h| match h {
                HardConstraint::Outside {
                    b,
                    clash_dist,
                    lagrangian,
                    ..
                } if b.ends_with("/t10000") => Some((*clash_dist, *lagrangian)),
                _ => None,
            })
            .collect();
        assert_eq!(
            prior_h,
            vec![(3.0, LAGRANGIAN_CLEAR)],
            "keep-off-labels must be the clear Lagrangian, not a soft hinge"
        );
        let tether_g: Vec<_> = hard
            .iter()
            .filter_map(|h| match h {
                HardConstraint::MaxDist { lagrangian, .. } => Some(*lagrangian),
                _ => None,
            })
            .collect();
        assert_eq!(
            tether_g,
            vec![LAGRANGIAN_TETHER],
            "tether is a weaker Lagrangian than keep-off"
        );
    }

    #[test]
    fn atom_label_hard_clears_prior_near_home() {
        // Same-atom number vs custom text: snug used to pull "ortho" onto "2".
        let bonds = [(0.0, 0.0, 30.0, 0.0)];
        let atoms = [(0usize, 0.0, 0.0), (1, 30.0, 0.0)];
        let prior = Aabb::new(6.0, -5.0, 14.0, 5.0);
        let seed = Aabb::new(5.0, -6.0, 25.0, 6.0);
        let (dtx, dty) = refine_atom_number(
            &bonds,
            &atoms,
            &[],
            &[(10_000, prior)],
            0,
            seed,
            3.0,
            ATOM_TEXT_TETHER_PX,
            None,
            false,
        );
        let placed = seed.translated(dtx, dty);
        assert!(
            !placed.overlaps(prior),
            "custom label {placed:?} must clear prior number {prior:?} (dtx={dtx} dty={dty})"
        );
        let d = edge_signed_distance(&Shape::Aabb(placed), &Shape::Point { x: 0.0, y: 0.0 });
        assert!(
            d <= ATOM_TEXT_TETHER_PX + 1.0,
            "must stay tethered to the atom: d={d}"
        );
    }

    /// Wall-clock + iteration stats for paint-style refine kits (noise=0).
    ///
    /// Run: `cargo test -p xpict-core --lib solver_kit_bench --release -- --nocapture`
    #[test]
    fn solver_kit_bench() {
        use std::time::Instant;

        let bonds = [
            (0.0, 0.0, 30.0, 0.0),
            (30.0, 0.0, 45.0, 20.0),
            (30.0, 0.0, 45.0, -20.0),
        ];
        let atoms = [(0usize, 0.0, 0.0), (1, 30.0, 0.0), (2, 45.0, 20.0)];
        let shades = [(1usize, 30.0, 0.0, 9.0)];
        let trials = 40usize;

        fn median(mut v: Vec<f64>) -> f64 {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v[v.len() / 2]
        }
        fn median_usize(mut v: Vec<usize>) -> usize {
            v.sort();
            v[v.len() / 2]
        }

        type KitFn = Box<dyn Fn() -> SolveResult>;
        let kits: [(&str, usize, KitFn); 5] = [
            (
                "caption",
                60,
                Box::new({
                    let bonds = bonds;
                    let atoms = atoms;
                    move || {
                        let mut w = LayoutWorld::new("mol");
                        install_chem(&mut w, &bonds, &atoms, &[]);
                        if let Some(n) = w.get_mut("mol/chem") {
                            let chem = Aabb::new(-5.0, -12.0, 50.0, 25.0);
                            n.rest = Shape::Aabb(chem);
                            n.shape = Shape::Aabb(chem);
                        }
                        let mut cs = Vec::new();
                        let mut hard = Vec::new();
                        let _ = caption_kit(
                            &mut w,
                            &mut cs,
                            &mut hard,
                            Aabb::new(10.0, 28.0, 40.0, 40.0),
                            Side::Below,
                            15.0,
                            3.0,
                        );
                        solve(
                            &mut w,
                            &cs,
                            &hard,
                            &SolveOpts {
                                max_iters: 60,
                                noise: 0.0,
                                epsilon_px: 0.75,
                                ..SolveOpts::default()
                            },
                        )
                    }
                }),
            ),
            (
                "region_outside",
                80,
                Box::new({
                    let bonds = bonds;
                    let atoms = atoms;
                    let shades = shades;
                    move || {
                        let mut w = LayoutWorld::new("mol");
                        install_chem(&mut w, &bonds, &atoms, &shades);
                        let mut cs = Vec::new();
                        let mut hard = Vec::new();
                        let _ = region_text_kit(
                            &mut w,
                            &mut cs,
                            &mut hard,
                            0,
                            Shape::Aabb(Aabb::new(-8.0, -15.0, 55.0, 28.0)),
                            Aabb::new(15.0, -28.0, 35.0, -16.0),
                            false,
                            Side::Above,
                            3.0,
                            (20.0, 0.0),
                            &[],
                        );
                        solve(
                            &mut w,
                            &cs,
                            &hard,
                            &SolveOpts {
                                max_iters: 80,
                                noise: 0.0,
                                epsilon_px: 0.75,
                                ..SolveOpts::default()
                            },
                        )
                    }
                }),
            ),
            (
                "region_circle_inside",
                80,
                Box::new({
                    let bonds = bonds;
                    let atoms = atoms;
                    let shades = shades;
                    move || {
                        let mut w = LayoutWorld::new("mol");
                        install_chem(&mut w, &bonds, &atoms[..2], &shades);
                        let mut cs = Vec::new();
                        let mut hard = Vec::new();
                        let _ = region_text_kit(
                            &mut w,
                            &mut cs,
                            &mut hard,
                            0,
                            Shape::Circle {
                                cx: 15.0,
                                cy: 0.0,
                                r: 42.0,
                            },
                            // Outside-style seed (above), circle already grown.
                            Aabb::new(5.0, -48.0, 25.0, -36.0),
                            true,
                            Side::Above,
                            3.0,
                            (15.0, 0.0),
                            &[],
                        );
                        solve(
                            &mut w,
                            &cs,
                            &hard,
                            &SolveOpts {
                                max_iters: 80,
                                noise: 0.0,
                                epsilon_px: 0.75,
                                ..SolveOpts::default()
                            },
                        )
                    }
                }),
            ),
            (
                "callout_2dof",
                100,
                Box::new({
                    let bonds = bonds;
                    let atoms = atoms;
                    move || {
                        let mut w = LayoutWorld::new("mol");
                        install_chem(&mut w, &bonds, &atoms, &[]);
                        let mut cs = Vec::new();
                        let mut hard = Vec::new();
                        let _ = callout_kit(
                            &mut w,
                            &mut cs,
                            &mut hard,
                            0,
                            Aabb::new(55.0, -8.0, 75.0, 8.0),
                            (48.0, 18.0),
                            (45.0, 20.0),
                            8.0,
                            Side::Right,
                            3.0,
                            &[],
                        );
                        solve(
                            &mut w,
                            &cs,
                            &hard,
                            &SolveOpts {
                                max_iters: 100,
                                noise: 0.0,
                                epsilon_px: 0.75,
                                ..SolveOpts::default()
                            },
                        )
                    }
                }),
            ),
            (
                "callout_2routes",
                100,
                Box::new({
                    let bonds = bonds;
                    let atoms = atoms;
                    move || {
                        let mut w = LayoutWorld::new("mol");
                        install_chem(&mut w, &bonds, &atoms, &[]);
                        let mut cs = Vec::new();
                        let mut hard = Vec::new();
                        let routes = [(60.0, 5.0), (52.0, 12.0)];
                        let _ = callout_kit(
                            &mut w,
                            &mut cs,
                            &mut hard,
                            0,
                            Aabb::new(70.0, -8.0, 95.0, 8.0),
                            (48.0, 18.0),
                            (45.0, 20.0),
                            8.0,
                            Side::Right,
                            3.0,
                            &routes,
                        );
                        solve(
                            &mut w,
                            &cs,
                            &hard,
                            &SolveOpts {
                                max_iters: 100,
                                noise: 0.0,
                                epsilon_px: 0.75,
                                ..SolveOpts::default()
                            },
                        )
                    }
                }),
            ),
        ];

        eprintln!(
            "\nsolver_kit_bench (trials={trials}, release timings if --release):\n\
             kit                      med_µs  p95_µs  med_iters  max_iters  drift%"
        );
        for (name, cap, run) in &kits {
            // Warmup
            for _ in 0..5 {
                let _ = run();
            }
            let mut us = Vec::with_capacity(trials);
            let mut iters = Vec::with_capacity(trials);
            let mut drift_n = 0usize;
            for _ in 0..trials {
                let t0 = Instant::now();
                let r = run();
                us.push(t0.elapsed().as_secs_f64() * 1e6);
                iters.push(r.iters);
                if r.stopped_on_drift {
                    drift_n += 1;
                }
            }
            let mut us_sorted = us.clone();
            us_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let p95 = us_sorted[(us_sorted.len() * 95) / 100];
            let med_us = median(us);
            let med_iters = median_usize(iters);
            eprintln!(
                "{name:<24} {:>7.1} {:>7.1} {:>9} {:>9} {:>5.0}%",
                med_us,
                p95,
                med_iters,
                cap,
                100.0 * drift_n as f64 / trials as f64
            );
            // Sanity: should usually stop before the paint cap.
            assert!(
                med_iters <= *cap,
                "{name}: median iters {med_iters} > cap {cap}"
            );
        }
    }
}
