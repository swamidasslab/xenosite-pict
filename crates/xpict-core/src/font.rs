//! Bundled Liberation Sans outlines (fontTools stand-in).
//!
//! **Stub — priority port.** Python still uses fontTools (`draw/font_face.py`,
//! `draw/glyphs.py`). Target stack:
//! - `ttf-parser` or `skrifa` for glyph contours + advances
//! - same Liberation Sans TTFs as `src/xpict/data/fonts/`
//! - emit SVG path `d` (and ink polygons for halo) shared by PyO3 + WASM
//!
//! Until this lands, JS cannot match Python atom labels without a second font stack.

#![allow(dead_code)]

/// Placeholder so the module path exists for incremental moves.
#[derive(Debug, Clone, Copy, Default)]
pub struct FontCore;
