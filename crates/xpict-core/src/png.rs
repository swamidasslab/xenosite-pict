//! Scene → PNG raster (resvg), for places SVG does not display.

use crate::scene::Scene;
use crate::svg::scene_to_svg;

/// Render a scene to PNG at `scale` × the viewBox size (1.0 = 1 px per unit).
pub fn scene_to_png(scene: &Scene, scale: f32) -> Result<Vec<u8>, String> {
    svg_to_png(&scene_to_svg(scene), scale)
}

/// Rasterize an xpict SVG string to PNG at `scale` × its size.
pub fn svg_to_png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    rasterize(svg, scale)?.encode_png().map_err(|e| e.to_string())
}

/// Uncompressed straight-alpha RGBA8 rows (no PNG encode) plus `(width, height)`.
pub fn svg_to_rgba(svg: &str, scale: f32) -> Result<(Vec<u8>, u32, u32), String> {
    let pixmap = rasterize(svg, scale)?;
    let (w, h) = (pixmap.width(), pixmap.height());
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Ok((rgba, w, h))
}

fn rasterize(svg: &str, scale: f32) -> Result<resvg::tiny_skia::Pixmap, String> {
    if !(scale.is_finite() && scale > 0.0) {
        return Err(format!("png scale must be > 0, got {scale}"));
    }
    let tree = crate::usvg_tree::parse(svg)?;
    let size = tree.size().to_int_size().scale_by(scale).ok_or("png: empty size")?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or("png: empty size")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_becomes_scaled_png_with_text_ink() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 20" width="80" height="20"><text x="2" y="15" font-size="12" font-family="Liberation Sans, Arial, sans-serif">Caption</text></svg>"##;
        let png = svg_to_png(svg, 2.0).expect("png");
        assert!(png.starts_with(b"\x89PNG"));
        let pix = resvg::tiny_skia::Pixmap::decode_png(&png).expect("decode");
        assert_eq!((pix.width(), pix.height()), (160, 40));
        assert!(pix.pixels().iter().any(|p| p.alpha() > 0), "text drew no ink");
        assert!(svg_to_png(svg, 0.0).is_err());
        let (rgba, w, h) = svg_to_rgba(svg, 2.0).expect("rgba");
        assert_eq!((w, h, rgba.len()), (160, 40, 160 * 40 * 4));
        assert!(rgba.chunks(4).any(|p| p[3] == 255), "rgba lost the opaque ink");
    }
}
