//! Bond strokes: skeleton centerlines, multi-bond offsets, stereo, joins.
//!
//! Keep in sync with `src/xpict/draw/bonds.py`.

use std::collections::HashMap;

use crate::metrics::{
    hash_count, BOND_PX, CHAIN_END_GAP_PX, END_GAP_PX, OFFSET_PX, STROKE_PX, WEDGE_HALF_PX,
};
use crate::scene::Primitive;

/// Normalize engine bond orders for 2D depiction.
///
/// Indigo aromatic = 4; RDKit aromatic = 1.5. Prefer Kekulé from backends;
/// this is a safety net so aromatics never become triple lines.
pub fn depict_order(order: f64) -> f64 {
    if order >= 3.5 || (1.4..1.6).contains(&order) {
        1.0
    } else {
        order
    }
}

/// Parallel spacing for double/triple strokes (RDKit `multipleBondOffset`).
///
/// Prefer [`OFFSET_PX`]. Do **not** scale down to the post-label stroke length —
/// heteroatom insets must not collapse carbonyl spacing. Only shrink for
/// degenerate stubs shorter than two offset widths.
pub fn multi_bond_offset(length: f64) -> f64 {
    if length < 2.0 * OFFSET_PX {
        OFFSET_PX.min(length * 0.25)
    } else {
        OFFSET_PX
    }
}

/// Signed offsets of a centered multiple bond along the left normal.
///
/// A double is two lines split evenly about the axis (separation `off`).
/// A triple keeps the axis and one line `off` to each side.
pub fn centered_displacements(order: f64, off: f64) -> Vec<f64> {
    if order >= 2.5 {
        vec![-off, 0.0, off]
    } else {
        vec![-off * 0.5, off * 0.5]
    }
}

pub fn shorten(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    gap1: f64,
    gap2: f64,
) -> (f64, f64, f64, f64) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let length = (dx * dx + dy * dy).sqrt().max(1.0);
    if gap1 + gap2 >= length {
        let mx = (x1 + x2) * 0.5;
        let my = (y1 + y2) * 0.5;
        return (mx, my, mx, my);
    }
    let ux = dx / length;
    let uy = dy / length;
    (
        x1 + ux * gap1,
        y1 + uy * gap1,
        x2 - ux * gap2,
        y2 - uy * gap2,
    )
}

fn unit(x1: f64, y1: f64, x2: f64, y2: f64) -> (f64, f64, f64, f64, f64) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let length = (dx * dx + dy * dy).sqrt().max(1.0);
    (dx / length, dy / length, -dy / length, dx / length, length)
}

fn fmt2(v: f64) -> String {
    format!("{v:.2}")
}

fn line_path(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    class: &str,
    width: f64,
) -> StrokePath {
    StrokePath {
        d: format!(
            "M {} {} L {} {}",
            fmt2(x1),
            fmt2(y1),
            fmt2(x2),
            fmt2(y2)
        ),
        stroke: "#111".into(),
        fill: None,
        stroke_width: width,
        stroke_linecap: Some("round".into()),
        class: class.into(),
    }
}

/// One SVG stroke before it becomes a [`Primitive`].
///
/// `stroke_width` here is in **drawing px**. [`StrokePath::to_primitive`]
/// converts to stem units (`1.0` = [`STROKE_PX`]) for the Scene ABI.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokePath {
    pub d: String,
    pub stroke: String,
    pub fill: Option<String>,
    pub stroke_width: f64,
    pub stroke_linecap: Option<String>,
    pub class: String,
}

impl StrokePath {
    pub fn to_primitive(&self) -> Primitive {
        // Scene ABI: stroke_width is multiples of the font-stem bond ink.
        let stem_units = if self.stroke_width == 0.0 {
            0.0
        } else {
            self.stroke_width / STROKE_PX
        };
        Primitive::Path {
            d: self.d.clone(),
            stroke: Some(self.stroke.clone()),
            fill: self.fill.clone().or_else(|| Some("none".into())),
            stroke_width: stem_units,
            opacity: 1.0,
            stroke_dasharray: None,
            stroke_linecap: self.stroke_linecap.clone(),
            class: Some(self.class.clone()),
        }
    }

    pub fn with_color(mut self, color: &str) -> Self {
        self.stroke = color.into();
        if self
            .fill
            .as_ref()
            .is_some_and(|f| f != "none")
        {
            self.fill = Some(color.into());
        }
        self
    }
}

