//! Rigid 2D alignment (Kabsch) — Indigo / no-RDKit fallback.
//!
//! **Template depiction** (fix matched atoms and redraw the rest) stays at
//! the language edge via RDKit. This module only rotates/translates point
//! sets so a caller-supplied correspondence lands on a **template** frame.
//!
//! Keep in sync with `src/xpict/align.py` (`_kabsch_2d`, `_apply_transform`).

/// Rigid transform: `x' = cos·x ∓ sin·y + tx`, `y' = sin·x ± cos·y + ty`
/// (sign of the sin terms flips when [`RigidTransform::det`] is negative).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RigidTransform {
    pub cos: f64,
    pub sin: f64,
    pub tx: f64,
    pub ty: f64,
    /// `+1` proper rotation, `-1` improper (reflection).
    pub det: f64,
}

impl Default for RigidTransform {
    fn default() -> Self {
        Self {
            cos: 1.0,
            sin: 0.0,
            tx: 0.0,
            ty: 0.0,
            det: 1.0,
        }
    }
}

impl RigidTransform {
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        if self.det >= 0.0 {
            (
                self.cos * x - self.sin * y + self.tx,
                self.sin * x + self.cos * y + self.ty,
            )
        } else {
            (
                self.cos * x + self.sin * y + self.tx,
                self.sin * x - self.cos * y + self.ty,
            )
        }
    }

    pub fn apply_all(&self, pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
        pts.iter().map(|&(x, y)| self.apply(x, y)).collect()
    }
}

/// Kabsch / Procrustes 2D: map `src` → `dst` (paired points).
///
/// Returns `(cos, sin, tx, ty, det_sign)`. Empty input → identity.
pub fn kabsch_2d(src: &[(f64, f64)], dst: &[(f64, f64)], allow_reflect: bool) -> RigidTransform {
    assert_eq!(src.len(), dst.len(), "src/dst length mismatch");
    let n = src.len();
    if n == 0 {
        return RigidTransform::default();
    }
    let sx: f64 = src.iter().map(|p| p.0).sum::<f64>() / n as f64;
    let sy: f64 = src.iter().map(|p| p.1).sum::<f64>() / n as f64;
    let dx: f64 = dst.iter().map(|p| p.0).sum::<f64>() / n as f64;
    let dy: f64 = dst.iter().map(|p| p.1).sum::<f64>() / n as f64;
    let mut sxx = 0.0;
    let mut syy = 0.0;
    let mut sxy = 0.0;
    let mut syx = 0.0;
    for (&(x, y), &(u, v)) in src.iter().zip(dst.iter()) {
        let x0 = x - sx;
        let y0 = y - sy;
        let u0 = u - dx;
        let v0 = v - dy;
        sxx += x0 * u0;
        sxy += x0 * v0;
        syx += y0 * u0;
        syy += y0 * v0;
    }

    let rot_score = |c: f64, s: f64| c * (sxx + syy) + s * (sxy - syx);

    let ang = (sxy - syx).atan2(sxx + syy);
    let c1 = ang.cos();
    let s1 = ang.sin();
    let mut best_c = c1;
    let mut best_s = s1;
    let mut best_det = 1.0;
    let mut best_sc = rot_score(c1, s1);
    if allow_reflect {
        let ang2 = (sxy + syx).atan2(sxx - syy);
        let c2 = ang2.cos();
        let s2 = ang2.sin();
        let sc2 = c2 * (sxx - syy) + s2 * (sxy + syx);
        if sc2 > best_sc {
            best_c = c2;
            best_s = s2;
            best_det = -1.0;
            best_sc = sc2;
        }
    }
    let _ = best_sc;

    let (tx, ty) = if best_det >= 0.0 {
        (
            dx - (best_c * sx - best_s * sy),
            dy - (best_s * sx + best_c * sy),
        )
    } else {
        (
            dx - (best_c * sx + best_s * sy),
            dy - (best_s * sx - best_c * sy),
        )
    };
    RigidTransform {
        cos: best_c,
        sin: best_s,
        tx,
        ty,
        det: best_det,
    }
}

