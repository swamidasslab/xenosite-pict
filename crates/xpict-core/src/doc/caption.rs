//! Mol captions — text-node refs on [`MolNode::label`], painted into the mol scene.

use crate::doc::spec::{DepictSpec, Label, LabelItem, LabelPos, MolNode, Node, TextNode};
use crate::font::{compile_text_shapes, face_metrics, FaceStyle};
use crate::metrics::FONT_PX;
use crate::scene::{Primitive, Scene};

/// Caption em size vs atom-label [`FONT_PX`] (slightly larger).
const CAPTION_FONT_FRAC: f64 = 1.15;
const CAPTION_GAP_PX: f64 = 4.0;

/// Bare id / omitted `pos` on mol captions → below (edges keep their own defaults).
fn mol_caption_placements(lab: &Label) -> Vec<(String, LabelPos)> {
    match lab {
        Label::Id(id) => vec![(id.clone(), LabelPos::Below)],
        Label::Placed(p) => vec![(p.id.clone(), p.pos.unwrap_or(LabelPos::Below))],
        Label::Items(items) => items
            .iter()
            .map(|it| match it {
                LabelItem::Id(s) => (s.clone(), LabelPos::Below),
                LabelItem::Placed(p) => (p.id.clone(), p.pos.unwrap_or(LabelPos::Below)),
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

fn caption_font_px(text: &TextNode) -> f64 {
    let scale = text.scale.filter(|s| s.is_finite() && *s > 0.0).unwrap_or(1.0);
    FONT_PX * CAPTION_FONT_FRAC * scale
}

fn baseline_offset(font_px: f64) -> f64 {
    let face = face_metrics(FaceStyle::Regular);
    0.5 * (face.cap_height / face.upem) * font_px
}

fn typo_height(font_px: f64) -> f64 {
    let face = face_metrics(FaceStyle::Regular);
    let typo = (face.ascent - face.descent).abs().max(face.upem * 0.8);
    typo * (font_px / face.upem.max(1.0))
}

fn estimate_width(text: &str, font_px: f64) -> f64 {
    if let Some(shape) =
        compile_text_shapes(text, 0.0, 0.0, font_px, "start", FaceStyle::Regular)
    {
        if let Some((x0, _, x1, _)) = shape.bounds() {
            return (x1 - x0).abs();
        }
    }
    font_px * text.chars().count() as f64 * 0.55
}

/// Fold mol caption glyph paths into `scene` (expands canvas; default side below).
pub fn apply_mol_captions(spec: &DepictSpec, node: &MolNode, mut scene: Scene) -> Scene {
    let Some(ref lab) = node.label else {
        return scene;
    };
    let placements = mol_caption_placements(lab);
    if placements.is_empty() {
        return scene;
    }

    let mut above_h = 0.0_f64;
    let mut below_h = 0.0_f64;
    let mut left_w = 0.0_f64;
    let mut right_w = 0.0_f64;
    let mut resolved: Vec<(&TextNode, LabelPos, f64, f64)> = Vec::new();
    for (id, pos) in &placements {
        let Some(t) = text_node(spec, id) else {
            continue;
        };
        let text = t.text.trim();
        if text.is_empty() {
            continue;
        }
        let font_px = caption_font_px(t);
        let h = typo_height(font_px);
        let w = estimate_width(text, font_px);
        match pos {
            LabelPos::Above => above_h += h + CAPTION_GAP_PX,
            LabelPos::Below => below_h += h + CAPTION_GAP_PX,
            LabelPos::Left => left_w = left_w.max(w + CAPTION_GAP_PX),
            LabelPos::Right => right_w = right_w.max(w + CAPTION_GAP_PX),
        }
        resolved.push((t, *pos, font_px, w));
    }
    if resolved.is_empty() {
        return scene;
    }

    // Shift mol viewports (and halo via translate of circle/text; path halos
    // stay correct because depict halo is regenerated in viewport space — we
    // offset the viewport instead of rewriting path `d`).
    let mol_w = scene.width;
    let mol_h = scene.height;
    for vp in &mut scene.viewports {
        vp.x += left_w;
        vp.y += above_h;
    }
    if left_w > 0.0 || above_h > 0.0 {
        scene.halo = scene
            .halo
            .iter()
            .map(|p| shift_prim(p, left_w, above_h))
            .collect();
        scene.overlays = scene
            .overlays
            .iter()
            .map(|p| shift_prim(p, left_w, above_h))
            .collect();
    }
    scene.width = mol_w + left_w + right_w;
    scene.height = mol_h + above_h + below_h;

    let cx = left_w + mol_w * 0.5;
    let cy = above_h + mol_h * 0.5;
    let mut above_y = CAPTION_GAP_PX;
    let mut below_y = above_h + mol_h + CAPTION_GAP_PX;
    let mut prims: Vec<Primitive> = Vec::new();

    for (t, pos, font_px, w) in resolved {
        let text = t.text.trim();
        let fill = t
            .color
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("#111")
            .to_string();
        let h = typo_height(font_px);
        let base = baseline_offset(font_px);
        let (x, y, anchor) = match pos {
            LabelPos::Above => {
                let y = above_y + base;
                above_y += h + CAPTION_GAP_PX;
                (cx, y, "middle")
            }
            LabelPos::Below => {
                let y = below_y + base;
                below_y += h + CAPTION_GAP_PX;
                (cx, y, "middle")
            }
            LabelPos::Left => (left_w - CAPTION_GAP_PX, cy + base, "end"),
            LabelPos::Right => (left_w + mol_w + CAPTION_GAP_PX, cy + base, "start"),
        };
        let _ = w;
        let Some(shape) =
            compile_text_shapes(text, x, y, font_px, anchor, FaceStyle::Regular)
        else {
            continue;
        };
        let d = shape.to_svg_d();
        if d.is_empty() {
            continue;
        }
        if let Some((_x0, _y0, x1, y1)) = shape.bounds() {
            scene.width = scene.width.max(x1 + CAPTION_GAP_PX);
            scene.height = scene.height.max(y1 + CAPTION_GAP_PX);
        }
        prims.push(Primitive::Path {
            d,
            stroke: Some("none".into()),
            fill: Some(fill),
            stroke_width: 0.0,
            opacity: 1.0,
            stroke_dasharray: None,
            stroke_linecap: None,
            class: Some("mol-caption label".into()),
            data_text: Some(text.to_string()),
        });
    }

    // Captions live in scene overlays (document space), not viewport-local
    // layers — same idea as a thin outer frame, but ink is in the SVG.
    scene.overlays.extend(prims);
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{AtomIn, BondIn, MoleculeIn};

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
                },
                AtomIn {
                    index: 1,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 1.5,
                    y: 0.0,
                    label: None,
                    charge: 0,
                },
                AtomIn {
                    index: 2,
                    element: Some("O".into()),
                    z: Some(8),
                    x: 2.2,
                    y: 1.0,
                    label: Some("OH".into()),
                    charge: 0,
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
            mark_atoms: vec![],
            mark_bonds: vec![],
            scale: 1.0,
            weight: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
        }
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
        let base_h = base.height;
        let out = apply_mol_captions(&spec, &node, base);
        assert!(
            out.height > base_h,
            "caption below should grow scene height: {} vs {}",
            out.height,
            base_h
        );
        let hit = out.overlays.iter().any(|p| {
            matches!(
                p,
                Primitive::Path {
                    data_text: Some(t),
                    class: Some(c),
                    ..
                } if t == "ethanol" && c.contains("mol-caption")
            )
        });
        assert!(hit, "expected mol-caption overlay, got {:?}", out.overlays);
        assert!(CAPTION_FONT_FRAC > 1.0);
    }
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

/// Shift absolute M/L coordinate pairs (depict halo paths).
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
        if matches!(cmd, 'M' | 'L') {
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
                out.push_str(&format!("{} {}", x + dx, y + dy));
                continue;
            }
        }
        out.push_str(&num);
    }
    out
}
