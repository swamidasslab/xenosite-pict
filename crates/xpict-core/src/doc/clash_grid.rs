//! On-demand occupancy raster for layout objects.
//!
//! Each node can own painted ink ([`InkStamp`]s). The first clash query
//! rasterizes a signed-distance grid and caches it; later queries are samples.

use super::layout_items::Aabb;
use crate::metrics::{BOND_PX, STROKE_PX};

/// Cell size in CSS-px. Fine enough for ⅓-bond standoff, cheap for a mol.
pub const CLASH_CELL: f64 = 2.0;
/// Extra halo around the union AABB so standoff queries stay in-grid.
pub const CLASH_PAD: f64 = BOND_PX;

/// Primitive ink a layout object can stamp into its clash grid.
#[derive(Debug, Clone)]
pub enum InkStamp {
    Aabb(Aabb),
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
        r: f64,
    },
    Point {
        x: f64,
        y: f64,
    },
}

/// Cached occupancy + chamfer signed distance (positive = outside ink).
#[derive(Debug, Clone)]
pub struct ClashGrid {
    pub x0: f64,
    pub y0: f64,
    pub cell: f64,
    pub cols: usize,
    pub rows: usize,
    /// Signed distance in CSS-px, row-major.
    sdf: Vec<f32>,
}

impl ClashGrid {
    pub fn from_stamps(stamps: &[InkStamp]) -> Option<Self> {
        if stamps.is_empty() {
            return None;
        }
        let mut box_ = Aabb::new(0.0, 0.0, 0.0, 0.0);
        let mut any = false;
        for s in stamps {
            let a = stamp_aabb(s);
            if !any {
                box_ = a;
                any = true;
            } else {
                box_ = Aabb::new(
                    box_.x0.min(a.x0),
                    box_.y0.min(a.y0),
                    box_.x1.max(a.x1),
                    box_.y1.max(a.y1),
                );
            }
        }
        if !any {
            return None;
        }
        let box_ = box_.inflate(CLASH_PAD);
        let mut cell = CLASH_CELL;
        let mut cols = ((box_.width() / cell).ceil() as usize).max(1);
        let mut rows = ((box_.height() / cell).ceil() as usize).max(1);
        const MAX: usize = 256 * 256;
        while cols * rows > MAX {
            cell *= 2.0;
            cols = ((box_.width() / cell).ceil() as usize).max(1);
            rows = ((box_.height() / cell).ceil() as usize).max(1);
        }
        let mut occ = vec![0u8; cols * rows];
        for s in stamps {
            stamp(&mut occ, cols, rows, box_.x0, box_.y0, cell, s);
        }
        let sdf = chamfer_sdf(&occ, cols, rows, cell);
        Some(Self {
            x0: box_.x0,
            y0: box_.y0,
            cell,
            cols,
            rows,
            sdf,
        })
    }

    /// Signed distance in CSS-px: negative inside ink, positive outside.
    pub fn signed_at(&self, x: f64, y: f64) -> f64 {
        let gx = (x - self.x0) / self.cell - 0.5;
        let gy = (y - self.y0) / self.cell - 0.5;
        if gx < 0.0 || gy < 0.0 || gx > (self.cols - 1) as f64 || gy > (self.rows - 1) as f64 {
            let cx = gx.clamp(0.0, (self.cols.saturating_sub(1)) as f64);
            let cy = gy.clamp(0.0, (self.rows.saturating_sub(1)) as f64);
            let edge = self.sample(cx, cy);
            let dx = (gx - cx) * self.cell;
            let dy = (gy - cy) * self.cell;
            return edge + dx.hypot(dy);
        }
        self.sample(gx, gy)
    }
}

impl ClashGrid {
    fn sample(&self, gx: f64, gy: f64) -> f64 {
        let x0 = gx.floor() as usize;
        let y0 = gy.floor() as usize;
        let x1 = (x0 + 1).min(self.cols - 1);
        let y1 = (y0 + 1).min(self.rows - 1);
        let tx = (gx - x0 as f64).clamp(0.0, 1.0);
        let ty = (gy - y0 as f64).clamp(0.0, 1.0);
        let a = self.sdf[y0 * self.cols + x0] as f64;
        let b = self.sdf[y0 * self.cols + x1] as f64;
        let c = self.sdf[y1 * self.cols + x0] as f64;
        let d = self.sdf[y1 * self.cols + x1] as f64;
        let top = a + (b - a) * tx;
        let bot = c + (d - c) * tx;
        top + (bot - top) * ty
    }
}

fn stamp_aabb(s: &InkStamp) -> Aabb {
    match *s {
        InkStamp::Aabb(a) => a,
        InkStamp::Circle { cx, cy, r } => Aabb::new(cx - r, cy - r, cx + r, cy + r),
        InkStamp::Line { x0, y0, x1, y1, r } => Aabb::new(
            x0.min(x1) - r,
            y0.min(y1) - r,
            x0.max(x1) + r,
            y0.max(y1) + r,
        ),
        InkStamp::Point { x, y } => Aabb::new(x, y, x, y),
    }
}

fn stamp(occ: &mut [u8], cols: usize, rows: usize, x0: f64, y0: f64, cell: f64, s: &InkStamp) {
    match *s {
        InkStamp::Aabb(a) => fill_aabb(occ, cols, rows, x0, y0, cell, a),
        InkStamp::Circle { cx, cy, r } => fill_circle(occ, cols, rows, x0, y0, cell, cx, cy, r),
        InkStamp::Line {
            x0: ax,
            y0: ay,
            x1: bx,
            y1: by,
            r,
        } => fill_line(occ, cols, rows, x0, y0, cell, ax, ay, bx, by, r),
        InkStamp::Point { x, y } => fill_circle(
            occ,
            cols,
            rows,
            x0,
            y0,
            cell,
            x,
            y,
            (STROKE_PX * 0.5).max(cell),
        ),
    }
}