/// Strokes for one bond, ordered for painting: skeleton → offsets → stereo.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BondStrokes {
    pub skeleton: Option<StrokePath>,
    pub offsets: Vec<StrokePath>,
    pub stereo: Vec<StrokePath>,
}

impl BondStrokes {
    pub fn paint_order(&self) -> Vec<&StrokePath> {
        let mut out = Vec::new();
        if let Some(ref s) = self.skeleton {
            out.push(s);
        }
        out.extend(self.offsets.iter());
        out.extend(self.stereo.iter());
        out
    }

    pub fn into_primitives(self, color: &str) -> Vec<Primitive> {
        let mut out = Vec::new();
        if let Some(s) = self.skeleton {
            out.push(s.with_color(color).to_primitive());
        }
        for p in self.offsets {
            out.push(p.with_color(color).to_primitive());
        }
        for p in self.stereo {
            out.push(p.with_color(color).to_primitive());
        }
        out
    }
}

/// Filled wedge; tip at `(x1,y1)` stereocenter — RDKit BEGINWEDGE.
pub fn solid_wedge(x1: f64, y1: f64, x2: f64, y2: f64, half: Option<f64>) -> StrokePath {
    let half = half.unwrap_or(WEDGE_HALF_PX);
    let (_ux, _uy, nx, ny, _) = unit(x1, y1, x2, y2);
    let ax = x2 + nx * half;
    let ay = y2 + ny * half;
    let bx = x2 - nx * half;
    let by = y2 - ny * half;
    StrokePath {
        d: format!(
            "M {} {} L {} {} L {} {} Z",
            fmt2(x1),
            fmt2(y1),
            fmt2(ax),
            fmt2(ay),
            fmt2(bx),
            fmt2(by)
        ),
        stroke: "#111".into(),
        fill: Some("#111".into()),
        stroke_width: 0.6,
        stroke_linecap: Some("round".into()),
        class: "bond bond-wedge-up".into(),
    }
}

/// Hashed wedge for a bond going away from the viewer.
///
/// `(x1, y1)` is the stereocenter. Hashes are wide there and narrow at the
/// substituent.
pub fn hashed_wedge(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    half: Option<f64>,
    n: Option<usize>,
) -> Vec<StrokePath> {
    let half = half.unwrap_or(WEDGE_HALF_PX);
    let (ux, uy, nx, ny, length) = unit(x1, y1, x2, y2);
    let count = n.unwrap_or_else(|| hash_count(length));
    let mut paths = Vec::with_capacity(count);
    for i in 0..count {
        let t = (i + 1) as f64 / (count + 1) as f64;
        let w = half * (1.0 - t);
        let cx = x1 + ux * length * t;
        let cy = y1 + uy * length * t;
        paths.push(line_path(
            cx + nx * w,
            cy + ny * w,
            cx - nx * w,
            cy - ny * w,
            "bond bond-wedge-down",
            STROKE_PX,
        ));
    }
    paths
}

/// Wiggly single for unspecified stereo (RDKit UNKNOWN / Indigo EITHER).
pub fn wavy_bond(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    amp: Option<f64>,
    waves: usize,
) -> StrokePath {
    let (ux, uy, nx, ny, length) = unit(x1, y1, x2, y2);
    let amp = amp.unwrap_or(0.055 * BOND_PX);
    let mut parts = vec![format!("M {} {}", fmt2(x1), fmt2(y1))];
    let steps = waves * 2;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let side = if i % 2 == 1 { 1.0 } else { -1.0 };
        let px = x1 + ux * length * t + nx * amp * side;
        let py = y1 + uy * length * t + ny * amp * side;
        parts.push(format!("L {} {}", fmt2(px), fmt2(py)));
    }
    StrokePath {
        d: parts.join(" "),
        stroke: "#111".into(),
        fill: Some("none".into()),
        stroke_width: STROKE_PX,
        stroke_linecap: Some("round".into()),
        class: "bond bond-either".into(),
    }
}

fn offset_gap(length: f64, chain: bool) -> f64 {
    let px = if chain { CHAIN_END_GAP_PX } else { END_GAP_PX };
    px.min(length * 0.22)
}

