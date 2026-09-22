//! Concentric plot-dot shading (xenopict `PlotDot` semantics).
//!
//! Keep in sync with `python/xpict/draw/plotdot.py`.

/// One disk before paint: radius fraction in `[0, 1]`, color stop in `[-1, 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadeDisk {
    pub radius_frac: f64,
    pub color_z: f64,
    pub x: f64,
    pub y: f64,
}

/// Map `z ∈ [-1, 1]` to concentric `(radius_frac, color_z)` rings.
///
/// Rings sort weak → strong so intense colors paint on top — same order as
/// xenopict / Python `PlotDot`.
#[derive(Debug, Clone)]
pub struct PlotDot {
    levels: usize,
    stops: Vec<f64>,
}

impl Default for PlotDot {
    fn default() -> Self {
        Self::new(4)
    }
}

impl PlotDot {
    pub fn new(levels: usize) -> Self {
        let levels = levels.max(1);
        let stops = (0..levels).map(|i| (i + 1) as f64 / levels as f64).collect();
        Self { levels, stops }
    }

    fn radius(&self, z: f64, level: usize) -> f64 {
        let az = z.abs();
        if level == 0 {
            return self.stops[0].sqrt();
        }
        let offset = 1.0 - self.stops[level];
        let r = az - offset;
        if r < self.stops[0] {
            0.0
        } else {
            r.sqrt()
        }
    }

    fn color(&self, z: f64, level: usize) -> f64 {
        let sign = if z < 0.0 { -1.0 } else { 1.0 };
        if level == 0 {
            z
        } else {
            sign * self.stops[self.stops.len() - level - 1]
        }
    }

    /// Rings for one score; empty when `|z| < 0.05`.
    pub fn rings(&self, z: f64) -> Vec<(f64, f64)> {
        if z.abs() < 0.05 {
            return Vec::new();
        }
        (0..self.levels)
            .map(|lvl| (self.radius(z, lvl), self.color(z, lvl)))
            .filter(|(r, _)| *r > 0.0)
            .collect()
    }

    /// All disks for a molecule, sorted weak/small first.
    pub fn disks(&self, zs: &[f64], coords: &[(f64, f64)]) -> Vec<ShadeDisk> {
        let mut dots = Vec::new();
        for (z, &(x, y)) in zs.iter().zip(coords.iter()) {
            for (radius_frac, color_z) in self.rings(*z) {
                dots.push(ShadeDisk {
                    radius_frac,
                    color_z,
                    x,
                    y,
                });
            }
        }
        dots.sort_by(|a, b| {
            a.color_z
                .abs()
                .partial_cmp(&b.color_z.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    a.radius_frac
                        .partial_cmp(&b.radius_frac)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        dots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_near_zero_scores() {
        let dots = PlotDot::default().disks(
            &[0.0, 0.01, 0.9],
            &[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)],
        );
        assert!(dots.iter().all(|d| d.color_z.abs() >= 0.05));
        assert!(dots.iter().any(|d| d.color_z.abs() > 0.5));
    }

    #[test]
    fn full_score_has_four_rings() {
        let rings = PlotDot::default().rings(1.0);
        assert_eq!(rings.len(), 4);
        assert!((rings[0].0 - 0.5).abs() < 1e-9);
        assert!((rings[0].1 - 1.0).abs() < 1e-9);
    }

    /// Parity with Python ``test_plotdot_rings_match_python`` sample scores.
    #[test]
    fn plotdot_rings_sample_scores() {
        let pd = PlotDot::default();
        for z in [0.25, 0.5, 0.9, 1.0] {
            let rings = pd.rings(z);
            assert!(!rings.is_empty() || z < 0.05);
            for &(r, c) in &rings {
                assert!(r > 0.0 && r <= 1.0);
                assert!(c.abs() <= 1.0 + 1e-9);
            }
        }
    }
}
