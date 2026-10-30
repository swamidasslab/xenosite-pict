//! Mol captions — privileged [`MolNode::name`] and text-node refs on
//! [`MolNode::label`], painted into the mol scene.

use crate::doc::chrome::MolText;
use crate::doc::layout_items::{
    padded_label_ink, place_region_auto, place_region_edge, Aabb, InkAtOrigin, Occupancy, Side,
    CLASH_MARGIN_PX, LABEL_PAD_X_PX,
};
use crate::doc::spec::{
    DepictSpec, Label, LabelItem, LabelPos, LayoutDirection, MolNode, Node, TextNode,
};
use crate::doc::text_paint::{markup_ascent_descent_px, markup_bounds_at_origin, outline_markup};
use crate::metrics::{BOND_PX, FONT_PX};
use crate::scene::{LayerName, Primitive, Scene};

/// Caption em size vs atom-label [`FONT_PX`] (1.5× house atom labels).
pub const CAPTION_FONT_FRAC: f64 = 1.5;
/// Default gap (px) between mol **structure** and caption: half a CC-bond.
pub const CAPTION_MARGIN_DEFAULT: f64 = 0.5 * BOND_PX;

/// Chem structure only: bonds + heteroatom labels. Skips shade, regions,
/// callouts, circles, and chrome atom numbers/labels.
fn is_structure_class(class: Option<&str>) -> bool {
    let Some(c) = class.map(|s| s.to_ascii_lowercase()) else {
        return true;
    };
    // Radical dots live on the marks layer (`class="atom-N radical"`) but they
    // are chem ink, same as heteroatom glyphs — not chrome.
    if c.contains("radical") {
        return true;
    }
    !(c.contains("shade")
        || c.contains("region")
        || c.contains("callout")
        || c.contains("atom-number")
        || c.contains("atom-label")
        || c.contains("mark")
        || c.contains("halo")
        || c.contains("debug")
        || (c.contains("circle") && !c.contains("aromatic")))
}

/// Structure AABB (not the full diagram with annotations).
fn structure_aabb(scene: &Scene) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut any = false;
    for vp in &scene.viewports {
        let (ox, oy) = vp.local_origin();
        for layer in &vp.layers {
            for p in &layer.primitives {
                if !is_structure_class(prim_class(p)) {
                    continue;
                }
                expand_prim_aabb(p, ox, oy, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
                any = true;
            }
        }
    }
    if !any || !min_x.is_finite() {
        return None;
    }
    Some((min_x, min_y, max_x, max_y))
}

fn prim_class(p: &Primitive) -> Option<&str> {
    match p {
        Primitive::Circle { class, .. } | Primitive::Text { class, .. } => class.as_deref(),
        Primitive::Path { class, .. } => class.as_deref(),
    }
}

/// Chrome + shade obstacles the mol name must clear (placed **after** solve).
///
/// Includes regions, callouts, circles, atom numbers/labels, and shade — not
/// bonds/chem text. Captions themselves are skipped (we are placing them).
fn annotation_occupancy_from_scene(scene: &Scene) -> Occupancy {
    let mut occ = Occupancy::new();
    for vp in &scene.viewports {
        for layer in &vp.layers {
            for p in &layer.primitives {
                if is_structure_class(prim_class(p)) {
                    continue;
                }
                add_prim_to_occupancy(&mut occ, p, vp.local_origin().0, vp.local_origin().1);
            }
        }
    }
    for p in &scene.overlays {
        let c = prim_class(p);
        if let Some(s) = c {
            if s.contains("mol-caption") {
                continue;
            }
        }
        if is_structure_class(c) {
            continue;
        }
        add_prim_to_occupancy(&mut occ, p, 0.0, 0.0);
    }
    for p in &scene.halo {
        if is_structure_class(prim_class(p)) {
            continue;
        }
        add_prim_to_occupancy(&mut occ, p, 0.0, 0.0);
    }
    occ
}

fn add_prim_to_occupancy(occ: &mut Occupancy, p: &Primitive, ox: f64, oy: f64) {
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    expand_prim_aabb(p, ox, oy, &mut x0, &mut y0, &mut x1, &mut y1);
    if x0.is_finite() && x1 > x0 && y1 > y0 {
        let box_ = Aabb::new(x0, y0, x1, y1);
        let class = prim_class(p).unwrap_or("");
        // Circles keep their true radius; text occupancy gets horizontal air.
        let box_ = if class.contains("circle") || class.contains("shade") {
            box_
        } else {
            box_.inflate_xy(LABEL_PAD_X_PX, 0.0)
        };
        occ.add_aabb(box_);
    }
}

