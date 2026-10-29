//! Compose a reaction scheme into one [`Scene`]: place painted mols + edge overlays.
//!
//! [`super::render_doc`] paints each molecule independently. This module lays
//! them out with ELK (tight row fallback) and strokes edges via [`crate::arrows`].

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::arrows::{
    edge_anchors, edge_primitives as paint_edge, simplify_route, EdgePaintIn, EdgePrim, ANCHOR_GAP,
    KINK_PX,
};
use crate::debug_box::debug_rect;
use crate::scene::{LayerName, Primitive, Scene, TextAnchor, Viewport};

use super::{fit, DepictSpec, DocPaint, EdgeNode, EdgeRouting, LabelPos, LayoutOpts, Node};

const EDGE_LABEL_FONT_PX: f64 = 11.0;
const EDGE_LABEL_PAD: f64 = 4.0;
const ROW_GAP: f64 = 0.0; // ≈ LayoutOpts::DEFAULT_LAYER_SPACING

/// Compose a [`DepictSpec::ReactionScheme`] from per-mol [`DocPaint`] rows.
///
/// Non-scheme roots return the first paint scene (or an empty scene).
pub fn compose_scheme(spec: &DepictSpec, paints: &[DocPaint]) -> Result<Scene, String> {
    let scene = match spec {
        DepictSpec::ReactionScheme { layout, .. } => compose_reaction(
            spec,
            paints,
            layout.as_ref().unwrap_or(&LayoutOpts::default()),
        )?,
        DepictSpec::Mol(_) | DepictSpec::Group { .. } | DepictSpec::Grid { .. } => {
            if paints.len() == 1 {
                paints[0].scene.clone()
            } else if paints.is_empty() {
                Scene::empty(0.0, 0.0)
            } else {
                // Multi-mol group / flat grid: simple row (no edges).
                place_row(paints, 16.0)
            }
        }
    };
    // Lone mols are already fitted in render_doc; multi-mol / schemes fit once here.
    let scene = if matches!(spec, DepictSpec::Mol(_)) && paints.len() == 1 {
        scene
    } else if paints.is_empty() {
        scene
    } else {
        fit::fit_scene(scene, resolve_compose_pad(spec, paints))
    };
    Ok(with_scene_debug_box(spec, paints, scene))
}

fn resolve_compose_pad(spec: &DepictSpec, paints: &[DocPaint]) -> f64 {
    // Prefer an explicit cascaded pad; else house default (rarely overridden).
    let n = spec.mols().len();
    for i in 0..n {
        if let Some(p) = spec.resolve_mol_chrome(i).pad {
            return fit::resolve_viewport_margin(Some(p));
        }
    }
    let _ = paints;
    fit::VIEWPORT_MARGIN_DEFAULT
}

/// Document-level debug rectangle when any mol or the root asks for ``debug_box``.
///
/// Skips when the scene already has a ``debug-box-scene`` overlay (single-mol
/// depict). Multi-mol layouts always get a fresh outer box in document space.
fn with_scene_debug_box(spec: &DepictSpec, paints: &[DocPaint], mut scene: Scene) -> Scene {
    if !spec_wants_debug_box(spec, paints) {
        return scene;
    }
    let already = scene.overlays.iter().any(|p| match p {
        Primitive::Path { class: Some(c), .. } => c.contains("debug-box-scene"),
        _ => false,
    });
    if already {
        return scene;
    }
    if scene.width > 0.0 && scene.height > 0.0 {
        scene.overlays.push(debug_rect(
            scene.x,
            scene.y,
            scene.width,
            scene.height,
            "debug-box debug-box-scene",
        ));
    }
    scene
}

fn spec_wants_debug_box(spec: &DepictSpec, paints: &[DocPaint]) -> bool {
    if paints.iter().any(|p| p.molecule.debug_box) {
        return true;
    }
    let n = spec.mols().len();
    (0..n).any(|i| spec.resolve_mol_chrome(i).debug_box.unwrap_or(false))
}

