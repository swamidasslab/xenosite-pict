//! Scene → PDF for journals that want vector PDF/EPS rather than SVG.
//!
//! Labels are already glyph outlines, so no font database is needed.

use crate::scene::Scene;
use crate::svg::scene_to_svg;

/// Render a scene to a single-page vector PDF (page = viewBox, in pt).
pub fn scene_to_pdf(scene: &Scene) -> Result<Vec<u8>, String> {
    svg_to_pdf(&scene_to_svg(scene))
}

/// Convert an xpict SVG string to a single-page vector PDF.
pub fn svg_to_pdf(svg: &str) -> Result<Vec<u8>, String> {
    let opts = svg2pdf::usvg::Options::default();
    let tree = svg2pdf::usvg::Tree::from_str(svg, &opts).map_err(|e| e.to_string())?;
    svg2pdf::to_pdf(
        &tree,
        svg2pdf::ConversionOptions::default(),
        svg2pdf::PageOptions::default(),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_becomes_one_page_pdf() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20" width="40" height="20"><path d="M 2 2 L 38 18" stroke="#111" stroke-width="1.5" fill="none"/></svg>"##;
        let pdf = svg_to_pdf(svg).expect("pdf");
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(pdf.windows(9).any(|w| w == b"/MediaBox"));
    }
}