/// Crossed double for unspecified E/Z (RDKit EITHERDOUBLE).
pub fn crossed_double(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    interior: Option<(f64, f64)>,
) -> Vec<StrokePath> {
    let (_ux, _uy, lx, ly, length) = unit(x1, y1, x2, y2);
    let (nx, ny) = interior.unwrap_or((lx, ly));
    let off = multi_bond_offset(length);
    let gap = offset_gap(length, interior.is_none());
    let (sx1, sy1, sx2, sy2) = shorten(x1, y1, x2, y2, gap, gap);
    let a1x = sx1 + nx * off;
    let a1y = sy1 + ny * off;
    let a2x = sx2 + nx * off;
    let a2y = sy2 + ny * off;
    let b1x = sx1 - nx * off;
    let b1y = sy1 - ny * off;
    let b2x = sx2 - nx * off;
    let b2y = sy2 - ny * off;
    vec![
        line_path(x1, y1, x2, y2, "bond bond-skeleton", STROKE_PX),
        line_path(a1x, a1y, b2x, b2y, "bond bond-either-cross", STROKE_PX),
        line_path(b1x, b1y, a2x, a2y, "bond bond-either-cross", STROKE_PX),
    ]
}

/// Parallel lines have `|cross(d0, d1)|` below this (unit directions).
const JOIN_PARALLEL_EPS: f64 = 1e-9;
/// Reject mitres that run farther than this fraction of the multi-bond length.
const JOIN_MAX_T_FRAC: f64 = 0.9;

/// Intersect `p0 + t d0` with `p1 + s d1`.
///
/// Returns `(t, s, ix, iy)` or `None` when the directions are parallel.
#[allow(clippy::too_many_arguments)]
pub fn line_intersect(
    p0x: f64,
    p0y: f64,
    d0x: f64,
    d0y: f64,
    p1x: f64,
    p1y: f64,
    d1x: f64,
    d1y: f64,
) -> Option<(f64, f64, f64, f64)> {
    let det = d0x * d1y - d0y * d1x;
    if det.abs() < JOIN_PARALLEL_EPS {
        return None;
    }
    let dx = p1x - p0x;
    let dy = p1y - p0y;
    let t = (dx * d1y - dy * d1x) / det;
    let s = (dx * d0y - dy * d0x) / det;
    Some((t, s, p0x + t * d0x, p0y + t * d0y))
}

/// Trim distance at begin, then end, one entry per centered line.
pub type EndTrims = (Vec<f64>, Vec<f64>);

/// One bond after label inset, before multiple-bond joins.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnBond {
    pub index: i32,
    pub begin: i32,
    pub end: i32,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub order: f64,
    pub interior: Option<(f64, f64)>,
    pub stereo: Option<String>,
    pub begin_labeled: bool,
    pub end_labeled: bool,
    /// Trim distance at begin, then end, one entry per centered line.
    pub trims: Option<EndTrims>,
}

impl DrawnBond {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index: i32,
        begin: i32,
        end: i32,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        order: f64,
    ) -> Self {
        Self {
            index,
            begin,
            end,
            x1,
            y1,
            x2,
            y2,
            order,
            interior: None,
            stereo: None,
            begin_labeled: false,
            end_labeled: false,
            trims: None,
        }
    }
}

fn plain_single(bond: &DrawnBond) -> bool {
    let order = depict_order(bond.order);
    let stereo = bond
        .stereo
        .as_deref()
        .unwrap_or("none")
        .to_ascii_lowercase();
    order < 1.5 && (stereo.is_empty() || stereo == "none")
}

fn centered_multi(bond: &DrawnBond) -> bool {
    let order = depict_order(bond.order);
    let stereo = bond
        .stereo
        .as_deref()
        .unwrap_or("none")
        .to_ascii_lowercase();
    if matches!(stereo.as_str(), "up" | "down" | "either") {
        return false;
    }
    bond.interior.is_none() && order >= 1.5
}

fn end_frame(bond: &DrawnBond, at_begin: bool) -> (f64, f64, f64, f64, f64, f64) {
    let (ux, uy, nx, ny, _length) = unit(bond.x1, bond.y1, bond.x2, bond.y2);
    if at_begin {
        (bond.x1, bond.y1, ux, uy, nx, ny)
    } else {
        (bond.x2, bond.y2, -ux, -uy, -nx, -ny)
    }
}

