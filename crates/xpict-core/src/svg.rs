//! Scene → SVG. Hosts call this; they do not reimplement markup.

use crate::metrics::HALO_OPACITY;
use crate::scene::{LayerName, Primitive, Scene, TextAnchor, Viewport};
use std::collections::BTreeSet;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn fmt(n: f64) -> String {
    if !n.is_finite() {
        return "0".into();
    }
    let t = (n * 100.0).round() / 100.0;
    let s = format!("{t}");
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

fn fmt_path_d(d: &str) -> String {
    let mut out = String::with_capacity(d.len());
    let bytes = d.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        let start_num = c == '-' || c == '+' || c == '.' || c.is_ascii_digit();
        if start_num {
            let start = i;
            i += 1;
            while i < bytes.len() {
                let ch = bytes[i] as char;
                if ch.is_ascii_digit() || ch == '.' {
                    i += 1;
                    continue;
                }
                if (ch == 'e' || ch == 'E') && i + 1 < bytes.len() {
                    let n = bytes[i + 1] as char;
                    if n == '+' || n == '-' || n.is_ascii_digit() {
                        i += 2;
                        while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                            i += 1;
                        }
                        break;
                    }
                }
                break;
            }
            let token = &d[start..i];
            if let Ok(v) = token.parse::<f64>() {
                out.push_str(&fmt(v));
            } else {
                out.push_str(token);
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn attr(name: &str, value: Option<&str>) -> String {
    match value {
        Some(v) if !v.is_empty() => format!(" {name}=\"{}\"", esc(v)),
        _ => String::new(),
    }
}

fn attr_num(name: &str, value: f64) -> String {
    format!(" {name}=\"{}\"", fmt(value))
}

fn render_primitive(p: &Primitive) -> String {
    match p {
        Primitive::Path {
            d,
            stroke,
            fill,
            stroke_width,
            opacity,
            stroke_dasharray,
            stroke_linecap,
            class,
            data_text,
        } => format!(
            "<path d=\"{}\"{}{}{}{}{}{}{}{}{}/>",
            fmt_path_d(d),
            attr("fill", Some(fill.as_deref().unwrap_or("none"))),
            attr("stroke", stroke.as_deref()),
            attr_num("stroke-width", *stroke_width),
            attr(
                "stroke-linecap",
                Some(stroke_linecap.as_deref().unwrap_or("round"))
            ),
            " stroke-linejoin=\"round\"",
            attr_num("opacity", *opacity),
            attr("stroke-dasharray", stroke_dasharray.as_deref()),
            attr("class", class.as_deref()),
            attr("data-text", data_text.as_deref()),
        ),
        Primitive::Circle {
            cx,
            cy,
            r,
            fill,
            stroke,
            stroke_width,
            opacity,
            class,
        } => format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\"{}{}{}{}{}/>",
            fmt(*cx),
            fmt(*cy),
            fmt(*r),
            attr("fill", Some(fill.as_deref().unwrap_or("none"))),
            attr("stroke", stroke.as_deref()),
            attr_num("stroke-width", *stroke_width),
            attr_num("opacity", *opacity),
            attr("class", class.as_deref()),
        ),
        Primitive::Text {
            x,
            y,
            text,
            fill,
            font_size,
            anchor,
            class,
            rotate,
        } => {
            let anchor_s = match anchor {
                TextAnchor::Start => "start",
                TextAnchor::Middle => "middle",
                TextAnchor::End => "end",
            };
            let rotated = rotate.filter(|r| r.abs() > 1e-6);
            // Upright text keeps an alphabetic baseline at `(x, y)`. Rotated
            // labels (edge chrome) use the glyph center so `rotate` does not
            // swing the box around a corner.
            let baseline = if rotated.is_some() {
                "central"
            } else {
                "alphabetic"
            };
            let rot = rotated
                .map(|r| format!(" transform=\"rotate({} {} {})\"", fmt(r), fmt(*x), fmt(*y)))
                .unwrap_or_default();
            format!(
                "<text x=\"{}\" y=\"{}\" fill=\"{}\" font-size=\"{}\" \
                 font-family=\"Liberation Sans, Arial, sans-serif\" text-anchor=\"{anchor_s}\" \
                 dominant-baseline=\"{baseline}\"{}{}>{}</text>",
                fmt(*x),
                fmt(*y),
                esc(fill),
                fmt(*font_size),
                attr("class", class.as_deref()),
                rot,
                esc(text),
            )
        }
    }
}

fn layer_name(n: LayerName) -> &'static str {
    match n {
        LayerName::Shading => "shading",
        LayerName::Halo => "halo",
        LayerName::Bonds => "bonds",
        LayerName::Labels => "labels",
        LayerName::Marks => "marks",
        LayerName::Overlay => "overlay",
    }
}

/// Chem ink layers that ride the contrast filter (bonds / labels / marks).
fn is_chem_filter_layer(n: LayerName) -> bool {
    matches!(n, LayerName::Bonds | LayerName::Labels | LayerName::Marks)
}

/// Quantize filter radius so mols with nearly the same weight share one def.
fn halo_filter_id(radius_px: f64) -> String {
    let q = (radius_px * 10.0).round() as i64;
    format!("xpict-halo-{q}")
}

fn render_halo_filter_def(radius_px: f64) -> String {
    let id = halo_filter_id(radius_px);
    let r = fmt(radius_px);
    let op = fmt(HALO_OPACITY);
    // Pad the filter region so dilation is not clipped at the chem bbox.
    format!(
        "<filter id=\"{id}\" filterUnits=\"userSpaceOnUse\" \
         x=\"-50%\" y=\"-50%\" width=\"200%\" height=\"200%\">\
         <feMorphology in=\"SourceAlpha\" operator=\"dilate\" radius=\"{r}\" result=\"spread\"/>\
         <feFlood flood-color=\"#fff\" flood-opacity=\"{op}\" result=\"white\"/>\
         <feComposite in=\"white\" in2=\"spread\" operator=\"in\" result=\"halo\"/>\
         <feMerge>\
         <feMergeNode in=\"halo\"/>\
         <feMergeNode in=\"SourceGraphic\"/>\
         </feMerge>\
         </filter>"
    )
}

fn render_layer_group(layer: &crate::scene::Layer) -> String {
    let name = layer_name(layer.name);
    let body: String = layer.primitives.iter().map(render_primitive).collect();
    // Halos are knockouts, not ink: clicks pass through to the chem below.
    let events = if layer.name == LayerName::Halo {
        " pointer-events=\"none\""
    } else {
        ""
    };
    format!("<g class=\"xpict-layer xpict-{name}\"{events}>{body}</g>")
}

fn render_viewport_layers(vp: &Viewport) -> String {
    let mut layers: Vec<&crate::scene::Layer> = vp
        .layers
        .iter()
        .filter(|l| !l.primitives.is_empty())
        .collect();
    layers.sort_by_key(|l| l.name.paint_order());

    let filter_url = vp.halo_filter_px.filter(|r| *r > 0.0 && r.is_finite()).map(|r| {
        format!(" filter=\"url(#{})\"", halo_filter_id(r))
    });

    let mut chunks = String::new();
    let mut chem = String::new();
    let mut chem_open = false;
    for layer in layers {
        let into_chem = filter_url.is_some() && is_chem_filter_layer(layer.name);
        if into_chem {
            if !chem_open {
                chem.push_str(&format!(
                    "<g class=\"xpict-chem\"{}>",
                    filter_url.as_deref().unwrap_or("")
                ));
                chem_open = true;
            }
            chem.push_str(&render_layer_group(layer));
        } else {
            if chem_open {
                chem.push_str("</g>");
                chunks.push_str(&chem);
                chem.clear();
                chem_open = false;
            }
            // Legacy Halo path layers (if any) still emit under xpict-halo.
            chunks.push_str(&render_layer_group(layer));
        }
    }
    if chem_open {
        chem.push_str("</g>");
        chunks.push_str(&chem);
    }
    chunks
}

fn mol_transform(vp: &Viewport) -> String {
    let t = format!("translate({},{})", fmt(vp.x), fmt(vp.y));
    if vp.ox.abs() < 1e-12 && vp.oy.abs() < 1e-12 {
        t
    } else {
        format!("{t} translate({},{})", fmt(vp.ox), fmt(vp.oy))
    }
}

/// One molecule group: `g.xpict-mol[data-id]` with nested layer groups.
fn render_viewport(vp: &Viewport) -> String {
    let body = render_viewport_layers(vp);
    if body.is_empty() {
        return String::new();
    }
    let mut g = String::from("<g class=\"xpict-mol\"");
    if let Some(id) = vp.id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        g.push_str(&format!(" data-id=\"{}\"", esc(id)));
    }
    g.push_str(&format!(" transform=\"{}\">{body}</g>", mol_transform(vp)));
    g
}

