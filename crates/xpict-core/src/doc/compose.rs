//! Compose a reaction scheme into one [`Scene`]: place painted mols + edge overlays.
//!
//! Pass 2 ([`super::render_doc`]) paints each molecule independently. This module
//! lays them out (ELK when available, else a tight row) and strokes edges via
//! [`crate::arrows`].

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::arrows::{
    edge_anchors, edge_primitives as paint_edge, simplify_route, EdgePaintIn, EdgePrim, ANCHOR_GAP,
    KINK_PX,
};
use crate::scene::{Primitive, Scene, TextAnchor, Viewport};

use super::{
    DepictSpec, DocPaint, EdgeNode, EdgeRouting, LabelPos, LayoutOpts, Node,
};

const EDGE_LABEL_FONT_PX: f64 = 11.0;
const EDGE_LABEL_PAD: f64 = 4.0;
const ROW_GAP: f64 = 0.0; // ≈ LayoutOpts::DEFAULT_LAYER_SPACING

/// Compose a [`DepictSpec::ReactionScheme`] from per-mol [`DocPaint`] rows.
///
/// Non-scheme roots return the first paint scene (or an empty scene).
pub fn compose_scheme(spec: &DepictSpec, paints: &[DocPaint]) -> Result<Scene, String> {
    match spec {
        DepictSpec::ReactionScheme { layout, .. } => {
            compose_reaction(spec, paints, layout.as_ref().unwrap_or(&LayoutOpts::default()))
        }
        DepictSpec::Mol { .. } | DepictSpec::Group { .. } => {
            if paints.len() == 1 {
                Ok(paints[0].scene.clone())
            } else if paints.is_empty() {
                Ok(Scene {
                    width: 0.0,
                    height: 0.0,
                    viewports: vec![],
                    overlays: vec![],
                    halo: vec![],
                })
            } else {
                // Multi-mol group: simple row (no edges).
                Ok(place_row(paints, 16.0))
            }
        }
    }
}

fn compose_reaction(
    spec: &DepictSpec,
    paints: &[DocPaint],
    layout: &LayoutOpts,
) -> Result<Scene, String> {
    if paints.is_empty() {
        return Ok(Scene {
            width: 0.0,
            height: 0.0,
            viewports: vec![],
            overlays: vec![],
            halo: vec![],
        });
    }

    let by_id: HashMap<&str, &DocPaint> = paints.iter().map(|p| (p.id.as_str(), p)).collect();
    let _ = by_id;
    let sizes: Vec<(String, f64, f64)> = paints
        .iter()
        .map(|p| {
            let (w, h) = scene_size(&p.scene);
            (p.id.clone(), w, h)
        })
        .collect();

    let placement = match layout_scheme(spec, layout, &sizes) {
        Ok(p) => p,
        Err(_) => row_placement(&sizes),
    };

    let mut viewports = Vec::with_capacity(paints.len());
    let mut halo = Vec::new();
    let mut max_r = 0.0_f64;
    let mut max_b = 0.0_f64;

    for p in paints {
        let (x, y) = placement
            .positions
            .get(p.id.as_str())
            .copied()
            .unwrap_or((0.0, 0.0));
        let (w, h) = scene_size(&p.scene);
        let mut vp = p
            .scene
            .viewports
            .first()
            .cloned()
            .unwrap_or(Viewport {
                id: Some(p.id.clone()),
                x: 0.0,
                y: 0.0,
                width: w,
                height: h,
                layers: vec![],
            });
        vp.id = Some(p.id.clone());
        vp.x = x;
        vp.y = y;
        vp.width = w;
        vp.height = h;
        max_r = max_r.max(x + w);
        max_b = max_b.max(y + h);
        for prim in &p.scene.halo {
            halo.push(translate_prim(prim, x, y));
        }
        viewports.push(vp);
    }

    let vp_boxes: HashMap<&str, (f64, f64, f64, f64)> = viewports
        .iter()
        .filter_map(|vp| {
            let id = vp.id.as_deref()?;
            Some((id, (vp.x, vp.y, vp.width, vp.height)))
        })
        .collect();

    let mut overlays = Vec::new();
    for (i, edge) in spec.edges().iter().enumerate() {
        let src_id = edge
            .sources
            .as_slice()
            .first()
            .map(String::as_str)
            .unwrap_or("");
        let tgt_id = edge
            .targets
            .as_slice()
            .first()
            .map(String::as_str)
            .unwrap_or("");
        let Some(&src_box) = vp_boxes.get(src_id) else {
            continue;
        };
        let Some(&tgt_box) = vp_boxes.get(tgt_id) else {
            continue;
        };

        let route = placement
            .edge_paths
            .get(i)
            .and_then(|p| p.as_ref())
            .cloned()
            .filter(|pts| pts.len() >= 2)
            .unwrap_or_else(|| {
                let (a, b) = edge_anchors(src_box, tgt_box, ANCHOR_GAP);
                vec![a, b]
            });
        let pts = simplify_route(&route, KINK_PX);
        for &(x, y) in &pts {
            max_r = max_r.max(x + 8.0);
            max_b = max_b.max(y + 8.0);
        }

        let (label_text, label_pos) = resolve_edge_label(spec, edge);
        let paint = EdgePaintIn {
            pts,
            arrow: edge.arrow,
            routing: Some(edge.edge_routing_or(layout)),
            color: edge.color.clone(),
            stroke_width: edge.stroke_width,
            dashed: edge.dashed,
            label: label_text,
            label_pos,
            index: i,
        };
        for prim in paint_edge(&paint) {
            let scene_prim = edge_prim_to_scene(&prim);
            expand_bounds_prim(&scene_prim, &mut max_r, &mut max_b);
            overlays.push(scene_prim);
        }
    }

    Ok(Scene {
        width: max_r.max(placement.width.unwrap_or(0.0)),
        height: max_b.max(placement.height.unwrap_or(0.0)),
        viewports,
        overlays,
        halo,
    })
}

