//! Rust-only depiction engine knobs (not DepictSpec / not `make types`).
//!
//! Author-facing intent stays on DepictSpec (`LayoutOpts`, cascade opts,
//! annotation fields). This module holds solver / routing / ELK policy
//! coefficients so tests and `LayoutOpts.quality` presets can tune them
//! without locking heuristics into the public schema.
//!
//! See `docs/dev/engine-constants.md`.

use crate::metrics::BOND_PX;

/// Full internal engine bundle. Defaults match today's house behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    pub route: RouteConfig,
    pub solve: SolveConfig,
    pub arrow: ArrowConfig,
    pub scheme: SchemeConfig,
    pub nudge: NudgeConfig,
    pub elk: ElkPolicyConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self::pretty()
    }
}

impl EngineConfig {
    /// Publication / default quality (current house constants).
    pub fn pretty() -> Self {
        Self {
            route: RouteConfig::pretty(),
            solve: SolveConfig::pretty(),
            arrow: ArrowConfig::default(),
            scheme: SchemeConfig::pretty(),
            nudge: NudgeConfig::default(),
            elk: ElkPolicyConfig::default(),
        }
    }

    /// Faster dense-scheme path: tighter search budgets, same paint geometry.
    pub fn fast() -> Self {
        Self {
            route: RouteConfig::fast(),
            solve: SolveConfig::fast(),
            arrow: ArrowConfig::default(),
            scheme: SchemeConfig::fast(),
            nudge: NudgeConfig::fast(),
            elk: ElkPolicyConfig::default(),
        }
    }

}

/// Scheme shaft routing search / scoring.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteConfig {
    pub segment_penalty: f64,
    pub angle_penalty: f64,
    pub cosmetic_simplify_eps: f64,
    pub shared_path_penalty: f64,
    pub crossing_penalty: f64,
    pub box_corner_margin: f64,
    pub frame_margin: f64,
    pub reopt_passes: usize,
    pub reopt_max_steps: usize,
    /// Inflate full-ink hulls before visibility (shapeBufferDistance).
    pub routing_clearance: f64,
    pub terminal_link_near: usize,
    pub max_hull_verts: usize,
    pub inter_vis_near: usize,
    pub frame_hull_near: usize,
    pub nudge_px: f64,
    pub coterminal_lane_px: f64,
    pub coterminal_px: f64,
}

impl RouteConfig {
    pub fn pretty() -> Self {
        Self {
            segment_penalty: 80.0,
            angle_penalty: 120.0,
            cosmetic_simplify_eps: 0.35 * BOND_PX,
            shared_path_penalty: 400.0,
            crossing_penalty: 2_500.0,
            box_corner_margin: BOND_PX,
            frame_margin: BOND_PX * 4.0,
            reopt_passes: 1,
            reopt_max_steps: 12,
            routing_clearance: 0.5 * BOND_PX + 0.5 * 1.6,
            terminal_link_near: 40,
            max_hull_verts: 24,
            inter_vis_near: 10,
            frame_hull_near: 4,
            nudge_px: 8.0,
            coterminal_lane_px: 8.0,
            coterminal_px: 8.0,
        }
    }

    pub fn fast() -> Self {
        Self {
            reopt_passes: 0,
            reopt_max_steps: 0,
            terminal_link_near: 16,
            max_hull_verts: 12,
            inter_vis_near: 4,
            frame_hull_near: 2,
            ..Self::pretty()
        }
    }
}

/// Soft / hard constraint solver defaults (`SolveOpts` seed).
#[derive(Debug, Clone, PartialEq)]
pub struct SolveConfig {
    pub max_iters: usize,
    pub drift_window: usize,
    pub epsilon_px: f64,
    pub w_travel: f64,
    pub noise: f64,
    pub initial_simplex_scale: f64,
    pub al_outer_max: usize,
    pub al_rho0: f64,
    pub al_rho_grow: f64,
    pub al_rho_max: f64,
    pub al_tol: f64,
    /// Scheme edge-label refine override.
    pub scheme_label_max_iters: usize,
    pub scheme_label_al_outer_max: usize,
    pub scheme_label_epsilon_px: f64,
}

impl SolveConfig {
    pub fn pretty() -> Self {
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
            scheme_label_max_iters: 40,
            scheme_label_al_outer_max: 2,
            scheme_label_epsilon_px: 1.0,
        }
    }

    pub fn fast() -> Self {
        Self {
            max_iters: 80,
            al_outer_max: 3,
            scheme_label_max_iters: 20,
            scheme_label_al_outer_max: 1,
            ..Self::pretty()
        }
    }
}

/// Arrow shaft / head geometry (paint). Quality presets leave these alone.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrowConfig {
    pub kink_px: f64,
    pub close_ctrl_px: f64,
    pub turn_radius: f64,
    pub default_color: &'static str,
    pub default_width: f64,
    pub head: f64,
    pub dash: &'static str,
    pub eq_sep: f64,
    pub anchor_gap: f64,
}