/// Serialize a [`Scene`] to inline SVG (no XML declaration).
///
/// Each viewport is one `g.xpict-mol`. Layers (shading, bonds, …) nest inside
/// that group; chem ink may ride a shared feMorphology contrast filter.
pub fn scene_to_svg(scene: &Scene) -> String {
    let w = fmt(scene.width);
    let h = fmt(scene.height);
    let mut radii = BTreeSet::new();
    for vp in &scene.viewports {
        if let Some(r) = vp.halo_filter_px {
            if r > 0.0 && r.is_finite() {
                // Store quantized key so defs match filter ids.
                let q = (r * 10.0).round() as i64;
                radii.insert(q);
            }
        }
    }
    let mut parts = String::new();
    if !radii.is_empty() {
        parts.push_str("<defs>");
        for q in radii {
            parts.push_str(&render_halo_filter_def(q as f64 / 10.0));
        }
        parts.push_str("</defs>");
    }
    if !scene.halo.is_empty() {
        let body: String = scene.halo.iter().map(render_primitive).collect();
        parts.push_str(&format!(
            "<g class=\"xpict-halo\" id=\"halo\" pointer-events=\"none\">{body}</g>"
        ));
    }
    for vp in &scene.viewports {
        parts.push_str(&render_viewport(vp));
    }
    if !scene.overlays.is_empty() {
        let body: String = scene.overlays.iter().map(render_primitive).collect();
        parts.push_str(&format!(
            "<g class=\"xpict-overlays\" id=\"overlays\">{body}</g>"
        ));
    }
    let vx = fmt(scene.x);
    let vy = fmt(scene.y);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"{vx} {vy} {w} {h}\" class=\"xpict\" \
         style=\"background:transparent;max-width:100%;max-height:100%\">{parts}</svg>"
    )
}