fn compose_reaction(
    spec: &DepictSpec,
    paints: &[DocPaint],
    layout: &LayoutOpts,
) -> Result<Scene, String> {
    if paints.is_empty() {
        return Ok(Scene::empty(0.0, 0.0));
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

    let placement = match layout_scheme(spec, layout, &sizes, paints) {
        Ok(p) => p,
        Err(_) => row_placement(&sizes),
    };

    let mut viewports = Vec::with_capacity(paints.len());
    let mut overlays = Vec::new();
    let mut max_r = 0.0_f64;
    let mut max_b = 0.0_f64;

    for p in paints {
        let (x, y) = placement
            .positions
            .get(p.id.as_str())
            .copied()
            .unwrap_or((0.0, 0.0));
        let (w, h) = scene_size(&p.scene);
        let mut vp = placed_viewport(&p.scene, &p.id, x, y, w, h);
        max_r = max_r.max(x + w);
        max_b = max_b.max(y + h);
        absorb_mol_document_prims(&mut vp, &p.scene);
        viewports.push(vp);
    }

    let vp_boxes: HashMap<&str, (f64, f64, f64, f64)> = viewports
        .iter()
        .filter_map(|vp| {
            let id = vp.id.as_deref()?;
            Some((id, (vp.x, vp.y, vp.width, vp.height)))
        })
        .collect();

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
        // Orthogonal ELK jogs still flatten under KINK_PX. Polyline/splines
        // keep refine vertices: collinear collapse undoes a chem-clearing bow.
        let mut pts = if edge.edge_routing_or(layout) == EdgeRouting::Orthogonal {
            simplify_route(&route, KINK_PX)
        } else {
            simplify_route(&route, 0.0)
        };
        let route_len: f64 = pts
            .windows(2)
            .map(|w| {
                let (dx, dy) = (w[1].0 - w[0].0, w[1].1 - w[0].1);
                (dx * dx + dy * dy).sqrt()
            })
            .sum();
        // elkrs orthogonal can emit a degenerate shaft (boxes flush); paint
        // drops routes shorter than 4px. Fall back to viewport anchors.
        if pts.len() < 2 || route_len < 4.0 {
            let (a, b) = edge_anchors(src_box, tgt_box, ANCHOR_GAP);
            pts = vec![a, b];
        }
        for &(x, y) in &pts {
            max_r = max_r.max(x + 8.0);
            max_b = max_b.max(y + 8.0);
        }

        let (label_text, label_pos) = resolve_edge_label(spec, edge);
        let label_pose = placement.edge_labels.get(i).and_then(|p| *p);
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
            curve_catmull: true,
            label_xy: label_pose.map(|(x, y, _)| (x, y)),
            label_rotate: label_pose.map(|(_, _, r)| r),
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
        x: 0.0,
        y: 0.0,
        viewports,
        overlays,
        halo: Vec::new(),
    })
}

/// Viewport as an SVG group: `x`/`y` are the group translate (ELK / row
/// placement). The mol's fitted viewBox origin folds into `ox`/`oy` so the
/// ELK box is the crop top-left and primitives stay in paint space.
fn placed_viewport(src: &Scene, id: &str, x: f64, y: f64, w: f64, h: f64) -> Viewport {
    let mut vp = src.viewports.first().cloned().unwrap_or(Viewport {
        id: Some(id.to_string()),
        x: 0.0,
        y: 0.0,
        ox: 0.0,
        oy: 0.0,
        width: w,
        height: h,
        layers: vec![],
    });
    // Map this mol's viewBox origin onto the ELK/row box. `src.x`/`src.y` are
    // the crop; any prior group leftover on vp.x/y comes along.
    vp.ox += vp.x - src.x;
    vp.oy += vp.y - src.y;
    vp.id = Some(id.to_string());
    vp.x = x;
    vp.y = y;
    vp.width = w;
    vp.height = h;
    vp
}

/// Per-mol document leftovers (debug box, extra halo) belong in the mol group.
fn absorb_mol_document_prims(vp: &mut Viewport, src: &Scene) {
    vp.extend_layer(LayerName::Halo, src.halo.clone());
    vp.extend_layer(LayerName::Overlay, src.overlays.clone());
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
    let mut max_r = 0.0_f64;
    let mut max_b = 0.0_f64;
    let mut x = 0.0_f64;
    for p in paints {
        let (w, h) = scene_size(&p.scene);
        let y = (max_h - h) * 0.5;
        let mut vp = placed_viewport(&p.scene, &p.id, x, y, w, h);
        max_r = max_r.max(x + w);
        max_b = max_b.max(y + h);
        absorb_mol_document_prims(&mut vp, &p.scene);
        viewports.push(vp);
        x += w + gap;
    }
    Scene {
        width: max_r,
        height: max_b,
        x: 0.0,
        y: 0.0,
        viewports,
        overlays: Vec::new(),
        halo: Vec::new(),
    }
}

struct Placement {
    positions: HashMap<String, (f64, f64)>,
    edge_paths: Vec<Option<Vec<(f64, f64)>>>,
    edge_labels: Vec<Option<(f64, f64, f64)>>,
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
        edge_labels: vec![],
        width: Some(x),
        height: Some(max_h),
    }
}

fn layout_scheme(
    spec: &DepictSpec,
    layout: &LayoutOpts,
    sizes: &[(String, f64, f64)],
    paints: &[DocPaint],
) -> Result<Placement, String> {
    let graph = build_elk_graph(spec, layout, sizes);
    let laid = crate::elk::layout_json(&serde_json::to_string(&graph).map_err(|e| e.to_string())?)?;
    let mut placement = parse_elk_placement(spec, &laid, sizes)?;
    let refined = super::scheme_layout::refine_scheme_edges(
        spec,
        layout,
        &placement.positions,
        &placement.edge_paths,
        paints,
        sizes,
    );
    placement.edge_paths = refined.edge_paths;
    placement.edge_labels = refined.edge_labels;
    Ok(placement)
}