/// Mitre acyclic doubles and triples to their single-bond neighbors.
///
/// Each parallel stroke is an infinite line; each neighboring single is another.
/// Junction ends are **line–line intersections** (no fixed-length stubs), so
/// acute and obtuse angles both close cleanly.
///
/// One single: extend it to the far parallel line; both multiple-bond strokes
/// stop on that single.
///
/// Two singles: leave them meeting at the atom; extend each multiple-bond
/// stroke past the atom until its end lies on a single (closes the vertex
/// angle gap).
pub fn join_centered_multibonds(bonds: &mut [DrawnBond]) {
    let mut by_atom: HashMap<i32, Vec<usize>> = HashMap::new();
    for (i, bond) in bonds.iter().enumerate() {
        by_atom.entry(bond.begin).or_default().push(i);
        by_atom.entry(bond.end).or_default().push(i);
    }

    // (single index, 0=begin) -> (x, y, distance past the atom)
    let mut moves: HashMap<(i32, i32), (f64, f64, f64)> = HashMap::new();
    // (bond index, (begin trims, end trims))
    let mut trim_updates: Vec<(usize, EndTrims)> = Vec::new();

    for bi in 0..bonds.len() {
        if !centered_multi(&bonds[bi]) {
            continue;
        }
        let (_ux, _uy, _nx, _ny, length) = unit(
            bonds[bi].x1,
            bonds[bi].y1,
            bonds[bi].x2,
            bonds[bi].y2,
        );
        if length < 1.0 {
            continue;
        }
        let off = multi_bond_offset(length);
        let disps = centered_displacements(depict_order(bonds[bi].order), off);
        let end_disps = (disps.clone(), disps.iter().map(|d| -d).collect::<Vec<_>>());
        let mut trims = (
            vec![0.0; disps.len()],
            vec![0.0; disps.len()],
        );
        let mut joined = false;
        let t_lo = -0.5 * length;
        let t_hi = JOIN_MAX_T_FRAC * length;

        let ends = [
            (0usize, bonds[bi].begin, bonds[bi].begin_labeled),
            (1usize, bonds[bi].end, bonds[bi].end_labeled),
        ];
        for &(end_i, atom, labeled) in &ends {
            if labeled {
                continue;
            }
            let singles: Vec<usize> = by_atom
                .get(&atom)
                .into_iter()
                .flatten()
                .copied()
                .filter(|&oi| oi != bi && plain_single(&bonds[oi]))
                .collect();
            if singles.is_empty() || singles.len() > 2 {
                continue;
            }
            let (ex, ey, ux, uy, nx, ny) = end_frame(&bonds[bi], end_i == 0);
            let disps_e = if end_i == 0 {
                &end_disps.0
            } else {
                &end_disps.1
            };

            let mut line_ts: Vec<Vec<f64>> = vec![Vec::new(); disps_e.len()];
            for &si in &singles {
                let single = &bonds[si];
                let (sx, sy, end_flag) = if single.begin == atom {
                    (single.x2, single.y2, 0i32)
                } else {
                    (single.x1, single.y1, 1i32)
                };
                let vx = sx - ex;
                let vy = sy - ey;
                let vlen = (vx * vx + vy * vy).sqrt();
                if vlen < 1e-6 {
                    continue;
                }
                let vhx = vx / vlen;
                let vhy = vy / vlen;
                for (i, &d) in disps_e.iter().enumerate() {
                    let Some((ti, _s, _ix, _iy)) =
                        line_intersect(ex + nx * d, ey + ny * d, ux, uy, ex, ey, vhx, vhy)
                    else {
                        continue;
                    };
                    if ti > t_lo && ti < t_hi && ti.abs() > 1e-9 {
                        line_ts[i].push(ti);
                    }
                }
                if singles.len() == 1 {
                    let side = ux * vhy - uy * vhx;
                    let d_far = if side > 0.0 {
                        disps_e.iter().cloned().fold(f64::INFINITY, f64::min)
                    } else {
                        disps_e.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                    };
                    if let Some((t_far, _s, px, py)) =
                        line_intersect(ex + nx * d_far, ey + ny * d_far, ux, uy, ex, ey, vhx, vhy)
                    {
                        if t_far > 0.0 && t_far < t_hi {
                            let dist = ((px - ex).powi(2) + (py - ey).powi(2)).sqrt();
                            let key = (single.index, end_flag);
                            let replace = match moves.get(&key) {
                                None => true,
                                Some(&(_, _, prev)) => dist > prev,
                            };
                            if replace {
                                moves.insert(key, (px, py, dist));
                            }
                        }
                    }
                }
            }

            for (i, ts) in line_ts.iter().enumerate() {
                if ts.is_empty() {
                    continue;
                }
                if singles.len() == 2 {
                    let neg: Vec<f64> = ts.iter().copied().filter(|&t| t < 0.0).collect();
                    let pos: Vec<f64> = ts.iter().copied().filter(|&t| t > 0.0).collect();
                    if !neg.is_empty() {
                        let v = neg.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                        if end_i == 0 {
                            trims.0[i] = v;
                        } else {
                            trims.1[i] = v;
                        }
                    } else if !pos.is_empty() {
                        let v = pos.iter().cloned().fold(f64::INFINITY, f64::min);
                        if end_i == 0 {
                            trims.0[i] = v;
                        } else {
                            trims.1[i] = v;
                        }
                    }
                } else {
                    for &ti in ts {
                        if ti > 0.0 {
                            let cur = if end_i == 0 { trims.0[i] } else { trims.1[i] };
                            if cur <= 0.0 || ti < cur {
                                if end_i == 0 {
                                    trims.0[i] = ti;
                                } else {
                                    trims.1[i] = ti;
                                }
                            }
                        } else {
                            let cur = if end_i == 0 { trims.0[i] } else { trims.1[i] };
                            if cur == 0.0 || (cur < 0.0 && ti > cur) {
                                if end_i == 0 {
                                    trims.0[i] = ti;
                                } else {
                                    trims.1[i] = ti;
                                }
                            }
                        }
                    }
                }
                joined = true;
            }
        }
        if joined {
            trim_updates.push((bi, trims));
        }
    }

    for (bi, trims) in trim_updates {
        bonds[bi].trims = Some(trims);
    }
    for ((index, end_flag), (px, py, _dist)) in moves {
        if let Some(single) = bonds.iter_mut().find(|b| b.index == index) {
            if end_flag == 0 {
                single.x1 = px;
                single.y1 = py;
            } else {
                single.x2 = px;
                single.y2 = py;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn centered_strokes(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    order: f64,
    off: f64,
    ux: f64,
    uy: f64,
    nx: f64,
    ny: f64,
    length: f64,
    trims: Option<&EndTrims>,
) -> BondStrokes {
    let disps = centered_displacements(order, off);
    let (t1_owned, t2_owned) = match trims {
        Some((a, b)) => (a.clone(), b.clone()),
        None => (vec![0.0; disps.len()], vec![0.0; disps.len()]),
    };
    let mut skeleton = None;
    let mut offsets = Vec::new();
    for (i, &disp) in disps.iter().enumerate() {
        let mut t1 = t1_owned.get(i).copied().unwrap_or(0.0);
        let mut t2 = t2_owned.get(i).copied().unwrap_or(0.0);
        if t1 + t2 > length * 0.9 && t1 + t2 > 0.0 {
            let scale = (length * 0.9) / (t1 + t2);
            t1 *= scale;
            t2 *= scale;
        }
        let ax = x1 + nx * disp + ux * t1;
        let ay = y1 + ny * disp + uy * t1;
        let bx = x2 + nx * disp - ux * t2;
        let by = y2 + ny * disp - uy * t2;
        let class = if disp.abs() < 1e-9 {
            "bond bond-skeleton"
        } else {
            "bond bond-offset"
        };
        let line = line_path(ax, ay, bx, by, class, STROKE_PX);
        if disp.abs() < 1e-9 {
            skeleton = Some(line);
        } else {
            offsets.push(line);
        }
    }
    BondStrokes {
        skeleton,
        offsets,
        stereo: Vec::new(),
    }
}

/// Build skeleton / offset / stereo strokes for one bond.
///
/// `stereo`: `up` | `down` | `either` | `none` | `None`.
/// Thin end of a solid wedge, and the thick end of a hashed wedge, are at
/// `(x1, y1)`. Callers pass the stereocenter first.
///
/// `trims` are per-line inset distances at `(x1, y1)` then `(x2, y2)`
/// for a centered multiple bond (see [`join_centered_multibonds`]).
#[allow(clippy::too_many_arguments)]
pub fn bond_strokes(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    order: f64,
    interior: Option<(f64, f64)>,
    stereo: Option<&str>,
    trims: Option<&EndTrims>,
) -> BondStrokes {
    let order = depict_order(order);
    let mut stereo = stereo.unwrap_or("none").to_ascii_lowercase();
    if stereo == "none" {
        stereo.clear();
    }

    if stereo == "up" && order < 1.5 {
        return BondStrokes {
            stereo: vec![solid_wedge(x1, y1, x2, y2, None)],
            ..Default::default()
        };
    }
    if stereo == "down" && order < 1.5 {
        return BondStrokes {
            stereo: hashed_wedge(x1, y1, x2, y2, None, None),
            ..Default::default()
        };
    }
    if stereo == "either" && order < 1.5 {
        return BondStrokes {
            stereo: vec![wavy_bond(x1, y1, x2, y2, None, 5)],
            ..Default::default()
        };
    }
    if stereo == "either" && order >= 1.5 {
        return BondStrokes {
            stereo: crossed_double(x1, y1, x2, y2, interior),
            ..Default::default()
        };
    }

    let (ux, uy, lx, ly, length) = unit(x1, y1, x2, y2);
    let skeleton = line_path(x1, y1, x2, y2, "bond bond-skeleton", STROKE_PX);
    if order < 1.5 {
        return BondStrokes {
            skeleton: Some(skeleton),
            ..Default::default()
        };
    }

    let off = multi_bond_offset(length);
    let Some(interior) = interior else {
        return centered_strokes(x1, y1, x2, y2, order, off, ux, uy, lx, ly, length, trims);
    };

    let gap = offset_gap(length, false);
    let (sx1, sy1, sx2, sy2) = shorten(x1, y1, x2, y2, gap, gap);
    let mut offsets = Vec::new();
    if order >= 2.5 {
        for side in [-1.0, 1.0] {
            let ox = lx * off * side;
            let oy = ly * off * side;
            offsets.push(line_path(
                sx1 + ox,
                sy1 + oy,
                sx2 + ox,
                sy2 + oy,
                "bond bond-offset",
                STROKE_PX,
            ));
        }
        return BondStrokes {
            skeleton: Some(skeleton),
            offsets,
            stereo: Vec::new(),
        };
    }
    offsets.push(line_path(
        sx1 + interior.0 * off,
        sy1 + interior.1 * off,
        sx2 + interior.0 * off,
        sy2 + interior.1 * off,
        "bond bond-offset",
        STROKE_PX,
    ));
    BondStrokes {
        skeleton: Some(skeleton),
        offsets,
        stereo: Vec::new(),
    }
}

/// Flat list (skeleton then offsets then stereo).
pub fn bond_paths(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    order: f64,
    interior: Option<(f64, f64)>,
    stereo: Option<&str>,
) -> Vec<StrokePath> {
    bond_strokes(x1, y1, x2, y2, order, interior, stereo, None)
        .paint_order()
        .into_iter()
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_ys(d: &str) -> f64 {
        // "M x y L ..." — second number is y of start.
        let nums: Vec<f64> = d
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        nums[1]
    }

    fn path_pts(d: &str) -> Vec<(f64, f64)> {
        let nums: Vec<f64> = d
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        nums.chunks(2).map(|c| (c[0], c[1])).collect()
    }

    fn on_line(px: f64, py: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> bool {
        let cross = ((x2 - x1) * (py - y1) - (y2 - y1) * (px - x1)).abs();
        let length = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt().max(1.0);
        cross / length < 0.08
    }

    #[test]
    fn short_visible_stroke_keeps_full_offset() {
        let stub = 10.0;
        assert!(stub * 0.22 < OFFSET_PX);
        assert!(stub >= 2.0 * OFFSET_PX);
        assert!((multi_bond_offset(stub) - OFFSET_PX).abs() < 1e-9);
        assert!((multi_bond_offset(BOND_PX) - OFFSET_PX).abs() < 1e-9);
    }

    #[test]
    fn double_is_symmetric_about_axis() {
        let d = centered_displacements(2.0, OFFSET_PX);
        assert_eq!(d.len(), 2);
        assert!((d[0] + d[1]).abs() < 1e-9);
        assert!((d[1] - d[0] - OFFSET_PX).abs() < 1e-9);
    }

    #[test]
    fn skeleton_then_offset_for_double() {
        let strokes = bond_strokes(0.0, 0.0, 20.0, 0.0, 2.0, None, None, None);
        assert!(strokes.skeleton.is_none());
        assert_eq!(strokes.offsets.len(), 2);
        assert!(strokes
            .offsets
            .iter()
            .all(|p| p.class.contains("bond-offset")));
        let mut ys: Vec<f64> = strokes.offsets.iter().map(|p| path_ys(&p.d)).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((ys[0] + ys[1]).abs() < 0.05);
        assert!((ys[0].abs() - OFFSET_PX / 2.0).abs() < 0.05);
    }

    #[test]
    fn triple_has_skeleton_and_two_offsets() {
        let strokes = bond_strokes(0.0, 0.0, 30.0, 0.0, 3.0, None, None, None);
        assert!(strokes.skeleton.is_some());
        assert!(strokes
            .skeleton
            .as_ref()
            .unwrap()
            .class
            .contains("bond-skeleton"));
        assert_eq!(strokes.offsets.len(), 2);
        let skel_y = path_ys(&strokes.skeleton.as_ref().unwrap().d);
        assert!(skel_y.abs() < 0.05);
        let mut off_y: Vec<f64> = strokes.offsets.iter().map(|p| path_ys(&p.d)).collect();
        off_y.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((off_y[0] + off_y[1]).abs() < 0.05);
    }

    #[test]
    fn solid_wedge_tip_at_begin() {
        let w = solid_wedge(1.0, 2.0, 11.0, 2.0, None);
        assert!(w.d.starts_with("M 1.00 2.00"));
        assert_eq!(w.fill.as_deref(), Some("#111"));
        assert!(w.class.contains("wedge-up"));
    }

    #[test]
    fn hashed_wedge_wide_at_stereocenter() {
        let dashes = hashed_wedge(0.0, 0.0, 40.0, 0.0, None, None);
        assert!(dashes.len() >= 4);
        let mut widths = Vec::new();
        let mut centers = Vec::new();
        for dash in &dashes {
            let pts = path_pts(&dash.d);
            widths.push((pts[0].1 - pts[1].1).abs());
            centers.push((pts[0].0 + pts[1].0) * 0.5);
        }
        assert!(centers[0] < *centers.last().unwrap());
        assert!(widths[0] > widths.last().unwrap() * 2.0);
    }

    #[test]
    fn stereo_up_replaces_skeleton() {
        let strokes = bond_strokes(0.0, 0.0, 10.0, 0.0, 1.0, None, Some("up"), None);
        assert!(strokes.skeleton.is_none());
        assert!(strokes.stereo[0].class.contains("wedge-up"));
    }

    #[test]
    fn either_single_is_wavy() {
        let strokes = bond_strokes(0.0, 0.0, 20.0, 0.0, 1.0, None, Some("either"), None);
        assert!(strokes.skeleton.is_none());
        assert!(strokes.stereo[0].class.contains("bond-either"));
    }

    #[test]
    fn either_double_is_crossed() {
        let strokes = bond_strokes(0.0, 0.0, 20.0, 0.0, 2.0, None, Some("either"), None);
        assert!(strokes
            .stereo
            .iter()
            .any(|p| p.class.contains("either-cross")));
    }

    #[test]
    fn one_single_extends_to_the_far_line() {
        let mut bonds = vec![
            DrawnBond::new(0, 0, 1, 0.0, 0.0, 20.0, 0.0, 2.0),
            DrawnBond::new(1, 0, 2, 0.0, 0.0, -10.0, 10.0, 1.0),
        ];
        join_centered_multibonds(&mut bonds);
        assert!(bonds[1].y1 < -0.5);
        assert!((bonds[1].y1 + OFFSET_PX / 2.0).abs() < 0.05);
        assert!(on_line(0.0, 0.0, bonds[1].x1, bonds[1].y1, -10.0, 10.0));
        let strokes = bond_strokes(
            0.0,
            0.0,
            20.0,
            0.0,
            2.0,
            None,
            None,
            bonds[0].trims.as_ref(),
        );
        for path in &strokes.offsets {
            let end = path_pts(&path.d)[0];
            assert!(on_line(
                end.0,
                end.1,
                bonds[1].x1,
                bonds[1].y1,
                -10.0,
                10.0
            ));
        }
    }

    #[test]
    fn two_singles_meet_and_clip_the_double() {
        let mut bonds = vec![
            DrawnBond::new(0, 0, 1, 0.0, 0.0, 20.0, 0.0, 2.0),
            DrawnBond::new(1, 0, 2, 0.0, 0.0, -10.0, 8.0, 1.0),
            DrawnBond::new(2, 0, 3, 0.0, 0.0, -10.0, -8.0, 1.0),
        ];
        join_centered_multibonds(&mut bonds);
        assert!((bonds[1].x1).abs() < 1e-9);
        assert!((bonds[1].y1).abs() < 1e-9);
        assert!((bonds[2].x1).abs() < 1e-9);
        assert!((bonds[2].y1).abs() < 1e-9);
        let trims = bonds[0].trims.as_ref().expect("trims");
        assert!(trims.0.iter().all(|&t| t < 0.0));
        let strokes = bond_strokes(0.0, 0.0, 20.0, 0.0, 2.0, None, None, Some(trims));
        for path in &strokes.offsets {
            let (x, y) = path_pts(&path.d)[0];
            let on_upper = on_line(x, y, 0.0, 0.0, -10.0, 8.0);
            let on_lower = on_line(x, y, 0.0, 0.0, -10.0, -8.0);
            assert!(on_upper || on_lower);
            assert!(x < -0.2);
        }
    }

    #[test]
    fn far_end_single_joins_too() {
        let mut bonds = vec![
            DrawnBond::new(0, 0, 1, 0.0, 0.0, 20.0, 0.0, 2.0),
            DrawnBond::new(1, 1, 2, 20.0, 0.0, 30.0, -10.0, 1.0),
        ];
        join_centered_multibonds(&mut bonds);
        assert!((bonds[1].y1 - OFFSET_PX / 2.0).abs() < 0.05);
        assert!(bonds[1].x1 < 20.0);
        let strokes = bond_strokes(
            0.0,
            0.0,
            20.0,
            0.0,
            2.0,
            None,
            None,
            bonds[0].trims.as_ref(),
        );
        for path in &strokes.offsets {
            let end = path_pts(&path.d)[1];
            assert!(on_line(
                end.0,
                end.1,
                bonds[1].x1,
                bonds[1].y1,
                30.0,
                -10.0
            ));
        }
    }

    #[test]
    fn acute_two_singles_still_land_on_lines() {
        // ~25° between singles — old JOIN_MIN_SIN (0.34) rejected this angle.
        let mut bonds = vec![
            DrawnBond::new(0, 0, 1, 0.0, 0.0, 20.0, 0.0, 2.0),
            DrawnBond::new(1, 0, 2, 0.0, 0.0, -18.0, 4.0, 1.0),
            DrawnBond::new(2, 0, 3, 0.0, 0.0, -18.0, -4.0, 1.0),
        ];
        join_centered_multibonds(&mut bonds);
        let trims = bonds[0].trims.as_ref().expect("trims at acute junction");
        assert!(trims.0.iter().all(|&t| t < 0.0));
        let strokes = bond_strokes(0.0, 0.0, 20.0, 0.0, 2.0, None, None, Some(trims));
        for path in &strokes.offsets {
            let (x, y) = path_pts(&path.d)[0];
            let on_upper = on_line(x, y, 0.0, 0.0, -18.0, 4.0);
            let on_lower = on_line(x, y, 0.0, 0.0, -18.0, -4.0);
            assert!(on_upper || on_lower, "end ({x},{y}) not on a single");
            assert!(x < -0.2);
        }
    }

    #[test]
    fn line_intersect_crosses_unit_axes() {
        let (t, s, ix, iy) = line_intersect(0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0).unwrap();
        assert!((t - 1.0).abs() < 1e-9);
        assert!((s - 1.0).abs() < 1e-9);
        assert!((ix - 1.0).abs() < 1e-9);
        assert!((iy - 1.0).abs() < 1e-9);
        assert!(line_intersect(0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0).is_none());
    }

    #[test]
    fn ring_double_keeps_skeleton_and_interior_offset() {
        let strokes = bond_strokes(
            0.0,
            0.0,
            BOND_PX,
            0.0,
            2.0,
            Some((0.0, -1.0)),
            None,
            None,
        );
        assert!(strokes.skeleton.is_some());
        assert_eq!(strokes.offsets.len(), 1);
        let y = path_ys(&strokes.offsets[0].d);
        assert!(y < -1.0);
    }
}