/// Align `other` atom coords onto a **template** using `mapping` (other→template).
///
/// `template` / `other` are `(index, x, y)`. Returns transformed other coords
/// as `(index, x, y)` in the same order as `other`. Mapping pairs missing from
/// either set are skipped; fewer than one pair → identity.
pub fn rigid_align_coords(
    template: &[(i32, f64, f64)],
    other: &[(i32, f64, f64)],
    mapping: &[(i32, i32)], // (other_index, template_index)
) -> (Vec<(i32, f64, f64)>, RigidTransform) {
    let tmpl: std::collections::HashMap<i32, (f64, f64)> =
        template.iter().map(|&(i, x, y)| (i, (x, y))).collect();
    let oth: std::collections::HashMap<i32, (f64, f64)> =
        other.iter().map(|&(i, x, y)| (i, (x, y))).collect();
    let mut src = Vec::new();
    let mut dst = Vec::new();
    for &(oi, ti) in mapping {
        let Some(&s) = oth.get(&oi) else {
            continue;
        };
        let Some(&d) = tmpl.get(&ti) else {
            continue;
        };
        src.push(s);
        dst.push(d);
    }
    if src.is_empty() {
        return (other.to_vec(), RigidTransform::default());
    }
    let xf = kabsch_2d(&src, &dst, true);
    let out = other
        .iter()
        .map(|&(i, x, y)| {
            let (nx, ny) = xf.apply(x, y);
            (i, nx, ny)
        })
        .collect();
    (out, xf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_on_same_points() {
        let pts = vec![(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)];
        let xf = kabsch_2d(&pts, &pts, true);
        assert!((xf.cos - 1.0).abs() < 1e-9);
        assert!(xf.sin.abs() < 1e-9);
        assert!(xf.tx.abs() < 1e-9 && xf.ty.abs() < 1e-9);
    }

    #[test]
    fn rotates_and_translates() {
        // 90° CCW + translate (2, 3): (1,0)→(0,1)+offset, etc.
        let src = vec![(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)];
        let dst: Vec<(f64, f64)> = src
            .iter()
            .map(|&(x, y)| (-y + 2.0, x + 3.0))
            .collect();
        let xf = kabsch_2d(&src, &dst, false);
        for (&(x, y), &(u, v)) in src.iter().zip(dst.iter()) {
            let (nx, ny) = xf.apply(x, y);
            assert!((nx - u).abs() < 1e-9, "{nx} vs {u}");
            assert!((ny - v).abs() < 1e-9, "{ny} vs {v}");
        }
    }

    #[test]
    fn flip_allowed_when_reflected() {
        let src = vec![(0.0, 0.0), (2.0, 0.0), (0.0, 1.0)];
        let dst = vec![(0.0, 0.0), (2.0, 0.0), (0.0, -1.0)]; // reflect Y
        let xf = kabsch_2d(&src, &dst, true);
        assert!(xf.det < 0.0);
        for (&(x, y), &(u, v)) in src.iter().zip(dst.iter()) {
            let (nx, ny) = xf.apply(x, y);
            assert!((nx - u).abs() < 1e-9);
            assert!((ny - v).abs() < 1e-9);
        }
    }

    #[test]
    fn rigid_align_coords_onto_template() {
        // Template: horizontal. Other: same shape, rotated 180° + shift.
        let template = vec![(0, 0.0, 0.0), (1, 20.0, 0.0), (2, 20.0, 10.0)];
        let other = vec![(0, 5.0, 5.0), (1, -15.0, 5.0), (2, -15.0, -5.0)];
        // Map all atoms 1:1 by index.
        let mapping = vec![(0, 0), (1, 1), (2, 2)];
        let (aligned, _xf) = rigid_align_coords(&template, &other, &mapping);
        for ((_, x, y), (_, u, v)) in aligned.iter().zip(template.iter()) {
            assert!((x - u).abs() < 1e-6, "{x},{y} vs {u},{v}");
            assert!((y - v).abs() < 1e-6);
        }
    }

    #[test]
    fn default_transform_is_identity() {
        let xf = RigidTransform::default();
        assert!((xf.cos - 1.0).abs() < 1e-12);
        assert!(xf.sin.abs() < 1e-12);
        assert!(xf.tx.abs() < 1e-12 && xf.ty.abs() < 1e-12);
        assert!((xf.det - 1.0).abs() < 1e-12);
        let pts = [(1.0, 2.0), (3.0, -4.0)];
        assert_eq!(xf.apply_all(&pts), pts.to_vec());
    }

    #[test]
    fn kabsch_empty_is_identity() {
        let xf = kabsch_2d(&[], &[], true);
        assert!((xf.cos - 1.0).abs() < 1e-12);
        assert!(xf.det > 0.0);
    }

    #[test]
    fn rigid_align_skips_missing_pairs_and_empty_mapping() {
        let template = vec![(0, 0.0, 0.0), (1, 10.0, 0.0)];
        let other = vec![(0, 1.0, 1.0), (1, 11.0, 1.0)];
        // Missing other / template indices are skipped → no pairs → identity.
        let (out, xf) = rigid_align_coords(&template, &other, &[(9, 0), (0, 9)]);
        assert_eq!(out, other);
        assert!((xf.cos - 1.0).abs() < 1e-12);
        let (out2, _) = rigid_align_coords(&template, &other, &[]);
        assert_eq!(out2, other);
    }
}
