//! Scene → SVG. Hosts call this; they do not reimplement markup.

use crate::scene::{LayerName, Primitive, Scene, TextAnchor, Viewport};

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
            let rot = rotate
                .filter(|r| r.abs() > 1e-6)
                .map(|r| format!(" transform=\"rotate({} {} {})\"", fmt(r), fmt(*x), fmt(*y)))
                .unwrap_or_default();
            format!(
                "<text x=\"{}\" y=\"{}\" fill=\"{}\" font-size=\"{}\" \
                 font-family=\"Liberation Sans, Arial, sans-serif\" text-anchor=\"{anchor_s}\" \
                 dominant-baseline=\"alphabetic\"{}{}>{}</text>",
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

fn render_viewport_layers(vp: &Viewport) -> String {
    let mut layers: Vec<&crate::scene::Layer> = vp
        .layers
        .iter()
        .filter(|l| !l.primitives.is_empty())
        .collect();
    layers.sort_by_key(|l| l.name.paint_order());
    let mut chunks = String::new();
    for layer in layers {
        let name = layer_name(layer.name);
        let body: String = layer.primitives.iter().map(render_primitive).collect();
        chunks.push_str(&format!("<g class=\"xpict-layer xpict-{name}\">{body}</g>"));
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
/// Each viewport is one `g.xpict-mol`. Layers (shading, halo, bonds, …) nest
/// inside that group so a mol transform applies to regions and chem together.
pub fn scene_to_svg(scene: &Scene) -> String {
    let w = fmt(scene.width);
    let h = fmt(scene.height);
    let mut parts = String::new();
    if !scene.halo.is_empty() {
        let body: String = scene.halo.iter().map(render_primitive).collect();
        parts.push_str(&format!("<g class=\"xpict-halo\" id=\"halo\">{body}</g>"));
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
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\" class=\"xpict\" \
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
            }],
            overlays: vec![],
            halo: vec![],
        }
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
}
