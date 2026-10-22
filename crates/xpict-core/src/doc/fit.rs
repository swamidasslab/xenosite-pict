//! Post-layout scene fit — AABB of all ink + configurable viewport margin.

use crate::scene::{Primitive, Scene};

/// Default gap (px) between content AABB and SVG viewBox edge (small house pad).
pub const VIEWPORT_MARGIN_DEFAULT: f64 = 4.0;

/// Resolve a viewport margin: finite ≥ 0, else house default.
pub fn resolve_viewport_margin(pad: Option<f64>) -> f64 {
    pad.filter(|p| p.is_finite() && *p >= 0.0)
        .unwrap_or(VIEWPORT_MARGIN_DEFAULT)
}

/// Recompute [`Scene::width`] / [`Scene::height`] from painted ink after layout.
///
/// Collects an AABB over viewport layers (document space), overlays, and halo,
/// then shifts content so the box sits at `(margin, margin)` and sets the
/// canvas to content size + `2 * margin`.
pub fn fit_scene(mut scene: Scene, margin: f64) -> Scene {
    let margin = if margin.is_finite() && margin >= 0.0 {
        margin
    } else {
        VIEWPORT_MARGIN_DEFAULT
    };

    let Some((min_x, min_y, max_x, max_y)) = content_aabb(&scene) else {
        // Nothing to measure — leave declared canvas alone.
        return scene;
    };

    let dx = margin - min_x;
    let dy = margin - min_y;
    if dx.abs() > 1e-12 || dy.abs() > 1e-12 {
        for vp in &mut scene.viewports {
            // Document-space leftover: move the mol group's placement so
            // arrows (overlays) and boxes stay aligned. Mol-local leftover
            // from per-mol fit lives on x/y until compose folds it into ox/oy.
            vp.x += dx;
            vp.y += dy;
        }
        scene.halo = scene.halo.iter().map(|p| shift_prim(p, dx, dy)).collect();
        scene.overlays = scene
            .overlays
            .iter()
            .map(|p| shift_prim(p, dx, dy))
            .collect();
    }

    let content_w = (max_x - min_x).max(0.0);
    let content_h = (max_y - min_y).max(0.0);
    scene.width = content_w + 2.0 * margin;
    scene.height = content_h + 2.0 * margin;

    // Keep the primary viewport frame aligned with the fitted canvas (layout
    // and lone-mol SVG both read these).
    if scene.viewports.len() == 1 {
        if let Some(vp) = scene.viewports.first_mut() {
            vp.width = scene.width;
            vp.height = scene.height;
        }
    }

    scene
}