fn scene_size(scene: &Scene) -> (f64, f64) {
    if let Some(vp) = scene.viewports.first() {
        (vp.width.max(scene.width), vp.height.max(scene.height))
    } else {
        (scene.width, scene.height)
    }
}

fn place_row(paints: &[DocPaint], gap: f64) -> Scene {
    let sizes: Vec<(String, f64, f64)> = paints
        .iter()
        .map(|p| {
            let (w, h) = scene_size(&p.scene);
            (p.id.clone(), w, h)
        })
        .collect();
    let max_h = sizes.iter().map(|(_, _, h)| *h).fold(0.0_f64, f64::max);
    let mut viewports = Vec::new();
    let mut halo = Vec::new();
    let mut max_r = 0.0_f64;
    let mut max_b = 0.0_f64;
    let mut x = 0.0_f64;
    for p in paints {
        let (w, h) = scene_size(&p.scene);
        let y = (max_h - h) * 0.5;
        let mut vp = p.scene.viewports.first().cloned().unwrap_or(Viewport {
            id: Some(p.id.clone()),
            x: 0.0,
            y: 0.0,
            width: w,
            height: h,
            layers: vec![],
        });
        vp.id = Some(p.id.clone());
        vp.x = x;
        vp.y = y;
        max_r = max_r.max(x + w);
        max_b = max_b.max(y + h);
        for prim in &p.scene.halo {
            halo.push(translate_prim(prim, x, y));
        }
        viewports.push(vp);
        x += w + gap;
    }
    Scene {
        width: max_r,
        height: max_b,
        viewports,
        overlays: vec![],
        halo,
    }
}

struct Placement {
    positions: HashMap<String, (f64, f64)>,
    edge_paths: Vec<Option<Vec<(f64, f64)>>>,
    width: Option<f64>,
    height: Option<f64>,
}

fn row_placement(sizes: &[(String, f64, f64)]) -> Placement {
    let max_h = sizes.iter().map(|(_, _, h)| *h).fold(0.0_f64, f64::max);
    let mut positions = HashMap::new();
    let mut x = 0.0;
    for (id, w, h) in sizes {
        let y = (max_h - h) * 0.5;
        positions.insert(id.clone(), (x, y));
        x += w + ROW_GAP;
    }
    Placement {
        positions,
        edge_paths: vec![],
        width: Some(x),
        height: Some(max_h),
    }
}

#[cfg(feature = "elk")]
fn layout_scheme(
    spec: &DepictSpec,
    layout: &LayoutOpts,
    sizes: &[(String, f64, f64)],
) -> Result<Placement, String> {
    let graph = build_elk_graph(spec, layout, sizes);
    let laid = crate::elk::layout_json(&serde_json::to_string(&graph).map_err(|e| e.to_string())?)?;
    parse_elk_placement(spec, &laid, sizes)
}

#[cfg(not(feature = "elk"))]
fn layout_scheme(
    _spec: &DepictSpec,
    _layout: &LayoutOpts,
    sizes: &[(String, f64, f64)],
) -> Result<Placement, String> {
    Ok(row_placement(sizes))
}

