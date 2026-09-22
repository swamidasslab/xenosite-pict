//! Bundled Liberation Sans outlines (fontTools stand-in).
//!
//! **Stub.** Python still uses fontTools + shapely (`draw/font_face.py`,
//! `draw/glyphs.py`). Candidates: `ttf-parser` / `skrifa` for contours and
//! advances; keep the same Liberation Sans files under `src/xpict/data/fonts/`.

#![allow(dead_code)]

/// Placeholder so the module path exists for incremental moves.
#[derive(Debug, Clone, Copy, Default)]
pub struct FontCore;
