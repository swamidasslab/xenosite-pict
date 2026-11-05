//! Gallery cases marked as problems. These tests name the failure mode so a
//! regression cannot slip back to "H—H" / pentagram / flush nested boxes.

use crate::doc::depict;
use crate::doc::layout_constraints::{edge_signed_distance, Shape};
use crate::doc::spec::DepictSpec;
use crate::metrics::FONT_PX;
use crate::scene::{LayerName, Primitive, Scene};

fn render_json(json: &str) -> Scene {
    let spec: DepictSpec = serde_json::from_str(json).expect("spec json");
    depict(&spec).expect("depict").remove(0).scene
}

fn path_nums(d: &str) -> Vec<f64> {
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
    nums
}

fn path_aabb(d: &str) -> Option<(f64, f64, f64, f64)> {
    let n = path_nums(d);
    if n.len() < 2 {
        return None;
    }
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    let mut i = 0;
    while i + 1 < n.len() {
        x0 = x0.min(n[i]);
        y0 = y0.min(n[i + 1]);
        x1 = x1.max(n[i]);
        y1 = y1.max(n[i + 1]);
        i += 2;
    }
    Some((x0, y0, x1, y1))
}

fn data_texts(scene: &Scene) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |p: &Primitive| match p {
        Primitive::Path {
            data_text: Some(t), ..
        } => out.push(t.clone()),
        Primitive::Text { text, .. } => out.push(text.clone()),
        _ => {}
    };
    for p in &scene.overlays {
        push(p);
    }
    for vp in &scene.viewports {
        for layer in &vp.layers {
            for p in &layer.primitives {
                push(p);
            }
        }
    }
    out
}

fn overlay_edge_paths(scene: &Scene) -> Vec<String> {
    scene
        .overlays
        .iter()
        .filter_map(|p| match p {
            Primitive::Path {
                d, class: Some(c), ..
            } if c.contains("edge") && !c.contains("head") && !c.contains("harpoon") => {
                Some(d.clone())
            }
            _ => None,
        })
        .collect()
}

fn sample_path(d: &str) -> Vec<(f64, f64)> {
    let n = path_nums(d);
    let mut pts = Vec::new();
    let mut i = 0;
    while i + 1 < n.len() {
        pts.push((n[i], n[i + 1]));
        i += 2;
    }
    pts
}

fn has_scheme_plus(scene: &Scene) -> bool {
    scene.overlays.iter().any(|p| match p {
        Primitive::Text { text, class, .. } => {
            text.trim() == "+" || class.as_deref().is_some_and(|c| c.contains("scheme-plus"))
        }
        Primitive::Path {
            data_text: Some(t),
            class,
            ..
        } => t.contains('+') && class.as_deref().is_some_and(|c| c.contains("plus")),
        _ => false,
    })
}

