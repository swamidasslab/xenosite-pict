//! Geometry for halos and glyph counters (Shapely stand-in).
//!
//! **Stub.** Python still uses Shapely (`draw/halo.py`, `draw/glyphs.py`).
//! Candidates when this moves: `geo` + `i_overlay` / `geo-clipper`, or a thin
//! Clipper binding. Need: buffer, unary union, polygon holes, even-odd XOR
//! for TrueType contours.

#![allow(dead_code)]

/// Placeholder so the module path exists for incremental moves.
#[derive(Debug, Clone, Copy, Default)]
pub struct GeomCore;