fn expand_prim_aabb(
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
            for (x, y) in path_xy(d) {
                *min_x = min_x.min(ox + x - grow);
                *min_y = min_y.min(oy + y - grow);
                *max_x = max_x.max(ox + x + grow);
                *max_y = max_y.max(oy + y + grow);
            }
        }
    }
}

fn path_xy(d: &str) -> Vec<(f64, f64)> {
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

/// Omitted caption `pos`: below the mol, except vertical schemes put the name
/// beside the structure so it does not sit in the arrow corridor.
fn scheme_caption_pos(spec: &DepictSpec) -> LabelPos {
    match spec {
        DepictSpec::ReactionScheme { layout, .. } => match layout
            .as_ref()
            .map(|l| l.direction_or_default())
            .unwrap_or_default()
        {
            LayoutDirection::Down | LayoutDirection::Up => LabelPos::Right,
            _ => LabelPos::Below,
        },
        _ => LabelPos::Below,
    }
}

/// Bare id / omitted `pos` on mol captions → [`scheme_caption_pos`] (edges keep
/// their own defaults).
fn mol_caption_placements(lab: &Label, default_pos: LabelPos) -> Vec<(String, LabelPos)> {
    match lab {
        Label::Id(id) => vec![(id.clone(), default_pos)],
        Label::Placed(p) => vec![(p.id.clone(), p.pos.unwrap_or(default_pos))],
        Label::Items(items) => items
            .iter()
            .map(|it| match it {
                LabelItem::Id(s) => (s.clone(), default_pos),
                LabelItem::Placed(p) => (p.id.clone(), p.pos.unwrap_or(default_pos)),
            })
            .collect(),
        Label::Lanes(lanes) => {
            let mut out = Vec::new();
            for id in &lanes.above {
                out.push((id.clone(), LabelPos::Above));
            }
            for id in &lanes.below {
                out.push((id.clone(), LabelPos::Below));
            }
            for id in &lanes.left {
                out.push((id.clone(), LabelPos::Left));
            }
            for id in &lanes.right {
                out.push((id.clone(), LabelPos::Right));
            }
            out
        }
    }
}

fn text_node<'a>(spec: &'a DepictSpec, id: &str) -> Option<&'a TextNode> {
    match spec.node_by_id(id.trim()) {
        Some(Node::Text(t)) => Some(t),
        _ => None,
    }
}

fn caption_font_px(scale: Option<f64>) -> f64 {
    let scale = scale.filter(|s| s.is_finite() && *s > 0.0).unwrap_or(1.0);
    FONT_PX * CAPTION_FONT_FRAC * scale
}

fn resolve_margin(margin: Option<f64>) -> f64 {
    margin
        .filter(|m| m.is_finite() && *m >= 0.0)
        .unwrap_or(CAPTION_MARGIN_DEFAULT)
}

struct CaptionInk {
    text: String,
    pos: LabelPos,
    font_px: f64,
    fill: String,
    margin: f64,
}

fn name_to_ink(name: &MolText, default_pos: LabelPos) -> Option<CaptionInk> {
    let styled = name.as_styled();
    let text = styled.content.trim().to_string();
    if text.is_empty() {
        return None;
    }
    let pos = styled.pos.unwrap_or(default_pos);
    let fill = styled
        .color
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("#111")
        .to_string();
    Some(CaptionInk {
        text,
        pos,
        font_px: caption_font_px(styled.scale),
        fill,
        margin: resolve_margin(styled.margin),
    })
}

struct PlacedCaption {
    ink: CaptionInk,
    x: f64,
    y: f64,
    anchor: &'static str,
    bounds: (f64, f64, f64, f64),
}