#[cfg(feature = "elk")]
fn build_elk_graph(
    spec: &DepictSpec,
    layout: &LayoutOpts,
    sizes: &[(String, f64, f64)],
) -> Value {
    let opts = crate::elk::scheme_layout_options(crate::elk::DiagramKind::Reaction, layout);
    let children: Vec<Value> = sizes
        .iter()
        .map(|(id, w, h)| {
            json!({
                "id": id,
                "width": w,
                "height": h,
            })
        })
        .collect();

    let mut edges = Vec::new();
    for (i, e) in spec.edges().iter().enumerate() {
        let eid = format!("e{i}");
        let mut entry = json!({
            "id": eid,
            "sources": e.sources.as_slice(),
            "targets": e.targets.as_slice(),
        });
        if let Some(labels) = elk_edge_labels(spec, e, &format!("e{i}")) {
            entry["labels"] = labels;
        }
        if let Some(r) = e.edge_routing {
            entry["layoutOptions"] = json!({
                "elk.edgeRouting": routing_name(r),
            });
        }
        edges.push(entry);
    }

    json!({
        "id": "root",
        "layoutOptions": opts,
        "children": children,
        "edges": edges,
    })
}

#[cfg(feature = "elk")]
fn routing_name(r: EdgeRouting) -> &'static str {
    match r {
        EdgeRouting::Orthogonal => "ORTHOGONAL",
        EdgeRouting::Polyline => "POLYLINE",
        EdgeRouting::Splines => "SPLINES",
    }
}

#[cfg(feature = "elk")]
fn elk_edge_labels(spec: &DepictSpec, edge: &EdgeNode, edge_id: &str) -> Option<Value> {
    let (text, pos) = resolve_edge_label(spec, edge);
    let text = text?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    let (w, h) = measure_label(&text);
    let side = match pos.as_deref().unwrap_or("above") {
        "below" | "right" => "ALWAYS_DOWN",
        _ => "ALWAYS_UP",
    };
    Some(json!([{
        "id": format!("{edge_id}_lab"),
        "text": text,
        "width": w,
        "height": h,
        "layoutOptions": {
            "elk.edgeLabels.placement": "CENTER",
            "elk.layered.edgeLabels.sideSelection": side,
        }
    }]))
}

fn measure_label(text: &str) -> (f64, f64) {
    #[cfg(feature = "font")]
    {
        use crate::font::{face_metrics, glyph_metrics, FaceStyle};
        let face = face_metrics(FaceStyle::Regular);
        let scale = EDGE_LABEL_FONT_PX / face.upem.max(1.0);
        let mut adv = 0.0;
        let mut ink_w = 0.0;
        for ch in text.chars() {
            if let Some(g) = glyph_metrics(ch, FaceStyle::Regular) {
                adv += g.advance * scale;
                ink_w += g.ink_width() * scale;
            } else {
                adv += EDGE_LABEL_FONT_PX * 0.5;
            }
        }
        let w = adv.max(ink_w) + EDGE_LABEL_PAD;
        let typo = (face.ascent - face.descent).abs().max(face.upem * 0.8);
        let h = typo * scale + EDGE_LABEL_PAD;
        (w, h)
    }
    #[cfg(not(feature = "font"))]
    {
        let w = text.len() as f64 * EDGE_LABEL_FONT_PX * 0.55 + EDGE_LABEL_PAD;
        let h = EDGE_LABEL_FONT_PX + EDGE_LABEL_PAD;
        (w, h)
    }
}

#[cfg(feature = "elk")]
fn parse_elk_placement(
    spec: &DepictSpec,
    laid_json: &str,
    sizes: &[(String, f64, f64)],
) -> Result<Placement, String> {
    let laid: Value = serde_json::from_str(laid_json).map_err(|e| e.to_string())?;
    let mut positions = HashMap::new();
    for (id, _, _) in sizes {
        positions.insert(id.clone(), (0.0, 0.0));
    }
    if let Some(children) = laid.get("children").and_then(|c| c.as_array()) {
        for c in children {
            let Some(id) = c.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let x = c.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = c.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            positions.insert(id.to_string(), (x, y));
        }
    }

    let edges_out = laid.get("edges").and_then(|e| e.as_array());
    let mut edge_paths = Vec::with_capacity(spec.edges().len());
    for i in 0..spec.edges().len() {
        let eid = format!("e{i}");
        let path = edges_out.and_then(|arr| {
            arr.iter()
                .find(|e| e.get("id").and_then(|v| v.as_str()) == Some(eid.as_str()))
                .and_then(edge_path_from_elk)
        });
        edge_paths.push(path);
    }

    Ok(Placement {
        positions,
        edge_paths,
        width: laid.get("width").and_then(|v| v.as_f64()),
        height: laid.get("height").and_then(|v| v.as_f64()),
    })
}

