//! Post-layout scene fit — AABB of all ink + configurable viewport margin.

use crate::scene::{Primitive, Scene};

/// Default gap (px) between content AABB and SVG viewBox edge (small house pad).
pub const VIEWPORT_MARGIN_DEFAULT: f64 = 4.0;

/// Resolve a viewport margin: finite ≥ 0, else house default.
pub fn resolve_viewport_margin(pad: Option<f64>) -> f64 {
    pad.filter(|p| p.is_finite() && *p >= 0.0)
        .unwrap_or(VIEWPORT_MARGIN_DEFAULT)
}

/// Recompute the SVG viewBox from painted ink after layout.
///
/// AABB over viewport layers (document space), overlays, and halo. The camera
/// is `viewBox = (min - margin) + content size + 2·margin`. Geometry is not
/// translated; path `d` is not rewritten.
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

    scene.x = min_x - margin;
    scene.y = min_y - margin;
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
            x: 0.0,
            y: 0.0,
        }
    }

    #[test]
    fn fit_tightens_canvas_to_ink_plus_margin() {
        let scene = path_scene("M 10 10 L 30 10 L 30 40");
        let margin = 4.0;
        let out = fit_scene(scene, margin);
        assert!(
            (out.x - (10.0 - 0.5 - margin)).abs() < 1.5,
            "viewBox min-x should be ink min minus pad: x={}",
            out.x
        );
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

    fn last_xy(d: &str) -> (f64, f64) {
        let nums: Vec<f64> = d
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .filter_map(|s| s.parse().ok())
            .collect();
        (nums[nums.len() - 2], nums[nums.len() - 1])
    }

    #[test]
    fn fit_keeps_catmull_head_on_the_shaft() {
        use crate::arrows::{edge_primitives, EdgePaintIn};
        use crate::doc::EdgeArrow;
        let edge = EdgePaintIn {
            pts: vec![(20.0, 10.0), (80.0, 40.0), (90.0, 140.0)],
            arrow: EdgeArrow::Forward,
            routing: None,
            color: Some("#0072b2".into()),
            stroke_width: Some(4.8),
            dashed: false,
            label: None,
            label_pos: None,
            index: 0,
            curve_catmull: true,
            label_xy: None,
            label_rotate: None,
        };
        let prims = edge_primitives(&edge);
        let overlays: Vec<Primitive> = prims
            .into_iter()
            .filter_map(|p| match p {
                crate::arrows::EdgePrim::Path {
                    d,
                    stroke,
                    fill,
                    stroke_width,
                    class,
                    ..
                } => Some(Primitive::Path {
                    d,
                    stroke: Some(stroke),
                    fill: Some(fill),
                    stroke_width,
                    opacity: 1.0,
                    stroke_dasharray: None,
                    stroke_linecap: None,
                    class: Some(class),
                    data_text: None,
                }),
                _ => None,
            })
            .collect();
        let before: Vec<String> = overlays
            .iter()
            .filter_map(|p| match p {
                Primitive::Path { d, .. } => Some(d.clone()),
                _ => None,
            })
            .collect();
        let scene = Scene {
            width: 400.0,
            height: 400.0,
            x: 0.0,
            y: 0.0,
            viewports: vec![],
            overlays,
            halo: vec![],
        };
        let out = fit_scene(scene, 4.0);
        let after: Vec<String> = out
            .overlays
            .iter()
            .filter_map(|p| match p {
                Primitive::Path { d, .. } => Some(d.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            after, before,
            "fit crops with viewBox; it must not rewrite d"
        );
        assert!(
            out.x.abs() > 1.0 || out.y.abs() > 1.0,
            "viewBox origin should track ink, not stay at 0: ({}, {})",
            out.x,
            out.y
        );
        let shaft = after.iter().find(|d| d.contains('C')).expect("cubic shaft");
        let head = after
            .iter()
            .find(|d| d.contains('Z') || d.contains('z'))
            .expect("head");
        let (sx, sy) = last_xy(shaft);
        let hnums: Vec<f64> = head
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .filter_map(|s| s.parse().ok())
            .collect();
        let (bx, by) = ((hnums[0] + hnums[4]) * 0.5, (hnums[1] + hnums[5]) * 0.5);
        let gap = (sx - bx).hypot(sy - by);
        assert!(
            gap < 0.6,
            "head stays on the shaft because neither d moved; gap={gap}"
        );
    }
}