fn content_aabb(scene: &Scene) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut any = false;

    for vp in &scene.viewports {
        let mut layer_ink = false;
        let (ox, oy) = vp.local_origin();
        for layer in &vp.layers {
            for p in &layer.primitives {
                expand_prim(p, ox, oy, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
                layer_ink = true;
                any = true;
            }
        }
        // Empty placeholder viewports (compose tests): use the layout frame.
        // Real mols measure ink only so house PAD_PX can shrink away.
        if !layer_ink && (vp.width > 0.0 || vp.height > 0.0) {
            min_x = min_x.min(vp.x);
            min_y = min_y.min(vp.y);
            max_x = max_x.max(vp.x + vp.width);
            max_y = max_y.max(vp.y + vp.height);
            any = true;
        }
    }
    for p in scene.halo.iter().chain(scene.overlays.iter()) {
        expand_prim(p, 0.0, 0.0, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
        any = true;
    }

    if !any || !min_x.is_finite() {
        return None;
    }
    Some((min_x, min_y, max_x, max_y))
}

fn expand_prim(
    p: &Primitive,
    ox: f64,
    oy: f64,
    min_x: &mut f64,
    min_y: &mut f64,
    max_x: &mut f64,
    max_y: &mut f64,
) {
    match p {
        Primitive::Circle {
            cx,
            cy,
            r,
            stroke_width,
            ..
        } => {
            let grow = r + stroke_width.max(0.0) * 0.5;
            *min_x = min_x.min(ox + cx - grow);
            *min_y = min_y.min(oy + cy - grow);
            *max_x = max_x.max(ox + cx + grow);
            *max_y = max_y.max(oy + cy + grow);
        }
        Primitive::Text {
            x, y, font_size, ..
        } => {
            // Conservative ink box around baseline.
            let em = font_size.max(1.0);
            *min_x = min_x.min(ox + x - em);
            *min_y = min_y.min(oy + y - em);
            *max_x = max_x.max(ox + x + em * 2.0);
            *max_y = max_y.max(oy + y + em * 0.4);
        }
        Primitive::Path {
            d, stroke_width, ..
        } => {
            let grow = stroke_width.max(0.0) * 0.5;
            for (x, y) in path_coords(d) {
                *min_x = min_x.min(ox + x - grow);
                *min_y = min_y.min(oy + y - grow);
                *max_x = max_x.max(ox + x + grow);
                *max_y = max_y.max(oy + y + grow);
            }
        }
    }
}

fn path_coords(d: &str) -> Vec<(f64, f64)> {
    let mut nums = Vec::new();
    let mut cur = String::new();
    for c in d.chars() {
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E' {
            cur.push(c);
        } else if !cur.is_empty() {
            if let Ok(v) = cur.parse::<f64>() {
                nums.push(v);
            }
            cur.clear();
        }
    }
    if !cur.is_empty() {
        if let Ok(v) = cur.parse::<f64>() {
            nums.push(v);
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < nums.len() {
        out.push((nums[i], nums[i + 1]));
        i += 2;
    }
    out
}

fn shift_prim(p: &Primitive, dx: f64, dy: f64) -> Primitive {
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
        } => Primitive::Path {
            d: shift_ml_path(d, dx, dy),
            stroke: stroke.clone(),
            fill: fill.clone(),
            stroke_width: *stroke_width,
            opacity: *opacity,
            stroke_dasharray: stroke_dasharray.clone(),
            stroke_linecap: stroke_linecap.clone(),
            class: class.clone(),
            data_text: data_text.clone(),
        },
        Primitive::Circle {
            cx,
            cy,
            r,
            fill,
            stroke,
            stroke_width,
            opacity,
            class,
        } => Primitive::Circle {
            cx: cx + dx,
            cy: cy + dy,
            r: *r,
            fill: fill.clone(),
            stroke: stroke.clone(),
            stroke_width: *stroke_width,
            opacity: *opacity,
            class: class.clone(),
        },
        Primitive::Text {
            x,
            y,
            text,
            fill,
            font_size,
            anchor,
            class,
            rotate,
        } => Primitive::Text {
            x: x + dx,
            y: y + dy,
            text: text.clone(),
            fill: fill.clone(),
            font_size: *font_size,
            anchor: *anchor,
            class: class.clone(),
            rotate: *rotate,
        },
    }
}