impl Default for ArrowConfig {
    fn default() -> Self {
        Self {
            kink_px: 10.0,
            close_ctrl_px: 2.0,
            turn_radius: 36.0,
            default_color: "#222",
            default_width: 1.6,
            head: 9.0,
            dash: "6 4",
            eq_sep: 3.2,
            anchor_gap: 6.0,
        }
    }
}

/// Scheme layout / compose heuristics beyond ELK spacing.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemeConfig {
    pub ink_standoff: f64,
    pub edge_clearance: f64,
    pub coterminal_lane_px: f64,
    pub shaft_stroke_px: f64,
    pub label_font_px: f64,
    pub label_shaft_pad_px: f64,
    pub pose_strength: f64,
    pub coterminal_px: f64,
    pub network_solve_mol_cap: usize,
    pub network_solve_edge_cap: usize,
}

impl SchemeConfig {
    pub fn pretty() -> Self {
        Self {
            ink_standoff: 0.5 * BOND_PX,
            edge_clearance: BOND_PX,
            coterminal_lane_px: 8.0,
            shaft_stroke_px: 1.6,
            label_font_px: 11.0,
            label_shaft_pad_px: 4.0,
            pose_strength: 0.35,
            coterminal_px: 8.0,
            network_solve_mol_cap: 7,
            network_solve_edge_cap: 7,
        }
    }

    pub fn fast() -> Self {
        // Prefer the cheaper network path sooner on mid-size schemes.
        Self {
            network_solve_mol_cap: 4,
            network_solve_edge_cap: 4,
            ..Self::pretty()
        }
    }
}

/// Soft mol nudge after ELK.
#[derive(Debug, Clone, PartialEq)]
pub struct NudgeConfig {
    pub max_nudge: f64,
    pub steps: [f64; 4],
    pub mol_gap_extra: f64,
    pub max_passes: usize,
}

impl Default for NudgeConfig {
    fn default() -> Self {
        Self {
            max_nudge: 3.0 * BOND_PX,
            steps: [0.5 * BOND_PX, BOND_PX, 2.0 * BOND_PX, 3.0 * BOND_PX],
            mol_gap_extra: 2.0,
            max_passes: 8,
        }
    }
}

impl NudgeConfig {
    pub fn fast() -> Self {
        Self {
            max_nudge: BOND_PX,
            steps: [0.5 * BOND_PX, BOND_PX, BOND_PX, BOND_PX],
            max_passes: 2,
            ..Self::default()
        }
    }
}

/// Fixed ELK policy strings / spacings not exposed on [`crate::doc::LayoutOpts`].
#[derive(Debug, Clone, PartialEq)]
pub struct ElkPolicyConfig {
    pub edge_edge_spacing: f64,
    pub edge_node_spacing: f64,
    pub unnecessary_bendpoints: bool,
    pub crossing_strategy: &'static str,
    pub node_placement: &'static str,
    pub cycle_breaking: &'static str,
    pub port_constraints: &'static str,
}

impl Default for ElkPolicyConfig {
    fn default() -> Self {
        Self {
            edge_edge_spacing: 16.0,
            edge_node_spacing: 20.0,
            unnecessary_bendpoints: false,
            crossing_strategy: "LAYER_SWEEP",
            node_placement: "NETWORK_SIMPLEX",
            cycle_breaking: "DEPTH_FIRST",
            port_constraints: "FREE",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_matches_legacy_route_house_numbers() {
        let r = RouteConfig::pretty();
        assert_eq!(r.segment_penalty, 80.0);
        assert_eq!(r.crossing_penalty, 2_500.0);
        assert_eq!(r.reopt_passes, 1);
        assert_eq!(r.terminal_link_near, 40);
    }

    #[test]
    fn fast_tightens_search_only() {
        let f = EngineConfig::fast();
        let p = EngineConfig::pretty();
        assert!(f.route.terminal_link_near < p.route.terminal_link_near);
        assert_eq!(f.route.crossing_penalty, p.route.crossing_penalty);
        assert_eq!(f.arrow.turn_radius, p.arrow.turn_radius);
        assert_eq!(f.scheme.ink_standoff, p.scheme.ink_standoff);
    }

    #[test]
    fn arrow_pub_consts_match_arrow_config() {
        let a = ArrowConfig::default();
        assert_eq!(a.kink_px, crate::arrows::KINK_PX);
        assert_eq!(a.close_ctrl_px, crate::arrows::CLOSE_CTRL_PX);
        assert_eq!(a.turn_radius, crate::arrows::TURN_RADIUS);
        assert_eq!(a.anchor_gap, crate::arrows::ANCHOR_GAP);
    }

    #[test]
    fn scheme_standoff_matches_house_bond_fraction() {
        assert!((SchemeConfig::pretty().ink_standoff - 0.5 * BOND_PX).abs() < 1e-12);
    }
}