fn skeleton_signed(scene: &Scene, mol_id: &str, p: (f64, f64)) -> f64 {
    let vp = scene
        .viewports
        .iter()
        .find(|v| v.id.as_deref() == Some(mol_id))
        .unwrap_or_else(|| panic!("viewport {mol_id}"));
    let (ox, oy) = vp.local_origin();
    let mut best = f64::INFINITY;
    for layer in &vp.layers {
        if layer.name != LayerName::Bonds {
            continue;
        }
        for prim in &layer.primitives {
            if let Primitive::Path { d, .. } = prim {
                let pts: Vec<(f64, f64)> = sample_path(d)
                    .into_iter()
                    .map(|(x, y)| (x + ox, y + oy))
                    .collect();
                for w in pts.windows(2) {
                    best = best.min(edge_signed_distance(
                        &Shape::Point { x: p.0, y: p.1 },
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
    best
}

/// Interior shaft samples must not sit inside another molecule's skeleton.
pub(super) fn shaft_interior_clear_of_mols(scene: &Scene, clash: f64) {
    let mol_ids: Vec<String> = scene
        .viewports
        .iter()
        .filter_map(|v| v.id.clone())
        .collect();
    for d in overlay_edge_paths(scene) {
        let pts = sample_path(&d);
        if pts.len() < 2 {
            continue;
        }
        let mut acc = 0.0_f64;
        let mut segs: Vec<((f64, f64), (f64, f64), f64)> = Vec::new();
        for w in pts.windows(2) {
            let len = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            segs.push((w[0], w[1], len));
            acc += len;
        }
        if acc < 8.0 {
            continue;
        }
        let skip = (acc * 0.18).max(6.0);
        let mut along = 0.0;
        for (a, b, len) in segs {
            let n = ((len / 3.0).ceil() as usize).max(1);
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let p = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                let s = along + len * t;
                if s < skip || s > acc - skip {
                    continue;
                }
                for id in &mol_ids {
                    if let Some(vp) = scene.viewports.iter().find(|v| v.id.as_deref() == Some(id)) {
                        let inset = 8.0;
                        let inside = p.0 > vp.x + inset
                            && p.0 < vp.x + vp.width - inset
                            && p.1 > vp.y + inset
                            && p.1 < vp.y + vp.height - inset;
                        if inside {
                            let dist = skeleton_signed(scene, id, p);
                            assert!(
                                dist > clash,
                                "shaft interior {p:?} sits in {id} box (bond d={dist:.1}) path={d}"
                            );
                        }
                    }
                }
            }
            along += len;
        }
    }
}

fn region_box_aabbs(scene: &Scene) -> Vec<(f64, f64, f64, f64)> {
    let mut out = Vec::new();
    let mut consider = |p: &Primitive, ox: f64, oy: f64| {
        if let Primitive::Path {
            d, class: Some(c), ..
        } = p
        {
            if c.contains("region") && c.contains("box") {
                if let Some((x0, y0, x1, y1)) = path_aabb(d) {
                    out.push((x0 + ox, y0 + oy, x1 + ox, y1 + oy));
                }
            }
        }
    };
    for p in &scene.overlays {
        consider(p, 0.0, 0.0);
    }
    for vp in &scene.viewports {
        let (ox, oy) = vp.local_origin();
        for layer in &vp.layers {
            for p in &layer.primitives {
                consider(p, ox, oy);
            }
        }
    }
    out
}

#[test]
fn deuteromethane_paints_d_not_h_h() {
    let scene = render_json(r#"{"type":"mol","smiles":"[2H]C"}"#);
    let texts = data_texts(&scene);
    let joined = texts.join(" ");
    assert!(
        texts
            .iter()
            .any(|t| t.contains('D') || t.contains("²H") || t.contains("2H")),
        "deuteromethane must show D / ²H, got {texts:?}"
    );
    let h_only = texts
        .iter()
        .filter(|t| t.trim() == "H" || t.trim() == "H₂" || t.trim() == "H2")
        .count();
    assert!(h_only < 2, "must not paint as H—H: {joined}");
}

#[test]
fn carbon_13_methane_shows_mass() {
    let scene = render_json(r#"{"type":"mol","smiles":"[13CH4]"}"#);
    let texts = data_texts(&scene);
    assert!(
        texts.iter().any(|t| t.contains("13") || t.contains("¹³")),
        "¹³C methane must show the mass number, got {texts:?}"
    );
}

#[test]
fn star_bare_is_asterisk_not_carbon() {
    let scene = render_json(r#"{"type":"mol","smiles":"*C"}"#);
    let texts = data_texts(&scene);
    assert!(
        texts.iter().any(|t| t.contains('*')),
        "bare star must paint *, got {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t.trim() == "C" || t.trim() == "R"),
        "bare star must not fall back to C/R, got {texts:?}"
    );
}

#[test]
fn atom_label_greek_is_readable() {
    let scene = render_json(r#"{"type":"mol","smiles":"CCO","atom_label":{"0":"$\\alpha$"}}"#);
    let mut alpha_h = 0.0_f64;
    for vp in &scene.viewports {
        for layer in &vp.layers {
            for p in &layer.primitives {
                if let Primitive::Path {
                    d,
                    data_text: Some(t),
                    ..
                } = p
                {
                    if t.contains('α') || t.contains("alpha") {
                        if let Some((_, y0, _, y1)) = path_aabb(d) {
                            alpha_h = alpha_h.max(y1 - y0);
                        }
                    }
                }
            }
        }
    }
    assert!(
        alpha_h >= FONT_PX * 0.55,
        "greek α too small: height={alpha_h:.1} vs FONT_PX={FONT_PX}"
    );
}

#[test]
fn overlap_z_nested_boxes_show_containment() {
    let scene = render_json(
        r##"{
          "type":"mol",
          "smiles":"c1ccc2ccccc2c1",
          "region":[
            {"style":"box","atoms":[0,1,2,3,8,9,4,5,6,7],"text":"both","z":-40,
             "fill":{"color":"#48a","opacity":0.18},"stroke":{"color":"#48a"}},
            {"style":"box","atoms":[3,4,5,6,7,8],"text":"right","z":-20,
             "fill":{"color":"#c40","opacity":0.22},"stroke":{"color":"#c40"}}
          ]
        }"##,
    );
    let boxes = region_box_aabbs(&scene);
    assert_eq!(boxes.len(), 2, "two region boxes: {boxes:?}");
    let a = boxes[0];
    let b = boxes[1];
    let (inner, outer) = if (a.2 - a.0) * (a.3 - a.1) <= (b.2 - b.0) * (b.3 - b.1) {
        (a, b)
    } else {
        (b, a)
    };
    let pad = 2.0;
    assert!(
        inner.0 >= outer.0 - 0.5
            && inner.1 >= outer.1 - 0.5
            && inner.2 <= outer.2 + 0.5
            && inner.3 <= outer.3 + 0.5,
        "inner {inner:?} should sit inside outer {outer:?}"
    );
    let extra_w = (outer.2 - outer.0) - (inner.2 - inner.0);
    let extra_h = (outer.3 - outer.1) - (inner.3 - inner.1);
    assert!(
        extra_w > pad || extra_h > pad,
        "container must outsize the child on at least one axis: inner={inner:?} outer={outer:?}"
    );
    let mut child_label = None;
    for vp in &scene.viewports {
        let (ox, oy) = vp.local_origin();
        for layer in &vp.layers {
            for p in &layer.primitives {
                if let Primitive::Path {
                    d,
                    class: Some(c),
                    data_text: Some(t),
                    ..
                } = p
                {
                    if c.contains("region-label") && t.trim() == "right" {
                        if let Some((x0, y0, x1, y1)) = path_aabb(d) {
                            child_label = Some((x0 + ox, y0 + oy, x1 + ox, y1 + oy));
                        }
                    }
                }
            }
        }
    }
    let lab = child_label.expect("child region-label 'right'");
    assert!(
        lab.0 >= outer.0 - 0.5
            && lab.1 >= outer.1 - 0.5
            && lab.2 <= outer.2 + 0.5
            && lab.3 <= outer.3 + 0.5,
        "contained region label {lab:?} must sit inside outer box {outer:?}"
    );
}

#[test]
fn equilibrium_heads_join_shaft() {
    let scene = render_json(
        r#"{
          "type":"reaction_scheme",
          "children":[
            {"type":"mol","id":"A","smiles":"CCO"},
            {"type":"mol","id":"B","smiles":"CC=O"},
            {"type":"edge","sources":"A","targets":"B","arrow":"equilibrium"}
          ]
        }"#,
    );
    let mut shafts = Vec::new();
    let mut barbs = Vec::new();
    for p in &scene.overlays {
        if let Primitive::Path {
            d, class: Some(c), ..
        } = p
        {
            if c.contains("eq-") && c.contains("harpoon") {
                barbs.push(d.as_str());
            } else if c.contains("eq-") {
                shafts.push(d.as_str());
            }
        }
    }
    assert!(!shafts.is_empty() && !barbs.is_empty(), "eq shaft/harpoon");
    for barb in &barbs {
        let bn = path_nums(barb);
        assert!(bn.len() >= 2);
        let (bx, by) = (bn[0], bn[1]);
        let mut best = f64::INFINITY;
        for shaft in &shafts {
            let sn = path_nums(shaft);
            let mut i = 0;
            while i + 1 < sn.len() {
                best = best.min((sn[i] - bx).hypot(sn[i + 1] - by));
                i += 2;
            }
        }
        assert!(
            best < 1.5,
            "harpoon start must lie on a shaft: gap={best:.2} barb={barb}"
        );
    }
}

#[test]
fn chrome_on_scheme_callout_misses_atom_number() {
    let scene = render_json(
        r##"{
          "type":"reaction_scheme",
          "children":[
            {"type":"mol","id":"ph","smiles":"c1ccccc1O","name":"phenol",
             "atom_number":"index",
             "region":{"style":"box","atoms":[0,1,2,3,4,5],
               "fill":{"color":"#48a","opacity":0.12},"stroke":{"color":"#48a"},"text":"ring"},
             "callout":{"atoms":[6],"text":"OH","arrow":true}},
            {"type":"mol","id":"et","smiles":"CCO","name":"ethanol"},
            {"type":"edge","sources":"ph","targets":"et","arrow":"forward"}
          ]
        }"##,
    );
    let ph = scene
        .viewports
        .iter()
        .find(|v| v.id.as_deref() == Some("ph"))
        .expect("phenol");
    let (ox, oy) = ph.local_origin();
    let mut six: Option<(f64, f64, f64, f64)> = None;
    let mut arrows: Vec<((f64, f64), (f64, f64))> = Vec::new();
    for layer in &ph.layers {
        for p in &layer.primitives {
            match p {
                Primitive::Path {
                    d,
                    data_text: Some(t),
                    class: Some(c),
                    ..
                } if t.trim() == "6" && c.contains("label") => {
                    if let Some((x0, y0, x1, y1)) = path_aabb(d.as_str()) {
                        six = Some((x0 + ox, y0 + oy, x1 + ox, y1 + oy));
                    }
                }
                Primitive::Path {
                    d, class: Some(c), ..
                } if c.contains("callout-arrow") => {
                    let pts = sample_path(d.as_str());
                    if pts.len() >= 2 {
                        let a = pts[0];
                        let b = pts[pts.len() - 1];
                        arrows.push(((a.0 + ox, a.1 + oy), (b.0 + ox, b.1 + oy)));
                    }
                }
                _ => {}
            }
        }
    }
    let six = six.expect("atom number 6");
    assert!(!arrows.is_empty(), "callout arrow");
    for (from, to) in arrows {
        let dist = super::layout_items::Aabb::new(six.0, six.1, six.2, six.3);
        let n = 20;
        let mut hit = false;
        for i in 0..=n {
            let t = i as f64 / n as f64;
            let x = from.0 + (to.0 - from.0) * t;
            let y = from.1 + (to.1 - from.1) * t;
            if dist.covers_point((x, y), 0.8) {
                hit = true;
                break;
            }
        }
        assert!(!hit, "callout shaft {from:?}→{to:?} hits number 6 {six:?}");
    }
}

fn multi_product_spec() -> &'static str {
    r#"{
      "type":"reaction_scheme",
      "children":[
        {"type":"mol","id":"A","smiles":"CCO"},
        {"type":"mol","id":"B","smiles":"CC=O"},
        {"type":"mol","id":"C","smiles":"C"},
        {"type":"edge","sources":"A","targets":["B","C"],"arrow":"forward"}
      ]
    }"#
}

fn multi_reactant_spec() -> &'static str {
    r#"{
      "type":"reaction_scheme",
      "children":[
        {"type":"mol","id":"A","smiles":"C"},
        {"type":"mol","id":"B","smiles":"O"},
        {"type":"mol","id":"C","smiles":"CO"},
        {"type":"edge","sources":["A","B"],"targets":"C","arrow":"forward"}
      ]
    }"#
}

fn multi_both_spec() -> &'static str {
    r#"{
      "type":"reaction_scheme",
      "children":[
        {"type":"mol","id":"A","smiles":"C"},
        {"type":"mol","id":"B","smiles":"O"},
        {"type":"mol","id":"C","smiles":"CO"},
        {"type":"mol","id":"D","smiles":"O=O"},
        {"type":"edge","sources":["A","B"],"targets":["C","D"],"arrow":"forward"}
      ]
    }"#
}

