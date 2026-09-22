//! Shared depiction core for **xpict**.
//!
//! Python under `src/xpict/` is still the working engine. This crate holds
//! algorithms we are ready to share with PyO3 and WASM — start small, move
//! modules as they stabilize. See the crate `README.md` for the migration
//! rule.

#![forbid(unsafe_code)]

pub mod bonds;
#[cfg(feature = "elk")]
pub mod elk;
pub mod font;
pub mod geom;
pub mod metrics;
pub mod plotdot;
pub mod rings;

pub use bonds::{centered_displacements, multi_bond_offset};
#[cfg(feature = "elk")]
pub use elk::layout_json as elk_layout_json;
pub use geom::{capsule_halo_path_d, disk_halo_path_d, polygon_to_svg_d};
pub use metrics::{BOND_PX, OFFSET_FRAC, OFFSET_PX, SHADE_FRAC, STROKE_PX};
pub use plotdot::{PlotDot, ShadeDisk};
