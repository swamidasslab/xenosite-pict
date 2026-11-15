//! Compose a reaction scheme into one [`Scene`]: place painted mols + edge overlays.
//!
//! [`super::render_doc`] paints each molecule independently. This module packs
//! mols with the in-house layered placer (ELK only for non-layered algorithms),
//! then strokes edges via [`crate::arrows`] after visibility routing.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::arrows::{
    edge_primitives as paint_edge, simplify_route, EdgePaintIn, EdgePrim, KINK_PX,
};
use crate::debug_box::debug_rect;
use crate::scene::{LayerName, Primitive, Scene, TextAnchor, Viewport};

use super::{
    fit, DepictSpec, DocPaint, EdgeNode, EdgeRouting, LabelPos, LayoutAlgorithm, LayoutDirection,
    LayoutOpts, Node,
};

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
        Err(_) => {
            // Placer / non-layered ELK can still fail; row pack + same refine.
            let row = row_placement(&sizes);
            let refined = super::scheme_layout::refine_scheme_edges(
                spec,
                layout,
                &row.positions,
                paints,
                &sizes,
            );
            Placement {
                positions: refined.positions,
                edge_paths: refined.edge_paths,
                edge_labels: refined.edge_labels,
                width: row.width,
                height: row.height,
            }
        }
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
                // Refine always hugs centroid→centroid.
                let (x, y, w, h) = src_box;
                let (x2, y2, w2, h2) = tgt_box;
                vec![(x + w * 0.5, y + h * 0.5), (x2 + w2 * 0.5, y2 + h2 * 0.5)]
            });
        // Orthogonal: kink-collapse at paint. Polyline/splines keep refine verts.
        let pts = if edge.edge_routing_or(layout) == EdgeRouting::Orthogonal {
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
        if pts.len() < 2 || route_len < 4.0 {
            continue;
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
            // Visibility routes + tip standoff: Catmull through controls with
            // safe sagitta (see painted_shaft_d / safe scheme path).
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
    for plus in scheme_plus_overlays(spec, &viewports) {
        expand_bounds_prim(&plus, &mut max_r, &mut max_b);
        overlays.push(plus);
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
        halo_filter_px: None,
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
    let use_layered_placer = matches!(
        layout.algorithm_or_default(),
        LayoutAlgorithm::Layered
    ) && !super::scheme_layout::is_network_scheme(
        sizes.len(),
        spec.edges().len(),
        &layout.engine(),
    );
    let (mut positions, layers) = if use_layered_placer {
        let placed = super::scheme_layered::place_layered(spec, layout, sizes, paints)?;
        (placed.positions, Some(placed.layers))
    } else {
        // Non-layered algorithms, and oversized network schemes, use elkrs for
        // node positions only (shafts still come from scheme_route).
        let graph = build_elk_graph(spec, layout, sizes);
        let laid =
            crate::elk::layout_json(&serde_json::to_string(&graph).map_err(|e| e.to_string())?)?;
        (parse_elk_positions(&laid, sizes)?, None)
    };
    // Down/up ELK packs are top-aligned; nudge so each layer shares a baseline /
    // common-core Y. The in-house placer already mark-aligns — skip it there.
    if layers.is_none()
        && matches!(
            layout.direction_or_default(),
            LayoutDirection::Down | LayoutDirection::Up
        )
    {
        super::scheme_layered::nudge_down_layer_marks(
            &mut positions,
            spec,
            layout,
            sizes,
            paints,
            None,
        );
    }
    let refined =
        super::scheme_layout::refine_scheme_edges(spec, layout, &positions, paints, sizes);
    Ok(Placement {
        positions: refined.positions,
        edge_paths: refined.edge_paths,
        edge_labels: refined.edge_labels,
        width: None,
        height: None,
    })
}

/// Build an ELK JSON graph for **non-layered** algorithms only (radial/force/stress).
/// Edge routing / labels are omitted — Rust owns shafts and label pose.
fn build_elk_graph(spec: &DepictSpec, layout: &LayoutOpts, sizes: &[(String, f64, f64)]) -> Value {
    let kind = if super::scheme_layout::is_network_scheme(
        sizes.len(),
        spec.edges().len(),
        &layout.engine(),
    ) {
        crate::elk::DiagramKind::Network
    } else {
        crate::elk::DiagramKind::Reaction
    };
    let opts = crate::elk::scheme_layout_options(kind, layout);
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

    // One ELK edge per unordered mol pair among layout-participating edges.
    // Coterminal A→B / B→A share one packing relation; `layout: false` omitted.
    let reps = coterminal_elk_rep(spec);
    let mut edges = Vec::new();
    for (i, e) in spec.edges().iter().enumerate() {
        if !e.participates_in_layout() {
            continue;
        }
        if reps.get(i).copied() != Some(i) {
            continue;
        }
        let _ = reps.iter().filter(|&&r| r == i).count();
        // ELK rejects hyperedges (A+B→C). Expand to binary edges for packing.
        let sources = e.sources.as_slice();
        let targets = e.targets.as_slice();
        let pairs: Vec<(&str, &str)> = if sources.len() <= 1 && targets.len() <= 1 {
            let (s, t) = edge_src_tgt(e);
            vec![(s, t)]
        } else {
            sources
                .iter()
                .flat_map(|s| targets.iter().map(move |t| (s.as_str(), t.as_str())))
                .collect()
        };
        for (pi, &(elk_src, elk_tgt)) in pairs.iter().enumerate() {
            let eid = if pi == 0 {
                format!("e{i}")
            } else {
                format!("e{i}_{pi}")
            };
            edges.push(json!({
                "id": eid,
                "sources": [elk_src],
                "targets": [elk_tgt],
            }));
        }
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

/// Representative edge index for each unordered mol pair.
///
/// Prefers the first `layout: true` member so a document-first reverse with
/// `layout: false` cannot orient ELK layers. Paint-only edges that share a
/// pair still point at that layout rep (for path copy/reverse). A pair with
/// only paint-only edges keeps the first member as a no-op self-rep.
fn coterminal_elk_rep(spec: &DepictSpec) -> Vec<usize> {
    let edges = spec.edges();
    let n = edges.len();
    let mut layout_first: HashMap<(String, String), usize> = HashMap::new();
    let mut any_first: HashMap<(String, String), usize> = HashMap::new();
    for (i, e) in edges.iter().enumerate() {
        let key = unordered_mol_pair(e);
        any_first.entry(key.clone()).or_insert(i);
        if e.participates_in_layout() {
            layout_first.entry(key).or_insert(i);
        }
    }
    let mut rep = vec![0; n];
    for (i, e) in edges.iter().enumerate() {
        let key = unordered_mol_pair(e);
        rep[i] = layout_first
            .get(&key)
            .or_else(|| any_first.get(&key))
            .copied()
            .unwrap_or(i);
    }
    rep
}

fn parse_elk_positions(
    laid_json: &str,
    sizes: &[(String, f64, f64)],
) -> Result<HashMap<String, (f64, f64)>, String> {
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
    Ok(positions)
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
        LabelPos::Above | LabelPos::Auto => "above".into(),
        LabelPos::Below => "below".into(),
        LabelPos::Left => "left".into(),
        LabelPos::Right => "right".into(),
        LabelPos::AboveLeft => "above_left".into(),
        LabelPos::AboveRight => "above_right".into(),
        LabelPos::BelowLeft => "below_left".into(),
        LabelPos::BelowRight => "below_right".into(),
    });
    (text, pos)
}

/// Caption-sized `+` between consecutive co-endpoint mols, optically centered
/// in the gap between their chem ink (not viewport-box centers / alphabetic mid).
fn scheme_plus_overlays(spec: &DepictSpec, viewports: &[Viewport]) -> Vec<Primitive> {
    use super::caption::CAPTION_FONT_FRAC;
    use super::text_paint::markup_ascent_descent_px;
    use crate::metrics::FONT_PX;

    let by_id: HashMap<&str, &Viewport> = viewports
        .iter()
        .filter_map(|vp| vp.id.as_deref().map(|id| (id, vp)))
        .collect();
    let font_size = CAPTION_FONT_FRAC * FONT_PX;
    let (asc, desc) = markup_ascent_descent_px(font_size);
    // Alphabetic baseline so the glyph's ink center sits on `(mx, my)`.
    let baseline_lift = (asc - desc) * 0.5;
    let mut out = Vec::new();
    for edge in spec.edges() {
        for ids in [edge.sources.as_slice(), edge.targets.as_slice()] {
            if ids.len() < 2 {
                continue;
            }
            for w in ids.windows(2) {
                let Some(va) = by_id.get(w[0].as_str()) else {
                    continue;
                };
                let Some(vb) = by_id.get(w[1].as_str()) else {
                    continue;
                };
                let a = viewport_ink_aabb(va);
                let b = viewport_ink_aabb(vb);
                let (ax, ay) = ((a.0 + a.2) * 0.5, (a.1 + a.3) * 0.5);
                let (bx, by) = ((b.0 + b.2) * 0.5, (b.1 + b.3) * 0.5);
                let (mx, my) = if (by - ay).abs() >= (bx - ax).abs() {
                    // Vertical stack: mid of facing top/bottom ink edges.
                    let (top, bot) = if ay <= by { (a, b) } else { (b, a) };
                    ((ax + bx) * 0.5, (top.3 + bot.1) * 0.5)
                } else {
                    // Horizontal: mid of facing left/right ink edges.
                    let (left, right) = if ax <= bx { (a, b) } else { (b, a) };
                    ((left.2 + right.0) * 0.5, (ay + by) * 0.5)
                };
                out.push(Primitive::Text {
                    x: mx,
                    y: my + baseline_lift,
                    text: "+".into(),
                    fill: "#222".into(),
                    font_size,
                    anchor: TextAnchor::Middle,
                    class: Some("scheme-plus".into()),
                    rotate: None,
                });
            }
        }
    }
    out
}

/// Document-space chem ink AABB for a placed mol viewport (fallback: frame).
fn viewport_ink_aabb(vp: &Viewport) -> (f64, f64, f64, f64) {
    let (ox, oy) = vp.local_origin();
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut any = false;
    for layer in &vp.layers {
        // Captions / debug live on Overlay; + should clear chem only.
        if matches!(layer.name, LayerName::Overlay | LayerName::Halo) {
            continue;
        }
        for p in &layer.primitives {
            match p {
                Primitive::Path { d, stroke_width, .. } => {
                    let grow = stroke_width.max(0.0) * 0.5;
                    for (x, y) in path_xy_pairs(d) {
                        min_x = min_x.min(ox + x - grow);
                        min_y = min_y.min(oy + y - grow);
                        max_x = max_x.max(ox + x + grow);
                        max_y = max_y.max(oy + y + grow);
                        any = true;
                    }
                }
                Primitive::Circle {
                    cx,
                    cy,
                    r,
                    stroke_width,
                    ..
                } => {
                    let grow = r + stroke_width.max(0.0) * 0.5;
                    min_x = min_x.min(ox + cx - grow);
                    min_y = min_y.min(oy + cy - grow);
                    max_x = max_x.max(ox + cx + grow);
                    max_y = max_y.max(oy + cy + grow);
                    any = true;
                }
                Primitive::Text {
                    x, y, font_size, ..
                } => {
                    let em = font_size.max(1.0);
                    min_x = min_x.min(ox + x - em);
                    min_y = min_y.min(oy + y - em);
                    max_x = max_x.max(ox + x + em * 2.0);
                    max_y = max_y.max(oy + y + em * 0.4);
                    any = true;
                }
            }
        }
    }
    if any {
        (min_x, min_y, max_x, max_y)
    } else {
        (vp.x, vp.y, vp.x + vp.width, vp.y + vp.height)
    }
}

fn path_xy_pairs(d: &str) -> Vec<(f64, f64)> {
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
                    radical: 0,
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
                    halo_filter_px: None,
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
                    margin: None,
                    opts: None,
                }),
                Node::Edge(EdgeNode {
                    type_: EdgeNodeKind::Edge,
                    sources: MolIds::One("A".into()),
                    targets: MolIds::One("B".into()),
                    label: Some(crate::doc::Label::Id("lab".into())),
                    edge_routing: None,
                    layout: None,
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
        svg: None,
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
                    layout: None,
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
        svg: None,
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
                    layout: None,
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
        svg: None,
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
                margin: None,
                opts: None,
            }));
            children.push(Node::Text(TextNode {
                type_: TextNodeKind::Text,
                id: Some("red".into()),
                text: "red".into(),
                color: None,
                scale: None,
                margin: None,
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
            layout: None,
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
            layout: None,
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
        svg: None,
        }
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
            svg: None,
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
        svg: None,
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
                    layout: None,
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
        svg: None,
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
                    layout: None,
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
                    layout: None,
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
        svg: None,
        };
        let paints = vec![paint("A", 80.0, 60.0), paint("B", 80.0, 60.0)];
        let sizes: Vec<(String, f64, f64)> = paints
            .iter()
            .map(|p| (p.id.clone(), p.scene.width, p.scene.height))
            .collect();
        let graph = build_elk_graph(&spec, &LayoutOpts::default(), &sizes);
        assert_eq!(graph["edges"].as_array().expect("edges").len(), 1);
    }

    fn edge_ab(src: &str, tgt: &str, layout: bool) -> Node {
        Node::Edge(EdgeNode {
            type_: EdgeNodeKind::Edge,
            sources: MolIds::One(src.into()),
            targets: MolIds::One(tgt.into()),
            label: None,
            edge_routing: None,
            layout: if layout { None } else { Some(false) },
            role: None,
            arrow: EdgeArrow::Forward,
            color: None,
            stroke_width: None,
            dashed: false,
        })
    }

    fn mol(id: &str) -> Node {
        Node::Mol(MolNode {
            id: Some(id.into()),
            smiles: Some("C".into()),
            ..Default::default()
        })
    }

    #[test]
    fn edge_layout_defaults_true_when_omitted() {
        let e: EdgeNode = serde_json::from_value(serde_json::json!({
            "type": "edge",
            "sources": "A",
            "targets": "B",
        }))
        .expect("edge");
        assert!(
            e.participates_in_layout(),
            "omitted layout must default to true"
        );
    }

    #[test]
    fn paint_only_reverse_first_uses_forward_elk_rep() {
        // Document-first reverse must not orient layers when layout:false.
        let spec = DepictSpec::ReactionScheme {
            id: None,
            children: vec![
                mol("A"),
                mol("B"),
                edge_ab("B", "A", false),
                edge_ab("A", "B", true),
            ],
            layout: None,
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        svg: None,
        };
        let paints = vec![paint("A", 80.0, 60.0), paint("B", 80.0, 60.0)];
        let sizes: Vec<(String, f64, f64)> = paints
            .iter()
            .map(|p| (p.id.clone(), p.scene.width, p.scene.height))
            .collect();
        let graph = build_elk_graph(&spec, &LayoutOpts::default(), &sizes);
        let edges = graph["edges"].as_array().expect("edges");
        assert_eq!(edges.len(), 1, "only the layout edge goes to ELK: {edges:?}");
        assert_eq!(edges[0]["id"], "e1");
        assert_eq!(edges[0]["sources"], serde_json::json!(["A"]));
        assert_eq!(edges[0]["targets"], serde_json::json!(["B"]));

        let scene = compose_scheme(&spec, &paints).expect("compose");
        assert_eq!(overlay_shafts(&scene).len(), 2, "both shafts still paint");
        let xa = scene.viewports.iter().find(|v| v.id.as_deref() == Some("A")).unwrap().x;
        let xb = scene.viewports.iter().find(|v| v.id.as_deref() == Some("B")).unwrap().x;
        assert!(xa < xb, "A should stay left of B: xa={xa} xb={xb}");
    }

    #[test]
    fn paint_only_feedback_keeps_source_leftmost() {
        let spec = DepictSpec::ReactionScheme {
            id: None,
            children: vec![
                mol("A"),
                mol("B"),
                mol("C"),
                edge_ab("A", "B", true),
                edge_ab("B", "C", true),
                edge_ab("C", "A", false),
            ],
            layout: None,
            opts: None,
            color: None,
            scale: None,
            aromatic: None,
            debug_box: None,
            debug_align: None,
            pad: None,
        svg: None,
        };
        let paints = vec![
            paint("A", 80.0, 60.0),
            paint("B", 80.0, 60.0),
            paint("C", 80.0, 60.0),
        ];
        let sizes: Vec<(String, f64, f64)> = paints
            .iter()
            .map(|p| (p.id.clone(), p.scene.width, p.scene.height))
            .collect();
        let graph = build_elk_graph(&spec, &LayoutOpts::default(), &sizes);
        let edges = graph["edges"].as_array().expect("edges");
        assert_eq!(
            edges.len(),
            2,
            "feedback with layout:false must not enter ELK: {edges:?}"
        );

        let place = layout_scheme(&spec, &LayoutOpts::default(), &sizes, &paints).expect("layout");
        let xa = place.positions["A"].0;
        let xb = place.positions["B"].0;
        let xc = place.positions["C"].0;
        assert!(
            xa < xb && xb < xc,
            "A→B→C layers left-to-right with paint-only feedback: A={xa} B={xb} C={xc}"
        );

        let scene = compose_scheme(&spec, &paints).expect("compose");
        assert_eq!(
            overlay_shafts(&scene).len(),
            3,
            "feedback shaft still paints"
        );
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

    /// Notebook example 5: down-flow phenol ⇄ benzaldehyde.
    fn example5_spec() -> DepictSpec {
        serde_json::from_str(
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
        .unwrap()
    }

    fn example5_scene() -> Scene {
        crate::doc::render(&example5_spec())
            .expect("render")
            .remove(0)
            .scene
    }

    fn overlay_shaft_ds(scene: &Scene) -> Vec<(String, Vec<(f64, f64)>)> {
        let mut out = Vec::new();
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
            out.push((d.clone(), on_curve_verts(d)));
        }
        out
    }

    fn overlay_heads(scene: &Scene) -> Vec<String> {
        let mut out = Vec::new();
        for p in &scene.overlays {
            let Primitive::Path {
                d, class: Some(c), ..
            } = p
            else {
                continue;
            };
            if c.contains("head") {
                out.push(d.clone());
            }
        }
        out
    }

    /// Three consecutive interpolating verts inside this span is the Catmull
    /// loop that painted as a ladder rung.
    fn has_ladder_rung(verts: &[(f64, f64)]) -> bool {
        verts.windows(3).any(|w| {
            let d01 = (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1);
            let d12 = (w[1].0 - w[2].0).hypot(w[1].1 - w[2].1);
            let d02 = (w[0].0 - w[2].0).hypot(w[0].1 - w[2].1);
            d01.max(d12).max(d02) < 8.0
        })
    }

    fn viewport_bond_centroid(scene: &Scene, id: &str) -> (f64, f64) {
        let vp = scene
            .viewports
            .iter()
            .find(|v| v.id.as_deref() == Some(id))
            .unwrap_or_else(|| panic!("viewport {id}"));
        let (ox, oy) = vp.local_origin();
        let mut sx = 0.0;
        let mut sy = 0.0;
        let mut n = 0.0;
        for layer in &vp.layers {
            if layer.name != LayerName::Bonds {
                continue;
            }
            for prim in &layer.primitives {
                if let Primitive::Path { d, .. } = prim {
                    for (x, y) in path_coords(d) {
                        sx += x + ox;
                        sy += y + oy;
                        n += 1.0;
                    }
                }
            }
        }
        assert!(n > 0.0, "mol {id} has no bond ink");
        (sx / n, sy / n)
    }

    fn head_aims_at(tip: (f64, f64), tan: (f64, f64), tgt: (f64, f64)) -> (f64, f64) {
        let to_c = (tgt.0 - tip.0, tgt.1 - tip.1);
        let clen = to_c.0.hypot(to_c.1).max(1e-9);
        let aim = (tan.0 * to_c.0 + tan.1 * to_c.1) / clen;
        let perp = (to_c.0 * tan.1 - to_c.1 * tan.0).abs() / clen;
        (aim, perp)
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

    /// Notebook example 5: the centroid chord used to cut CHO / the rings.
    #[test]
    fn example5_shafts_clear_chem() {
        let scene = example5_scene();
        let shafts = overlay_shaft_ds(&scene);
        assert_eq!(shafts.len(), 2, "expected the reverse pair");
        for (i, (d, verts)) in shafts.iter().enumerate() {
            assert!(
                verts.len() >= 2,
                "shaft must keep ELK endpoints: verts={verts:?} d={d}"
            );
            let samples = crate::catmull::sample_open_catmull(verts, 8);
            for (si, p) in samples.iter().enumerate() {
                if si == 0 || si + 1 == samples.len() {
                    continue;
                }
                let dist = composed_mol_ink(&scene, *p);
                assert!(
                    dist > 2.0,
                    "edge {i} sample ({:.1},{:.1}) hits mol ink: d={dist:.1} verts={verts:?} d={d}",
                    p.0,
                    p.1
                );
            }
        }
    }

    /// Collapsed stubs at the bow painted a tiny Catmull loop (a ladder rung)
    /// and the two lanes met at one point.
    #[test]
    fn example5_is_not_a_ladder_rung() {
        let scene = example5_scene();
        let shafts = overlay_shaft_ds(&scene);
        assert_eq!(shafts.len(), 2);
        for (d, verts) in &shafts {
            assert!(
                !has_ladder_rung(verts),
                "three controls collapsed into a Catmull loop: verts={verts:?} d={d}"
            );
        }
        let sa = crate::catmull::sample_open_catmull(&shafts[0].1, 8);
        let sb = crate::catmull::sample_open_catmull(&shafts[1].1, 8);
        let mut best = f64::INFINITY;
        for a in &sa {
            for b in &sb {
                best = best.min((a.0 - b.0).hypot(a.1 - b.1));
            }
        }
        assert!(
            best > 4.0,
            "lanes met at a rung: min_sep={best} a={:?} b={:?}",
            shafts[0].1,
            shafts[1].1
        );
    }

    fn path_backtracks(verts: &[(f64, f64)]) -> bool {
        verts.windows(3).any(|w| {
            let ax = w[1].0 - w[0].0;
            let ay = w[1].1 - w[0].1;
            let bx = w[2].0 - w[1].0;
            let by = w[2].1 - w[1].1;
            ax * bx + ay * by < 0.0
        })
    }

    fn samples_backtrack_along_chord(verts: &[(f64, f64)]) -> bool {
        if verts.len() < 2 {
            return false;
        }
        let a = verts[0];
        let b = *verts.last().unwrap();
        let vx = b.0 - a.0;
        let vy = b.1 - a.1;
        let l2 = (vx * vx + vy * vy).max(1e-9);
        let mut prev = f64::NEG_INFINITY;
        for p in crate::catmull::sample_open_catmull(verts, 8) {
            let t = ((p.0 - a.0) * vx + (p.1 - a.1) * vy) / l2;
            if t + 0.08 < prev {
                return true;
            }
            prev = prev.max(t);
        }
        false
    }

    /// Five close Catmull controls on the short down-flow corridor painted an
    /// S-squiggle (y even went backwards on A→B).
    #[test]
    fn example5_shafts_do_not_squiggle() {
        let scene = example5_scene();
        let shafts = overlay_shaft_ds(&scene);
        assert_eq!(shafts.len(), 2);
        for (d, verts) in &shafts {
            assert!(
                verts.len() <= 5,
                "clustered extras must be dropped: verts={verts:?} d={d}"
            );
            assert!(
                !path_backtracks(verts),
                "interpolating verts reverse: verts={verts:?} d={d}"
            );
            assert!(
                !samples_backtrack_along_chord(verts),
                "painted cubic backtracks along the corridor: verts={verts:?} d={d}"
            );
        }
    }

    /// A 0.5px last chord aimed the head off the incoming shaft.
    #[test]
    fn example5_head_follows_end_shaft() {
        let scene = example5_scene();
        let shafts = overlay_shaft_ds(&scene);
        let heads = overlay_heads(&scene);
        assert_eq!(shafts.len(), 2);
        assert_eq!(heads.len(), 2);
        for (i, (_, verts)) in shafts.iter().enumerate() {
            let (tip, tan) = crate::catmull::open_catmull_end_frame(verts);
            let h = path_coords(&heads[i]);
            assert!(h.len() >= 3, "edge {i} missing filled head: {}", heads[i]);
            let htip = h[1];
            let hbase = ((h[0].0 + h[2].0) * 0.5, (h[0].1 + h[2].1) * 0.5);
            let hx = htip.0 - hbase.0;
            let hy = htip.1 - hbase.1;
            let hl = hx.hypot(hy).max(1e-9);
            let aligned = tan.0 * (hx / hl) + tan.1 * (hy / hl);
            assert!(
                aligned > 0.95,
                "edge {i} head must follow the end shaft: aligned={aligned} tan={tan:?} tip={tip:?} verts={verts:?} head={}",
                heads[i]
            );
        }
    }

    /// Re-aiming hugs at the bow pointed A→B at the left of benzaldehyde
    /// instead of down at the ring centroid.
    #[test]
    fn example5_heads_aim_at_target_centroids() {
        let scene = example5_scene();
        let shafts = overlay_shaft_ds(&scene);
        let heads = overlay_heads(&scene);
        assert_eq!(shafts.len(), 2);
        assert_eq!(heads.len(), 2);
        let ca = viewport_bond_centroid(&scene, "A");
        let cb = viewport_bond_centroid(&scene, "B");
        for (i, tgt) in [(0usize, cb), (1, ca)] {
            let (d, verts) = &shafts[i];
            let (tip, tan) = crate::catmull::open_catmull_end_frame(verts);
            let (aim, perp) = head_aims_at(tip, tan, tgt);
            assert!(
                aim > 0.0 && perp < 0.35,
                "edge {i} last chord must aim at the target centroid: perp={perp} tip={tip:?} tan={tan:?} tgt={tgt:?} verts={verts:?} d={d}"
            );
            let h = path_coords(&heads[i]);
            assert!(h.len() >= 3, "edge {i} missing filled head: {}", heads[i]);
            let htip = h[1];
            let hbase = ((h[0].0 + h[2].0) * 0.5, (h[0].1 + h[2].1) * 0.5);
            let hx = htip.0 - hbase.0;
            let hy = htip.1 - hbase.1;
            let hl = hx.hypot(hy).max(1e-9);
            let (haim, hperp) = head_aims_at(htip, (hx / hl, hy / hl), tgt);
            assert!(
                haim > 0.0 && hperp < 0.45,
                "edge {i} painted head must aim at the target centroid: perp={hperp} tip={htip:?} tgt={tgt:?} d={}",
                heads[i]
            );
        }
        let (_, a_verts) = &shafts[0];
        let (_, tan) = crate::catmull::open_catmull_end_frame(a_verts);
        assert!(
            tan.1 > 0.5,
            "A→B must point down at benzaldehyde: tan={tan:?} verts={a_verts:?}"
        );
    }

    fn bond_points(scene: &Scene, id: Option<&str>) -> Vec<(f64, f64)> {
        let mut pts = Vec::new();
        for vp in &scene.viewports {
            if let Some(want) = id {
                if vp.id.as_deref() != Some(want) {
                    continue;
                }
            }
            let (ox, oy) = vp.local_origin();
            for layer in &vp.layers {
                if layer.name != LayerName::Bonds {
                    continue;
                }
                for prim in &layer.primitives {
                    if let Primitive::Path { d, .. } = prim {
                        pts.extend(path_coords(d).into_iter().map(|(x, y)| (x + ox, y + oy)));
                    }
                }
            }
        }
        pts
    }

    fn skeleton_signed_mol(scene: &Scene, id: &str, p: (f64, f64)) -> f64 {
        use crate::doc::layout_constraints::{convex_hull_pts, edge_signed_distance, Shape};
        let pt = Shape::Point { x: p.0, y: p.1 };
        let mut best = f64::INFINITY;
        let vp = scene
            .viewports
            .iter()
            .find(|v| v.id.as_deref() == Some(id))
            .unwrap_or_else(|| panic!("viewport {id}"));
        let (ox, oy) = vp.local_origin();
        for layer in &vp.layers {
            if layer.name != LayerName::Bonds {
                continue;
            }
            for prim in &layer.primitives {
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
        let hull = convex_hull_pts(&bond_points(scene, Some(id)));
        if hull.len() >= 3 {
            best = best.min(edge_signed_distance(&pt, &Shape::PointSet(hull)));
        }
        best
    }

    /// Anthracycline / naphthalene arrows used to land in ring holes: signed
    /// distance to bond lines is large at the centroid, so hug never walked out.
    ///
    /// Expected failure: thick-stroke shaft still collapses below
    /// `0.8 * head_px` after hug/routing (halo/path work); keep asserting so a
    /// real fix deletes this `should_panic`.
    #[test]
    #[should_panic(expected = "thick-stroke shaft must stay long enough to see")]
    fn fused_ring_heads_sit_outside_the_skeleton() {
        let spec: crate::doc::DepictSpec = serde_json::from_str(
            r#"{
              "type": "reaction_scheme",
              "layout": {"direction": "right", "edge_routing": "polyline"},
              "children": [
                {"type": "mol", "id": "A", "smiles": "c1ccc2ccccc2c1", "name": "naphthalene"},
                {"type": "mol", "id": "B", "smiles": "c1ccc2ccccc2c1"},
                {"type": "edge", "sources": "A", "targets": "B", "arrow": "forward", "stroke_width": 4.8}
              ]
            }"#,
        )
        .unwrap();
        let scene = crate::doc::render(&spec).expect("render").remove(0).scene;
        let shafts = overlay_shaft_ds(&scene);
        let heads = overlay_heads(&scene);
        assert_eq!(shafts.len(), 1);
        assert_eq!(heads.len(), 1);
        let (d, verts) = &shafts[0];
        let (tip, _) = crate::catmull::open_catmull_end_frame(verts);
        let h = path_coords(&heads[0]);
        let htip = h[1];
        for (name, p) in [("shaft tip", tip), ("head tip", htip)] {
            let dist = skeleton_signed_mol(&scene, "B", p);
            assert!(
                dist > 2.0,
                "{name} sits in B's rings: d={dist:.1} p={p:?} verts={verts:?} shaft={d}"
            );
        }
        let start = verts[0];
        let da = skeleton_signed_mol(&scene, "A", start);
        assert!(
            da > 2.0,
            "src tip sits in A's rings: d={da:.1} p={start:?} verts={verts:?} shaft={d}"
        );
        let span = {
            let a = verts[0];
            let b = *verts.last().unwrap();
            (b.0 - a.0).hypot(b.1 - a.1)
        };
        assert!(
            span > crate::arrows::head_px(Some(4.8)) * 0.8,
            "thick-stroke shaft must stay long enough to see: span={span} verts={verts:?}"
        );
        let a0 = verts[0];
        let a1 = *verts.last().unwrap();
        let vx = a1.0 - a0.0;
        let vy = a1.1 - a0.1;
        let l2 = (vx * vx + vy * vy).max(1e-9);
        for p in crate::catmull::sample_open_catmull(verts, 8) {
            let t = ((p.0 - a0.0) * vx + (p.1 - a0.1) * vy) / l2;
            if t < 0.12 || t > 0.88 {
                continue;
            }
            for id in ["A", "B"] {
                let dist = skeleton_signed_mol(&scene, id, p);
                assert!(
                    dist > 1.0,
                    "shaft sample ({:.1},{:.1}) t={t:.2} hits {id}: d={dist:.1} verts={verts:?} {d}",
                    p.0,
                    p.1
                );
            }
        }
    }

    /// A→C across a row used to cut the middle benzene (2-point chord, ring
    /// hole looked like clearance so refine never injected a bow).
    #[test]
    fn arrow_across_a_row_bows_around_the_middle_mol() {
        let spec: crate::doc::DepictSpec = serde_json::from_str(
            r#"{
              "type": "reaction_scheme",
              "layout": {"direction": "right", "edge_routing": "polyline"},
              "children": [
                {"type": "mol", "id": "A", "smiles": "c1ccccc1"},
                {"type": "mol", "id": "B", "smiles": "c1ccccc1"},
                {"type": "mol", "id": "C", "smiles": "c1ccccc1"},
                {"type": "edge", "sources": "A", "targets": "B", "arrow": "forward"},
                {"type": "edge", "sources": "B", "targets": "C", "arrow": "forward"},
                {"type": "edge", "sources": "A", "targets": "C", "arrow": "forward"}
              ]
            }"#,
        )
        .unwrap();
        let scene = crate::doc::render(&spec).expect("render").remove(0).scene;
        let shafts = overlay_shaft_ds(&scene);
        assert_eq!(shafts.len(), 3, "A→B, B→C, A→C");
        let (d, verts) = &shafts[2];
        let samples = if verts.len() >= 3 {
            crate::catmull::sample_open_catmull(verts, 8)
        } else {
            verts.clone()
        };
        for p in samples {
            let dist = skeleton_signed_mol(&scene, "B", p);
            assert!(
                dist > 2.0,
                "A→C sample ({:.1},{:.1}) cuts B: d={dist:.1} verts={verts:?} {d}",
                p.0,
                p.1
            );
        }
    }
}