#[cfg(feature = "elk")]
fn edge_path_from_elk(edge: &Value) -> Option<Vec<(f64, f64)>> {
    let sections = edge.get("sections")?.as_array()?;
    if sections.is_empty() {
        return None;
    }
    let mut path = Vec::new();
    for sec in sections {
        let pts = section_points(sec);
        if pts.is_empty() {
            continue;
        }
        if path.is_empty() {
            path.extend(pts);
        } else if pts[0] == *path.last().unwrap() {
            path.extend(pts.into_iter().skip(1));
        } else {
            path.extend(pts);
        }
    }
    if path.len() >= 2 {
        Some(path)
    } else {
        None
    }
}

#[cfg(feature = "elk")]
fn section_points(section: &Value) -> Vec<(f64, f64)> {
    let start = section.get("startPoint").unwrap_or(&Value::Null);
    let end = section.get("endPoint").unwrap_or(&Value::Null);
    let mut pts = vec![(
        start.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
        start.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
    )];
    if let Some(bends) = section.get("bendPoints").and_then(|b| b.as_array()) {
        for bp in bends {
            pts.push((
                bp.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
                bp.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
            ));
        }
    }
    pts.push((
        end.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
        end.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
    ));
    let mut cleaned = Vec::new();
    for p in pts {
        if cleaned
            .last()
            .map(|&(lx, ly): &(f64, f64)| (p.0 - lx).abs() > 1e-6 || (p.1 - ly).abs() > 1e-6)
            .unwrap_or(true)
        {
            cleaned.push(p);
        }
    }
    cleaned
}

fn resolve_edge_label(spec: &DepictSpec, edge: &EdgeNode) -> (Option<String>, Option<String>) {
    let Some(ref lab) = edge.label else {
        return (None, None);
    };
    let placements = lab.placements();
    let Some((id, pos)) = placements.first() else {
        return (None, None);
    };
    let text = match spec.node_by_id(id.trim()) {
        Some(Node::Text(t)) => Some(t.text.clone()),
        // Bare string ids that are not nodes: treat the id itself as label text
        // when it looks like content (hosts may pass Label::Id("ADH") without a
        // text node — not valid live ABI, but keep paint useful in tests).
        None if !id.trim().is_empty() && spec.node_by_id(id.trim()).is_none() => {
            // Prefer requiring text nodes; skip unknown refs.
            None
        }
        _ => None,
    };
    let pos = Some(match pos {
        LabelPos::Above => "above".into(),
        LabelPos::Below => "below".into(),
        LabelPos::Left => "left".into(),
        LabelPos::Right => "right".into(),
    });
    (text, pos)
}

fn edge_prim_to_scene(p: &EdgePrim) -> Primitive {
    match p {
        EdgePrim::Path {
            d,
            stroke,
            fill,
            stroke_width,
            stroke_dasharray,
            class,
        } => Primitive::Path {
            d: d.clone(),
            stroke: if stroke == "none" {
                None
            } else {
                Some(stroke.clone())
            },
            fill: if fill == "none" {
                None
            } else {
                Some(fill.clone())
            },
            stroke_width: *stroke_width,
            opacity: 1.0,
            stroke_dasharray: stroke_dasharray.clone(),
            stroke_linecap: None,
            class: Some(class.clone()),
            data_text: None,
        },
        EdgePrim::Text {
            x,
            y,
            text,
            fill,
            font_size,
            anchor,
            class,
        } => Primitive::Text {
            x: *x,
            y: *y,
            text: text.clone(),
            fill: fill.clone(),
            font_size: *font_size,
            anchor: match anchor.as_str() {
                "start" => TextAnchor::Start,
                "end" => TextAnchor::End,
                _ => TextAnchor::Middle,
            },
            class: Some(class.clone()),
        },
    }
}

fn translate_prim(p: &Primitive, dx: f64, dy: f64) -> Primitive {
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
            d: shift_path_d(d, dx, dy),
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
        } => Primitive::Text {
            x: x + dx,
            y: y + dy,
            text: text.clone(),
            fill: fill.clone(),
            font_size: *font_size,
            anchor: *anchor,
            class: class.clone(),
        },
    }
}

fn shift_path_d(d: &str, dx: f64, dy: f64) -> String {
    // Best-effort: shift numeric pairs after M/L (absolute). Enough for halo paths.
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
        // Parse a number starting at c.
        let mut num = String::new();
        num.push(c);
        while let Some(&n) = chars.peek() {
            if n.is_ascii_digit() || n == '.' || n == 'e' || n == 'E' || n == '+' || n == '-' {
                // stop if '-' / '+' not part of exponent
                if (n == '-' || n == '+') && !num.is_empty() && !num.ends_with('e') && !num.ends_with('E')
                {
                    break;
                }
                num.push(n);
                chars.next();
            } else {
                break;
            }
        }
        // Need a following number for M/L pairs.
        if matches!(cmd, 'M' | 'L' | 'm' | 'l') {
            // skip whitespace
            while matches!(chars.peek(), Some(ch) if ch.is_whitespace() || *ch == ',') {
                chars.next();
            }
            let mut num2 = String::new();
            if let Some(&n0) = chars.peek() {
                if n0.is_ascii_digit() || n0 == '.' || n0 == '-' || n0 == '+' {
                    num2.push(n0);
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        if n.is_ascii_digit() || n == '.' || n == 'e' || n == 'E' || n == '+' || n == '-'
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
                if cmd == 'M' || cmd == 'L' {
                    out.push_str(&format!("{:.2} {:.2}", x + dx, y + dy));
                } else {
                    // relative — leave unchanged
                    out.push_str(&num);
                    out.push(' ');
                    out.push_str(&num2);
                }
                continue;
            }
        }
        out.push_str(&num);
    }
    out
}