fn fan_spec() -> &'static str {
    r#"{
      "type":"reaction_scheme",
      "children":[
        {"type":"mol","id":"src","smiles":"c1ccccc1O","name":"phenol"},
        {"type":"mol","id":"t0","smiles":"CC(=O)O"},
        {"type":"mol","id":"t1","smiles":"CCO"},
        {"type":"mol","id":"t2","smiles":"c1ccccc1"},
        {"type":"mol","id":"t3","smiles":"c1ccncc1"},
        {"type":"mol","id":"t4","smiles":"C1CCCCC1"},
        {"type":"mol","id":"t5","smiles":"CC=O"},
        {"type":"mol","id":"t6","smiles":"O=C(C)C"},
        {"type":"edge","sources":"src","targets":"t0","arrow":"forward"},
        {"type":"edge","sources":"src","targets":"t1","arrow":"forward"},
        {"type":"edge","sources":"src","targets":"t2","arrow":"forward"},
        {"type":"edge","sources":"src","targets":"t3","arrow":"forward"},
        {"type":"edge","sources":"src","targets":"t4","arrow":"forward"},
        {"type":"edge","sources":"src","targets":"t5","arrow":"forward"},
        {"type":"edge","sources":"src","targets":"t6","arrow":"forward"}
      ]
    }"#
}

#[test]
fn multi_endpoint_schemes_paint_plus() {
    for (name, json) in [
        ("multi-product", multi_product_spec()),
        ("multi-reactant", multi_reactant_spec()),
        ("multi-both", multi_both_spec()),
    ] {
        let scene = render_json(json);
        assert!(
            has_scheme_plus(&scene),
            "{name} must show a + between co-endpoint mols"
        );
    }
}

#[test]
fn multi_product_shafts_clear_chem() {
    shaft_interior_clear_of_mols(&render_json(multi_product_spec()), -1.5);
}

#[test]
fn multi_reactant_shafts_clear_chem() {
    shaft_interior_clear_of_mols(&render_json(multi_reactant_spec()), -1.5);
}

#[test]
fn multi_both_shafts_clear_chem() {
    shaft_interior_clear_of_mols(&render_json(multi_both_spec()), -1.5);
}

#[test]
fn fan_shafts_clear_chem() {
    shaft_interior_clear_of_mols(&render_json(fan_spec()), -1.5);
}