/// Shift absolute M/L coordinate pairs (depict / caption paths).
fn shift_ml_path(d: &str, dx: f64, dy: f64) -> String {
    if dx.abs() < 1e-12 && dy.abs() < 1e-12 {
        return d.to_string();
    }
    let mut out = String::with_capacity(d.len() + 16);
    let mut chars = d.chars().peekable();
    let mut cmd = 'M';
    while let Some(c) = chars.next() {
        if c.is_ascii_alphabetic() {
            cmd = c;
            out.push(c);
            continue;
        }
        if c.is_whitespace() || c == ',' {
            out.push(c);
            continue;
        }
        let mut num = String::new();
        num.push(c);
        while let Some(&n) = chars.peek() {
            if n.is_ascii_digit() || n == '.' || n == 'e' || n == 'E' || n == '+' || n == '-' {
                if (n == '-' || n == '+')
                    && !num.is_empty()
                    && !num.ends_with('e')
                    && !num.ends_with('E')
                {
                    break;
                }
                num.push(n);
                chars.next();
            } else {
                break;
            }
        }
        if matches!(cmd, 'M' | 'L' | 'm' | 'l') {
            while matches!(chars.peek(), Some(ch) if ch.is_whitespace() || *ch == ',') {
                chars.next();
            }
            let mut num2 = String::new();
            if let Some(&n0) = chars.peek() {
                if n0 == '-' || n0 == '+' || n0.is_ascii_digit() || n0 == '.' {
                    num2.push(n0);
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        if n.is_ascii_digit()
                            || n == '.'
                            || n == 'e'
                            || n == 'E'
                            || n == '+'
                            || n == '-'
                        {
                            if (n == '-' || n == '+')
                                && !num2.is_empty()
                                && !num2.ends_with('e')
                                && !num2.ends_with('E')
                            {
                                break;
                            }
                            num2.push(n);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
            }
            if let (Ok(x), Ok(y)) = (num.parse::<f64>(), num2.parse::<f64>()) {
                // Relative m/l: only absolute M/L get an origin shift.
                if matches!(cmd, 'M' | 'L') {
                    out.push_str(&format!("{} {}", x + dx, y + dy));
                } else {
                    out.push_str(&format!("{} {}", x, y));
                }
                continue;
            }
        }
        out.push_str(&num);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Layer, LayerName, Viewport};

    fn path_scene(d: &str) -> Scene {
        // Oversized declared canvas; viewport frame left empty so fit uses ink only.
        Scene {
            width: 200.0,
            height: 200.0,
            viewports: vec![Viewport {
                id: Some("m".into()),
                x: 0.0,
                y: 0.0,
                ox: 0.0,
                oy: 0.0,
                width: 0.0,
                height: 0.0,
                layers: vec![Layer {
                    name: LayerName::Bonds,
                    primitives: vec![Primitive::Path {
                        d: d.into(),
                        stroke: Some("#000".into()),
                        fill: Some("none".into()),
                        stroke_width: 1.0,
                        opacity: 1.0,
                        stroke_dasharray: None,
                        stroke_linecap: None,
                        class: None,
                        data_text: None,
                    }],
                }],
            }],
            overlays: vec![],
            halo: vec![],
        }
    }

    #[test]
    fn fit_tightens_canvas_to_ink_plus_margin() {
        let scene = path_scene("M 10 10 L 30 10 L 30 40");
        let margin = 4.0;
        let out = fit_scene(scene, margin);
        // Ink AABB ≈ (10,10)-(30,40) plus half stroke.
        assert!(
            (out.width - (20.0 + 1.0 + 2.0 * margin)).abs() < 1.5,
            "width={}",
            out.width
        );
        assert!(
            (out.height - (30.0 + 1.0 + 2.0 * margin)).abs() < 1.5,
            "height={}",
            out.height
        );
        assert!(out.width < 100.0, "should shrink from 200");
    }

    #[test]
    fn fit_includes_overlay_caption() {
        let mut scene = path_scene("M 0 0 L 20 0 L 20 20");
        scene.overlays.push(Primitive::Path {
            d: "M 5 50 L 15 50".into(),
            stroke: Some("none".into()),
            fill: Some("#111".into()),
            stroke_width: 0.0,
            opacity: 1.0,
            stroke_dasharray: None,
            stroke_linecap: None,
            class: Some("mol-caption".into()),
            data_text: Some("cap".into()),
        });
        let out = fit_scene(scene, 4.0);
        assert!(
            out.height > 50.0,
            "fitted height should include caption at y=50: {}",
            out.height
        );
    }

    #[test]
    fn resolve_margin_default_and_override() {
        assert!((resolve_viewport_margin(None) - VIEWPORT_MARGIN_DEFAULT).abs() < 1e-12);
        assert!((resolve_viewport_margin(Some(8.0)) - 8.0).abs() < 1e-12);
        assert!((resolve_viewport_margin(Some(-1.0)) - VIEWPORT_MARGIN_DEFAULT).abs() < 1e-12);
    }
}