fn ix(x: f64, origin: f64, cell: f64, n: usize) -> usize {
    (((x - origin) / cell).floor() as i32).clamp(0, n.saturating_sub(1) as i32) as usize
}

fn fill_aabb(occ: &mut [u8], cols: usize, rows: usize, x0: f64, y0: f64, cell: f64, a: Aabb) {
    let c0 = ix(a.x0, x0, cell, cols);
    let c1 = ix(a.x1, x0, cell, cols);
    let r0 = ix(a.y0, y0, cell, rows);
    let r1 = ix(a.y1, y0, cell, rows);
    for r in r0..=r1 {
        for c in c0..=c1 {
            occ[r * cols + c] = 1;
        }
    }
}

fn fill_circle(
    occ: &mut [u8],
    cols: usize,
    rows: usize,
    x0: f64,
    y0: f64,
    cell: f64,
    cx: f64,
    cy: f64,
    r: f64,
) {
    let rr = r * r;
    let c0 = ix(cx - r, x0, cell, cols);
    let c1 = ix(cx + r, x0, cell, cols);
    let r0 = ix(cy - r, y0, cell, rows);
    let r1 = ix(cy + r, y0, cell, rows);
    for row in r0..=r1 {
        for col in c0..=c1 {
            let x = x0 + (col as f64 + 0.5) * cell;
            let y = y0 + (row as f64 + 0.5) * cell;
            if (x - cx) * (x - cx) + (y - cy) * (y - cy) <= rr {
                occ[row * cols + col] = 1;
            }
        }
    }
}

fn fill_line(
    occ: &mut [u8],
    cols: usize,
    rows: usize,
    ox: f64,
    oy: f64,
    cell: f64,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    r: f64,
) {
    let len = (x1 - x0).hypot(y1 - y0).max(cell);
    let n = ((len / (cell * 0.5)).ceil() as usize).max(1);
    for i in 0..=n {
        let t = i as f64 / n as f64;
        fill_circle(
            occ,
            cols,
            rows,
            ox,
            oy,
            cell,
            x0 + (x1 - x0) * t,
            y0 + (y1 - y0) * t,
            r,
        );
    }
}

/// 3-4 chamfer distance transform → signed CSS-px.
fn chamfer_sdf(occ: &[u8], cols: usize, rows: usize, cell: f64) -> Vec<f32> {
    const INF: f32 = 1.0e8;
    let n = cols * rows;
    let mut dout = vec![INF; n];
    let mut din = vec![INF; n];
    for i in 0..n {
        if occ[i] != 0 {
            dout[i] = 0.0;
        } else {
            din[i] = 0.0;
        }
    }
    chamfer_pass(&mut dout, cols, rows);
    chamfer_pass(&mut din, cols, rows);
    let scale = (cell / 3.0) as f32;
    let mut sdf = vec![0f32; n];
    for i in 0..n {
        if occ[i] != 0 {
            sdf[i] = -din[i] * scale - 0.5 * cell as f32;
        } else {
            sdf[i] = dout[i] * scale;
        }
    }
    sdf
}

fn chamfer_pass(d: &mut [f32], cols: usize, rows: usize) {
    for y in 0..rows {
        for x in 0..cols {
            let i = y * cols + x;
            let mut v = d[i];
            if x > 0 {
                v = v.min(d[i - 1] + 3.0);
            }
            if y > 0 {
                v = v.min(d[i - cols] + 3.0);
            }
            if x > 0 && y > 0 {
                v = v.min(d[i - cols - 1] + 4.0);
            }
            if x + 1 < cols && y > 0 {
                v = v.min(d[i - cols + 1] + 4.0);
            }
            d[i] = v;
        }
    }
    for y in (0..rows).rev() {
        for x in (0..cols).rev() {
            let i = y * cols + x;
            let mut v = d[i];
            if x + 1 < cols {
                v = v.min(d[i + 1] + 3.0);
            }
            if y + 1 < rows {
                v = v.min(d[i + cols] + 3.0);
            }
            if x + 1 < cols && y + 1 < rows {
                v = v.min(d[i + cols + 1] + 4.0);
            }
            if x > 0 && y + 1 < rows {
                v = v.min(d[i + cols - 1] + 4.0);
            }
            d[i] = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_grid_negative_inside_positive_out() {
        let g = ClashGrid::from_stamps(&[InkStamp::Circle {
            cx: 40.0,
            cy: 40.0,
            r: 10.0,
        }])
        .unwrap();
        assert!(
            g.signed_at(40.0, 40.0) < 0.0,
            "center must be inside: {}",
            g.signed_at(40.0, 40.0)
        );
        let d = g.signed_at(70.0, 40.0);
        assert!(
            d > 15.0 && d < 25.0,
            "30px outside a r=10 disk should be ~20px: {d}"
        );
    }

    #[test]
    fn aabb_grid_clear_of_distant_point() {
        let g = ClashGrid::from_stamps(&[InkStamp::Aabb(Aabb::new(0.0, 0.0, 20.0, 10.0))]).unwrap();
        let d = g.signed_at(40.0, 5.0);
        assert!(d > 15.0, "20px to the right of a 20-wide box: {d}");
        assert!(g.signed_at(10.0, 5.0) < 0.0);
    }
}
