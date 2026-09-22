//! Scene → SVG string (parity with JS ``sceneToSvg`` / Python ``scene_to_svg``).

use xpict_core::scene::{LayerName, Primitive, Scene, TextAnchor, Viewport};

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
        } => {
            let anchor_s = match anchor {
                TextAnchor::Start => "start",
                TextAnchor::Middle => "middle",
                TextAnchor::End => "end",
            };
            format!(
                "<text x=\"{}\" y=\"{}\" fill=\"{}\" font-size=\"{}\" \
                 font-family=\"Liberation Sans, Arial, sans-serif\" text-anchor=\"{anchor_s}\" \
                 dominant-baseline=\"alphabetic\"{}>{}</text>",
                fmt(*x),
                fmt(*y),
                esc(fill),
                fmt(*font_size),
                attr("class", class.as_deref()),
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

fn render_viewport_layers(vp: &Viewport, names: &[&str]) -> String {
    let mut chunks = String::new();
    for layer in &vp.layers {
        let name = layer_name(layer.name);
        if !names.contains(&name) || layer.primitives.is_empty() {
            continue;
        }
        let body: String = layer.primitives.iter().map(render_primitive).collect();
        chunks.push_str(&format!(
            "<g class=\"xpict-layer xpict-{name}\">{body}</g>"
        ));
    }
    chunks
}

/// Serialize a [`Scene`] to SVG (width/height = viewBox = SCALE units).
pub fn scene_to_svg(scene: &Scene) -> String {
    let w = fmt(scene.width);
    let h = fmt(scene.height);
    let mut parts = String::new();
    for vp in &scene.viewports {
        parts.push_str(&render_viewport_layers(vp, &["shading"]));
    }
    if !scene.halo.is_empty() {
        let body: String = scene.halo.iter().map(render_primitive).collect();
        parts.push_str(&format!("<g class=\"xpict-halo\" id=\"halo\">{body}</g>"));
    }
    for vp in &scene.viewports {
        parts.push_str(&render_viewport_layers(
            vp,
            &["bonds", "labels", "marks", "overlay"],
        ));
    }
    if !scene.overlays.is_empty() {
        let body: String = scene.overlays.iter().map(render_primitive).collect();
        parts.push_str(&format!(
            "<g class=\"xpict-overlays\" id=\"overlays\">{body}</g>"
        ));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\" class=\"xpict\" style=\"background:transparent\">{parts}</svg>"
    )
}