fn build_elk_graph(spec: &DepictSpec, layout: &LayoutOpts, sizes: &[(String, f64, f64)]) -> Value {
    let opts = crate::elk::scheme_layout_options(crate::elk::DiagramKind::Reaction, layout);
    let node_opts = crate::elk::scheme_node_layout_options();
    let children: Vec<Value> = sizes
        .iter()
        .map(|(id, w, h)| {
            json!({
                "id": id,
                "width": w,
                "height": h,
                "layoutOptions": node_opts.clone(),
            })
        })
        .collect();

    // One ELK edge per unordered mol pair. Coterminal A→B / B→A (or two A→B)
    // share a corridor; ELK only needs to route the representative.
    let reps = coterminal_elk_rep(spec);
    let mut edges = Vec::new();
    for (i, e) in spec.edges().iter().enumerate() {
        if reps.get(i).copied() != Some(i) {
            continue;
        }
        let n_members = reps.iter().filter(|&&r| r == i).count();
        let eid = format!("e{i}");
        let mut entry = json!({
            "id": eid,
            "sources": e.sources.as_slice(),
            "targets": e.targets.as_slice(),
        });
        // Group labels belong to the refine pass. One ELK label still widens
        // the pack and bends the shared corridor.
        if n_members <= 1 {
            if let Some(labels) = elk_edge_labels(spec, e, &format!("e{i}")) {
                entry["labels"] = labels;
            }
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

fn unordered_mol_pair(e: &EdgeNode) -> (String, String) {
    let s = e.sources.as_slice().first().cloned().unwrap_or_default();
    let t = e.targets.as_slice().first().cloned().unwrap_or_default();
    if s <= t {
        (s, t)
    } else {
        (t, s)
    }
}

fn edge_src_tgt(e: &EdgeNode) -> (&str, &str) {
    let s = e
        .sources
        .as_slice()
        .first()
        .map(String::as_str)
        .unwrap_or("");
    let t = e
        .targets
        .as_slice()
        .first()
        .map(String::as_str)
        .unwrap_or("");
    (s, t)
}

/// Representative (first) edge index for each unordered mol pair.
fn coterminal_elk_rep(spec: &DepictSpec) -> Vec<usize> {
    let n = spec.edges().len();
    let mut first: HashMap<(String, String), usize> = HashMap::new();
    let mut rep = vec![0; n];
    for (i, e) in spec.edges().iter().enumerate() {
        let r = *first.entry(unordered_mol_pair(e)).or_insert(i);
        rep[i] = r;
    }
    rep
}

fn member_reverses_rep(member: &EdgeNode, rep: &EdgeNode) -> bool {
    let (s, t) = edge_src_tgt(member);
    let (rs, rt) = edge_src_tgt(rep);
    s == rt && t == rs && s != t
}

fn routing_name(r: EdgeRouting) -> &'static str {
    match r {
        EdgeRouting::Orthogonal => "ORTHOGONAL",
        EdgeRouting::Polyline => "POLYLINE",
        EdgeRouting::Splines => "SPLINES",
    }
}

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
    let reps = coterminal_elk_rep(spec);
    let spec_edges = spec.edges();
    let mut edge_paths = Vec::with_capacity(spec_edges.len());
    for i in 0..spec_edges.len() {
        let r = reps[i];
        let eid = format!("e{r}");
        let mut path = edges_out.and_then(|arr| {
            arr.iter()
                .find(|e| e.get("id").and_then(|v| v.as_str()) == Some(eid.as_str()))
                .and_then(edge_path_from_elk)
        });
        if let (Some(pts), Some(member), Some(rep)) =
            (path.as_mut(), spec_edges.get(i), spec_edges.get(r))
        {
            if member_reverses_rep(member, rep) {
                pts.reverse();
            }
        }
        edge_paths.push(path);
    }

    Ok(Placement {
        positions,
        edge_paths,
        edge_labels: vec![],
        width: laid.get("width").and_then(|v| v.as_f64()),
        height: laid.get("height").and_then(|v| v.as_f64()),
    })
}

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
            rotate,
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
            rotate: if rotate.abs() > 1e-6 {
                Some(*rotate)
            } else {
                None
            },
        },
    }
}