/// Fold mol caption glyph paths into `scene` (expands canvas; default side
/// below, or right when the reaction scheme flows up/down).
///
/// Prefers privileged [`MolNode::name`]; also paints legacy [`MolNode::label`]
/// text-node refs. Captions use chem markup. Gap to mol ink uses typographic
/// **ascent**; viewBox / stack extent uses **descent** (not glyph outline AABB).
///
/// Call **after** chrome constraints are solved — the name is not a layout
/// DOF. Placement snugs to structure, then nudges clear of regions and other
/// annotations so they do not overlap the label.
pub fn apply_mol_captions(spec: &DepictSpec, node: &MolNode, mut scene: Scene) -> Scene {
    let default_pos = scheme_caption_pos(spec);
    let mut resolved: Vec<CaptionInk> = Vec::new();
    if let Some(ref name) = node.name {
        if let Some(ink) = name_to_ink(name, default_pos) {
            resolved.push(ink);
        }
    }
    if let Some(ref lab) = node.label {
        for (id, pos) in mol_caption_placements(lab, default_pos) {
            let Some(t) = text_node(spec, &id) else {
                continue;
            };
            let text = t.text.trim();
            if text.is_empty() {
                continue;
            }
            let fill = t
                .color
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("#111")
                .to_string();
            resolved.push(CaptionInk {
                text: text.to_string(),
                pos,
                font_px: caption_font_px(t.scale),
                fill,
                margin: CAPTION_MARGIN_DEFAULT,
            });
        }
    }
    if resolved.is_empty() {
        return scene;
    }

    let mol_w = scene.width;
    let mol_h = scene.height;
    // Center and gap against the molecule structure, not chrome annotations.
    let (mol_x0, mol_y0, mol_x1, mol_y1) =
        structure_aabb(&scene).unwrap_or((0.0, 0.0, mol_w, mol_h));
    let cx = (mol_x0 + mol_x1) * 0.5;
    let cy = (mol_y0 + mol_y1) * 0.5;

    let mut above: Vec<&CaptionInk> = Vec::new();
    let mut below: Vec<&CaptionInk> = Vec::new();
    let mut left: Vec<&CaptionInk> = Vec::new();
    let mut right: Vec<&CaptionInk> = Vec::new();
    let mut auto: Vec<&CaptionInk> = Vec::new();
    let mut corner: Vec<&CaptionInk> = Vec::new();
    for ink in &resolved {
        match ink.pos {
            LabelPos::Above => above.push(ink),
            LabelPos::Below => below.push(ink),
            LabelPos::Left => left.push(ink),
            LabelPos::Right => right.push(ink),
            LabelPos::Auto => auto.push(ink),
            LabelPos::AboveLeft
            | LabelPos::AboveRight
            | LabelPos::BelowLeft
            | LabelPos::BelowRight => corner.push(ink),
        }
    }

    let mut placed: Vec<PlacedCaption> = Vec::new();

    // Below: margin from mol ink → typographic ascent; stack by descent.
    let mut stack_top = mol_y1;
    for ink in below {
        let anchor = "middle";
        let Some((tx0, _ty0, tx1, _ty1)) = markup_bounds_at_origin(&ink.text, ink.font_px, anchor)
        else {
            continue;
        };
        let (asc, desc) = markup_ascent_descent_px(ink.font_px);
        // Ascent line sits margin below mol (or previous caption descent).
        let baseline = stack_top + ink.margin + asc;
        let x = cx;
        let bounds = (tx0 + x, baseline - asc, tx1 + x, baseline + desc);
        stack_top = bounds.3;
        placed.push(PlacedCaption {
            ink: CaptionInk {
                text: ink.text.clone(),
                pos: ink.pos,
                font_px: ink.font_px,
                fill: ink.fill.clone(),
                margin: ink.margin,
            },
            x,
            y: baseline,
            anchor,
            bounds,
        });
    }

    // Above: margin from mol ink → typographic descent; stack by ascent.
    let mut stack_bottom = mol_y0;
    for ink in above {
        let anchor = "middle";
        let Some((tx0, _ty0, tx1, _ty1)) = markup_bounds_at_origin(&ink.text, ink.font_px, anchor)
        else {
            continue;
        };
        let (asc, desc) = markup_ascent_descent_px(ink.font_px);
        let baseline = stack_bottom - ink.margin - desc;
        let x = cx;
        let bounds = (tx0 + x, baseline - asc, tx1 + x, baseline + desc);
        stack_bottom = bounds.1;
        placed.push(PlacedCaption {
            ink: CaptionInk {
                text: ink.text.clone(),
                pos: ink.pos,
                font_px: ink.font_px,
                fill: ink.fill.clone(),
                margin: ink.margin,
            },
            x,
            y: baseline,
            anchor,
            bounds,
        });
    }

    // Left / right: snug to mol sides; vertically center the em box on mol.
    for ink in left {
        let anchor = "end";
        let Some((tx0, _ty0, tx1, _ty1)) = markup_bounds_at_origin(&ink.text, ink.font_px, anchor)
        else {
            continue;
        };
        let (asc, desc) = markup_ascent_descent_px(ink.font_px);
        let target_right = mol_x0 - ink.margin - LABEL_PAD_X_PX;
        let x = target_right - tx1;
        let em_h = asc + desc;
        let y = cy - em_h * 0.5 + asc; // baseline so em box is centered
        let bounds = (tx0 + x, y - asc, tx1 + x, y + desc);
        placed.push(PlacedCaption {
            ink: CaptionInk {
                text: ink.text.clone(),
                pos: ink.pos,
                font_px: ink.font_px,
                fill: ink.fill.clone(),
                margin: ink.margin,
            },
            x,
            y,
            anchor,
            bounds,
        });
    }
    for ink in right {
        let anchor = "start";
        let Some((tx0, _ty0, tx1, _ty1)) = markup_bounds_at_origin(&ink.text, ink.font_px, anchor)
        else {
            continue;
        };
        let (asc, desc) = markup_ascent_descent_px(ink.font_px);
        let target_left = mol_x1 + ink.margin + LABEL_PAD_X_PX;
        let x = target_left - tx0;
        let em_h = asc + desc;
        let y = cy - em_h * 0.5 + asc;
        let bounds = (tx0 + x, y - asc, tx1 + x, y + desc);
        placed.push(PlacedCaption {
            ink: CaptionInk {
                text: ink.text.clone(),
                pos: ink.pos,
                font_px: ink.font_px,
                fill: ink.fill.clone(),
                margin: ink.margin,
            },
            x,
            y,
            anchor,
            bounds,
        });
    }

    // 45° corners sit outside the structure AABB at the matching corner.
    let structure = Aabb::new(mol_x0, mol_y0, mol_x1, mol_y1);
    let ann_occ_auto = annotation_occupancy_from_scene(&scene);
    for ink in corner {
        let Some((tx0, ty0, tx1, ty1)) = markup_bounds_at_origin(&ink.text, ink.font_px, "middle")
        else {
            continue;
        };
        let ink_at = padded_label_ink(InkAtOrigin {
            x0: tx0,
            y0: ty0,
            x1: tx1,
            y1: ty1,
        });
        let side = match ink.pos {
            LabelPos::AboveLeft => Side::AboveLeft,
            LabelPos::AboveRight => Side::AboveRight,
            LabelPos::BelowLeft => Side::BelowLeft,
            LabelPos::BelowRight => Side::BelowRight,
            _ => continue,
        };
        let p = place_region_edge(
            &ann_occ_auto,
            structure,
            ink_at,
            false,
            side,
            ink.margin,
            CLASH_MARGIN_PX,
            (cx, cy),
            false,
        );
        placed.push(PlacedCaption {
            ink: CaptionInk {
                text: ink.text.clone(),
                pos: ink.pos,
                font_px: ink.font_px,
                fill: ink.fill.clone(),
                margin: ink.margin,
            },
            x: p.x,
            y: p.y,
            anchor: "middle",
            bounds: (p.bounds.x0, p.bounds.y0, p.bounds.x1, p.bounds.y1),
        });
    }

    // Auto: sit outside the structure AABB on the side farthest from other chrome.
    for ink in auto {
        let Some((tx0, ty0, tx1, ty1)) = markup_bounds_at_origin(&ink.text, ink.font_px, "middle")
        else {
            continue;
        };
        let ink_at = padded_label_ink(InkAtOrigin {
            x0: tx0,
            y0: ty0,
            x1: tx1,
            y1: ty1,
        });
        let p = place_region_auto(
            &ann_occ_auto,
            structure,
            ink_at,
            ink.margin,
            CLASH_MARGIN_PX,
            (cx, cy),
        );
        placed.push(PlacedCaption {
            ink: CaptionInk {
                text: ink.text.clone(),
                pos: ink.pos,
                font_px: ink.font_px,
                fill: ink.fill.clone(),
                margin: ink.margin,
            },
            x: p.x,
            y: p.y,
            anchor: "middle",
            bounds: (p.bounds.x0, p.bounds.y0, p.bounds.x1, p.bounds.y1),
        });
    }

    if placed.is_empty() {
        return scene;
    }

    // Name must clear shade + chrome (regions, callouts, …) already solved.
    let ann_occ = annotation_occupancy_from_scene(&scene);
    for p in &mut placed {
        let rest = Aabb::new(p.bounds.0, p.bounds.1, p.bounds.2, p.bounds.3);
        if !ann_occ.clashes(rest, CLASH_MARGIN_PX) {
            continue;
        }
        // Nudge along preferred axis away from the mol / annotations.
        let (dx, dy) = match p.ink.pos {
            LabelPos::Below => (0.0, 2.0),
            LabelPos::Above => (0.0, -2.0),
            LabelPos::Left => (-2.0, 0.0),
            LabelPos::Right => (2.0, 0.0),
            LabelPos::BelowRight => {
                let s = std::f64::consts::FRAC_1_SQRT_2;
                (2.0 * s, 2.0 * s)
            }
            LabelPos::BelowLeft => {
                let s = std::f64::consts::FRAC_1_SQRT_2;
                (-2.0 * s, 2.0 * s)
            }
            LabelPos::AboveRight => {
                let s = std::f64::consts::FRAC_1_SQRT_2;
                (2.0 * s, -2.0 * s)
            }
            LabelPos::AboveLeft => {
                let s = std::f64::consts::FRAC_1_SQRT_2;
                (-2.0 * s, -2.0 * s)
            }
            LabelPos::Auto => {
                let (pcx, pcy) = ((rest.x0 + rest.x1) * 0.5, (rest.y0 + rest.y1) * 0.5);
                let vx = pcx - cx;
                let vy = pcy - cy;
                let l = vx.hypot(vy).max(1e-6);
                (2.0 * vx / l, 2.0 * vy / l)
            }
        };
        for step in 1..120 {
            let ox = dx * step as f64;
            let oy = dy * step as f64;
            let cand = Aabb::new(rest.x0 + ox, rest.y0 + oy, rest.x1 + ox, rest.y1 + oy);
            if !ann_occ.clashes(cand, CLASH_MARGIN_PX) {
                p.x += ox;
                p.y += oy;
                p.bounds.0 += ox;
                p.bounds.1 += oy;
                p.bounds.2 += ox;
                p.bounds.3 += oy;
                break;
            }
        }
    }

    let mut max_x = mol_x1;
    let mut max_y = mol_y1;
    for p in &placed {
        max_x = max_x.max(p.bounds.2);
        max_y = max_y.max(p.bounds.3);
    }

    let mut prims: Vec<Primitive> = Vec::new();
    for p in &placed {
        let Some((d, data_text, _)) =
            outline_markup(&p.ink.text, p.x, p.y, p.ink.font_px, p.anchor)
        else {
            continue;
        };
        // Invisible typographic em box so fit/viewBox uses ascent/descent, not
        // glyph outline (keeps baselines aligned across captions).
        let (x0, y0, x1, y1) = p.bounds;
        prims.push(Primitive::Path {
            d: format!(
                "M {:.3} {:.3} L {:.3} {:.3} L {:.3} {:.3} L {:.3} {:.3} Z",
                x0, y0, x1, y0, x1, y1, x0, y1
            ),
            stroke: Some("none".into()),
            fill: Some("none".into()),
            stroke_width: 0.0,
            opacity: 0.0,
            stroke_dasharray: None,
            stroke_linecap: None,
            class: Some("mol-caption-metrics".into()),
            data_text: None,
        });
        prims.push(Primitive::Path {
            d,
            stroke: Some("none".into()),
            fill: Some(p.ink.fill.clone()),
            stroke_width: 0.0,
            opacity: 1.0,
            stroke_dasharray: None,
            stroke_linecap: None,
            class: Some("mol-caption label".into()),
            data_text: Some(data_text),
        });
    }

    scene.width = max_x.max(mol_w);
    scene.height = max_y.max(mol_h);
    if let Some(vp) = scene.viewports.first_mut() {
        vp.width = scene.width;
        vp.height = scene.height;
        vp.extend_layer(LayerName::Overlay, prims);
    }
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::chrome::{MolText, MolTextSpec};
    use crate::metrics::FONT_PX;
    use crate::scene::{AtomIn, BondIn, MoleculeIn};

    fn overlay_prims(scene: &Scene) -> Vec<&Primitive> {
        scene
            .viewports
            .iter()
            .flat_map(|vp| vp.layers.iter())
            .filter(|l| l.name == LayerName::Overlay)
            .flat_map(|l| l.primitives.iter())
            .chain(scene.overlays.iter())
            .collect()
    }

    fn ethanol() -> MoleculeIn {
        MoleculeIn {
            id: Some("a".into()),
            atoms: vec![
                AtomIn {
                    index: 0,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 0.0,
                    y: 0.0,
                    label: None,
                    charge: 0,
                    radical: 0,
                    mapno: None,
                },
                AtomIn {
                    index: 1,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 1.5,
                    y: 0.0,
                    label: None,
                    charge: 0,
                    radical: 0,
                    mapno: None,
                },
                AtomIn {
                    index: 2,
                    element: Some("O".into()),
                    z: Some(8),
                    x: 2.2,
                    y: 1.0,
                    label: Some("OH".into()),
                    charge: 0,
                    radical: 0,
                    mapno: None,
                },
            ],
            bonds: vec![
                BondIn {
                    index: 0,
                    begin: 0,
                    end: 1,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
                BondIn {
                    index: 1,
                    begin: 1,
                    end: 2,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
            ],
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
        }
    }

    #[test]
    fn caption_font_is_one_point_five_times_atom() {
        assert!((CAPTION_FONT_FRAC - 1.5).abs() < 1e-12);
        assert!((caption_font_px(None) - FONT_PX * 1.5).abs() < 1e-9);
    }

    #[test]
    fn mol_caption_defaults_below_and_paints_overlay() {
        let spec: DepictSpec = serde_json::from_str(
            r#"{
              "type": "group",
              "children": [
                {"type": "text", "id": "cap", "text": "ethanol"},
                {"type": "mol", "id": "a", "smiles": "CCO", "label": "cap"}
              ]
            }"#,
        )
        .unwrap();
        let node = spec.mols()[0].clone();
        let base = crate::depict::depict_molecule(&ethanol());
        let out = apply_mol_captions(&spec, &node, base);
        // Caption snugs into house PAD below ink — height may stay flat.
        let hit = overlay_prims(&out).iter().any(|p| {
            matches!(
                p,
                Primitive::Path {
                    data_text: Some(t),
                    class: Some(c),
                    ..
                } if t == "ethanol" && c.contains("mol-caption")
            )
        });
        assert!(
            hit,
            "expected mol-caption overlay, got {:?}",
            overlay_prims(&out)
        );
    }

    #[test]
    fn vertical_scheme_default_name_sits_beside_not_in_corridor() {
        let spec: DepictSpec = serde_json::from_str(
            r#"{
              "type": "reaction_scheme",
              "layout": {"direction": "down"},
              "children": [
                {"type": "mol", "id": "A", "smiles": "CCO", "name": "ethanol"},
                {"type": "mol", "id": "B", "smiles": "CC=O", "name": "acetaldehyde"},
                {"type": "edge", "source": "A", "target": "B", "arrow": "forward"}
              ]
            }"#,
        )
        .unwrap();
        assert_eq!(scheme_caption_pos(&spec), LabelPos::Right);
        let node = spec.mols()[0].clone();
        let base = crate::depict::depict_molecule(&ethanol());
        let (_sx0, sy0, sx1, sy1) = structure_aabb(&base).expect("structure");
        let out = apply_mol_captions(&spec, &node, base);
        let mut cap_x0 = f64::INFINITY;
        let mut cap_y0 = f64::INFINITY;
        let mut cap_y1 = f64::NEG_INFINITY;
        for p in overlay_prims(&out) {
            if let Primitive::Path {
                d, class: Some(c), ..
            } = p
            {
                if c.contains("mol-caption-metrics") {
                    for (x, y) in path_xy(d) {
                        cap_x0 = cap_x0.min(x);
                        cap_y0 = cap_y0.min(y);
                        cap_y1 = cap_y1.max(y);
                    }
                }
            }
        }
        assert!(cap_x0.is_finite(), "expected caption metrics");
        assert!(
            cap_x0 + 1.0 >= sx1,
            "down-flow name should sit to the right of the structure, not below: cap_x0={cap_x0} struct_right={sx1}"
        );
        let cy = (cap_y0 + cap_y1) * 0.5;
        assert!(
            cy > sy0 && cy < sy1 + (sy1 - sy0),
            "side caption should sit next to the mol, not under it: cy={cy} struct y={sy0}..{sy1}"
        );
    }

    #[test]
    fn mol_name_paints_without_text_node() {
        let spec: DepictSpec = serde_json::from_str(
            r#"{
              "type": "mol",
              "smiles": "CCO",
              "name": "ethanol"
            }"#,
        )
        .unwrap();
        let node = spec.mols()[0].clone();
        assert!(node.name.is_some());
        let base = crate::depict::depict_molecule(&ethanol());
        let out = apply_mol_captions(&spec, &node, base);
        let hit = overlay_prims(&out).iter().any(|p| {
            matches!(
                p,
                Primitive::Path {
                    data_text: Some(t),
                    class: Some(c),
                    ..
                } if t == "ethanol" && c.contains("mol-caption")
            )
        });
        assert!(hit, "expected name caption, got {:?}", overlay_prims(&out));
    }

    #[test]
    fn mol_name_markup_subscript() {
        let spec: DepictSpec = serde_json::from_str(
            r#"{
              "type": "mol",
              "smiles": "CCO",
              "name": {"content": "$R_1$", "pos": "below"}
            }"#,
        )
        .unwrap();
        let node = spec.mols()[0].clone();
        let out = apply_mol_captions(&spec, &node, crate::depict::depict_molecule(&ethanol()));
        let data: Vec<_> = overlay_prims(&out)
            .iter()
            .filter_map(|p| match p {
                Primitive::Path {
                    data_text: Some(t),
                    class: Some(c),
                    ..
                } if c.contains("mol-caption") => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            data.iter().any(|t| t.contains('₁') || t.contains('R')),
            "expected markup data_text, got {data:?}"
        );
    }

    #[test]
    fn below_placement_gap_uses_ascent_not_glyph_ink() {
        let margin = 8.0_f64;
        let mol_y1 = 100.0_f64;
        let font_px = caption_font_px(None);
        let (asc, desc) = markup_ascent_descent_px(font_px);
        let baseline = mol_y1 + margin + asc;
        let ascent_top = baseline - asc;
        let descent_bottom = baseline + desc;
        assert!(
            (ascent_top - (mol_y1 + margin)).abs() < 1e-9,
            "ascent line {ascent_top} vs mol+margin {}",
            mol_y1 + margin
        );
        assert!(descent_bottom > ascent_top);
        // Glyph ink top is at/above ascent; gap is defined on ascent, not ink.
        let (_tx0, ty0, _tx1, ty1) =
            markup_bounds_at_origin("EtOH", font_px, "middle").expect("bounds");
        let glyph_top = ty0 + baseline;
        let glyph_bottom = ty1 + baseline;
        assert!(
            glyph_top + 1e-6 >= ascent_top - 1.0,
            "glyph should sit near ascent box"
        );
        assert!(
            glyph_bottom <= descent_bottom + 1.0,
            "glyph should sit inside descent box"
        );
    }

    #[test]
    fn mol_name_snugs_to_ink_via_ascent_metrics() {
        assert!((CAPTION_MARGIN_DEFAULT - 0.5 * BOND_PX).abs() < 1e-12);
        let node = MolNode {
            name: Some(MolText::Styled(MolTextSpec {
                content: "EtOH".into(),
                pos: Some(LabelPos::Below),
                ..Default::default()
            })),
            ..Default::default()
        };
        let spec: DepictSpec = serde_json::from_str(
            r#"{"type":"mol","smiles":"CCO","name":{"content":"EtOH","pos":"below"}}"#,
        )
        .unwrap();
        let base = crate::depict::depict_molecule(&ethanol());
        let ink = structure_aabb(&base).expect("ink");
        let canvas_h = base.height;
        assert!(
            canvas_h > ink.3 + 5.0,
            "fixture expects padded canvas below ink: canvas={canvas_h} ink_bottom={}",
            ink.3
        );
        let out = apply_mol_captions(&spec, &node, base);
        let mut metric_top = f64::INFINITY;
        let mut metric_bottom = f64::NEG_INFINITY;
        for p in overlay_prims(&out) {
            if let Primitive::Path {
                d, class: Some(c), ..
            } = p
            {
                if c.contains("mol-caption-metrics") {
                    for (_, y) in path_xy(d) {
                        metric_top = metric_top.min(y);
                        metric_bottom = metric_bottom.max(y);
                    }
                }
            }
        }
        assert!(metric_top.is_finite(), "expected caption metric frame");
        let expected_top = ink.3 + CAPTION_MARGIN_DEFAULT;
        assert!(
            (metric_top - expected_top).abs() < 2.0,
            "ascent line {metric_top} should be ~ink_bottom+margin {expected_top}"
        );
        assert!(
            metric_top + 5.0 < canvas_h + CAPTION_MARGIN_DEFAULT,
            "must not snug to padded canvas"
        );
        let font_px = caption_font_px(None);
        let (asc, desc) = markup_ascent_descent_px(font_px);
        assert!(
            ((metric_bottom - metric_top) - (asc + desc)).abs() < 1.0,
            "metric box height should be ascent+descent"
        );
    }

    #[test]
    fn flat_and_descender_captions_share_descent_extent() {
        // "aa" has no descenders; "yg" does — layout bottoms must match so
        // flex-end viewBoxes keep baselines aligned.
        let mk = |name: &str| {
            let node = MolNode {
                name: Some(MolText::Plain(name.into())),
                ..Default::default()
            };
            let spec: DepictSpec = serde_json::from_str(&format!(
                r#"{{"type":"mol","smiles":"CCO","name":"{name}"}}"#
            ))
            .unwrap();
            let out = apply_mol_captions(&spec, &node, crate::depict::depict_molecule(&ethanol()));
            let mut bottom = f64::NEG_INFINITY;
            let mut baseline = f64::NAN;
            for p in overlay_prims(&out) {
                if let Primitive::Path {
                    d, class: Some(c), ..
                } = p
                {
                    if c.contains("mol-caption-metrics") {
                        let ys: Vec<f64> = path_xy(d).into_iter().map(|(_, y)| y).collect();
                        let y0 = ys.iter().cloned().fold(f64::INFINITY, f64::min);
                        let y1 = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                        bottom = bottom.max(y1);
                        let (asc, _desc) = markup_ascent_descent_px(caption_font_px(None));
                        baseline = y0 + asc;
                    }
                }
            }
            (bottom, baseline)
        };
        let (b_flat, base_flat) = mk("aa");
        let (b_desc, base_desc) = mk("yg");
        assert!(
            (b_flat - b_desc).abs() < 1.0,
            "descent bottoms should match: flat={b_flat} desc={b_desc}"
        );
        assert!(
            (base_flat - base_desc).abs() < 1.0,
            "baselines should match: flat={base_flat} desc={base_desc}"
        );
    }

    #[test]
    fn name_placed_after_chrome_clears_region() {
        // Chrome first, then name — caption must not sit on the region hull.
        use crate::doc::chrome::{FillSpec, RegionField, RegionSpec, RegionStyle, StrokeSpec};
        use crate::doc::chrome_paint::apply_mol_chrome;

        let mol = ethanol();
        let scene = crate::depict::depict_molecule(&mol);
        let node = MolNode {
            name: Some(MolText::Styled(MolTextSpec {
                content: "ethanol".into(),
                pos: Some(LabelPos::Below),
                ..Default::default()
            })),
            region: Some(RegionField::One(RegionSpec {
                style: RegionStyle::Box,
                atoms: vec![0, 1, 2],
                exclude: vec![],
                fill: Some(FillSpec {
                    color: Some("#eee".into()),
                    opacity: None,
                }),
                stroke: Some(StrokeSpec {
                    color: Some("#c44".into()),
                    width: Some(1.0),
                    dash: None,
                }),
                text: None,
                z: None,
            })),
            ..Default::default()
        };
        let scene = apply_mol_chrome(&node, &mol, scene);
        let spec: DepictSpec =
            serde_json::from_str(r#"{"type":"mol","smiles":"CCO","name":"ethanol"}"#).unwrap();
        let out = apply_mol_captions(&spec, &node, scene);

        let region = out
            .viewports
            .iter()
            .find_map(|vp| {
                vp.layers.iter().find_map(|l| {
                    l.primitives.iter().find_map(|p| match p {
                        Primitive::Path {
                            d, class: Some(c), ..
                        } if c.contains("region") => {
                            let (ox, oy) = vp.local_origin();
                            let pts = path_xy(d);
                            let xs: Vec<_> = pts.iter().map(|p| ox + p.0).collect();
                            let ys: Vec<_> = pts.iter().map(|p| oy + p.1).collect();
                            Some(Aabb::new(
                                xs.iter().cloned().fold(f64::INFINITY, f64::min),
                                ys.iter().cloned().fold(f64::INFINITY, f64::min),
                                xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                                ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                            ))
                        }
                        _ => None,
                    })
                })
            })
            .expect("region box");
        let caption = overlay_prims(&out)
            .into_iter()
            .find_map(|p| match p {
                Primitive::Path {
                    d, class: Some(c), ..
                } if c.contains("mol-caption-metrics") => {
                    let pts = path_xy(d);
                    let xs: Vec<_> = pts.iter().map(|p| p.0).collect();
                    let ys: Vec<_> = pts.iter().map(|p| p.1).collect();
                    Some(Aabb::new(
                        xs.iter().cloned().fold(f64::INFINITY, f64::min),
                        ys.iter().cloned().fold(f64::INFINITY, f64::min),
                        xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                        ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                    ))
                }
                _ => None,
            })
            .expect("mol-caption-metrics");
        let gap = CLASH_MARGIN_PX;
        let padded = caption.inflate(gap);
        assert!(
            !padded.overlaps(region),
            "name {caption:?} must clear region {region:?} by ≥{gap}"
        );
        assert!(
            caption.y0 >= region.y1 - 1e-6
                || caption.y1 <= region.y0 + 1e-6
                || caption.x0 >= region.x1 - 1e-6
                || caption.x1 <= region.x0 + 1e-6,
            "name should sit outside the region box (pos=Below)"
        );
    }
}