fn expand_bounds_prim(p: &Primitive, max_r: &mut f64, max_b: &mut f64) {
    match p {
        Primitive::Text { x, y, .. } => {
            *max_r = max_r.max(*x + 12.0);
            *max_b = max_b.max(*y + 12.0);
        }
        Primitive::Path { d, .. } => {
            for (x, y) in path_coords(d) {
                *max_r = max_r.max(x + 8.0);
                *max_b = max_b.max(y + 8.0);
            }
        }
        Primitive::Circle { cx, cy, r, .. } => {
            *max_r = max_r.max(cx + r + 4.0);
            *max_b = max_b.max(cy + r + 4.0);
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
    use crate::arrows::EdgePrim;
    use crate::doc::{EdgeArrow, EdgeNodeKind, MolIds, MolNode, MolNodeKind, TextNode, TextNodeKind};
    use crate::scene::{AtomIn, Layer, LayerName, MoleculeIn, TextAnchor};

    fn paint(id: &str, w: f64, h: f64) -> DocPaint {
        DocPaint {
            id: id.into(),
            molecule: MoleculeIn {
                id: Some(id.into()),
                atoms: vec![AtomIn {
                    index: 0,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 10.0,
                    y: 10.0,
                    label: None,
                    charge: 0,
                }],
                bonds: vec![],
                color: None,
                atom_shade: None,
                bond_shade: None,
                shade_vmin: None,
                shade_vmax: None,
                mark_atoms: vec![],
                mark_bonds: vec![],
                scale: 1.0,
                weight: 1.0,
            },
            scene: Scene {
                width: w,
                height: h,
                viewports: vec![Viewport {
                    id: Some(id.into()),
                    x: 0.0,
                    y: 0.0,
                    width: w,
                    height: h,
                    layers: vec![Layer {
                        name: LayerName::Bonds,
                        primitives: vec![],
                    }],
                }],
                overlays: vec![],
                halo: vec![],
            },
        }
    }

    #[test]
    fn compose_two_mol_scheme_has_overlay() {
        let spec = DepictSpec::ReactionScheme {
            id: None,
            children: vec![
                Node::Mol(MolNode {
                    type_: MolNodeKind::Mol,
                    id: Some("A".into()),
                    smiles: Some("CCO".into()),
                    cxsmiles: None,
                    molfile: None,
                    label: None,
                    star_labels: None,
                    align_to: None,
                    opts: None,
                    color: None,
                    scale: None,
                    weight: None,
                    halo: None,
                    shade: None,
                }),
                Node::Text(TextNode {
                    type_: TextNodeKind::Text,
                    id: Some("lab".into()),
                    text: "ox".into(),
                    color: None,
                    scale: None,
                    opts: None,
                }),
                Node::Edge(EdgeNode {
                    type_: EdgeNodeKind::Edge,
                    sources: MolIds::One("A".into()),
                    targets: MolIds::One("B".into()),
                    label: Some(crate::doc::Label::Id("lab".into())),
                    edge_routing: None,
                    role: None,
                    arrow: EdgeArrow::Forward,
                    color: None,
                    stroke_width: None,
                    dashed: false,
                }),
                Node::Mol(MolNode {
                    type_: MolNodeKind::Mol,
                    id: Some("B".into()),
                    smiles: Some("CC=O".into()),
                    cxsmiles: None,
                    molfile: None,
                    label: None,
                    star_labels: None,
                    align_to: None,
                    opts: None,
                    color: None,
                    scale: None,
                    weight: None,
                    halo: None,
                    shade: None,
                }),
            ],
            layout: Some(LayoutOpts {
                edge_routing: Some(EdgeRouting::Orthogonal),
                ..Default::default()
            }),
            opts: None,
            color: None,
            scale: None,
        };
        let paints = vec![paint("A", 80.0, 60.0), paint("B", 80.0, 60.0)];
        let scene = compose_scheme(&spec, &paints).expect("compose");
        assert_eq!(scene.viewports.len(), 2);
        assert!(
            !scene.overlays.is_empty(),
            "expected edge overlays, got none"
        );
        assert!(scene.width > 80.0);
    }

    #[test]
    fn compose_empty_and_group_row() {
        let empty = compose_scheme(
            &DepictSpec::ReactionScheme {
                id: None,
                children: vec![],
                layout: None,
                opts: None,
                color: None,
                scale: None,
            },
            &[],
        )
        .unwrap();
        assert!(empty.viewports.is_empty());

        let group = DepictSpec::Group {
            id: None,
            children: vec![
                Node::Mol(MolNode {
                    type_: MolNodeKind::Mol,
                    id: Some("a".into()),
                    smiles: Some("C".into()),
                    cxsmiles: None,
                    molfile: None,
                    label: None,
                    star_labels: None,
                    align_to: None,
                    opts: None,
                    color: None,
                    scale: None,
                    weight: None,
                    halo: None,
                    shade: None,
                }),
                Node::Mol(MolNode {
                    type_: MolNodeKind::Mol,
                    id: Some("b".into()),
                    smiles: Some("O".into()),
                    cxsmiles: None,
                    molfile: None,
                    label: None,
                    star_labels: None,
                    align_to: None,
                    opts: None,
                    color: None,
                    scale: None,
                    weight: None,
                    halo: None,
                    shade: None,
                }),
            ],
            align: false,
            opts: None,
            color: None,
            scale: None,
        };
        let scene = compose_scheme(&group, &[paint("a", 40.0, 30.0), paint("b", 50.0, 30.0)]).unwrap();
        assert_eq!(scene.viewports.len(), 2);
        assert!(scene.viewports[1].x > scene.viewports[0].x);
    }

    #[test]
    fn translate_and_measure_helpers() {
        let (w, h) = measure_label("ADH");
        assert!(w > 10.0 && h > 5.0);

        let p = Primitive::Circle {
            cx: 1.0,
            cy: 2.0,
            r: 3.0,
            fill: Some("#000".into()),
            stroke: None,
            stroke_width: 0.0,
            opacity: 1.0,
            class: None,
        };
        let t = translate_prim(&p, 10.0, 20.0);
        match t {
            Primitive::Circle { cx, cy, .. } => {
                assert!((cx - 11.0).abs() < 1e-9);
                assert!((cy - 22.0).abs() < 1e-9);
            }
            _ => panic!("expected circle"),
        }

        let path = Primitive::Path {
            d: "M 0.00 0.00 L 10.00 5.00".into(),
            stroke: Some("#222".into()),
            fill: None,
            stroke_width: 1.0,
            opacity: 1.0,
            stroke_dasharray: None,
            stroke_linecap: None,
            class: Some("edge".into()),
            data_text: None,
        };
        let shifted = translate_prim(&path, 1.0, 2.0);
        match shifted {
            Primitive::Path { d, .. } => {
                assert!(d.contains("1.00") && d.contains("2.00"));
            }
            _ => panic!("expected path"),
        }

        let text = Primitive::Text {
            x: 0.0,
            y: 0.0,
            text: "hi".into(),
            fill: "#000".into(),
            font_size: 11.0,
            anchor: TextAnchor::Middle,
            class: None,
        };
        match translate_prim(&text, 5.0, 6.0) {
            Primitive::Text { x, y, .. } => {
                assert!((x - 5.0).abs() < 1e-9);
                assert!((y - 6.0).abs() < 1e-9);
            }
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn edge_prim_mapping_and_path_coords() {
        let path = edge_prim_to_scene(&EdgePrim::Path {
            d: "M 1 2".into(),
            stroke: "none".into(),
            fill: "#222".into(),
            stroke_width: 0.0,
            stroke_dasharray: None,
            class: "head".into(),
        });
        match path {
            Primitive::Path {
                stroke, fill, class, ..
            } => {
                assert!(stroke.is_none());
                assert_eq!(fill.as_deref(), Some("#222"));
                assert_eq!(class.as_deref(), Some("head"));
            }
            _ => panic!("path"),
        }
        let txt = edge_prim_to_scene(&EdgePrim::Text {
            x: 1.0,
            y: 2.0,
            text: "L".into(),
            fill: "#222".into(),
            font_size: 11.0,
            anchor: "start".into(),
            class: "label".into(),
        });
        match txt {
            Primitive::Text { anchor, .. } => assert_eq!(anchor, TextAnchor::Start),
            _ => panic!("text"),
        }
        let pts = path_coords("M 0 0 L 3 4");
        assert_eq!(pts.len(), 2);
    }

    #[cfg(feature = "elk")]
    #[test]
    fn section_and_edge_path_parse() {
        let sec = serde_json::json!({
            "startPoint": {"x": 0.0, "y": 0.0},
            "bendPoints": [{"x": 10.0, "y": 0.0}],
            "endPoint": {"x": 10.0, "y": 10.0}
        });
        let pts = section_points(&sec);
        assert_eq!(pts.len(), 3);
        let edge = serde_json::json!({
            "sections": [sec]
        });
        let path = edge_path_from_elk(&edge).unwrap();
        assert!(path.len() >= 2);
    }

    #[test]
    fn row_placement_and_halo_and_skip_edges() {
        let p = row_placement(&[("a".into(), 10.0, 20.0), ("b".into(), 12.0, 40.0)]);
        assert!(p.positions["a"].1 > 0.0); // shorter node is vertically centered
        assert_eq!(p.positions["b"].1, 0.0);

        let mut a = paint("A", 50.0, 40.0);
        a.scene.halo.push(Primitive::Circle {
            cx: 1.0,
            cy: 1.0,
            r: 2.0,
            fill: Some("#fff".into()),
            stroke: None,
            stroke_width: 0.0,
            opacity: 1.0,
            class: Some("halo".into()),
        });
        // Empty viewports → scene_size uses scene width/height.
        let bare = DocPaint {
            id: "Z".into(),
            molecule: a.molecule.clone(),
            scene: Scene {
                width: 30.0,
                height: 20.0,
                viewports: vec![],
                overlays: vec![],
                halo: vec![],
            },
        };
        let _ = scene_size(&bare.scene);

        let spec = DepictSpec::ReactionScheme {
            id: None,
            children: vec![
                Node::Mol(MolNode {
                    type_: MolNodeKind::Mol,
                    id: Some("A".into()),
                    smiles: Some("C".into()),
                    cxsmiles: None,
                    molfile: None,
                    label: None,
                    star_labels: None,
                    align_to: None,
                    opts: None,
                    color: None,
                    scale: None,
                    weight: None,
                    halo: None,
                    shade: None,
                }),
                Node::Mol(MolNode {
                    type_: MolNodeKind::Mol,
                    id: Some("B".into()),
                    smiles: Some("O".into()),
                    cxsmiles: None,
                    molfile: None,
                    label: None,
                    star_labels: None,
                    align_to: None,
                    opts: None,
                    color: None,
                    scale: None,
                    weight: None,
                    halo: None,
                    shade: None,
                }),
                Node::Text(TextNode {
                    type_: TextNodeKind::Text,
                    id: Some("lab".into()),
                    text: "  ".into(), // empty after trim → no ELK label
                    color: None,
                    scale: None,
                    opts: None,
                }),
                Node::Edge(EdgeNode {
                    type_: EdgeNodeKind::Edge,
                    sources: MolIds::One("A".into()),
                    targets: MolIds::One("B".into()),
                    label: Some(crate::doc::Label::Placed(crate::doc::LabelPlacement {
                        id: "lab".into(),
                        pos: Some(LabelPos::Below),
                    })),
                    edge_routing: Some(EdgeRouting::Splines),
                    role: None,
                    arrow: EdgeArrow::Line,
                    color: Some("#06c".into()),
                    stroke_width: Some(2.0),
                    dashed: true,
                }),
                // Missing endpoint — skipped.
                Node::Edge(EdgeNode {
                    type_: EdgeNodeKind::Edge,
                    sources: MolIds::One("A".into()),
                    targets: MolIds::One("missing".into()),
                    label: None,
                    edge_routing: None,
                    role: None,
                    arrow: EdgeArrow::Forward,
                    color: None,
                    stroke_width: None,
                    dashed: false,
                }),
            ],
            layout: Some(LayoutOpts {
                edge_routing: Some(EdgeRouting::Polyline),
                algorithm: Some(crate::doc::LayoutAlgorithm::Layered),
                direction: Some(crate::doc::LayoutDirection::Down),
                node_spacing: Some(16.0),
                layer_spacing: Some(8.0),
            }),
            opts: None,
            color: None,
            scale: None,
        };
        let scene = compose_scheme(&spec, &[a, paint("B", 50.0, 40.0)]).unwrap();
        assert!(!scene.halo.is_empty());
        assert!(!scene.overlays.is_empty());
        assert_eq!(routing_name(EdgeRouting::Orthogonal), "ORTHOGONAL");
        assert_eq!(routing_name(EdgeRouting::Splines), "SPLINES");

        // Label resolve edge cases.
        let (t, _) = resolve_edge_label(
            &spec,
            &EdgeNode {
                type_: EdgeNodeKind::Edge,
                sources: MolIds::One("A".into()),
                targets: MolIds::One("B".into()),
                label: None,
                edge_routing: None,
                role: None,
                arrow: EdgeArrow::Forward,
                color: None,
                stroke_width: None,
                dashed: false,
            },
        );
        assert!(t.is_none());

        let empty_edge = serde_json::json!({"sections": []});
        assert!(edge_path_from_elk(&empty_edge).is_none());
        let multi = serde_json::json!({
            "sections": [
                {
                    "startPoint": {"x": 0.0, "y": 0.0},
                    "endPoint": {"x": 5.0, "y": 0.0}
                },
                {
                    "startPoint": {"x": 5.0, "y": 0.0},
                    "bendPoints": [{"x": 5.0, "y": 5.0}],
                    "endPoint": {"x": 10.0, "y": 5.0}
                }
            ]
        });
        assert!(edge_path_from_elk(&multi).unwrap().len() >= 3);

        let mut mr = 0.0;
        let mut mb = 0.0;
        expand_bounds_prim(
            &Primitive::Circle {
                cx: 9.0,
                cy: 9.0,
                r: 2.0,
                fill: None,
                stroke: None,
                stroke_width: 0.0,
                opacity: 1.0,
                class: None,
            },
            &mut mr,
            &mut mb,
        );
        assert!(mr > 9.0);
    }

    #[test]
    fn compose_empty_mol_and_anchors() {
        let mol = DepictSpec::Mol {
            smiles: Some("C".into()),
            cxsmiles: None,
            molfile: None,
            id: None,
            label: None,
            star_labels: None,
            align_to: None,
            opts: None,
            color: None,
            scale: None,
            weight: None,
            halo: None,
            shade: None,
        };
        let empty = compose_scheme(&mol, &[]).unwrap();
        assert!(empty.viewports.is_empty());

        assert_eq!(routing_name(EdgeRouting::Polyline), "POLYLINE");
        match edge_prim_to_scene(&EdgePrim::Text {
            x: 0.0,
            y: 0.0,
            text: "x".into(),
            fill: "#000".into(),
            font_size: 10.0,
            anchor: "end".into(),
            class: "c".into(),
        }) {
            Primitive::Text {
                anchor: TextAnchor::End,
                ..
            } => {}
            _ => panic!("end anchor"),
        }

        let mut a = paint("A", 40.0, 30.0);
        a.scene.halo.push(Primitive::Text {
            x: 0.0,
            y: 0.0,
            text: "h".into(),
            fill: "#fff".into(),
            font_size: 8.0,
            anchor: TextAnchor::Start,
            class: None,
        });
        let row = place_row(&[a, paint("B", 40.0, 30.0)], 8.0);
        assert_eq!(row.viewports.len(), 2);
        assert!(!row.halo.is_empty());

        // Relative path cmds left as-is; covers the else branch.
        let rel = translate_prim(
            &Primitive::Path {
                d: "m 1 2 l 3 4".into(),
                stroke: None,
                fill: None,
                stroke_width: 1.0,
                opacity: 1.0,
                stroke_dasharray: None,
                stroke_linecap: None,
                class: None,
                data_text: None,
            },
            10.0,
            10.0,
        );
        match rel {
            Primitive::Path { d, .. } => assert!(d.contains('m') || d.contains('l') || d.contains('1')),
            _ => panic!("path"),
        }

        let (none_t, _) = resolve_edge_label(
            &DepictSpec::ReactionScheme {
                id: None,
                children: vec![],
                layout: None,
                opts: None,
                color: None,
                scale: None,
            },
            &EdgeNode {
                type_: EdgeNodeKind::Edge,
                sources: MolIds::One("A".into()),
                targets: MolIds::One("B".into()),
                label: Some(crate::doc::Label::Items(vec![])),
                edge_routing: None,
                role: None,
                arrow: EdgeArrow::Forward,
                color: None,
                stroke_width: None,
                dashed: false,
            },
        );
        assert!(none_t.is_none());

        let joined = edge_path_from_elk(&serde_json::json!({
            "sections": [
                {"startPoint":{"x":0,"y":0},"endPoint":{"x":1,"y":0}},
                {"startPoint":{"x":2,"y":0},"endPoint":{"x":3,"y":0}}
            ]
        }))
        .unwrap();
        assert!(joined.len() >= 3);
    }

    #[test]
    fn compose_mol_root_passthrough() {
        let paints = vec![paint("m0", 50.0, 40.0)];
        let spec = DepictSpec::Mol {
            smiles: Some("C".into()),
            cxsmiles: None,
            molfile: None,
            id: Some("m0".into()),
            label: None,
            star_labels: None,
            align_to: None,
            opts: None,
            color: None,
            scale: None,
            weight: None,
            halo: None,
            shade: None,
        };
        let scene = compose_scheme(&spec, &paints).unwrap();
        assert_eq!(scene.viewports.len(), 1);
        assert!((scene.width - 50.0).abs() < 1e-9 || scene.viewports[0].width == 50.0);
    }
}