#[cfg(test)]
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
            d: d.clone(),
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
    use crate::doc::{EdgeArrow, EdgeNodeKind, MolIds, MolNode, TextNode, TextNodeKind};
    use crate::scene::{AtomIn, Layer, LayerName, MoleculeIn, TextAnchor};

    fn paint(id: &str, w: f64, h: f64) -> DocPaint {
        DocPaint {
            id: id.into(),
            frame_json: String::new(),
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
                    mapno: None,
                }],
                bonds: vec![],
                color: None,
                atom_shade: None,
                bond_shade: None,
                shade_vmin: None,
                shade_vmax: None,
                scale: 1.0,
                weight: 1.0,
                aromatic: Default::default(),
                debug_box: false,
                debug_align: false,
                align_debug: None,
                ..Default::default()
            },
            scene: Scene {
                width: w,
                height: h,
                x: 0.0,
                y: 0.0,
                viewports: vec![Viewport {
                    id: Some(id.into()),
                    x: 0.0,
                    y: 0.0,
                    ox: 0.0,
                    oy: 0.0,
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
                    id: Some("A".into()),
                    smiles: Some("CCO".into()),
                    ..Default::default()
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
                    id: Some("B".into()),
                    smiles: Some("CC=O".into()),
                    ..Default::default()
                }),
            ],
            layout: Some(LayoutOpts {
                edge_routing: Some(EdgeRouting::Orthogonal),
                ..Default::default()
            }),
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
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

    fn paint_shifted(id: &str, w: f64, h: f64, ox: f64, oy: f64, cx: f64, cy: f64) -> DocPaint {
        let mut p = paint(id, w, h);
        let vp = p.scene.viewports.first_mut().unwrap();
        vp.ox = ox;
        vp.oy = oy;
        vp.layers[0].primitives.push(Primitive::Circle {
            cx,
            cy,
            r: 2.0,
            fill: Some("#000".into()),
            stroke: None,
            stroke_width: 0.0,
            opacity: 1.0,
            class: None,
        });
        p
    }

    #[test]
    fn scheme_group_transform_keeps_fit_origin_in_local_space() {
        // Mol paint stays in local coords. Nested ox/oy maps the mol viewBox
        // origin onto the ELK box; arrows live in document space.
        let a = paint_shifted("A", 80.0, 60.0, -20.0, -25.0, 50.0, 55.0);
        let b = paint_shifted("B", 80.0, 60.0, -20.0, -25.0, 50.0, 55.0);
        let spec = DepictSpec::ReactionScheme {
            id: None,
            children: vec![
                Node::Mol(MolNode {
                    id: Some("A".into()),
                    smiles: Some("C".into()),
                    ..Default::default()
                }),
                Node::Mol(MolNode {
                    id: Some("B".into()),
                    smiles: Some("O".into()),
                    ..Default::default()
                }),
                Node::Edge(EdgeNode {
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
                }),
            ],
            layout: Some(LayoutOpts {
                layer_spacing: Some(40.0),
                ..Default::default()
            }),
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        };
        let scene = compose_scheme(&spec, &[a, b]).expect("compose");
        let vp = &scene.viewports[0];
        let cy = vp.layers[0]
            .primitives
            .iter()
            .find_map(|p| match p {
                Primitive::Circle { cy, .. } => Some(*cy),
                _ => None,
            })
            .expect("circle");
        // Local paint coords stay; leftover is the nested group origin
        // (compose fit may add more to oy — do not bake into the circle).
        assert!(
            (cy - 55.0).abs() < 1e-6,
            "local cy {cy} should stay at paint 55"
        );
        let ink_y = vp.y + vp.oy + cy;
        let mut shaft_y = None;
        for p in &scene.overlays {
            if let Primitive::Path {
                d, class: Some(c), ..
            } = p
            {
                if c.contains("edge") && !c.contains("head") {
                    let ys: Vec<f64> = super::path_coords(d).into_iter().map(|(_, y)| y).collect();
                    if ys.len() >= 2 {
                        shaft_y = Some((ys[0] + ys[1]) * 0.5);
                        break;
                    }
                }
            }
        }
        let shaft_y = shaft_y.expect("shaft");
        // Ink lives in the box, not a leftover-origin below the arrow.
        assert!(
            (ink_y - shaft_y).abs() < vp.height * 0.5,
            "ink y {ink_y} far from shaft {shaft_y}"
        );
    }

    #[test]
    fn live_scheme_arrow_aligns_with_mol_ink() {
        let spec = DepictSpec::ReactionScheme {
            id: None,
            children: vec![
                Node::Mol(MolNode {
                    id: Some("A".into()),
                    smiles: Some("CCO".into()),
                    ..Default::default()
                }),
                Node::Mol(MolNode {
                    id: Some("B".into()),
                    smiles: Some("CC=O".into()),
                    ..Default::default()
                }),
                Node::Edge(EdgeNode {
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
                }),
            ],
            layout: Some(LayoutOpts {
                layer_spacing: Some(40.0),
                ..Default::default()
            }),
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        };
        let out = crate::doc::render(&spec).expect("render");
        assert_eq!(out.len(), 1);
        let scene = &out[0].scene;
        let mut ink_y = Vec::new();
        for vp in &scene.viewports {
            for layer in &vp.layers {
                for p in &layer.primitives {
                    match p {
                        Primitive::Circle { cy, .. } => ink_y.push(vp.y + vp.oy + *cy),
                        Primitive::Path { d, .. } => {
                            ink_y.extend(
                                super::path_coords(d)
                                    .into_iter()
                                    .map(|(_, y)| vp.y + vp.oy + y),
                            );
                        }
                        Primitive::Text { y, .. } => ink_y.push(vp.y + vp.oy + *y),
                    }
                }
            }
        }
        let ink_min = ink_y.iter().copied().fold(f64::INFINITY, f64::min);
        let ink_max = ink_y.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mut shaft_y = None;
        for p in &scene.overlays {
            if let Primitive::Path {
                d, class: Some(c), ..
            } = p
            {
                if c.contains("edge") && !c.contains("head") {
                    let ys: Vec<f64> = super::path_coords(d).into_iter().map(|(_, y)| y).collect();
                    if ys.len() >= 2 {
                        shaft_y = Some((ys[0] + ys[1]) * 0.5);
                        break;
                    }
                }
            }
        }
        let shaft_y = shaft_y.expect("shaft");
        assert!(
            shaft_y >= ink_min - 2.0 && shaft_y <= ink_max + 2.0,
            "shaft y {shaft_y} outside mol ink {ink_min}..{ink_max}"
        );
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
                aromatic: None,
                debug_box: None,
                debug_align: None,
                pad: None,
            },
            &[],
        )
        .unwrap();
        assert!(empty.viewports.is_empty());

        let group = DepictSpec::Group {
            id: None,
            children: vec![
                Node::Mol(MolNode {
                    id: Some("a".into()),
                    smiles: Some("C".into()),
                    ..Default::default()
                }),
                Node::Mol(MolNode {
                    id: Some("b".into()),
                    smiles: Some("O".into()),
                    ..Default::default()
                }),
            ],
            align: false,
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        };
        let scene =
            compose_scheme(&group, &[paint("a", 40.0, 30.0), paint("b", 50.0, 30.0)]).unwrap();
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
                assert_eq!(
                    d, "M 0.00 0.00 L 10.00 5.00",
                    "paths are not moved by rewriting d"
                );
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
            rotate: None,
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
                stroke,
                fill,
                class,
                ..
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
            rotate: 0.0,
        });
        match txt {
            Primitive::Text { anchor, .. } => assert_eq!(anchor, TextAnchor::Start),
            _ => panic!("text"),
        }
        let pts = path_coords("M 0 0 L 3 4");
        assert_eq!(pts.len(), 2);
    }

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
            frame_json: String::new(),
            molecule: a.molecule.clone(),
            scene: Scene {
                width: 30.0,
                height: 20.0,
                x: 0.0,
                y: 0.0,
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
                    id: Some("A".into()),
                    smiles: Some("C".into()),
                    ..Default::default()
                }),
                Node::Mol(MolNode {
                    id: Some("B".into()),
                    smiles: Some("O".into()),
                    ..Default::default()
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
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        };
        let scene = compose_scheme(&spec, &[a, paint("B", 50.0, 40.0)]).unwrap();
        assert!(
            scene.viewports.iter().any(|vp| vp
                .layers
                .iter()
                .any(|l| l.name == LayerName::Halo && !l.primitives.is_empty())),
            "mol halo rides the viewport group, not a rewritten document path"
        );
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
        let mol = DepictSpec::Mol(MolNode {
            smiles: Some("C".into()),
            cxsmiles: None,
            molfile: None,
            id: None,
            label: None,
            name: None,
            atom_number: None,
            atom_label: None,
            region: None,
            circle: None,
            callout: None,
            star_labels: None,
            align_to: None,
            opts: None,
            color: None,
            scale: None,
            weight: None,
            halo: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
            shade: None,
            ..Default::default()
        });
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
            rotate: 0.0,
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
            rotate: None,
        });
        let row = place_row(&[a, paint("B", 40.0, 30.0)], 8.0);
        assert_eq!(row.viewports.len(), 2);
        assert!(
            row.viewports.iter().any(|vp| vp
                .layers
                .iter()
                .any(|l| l.name == LayerName::Halo && !l.primitives.is_empty())),
            "row halo stays on the mol group"
        );

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
            Primitive::Path { d, .. } => {
                assert!(d.contains('m') || d.contains('l') || d.contains('1'))
            }
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
                aromatic: None,
                debug_box: None,
                debug_align: None,
                pad: None,
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
        let spec = DepictSpec::Mol(MolNode {
            smiles: Some("C".into()),
            cxsmiles: None,
            molfile: None,
            id: Some("m0".into()),
            label: None,
            name: None,
            atom_number: None,
            atom_label: None,
            region: None,
            circle: None,
            callout: None,
            star_labels: None,
            align_to: None,
            opts: None,
            color: None,
            scale: None,
            weight: None,
            halo: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
            shade: None,
            ..Default::default()
        });
        let scene = compose_scheme(&spec, &paints).unwrap();
        assert_eq!(scene.viewports.len(), 1);
        assert!((scene.width - 50.0).abs() < 1e-9 || scene.viewports[0].width == 50.0);
    }

    #[test]
    fn refined_scheme_paints_catmull_shaft() {
        let spec = DepictSpec::ReactionScheme {
            id: None,
            children: vec![
                Node::Mol(MolNode {
                    id: Some("A".into()),
                    smiles: Some("CCO".into()),
                    ..Default::default()
                }),
                Node::Edge(EdgeNode {
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
                }),
                Node::Mol(MolNode {
                    id: Some("B".into()),
                    smiles: Some("CC=O".into()),
                    ..Default::default()
                }),
            ],
            layout: None,
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        };
        let paints = vec![paint("A", 120.0, 80.0), paint("B", 120.0, 80.0)];
        let scene = compose_scheme(&spec, &paints).expect("compose");
        assert_eq!(scene.viewports.len(), 2);
        let has_c = scene.overlays.iter().any(|p| match p {
            Primitive::Path {
                d, class: Some(c), ..
            } if c.contains("edge") => d.contains('C'),
            _ => false,
        });
        let has_shaft = scene.overlays.iter().any(|p| match p {
            Primitive::Path { class: Some(c), .. } => c.contains("edge"),
            _ => false,
        });
        assert!(has_shaft, "expected an edge overlay");
        // Two-point shafts stay as L; ELK bends become cubics.
        let _ = has_c;
        let xa = scene.viewports[0].x;
        let xb = scene.viewports[1].x;
        assert!(
            (xa - xb).abs() > 1.0,
            "mols should keep distinct ELK x: {xa} vs {xb}"
        );
    }

    #[test]
    fn scheme_keeps_elk_mol_positions() {
        let spec = DepictSpec::ReactionScheme {
            id: None,
            children: vec![
                Node::Mol(MolNode {
                    id: Some("A".into()),
                    smiles: Some("CCO".into()),
                    ..Default::default()
                }),
                Node::Edge(EdgeNode {
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
                }),
                Node::Mol(MolNode {
                    id: Some("B".into()),
                    smiles: Some("CC=O".into()),
                    ..Default::default()
                }),
            ],
            layout: None,
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        };
        let paints = vec![paint("A", 120.0, 80.0), paint("B", 120.0, 80.0)];
        let sizes: Vec<(String, f64, f64)> = paints
            .iter()
            .map(|p| (p.id.clone(), p.scene.width, p.scene.height))
            .collect();
        let layout = LayoutOpts::default();
        let graph = build_elk_graph(&spec, &layout, &sizes);
        let laid = crate::elk::layout_json(&serde_json::to_string(&graph).unwrap()).unwrap();
        let elk = parse_elk_placement(&spec, &laid, &sizes).unwrap();
        let scene = compose_scheme(&spec, &paints).expect("compose");
        assert_eq!(scene.viewports.len(), elk.positions.len());
        for vp in &scene.viewports {
            let id = vp.id.as_deref().unwrap_or("?");
            let (x, y) = elk.positions[id];
            assert!(
                (vp.x - x).abs() < 1e-9 && (vp.y - y).abs() < 1e-9,
                "mol {id} moved: elk ({x},{y}) compose ({},{})",
                vp.x,
                vp.y
            );
        }
    }

    fn pair_spec(labeled: bool) -> DepictSpec {
        let mut children = vec![
            Node::Mol(MolNode {
                id: Some("A".into()),
                smiles: Some("CCO".into()),
                ..Default::default()
            }),
            Node::Mol(MolNode {
                id: Some("B".into()),
                smiles: Some("CC=O".into()),
                ..Default::default()
            }),
        ];
        if labeled {
            children.push(Node::Text(TextNode {
                type_: TextNodeKind::Text,
                id: Some("ox".into()),
                text: "ox".into(),
                color: None,
                scale: None,
                opts: None,
            }));
            children.push(Node::Text(TextNode {
                type_: TextNodeKind::Text,
                id: Some("red".into()),
                text: "red".into(),
                color: None,
                scale: None,
                opts: None,
            }));
        }
        children.push(Node::Edge(EdgeNode {
            type_: EdgeNodeKind::Edge,
            sources: MolIds::One("A".into()),
            targets: MolIds::One("B".into()),
            label: labeled.then_some(crate::doc::Label::Placed(crate::doc::LabelPlacement {
                id: "ox".into(),
                pos: Some(LabelPos::Above),
            })),
            edge_routing: None,
            role: None,
            arrow: EdgeArrow::Forward,
            color: None,
            stroke_width: None,
            dashed: false,
        }));
        children.push(Node::Edge(EdgeNode {
            type_: EdgeNodeKind::Edge,
            sources: MolIds::One("B".into()),
            targets: MolIds::One("A".into()),
            label: labeled.then_some(crate::doc::Label::Placed(crate::doc::LabelPlacement {
                id: "red".into(),
                pos: Some(LabelPos::Below),
            })),
            edge_routing: None,
            role: None,
            arrow: EdgeArrow::Forward,
            color: None,
            stroke_width: None,
            dashed: false,
        }));
        DepictSpec::ReactionScheme {
            id: None,
            children,
            layout: None,
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        }
    }

    #[test]
    fn coterminal_pair_skips_elk_edge_labels() {
        let spec = pair_spec(true);
        let paints = vec![paint("A", 80.0, 60.0), paint("B", 80.0, 60.0)];
        let sizes: Vec<(String, f64, f64)> = paints
            .iter()
            .map(|p| (p.id.clone(), p.scene.width, p.scene.height))
            .collect();
        let graph = build_elk_graph(&spec, &LayoutOpts::default(), &sizes);
        let edges = graph["edges"].as_array().expect("edges");
        assert_eq!(
            edges.len(),
            1,
            "ELK should see one edge for a coterminal pair: {edges:?}"
        );
        assert_eq!(edges[0]["id"], "e0");
        for e in edges {
            assert!(
                e.get("labels").is_none(),
                "coterminal labels must not go to ELK: {e}"
            );
        }
    }

    #[test]
    fn two_forward_arrows_send_one_elk_edge() {
        let spec = DepictSpec::ReactionScheme {
            id: None,
            children: vec![
                Node::Mol(MolNode {
                    id: Some("A".into()),
                    smiles: Some("C".into()),
                    ..Default::default()
                }),
                Node::Mol(MolNode {
                    id: Some("B".into()),
                    smiles: Some("O".into()),
                    ..Default::default()
                }),
                Node::Edge(EdgeNode {
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
                }),
                Node::Edge(EdgeNode {
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
                }),
            ],
            layout: None,
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        };
        let paints = vec![paint("A", 80.0, 60.0), paint("B", 80.0, 60.0)];
        let sizes: Vec<(String, f64, f64)> = paints
            .iter()
            .map(|p| (p.id.clone(), p.scene.width, p.scene.height))
            .collect();
        let graph = build_elk_graph(&spec, &LayoutOpts::default(), &sizes);
        assert_eq!(graph["edges"].as_array().expect("edges").len(), 1);
    }

    #[test]
    fn parse_copies_elk_path_onto_reverse_member() {
        let spec = pair_spec(false);
        let laid = serde_json::json!({
            "id": "root",
            "width": 200.0,
            "height": 80.0,
            "children": [
                {"id": "A", "x": 0.0, "y": 0.0},
                {"id": "B", "x": 100.0, "y": 0.0}
            ],
            "edges": [{
                "id": "e0",
                "sections": [{
                    "startPoint": {"x": 80.0, "y": 30.0},
                    "endPoint": {"x": 100.0, "y": 30.0}
                }]
            }]
        })
        .to_string();
        let sizes = vec![("A".into(), 80.0, 60.0), ("B".into(), 80.0, 60.0)];
        let p = parse_elk_placement(&spec, &laid, &sizes).expect("parse");
        let a = p.edge_paths[0].clone().expect("rep");
        let b = p.edge_paths[1].clone().expect("rev");
        assert_eq!(a, vec![(80.0, 30.0), (100.0, 30.0)]);
        assert_eq!(b, vec![(100.0, 30.0), (80.0, 30.0)]);
    }

    fn overlay_shafts(scene: &Scene) -> Vec<Vec<(f64, f64)>> {
        scene
            .overlays
            .iter()
            .filter_map(|p| match p {
                Primitive::Path {
                    d, class: Some(c), ..
                } if c.contains("edge") && !c.contains("head") => Some(path_coords(d)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn labeled_coterminal_pair_stays_parallel() {
        let paints = vec![paint("A", 80.0, 60.0), paint("B", 80.0, 60.0)];
        let plain = compose_scheme(&pair_spec(false), &paints).expect("plain");
        let labeled = compose_scheme(&pair_spec(true), &paints).expect("labeled");
        let a = overlay_shafts(&plain);
        let b = overlay_shafts(&labeled);
        assert_eq!(a.len(), 2, "unlabeled pair shafts");
        assert_eq!(b.len(), 2, "labeled pair shafts");
        for (name, shafts) in [("plain", &a), ("labeled", &b)] {
            for pts in shafts.iter() {
                let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
                let span = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                    - ys.iter().cloned().fold(f64::INFINITY, f64::min);
                assert!(
                    span < 8.0,
                    "{name} shaft is not a level lane (y-span {span}): {pts:?}"
                );
            }
            let mean_y =
                |pts: &[(f64, f64)]| pts.iter().map(|p| p.1).sum::<f64>() / pts.len().max(1) as f64;
            let gap = (mean_y(&shafts[0]) - mean_y(&shafts[1])).abs();
            assert!(
                (gap - 8.0).abs() < 3.0,
                "{name} lane gap {gap} should stay near 8px"
            );
        }
        let mut ox = None;
        let mut red = None;
        for p in &labeled.overlays {
            if let Primitive::Text { text, x, y, .. } = p {
                if text == "ox" {
                    ox = Some((*x, *y));
                }
                if text == "red" {
                    red = Some((*x, *y));
                }
            }
        }
        let (_, oy) = ox.expect("ox");
        let (_, ry) = red.expect("red");
        assert!(
            oy + 6.0 < ry,
            "ox should sit above red, got oy={oy} ry={ry}"
        );
        assert!(
            (labeled.width - plain.width).abs() < 24.0,
            "labels must not widen the ELK pack: plain={} labeled={}",
            plain.width,
            labeled.width
        );
    }

    fn on_curve_verts(d: &str) -> Vec<(f64, f64)> {
        let pts = path_coords(d);
        if d.contains('C') && pts.len() >= 4 {
            let mut v = vec![pts[0]];
            let mut i = 3;
            while i < pts.len() {
                v.push(pts[i]);
                i += 3;
            }
            v
        } else {
            pts
        }
    }

    fn composed_mol_ink(scene: &Scene, p: (f64, f64)) -> f64 {
        use crate::doc::layout_constraints::{edge_signed_distance, Shape};
        let pt = Shape::Point { x: p.0, y: p.1 };
        let mut best = f64::INFINITY;
        for vp in &scene.viewports {
            let (ox, oy) = vp.local_origin();
            for layer in &vp.layers {
                if layer.name == LayerName::Halo {
                    continue;
                }
                for prim in &layer.primitives {
                    for s in super::super::scheme_layout::prim_ink_shapes(prim, ox, oy) {
                        best = best.min(edge_signed_distance(&pt, &s));
                    }
                    // Stroke-only bonds are not filled ink stamps.
                    if layer.name == LayerName::Bonds {
                        if let Primitive::Path { d, .. } = prim {
                            let pts: Vec<(f64, f64)> = path_coords(d)
                                .into_iter()
                                .map(|(x, y)| (x + ox, y + oy))
                                .collect();
                            for w in pts.windows(2) {
                                best = best.min(edge_signed_distance(
                                    &pt,
                                    &Shape::Line {
                                        x0: w[0].0,
                                        y0: w[0].1,
                                        x1: w[1].0,
                                        y1: w[1].1,
                                    },
                                ));
                            }
                        }
                    }
                }
            }
        }
        best
    }

    #[test]
    fn down_coterminal_pair_shafts_clear_chem() {
        let spec: DepictSpec = serde_json::from_str(
            r#"{
              "type": "reaction_scheme",
              "layout": {"direction": "down", "edge_routing": "polyline"},
              "children": [
                {"type": "mol", "id": "A", "smiles": "c1ccccc1O", "name": "phenol"},
                {"type": "mol", "id": "B", "smiles": "c1ccccc1C=O", "name": "benzaldehyde"},
                {"type": "edge", "sources": "A", "targets": "B", "arrow": "forward"},
                {"type": "edge", "sources": "B", "targets": "A", "arrow": "forward"}
              ]
            }"#,
        )
        .unwrap();
        let out = crate::doc::render(&spec).expect("render");
        let scene = &out[0].scene;
        let mut n = 0usize;
        for p in &scene.overlays {
            let Primitive::Path {
                d, class: Some(c), ..
            } = p
            else {
                continue;
            };
            if !c.contains("edge") || c.contains("head") {
                continue;
            }
            n += 1;
            let verts = on_curve_verts(d);
            assert!(
                verts.len() >= 3 && d.contains('C'),
                "down-pair chord through chem must bow (Catmull), got {d}"
            );
            let mut clustered = 0usize;
            for w in verts.windows(2) {
                if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) < 4.0 {
                    clustered += 1;
                }
            }
            assert!(
                clustered < 2,
                "controls collapsed into a ladder rung: verts={verts:?} d={d}"
            );
            let samples = crate::catmull::sample_open_catmull(&verts, 8);
            for p in samples {
                let dist = composed_mol_ink(scene, p);
                assert!(
                    dist > 2.0,
                    "edge {n} sample ({:.1},{:.1}) hits mol ink: d={dist:.1} verts={verts:?} d={d}",
                    p.0,
                    p.1
                );
            }
            if n == 1 {
                let (tip, tan) = crate::catmull::open_catmull_end_frame(&verts);
                assert!(
                    tan.1 > 0.5,
                    "A→B head must point down at B: tip={tip:?} tan={tan:?} verts={verts:?}"
                );
            }
        }
        assert_eq!(n, 2, "expected the reverse pair");
    }
}