/// Scene JSON → SVG (Python / wasm hosts).
pub fn scene_to_svg_json(scene_json: &str) -> Result<String, String> {
    let scene: Scene = serde_json::from_str(scene_json).map_err(|e| format!("Scene JSON: {e}"))?;
    Ok(scene_to_svg(&scene))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Layer, LayerName, Primitive};

    fn circle() -> Primitive {
        Primitive::Circle {
            cx: 1.0,
            cy: 2.0,
            r: 3.0,
            fill: Some("#000".into()),
            stroke: None,
            stroke_width: 1.5,
            opacity: 1.0,
            class: None,
        }
    }

    fn scene_with_id(id: Option<&str>, x: f64, y: f64) -> Scene {
        Scene {
            width: 10.0,
            height: 20.0,
            x: 0.0,
            y: 0.0,
            viewports: vec![Viewport {
                id: id.map(str::to_string),
                x,
                y,
                ox: 0.0,
                oy: 0.0,
                width: 10.0,
                height: 20.0,
                layers: vec![Layer {
                    name: LayerName::Bonds,
                    primitives: vec![circle()],
                }],
                halo_filter_px: None,
            }],
            overlays: vec![],
            halo: vec![],
        }
    }

    #[test]
    fn chem_filter_wraps_bonds_outside_shading() {
        use crate::scene::Layer;
        let mut scene = scene_with_id(Some("m0"), 0.0, 0.0);
        scene.viewports[0].halo_filter_px = Some(1.5);
        scene.viewports[0].layers.insert(
            0,
            Layer {
                name: LayerName::Shading,
                primitives: vec![circle()],
            },
        );
        let svg = scene_to_svg(&scene);
        assert!(svg.contains(r#"id="xpict-halo-15""#), "{svg}");
        assert!(svg.contains("feMorphology"), "{svg}");
        assert!(svg.contains(r#"class="xpict-chem" filter="url(#xpict-halo-15)""#), "{svg}");
        let shade_i = svg.find(r#"class="xpict-layer xpict-shading""#).unwrap();
        let chem_i = svg.find(r#"class="xpict-chem""#).unwrap();
        let bonds_i = svg.find(r#"class="xpict-layer xpict-bonds""#).unwrap();
        assert!(shade_i < chem_i && chem_i < bonds_i, "shading then filtered chem");
    }

    #[test]
    fn mol_group_carries_escaped_data_id_and_transform() {
        let svg = scene_to_svg(&scene_with_id(Some("m\"3&"), 4.5, 0.0));
        assert!(
            svg.contains(
                r#"<g class="xpict-mol" data-id="m&quot;3&amp;" transform="translate(4.5,0)""#
            ),
            "{svg}"
        );
        assert!(svg.contains(r#"class="xpict-layer xpict-bonds""#), "{svg}");
        assert!(!svg.contains("<?xml"), "{svg}");
        assert!(svg.starts_with("<svg"), "{svg}");
    }

    #[test]
    fn missing_id_still_emits_mol_group() {
        let svg = scene_to_svg(&scene_with_id(None, 0.0, 0.0));
        assert!(
            svg.contains(r#"<g class="xpict-mol" transform="translate(0,0)""#),
            "{svg}"
        );
        assert!(!svg.contains("data-id="), "{svg}");
    }

    #[test]
    fn json_roundtrip_keeps_id() {
        let scene = scene_with_id(Some("A"), 1.0, 2.0);
        let json = serde_json::to_string(&scene).unwrap();
        let svg = scene_to_svg_json(&json).unwrap();
        assert!(svg.contains(r#"data-id="A""#), "{svg}");
        assert!(svg.contains(r#"transform="translate(1,2)""#), "{svg}");
    }

    #[test]
    fn shading_nests_inside_one_mol_group() {
        let mut scene = scene_with_id(Some("m3"), 10.0, 20.0);
        scene.viewports[0].ox = -6.0;
        scene.viewports[0].oy = -8.0;
        scene.viewports[0].layers.insert(
            0,
            Layer {
                name: LayerName::Shading,
                primitives: vec![circle()],
            },
        );
        let svg = scene_to_svg(&scene);
        assert_eq!(
            svg.matches(r#"class="xpict-mol""#).count(),
            1,
            "one mol group, not a sibling shade copy: {svg}"
        );
        let mol = svg.find(r#"class="xpict-mol""#).expect("mol");
        let shade = svg
            .find(r#"class="xpict-layer xpict-shading""#)
            .expect("shading");
        let bonds = svg
            .find(r#"class="xpict-layer xpict-bonds""#)
            .expect("bonds");
        assert!(
            mol < shade && shade < bonds,
            "layers nest inside mol: {svg}"
        );
        assert!(
            svg.contains(r#"transform="translate(10,20) translate(-6,-8)""#),
            "leftover origin is a nested translate, not baked coords: {svg}"
        );
    }

    #[test]
    fn rotated_text_spins_about_glyph_center() {
        let scene = Scene {
            width: 40.0,
            height: 20.0,
            x: 0.0,
            y: 0.0,
            viewports: vec![],
            overlays: vec![Primitive::Text {
                x: 12.0,
                y: 8.0,
                text: "ADH".into(),
                fill: "#222".into(),
                font_size: 11.0,
                anchor: TextAnchor::Middle,
                class: Some("edge edge-0 label".into()),
                rotate: Some(35.0),
            }],
            halo: vec![],
        };
        let svg = scene_to_svg(&scene);
        assert!(svg.contains(r#"dominant-baseline="central""#), "{svg}");
        assert!(
            svg.contains(r#"transform="rotate(35 12 8)""#),
            "rotate origin must be the text center: {svg}"
        );
        let upright = Scene {
            overlays: vec![Primitive::Text {
                x: 12.0,
                y: 8.0,
                text: "ADH".into(),
                fill: "#222".into(),
                font_size: 11.0,
                anchor: TextAnchor::Middle,
                class: None,
                rotate: None,
            }],
            ..scene
        };
        let u = scene_to_svg(&upright);
        assert!(u.contains(r#"dominant-baseline="alphabetic""#), "{u}");
        assert!(!u.contains("rotate("), "{u}");
    }

    #[test]
    fn halos_let_clicks_through() {
        let mut scene = scene_with_id(Some("m"), 0.0, 0.0);
        scene.halo = vec![circle()];
        scene.viewports[0].layers.push(Layer {
            name: LayerName::Halo,
            primitives: vec![circle()],
        });
        let svg = scene_to_svg(&scene);
        assert!(
            svg.contains(r#"<g class="xpict-halo" id="halo" pointer-events="none">"#),
            "{svg}"
        );
        assert!(
            svg.contains(r#"<g class="xpict-layer xpict-halo" pointer-events="none">"#),
            "{svg}"
        );
        assert!(
            svg.contains(r#"<g class="xpict-layer xpict-bonds">"#),
            "{svg}"
        );
    }

    #[test]
    fn viewbox_uses_scene_origin() {
        let mut scene = scene_with_id(Some("m"), 0.0, 0.0);
        scene.x = 12.4;
        scene.y = -3.1;
        let svg = scene_to_svg(&scene);
        assert!(
            svg.contains(r#"viewBox="12.4 -3.1 10 20""#),
            "crop is the viewBox, not a leftover translate: {svg}"
        );
        assert!(!svg.contains(r#"viewBox="0 0"#), "{svg}");
    }
}
