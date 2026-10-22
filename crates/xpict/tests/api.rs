use xpict::{mol, scene_to_svg, Error, Mol, MolRenderOptions};

#[test]
fn ethanol_svg_has_root_and_paths() {
    let mut m = mol("CCO").expect("parse");
    let rendered = m.render(MolRenderOptions::default()).expect("render");
    let svg = rendered.to_svg();
    assert!(svg.contains("<svg"), "{svg}");
    assert!(svg.contains("viewBox="), "{svg}");
    assert!(svg.contains(r#"class="xpict-mol""#), "{svg}");
    assert!(svg.contains("data-id="), "{svg}");
    assert!(!svg.contains("<?xml"), "{svg}");
    assert!(
        svg.contains("<path") || svg.contains("<circle"),
        "expected ink primitives: {svg}"
    );
    assert!(rendered.width > 0.0 && rendered.height > 0.0);
    assert_eq!(rendered.molecule.atoms.len(), 3);
    assert_eq!(rendered.molecule.bonds.len(), 2);
    assert!(!rendered.frame_molblock.is_empty());
}

#[test]
fn benzene_scene_roundtrip_svg() {
    let mut m = Mol::from_source("c1ccccc1").unwrap();
    let r = m.render(MolRenderOptions::default()).unwrap();
    assert_eq!(r.molecule.atoms.len(), 6);
    let svg = scene_to_svg(&r.scene);
    assert!(svg.starts_with("<svg"), "{svg}");
    assert!(!svg.contains("<?xml"), "{svg}");
    assert!(svg.contains("xpict-mol"), "{svg}");
}

#[test]
fn cx_star_labels_by_index() {
    let mut m = mol("*C* |$;;R2;$|").unwrap();
    let r = m.render(MolRenderOptions::default()).unwrap();
    let stars: Vec<_> = r
        .molecule
        .atoms
        .iter()
        .filter(|a| a.element.as_deref() == Some("*") || a.z == Some(0))
        .collect();
    assert!(stars.len() >= 2, "expected star atoms");
    // Second star (index 2 in *C*) gets R2 from CX trailer.
    let labeled = r
        .molecule
        .atoms
        .iter()
        .find(|a| a.label.as_deref() == Some("R2"));
    assert!(
        labeled.is_some(),
        "CX R2 label missing: {:?}",
        r.molecule.atoms
    );
}

#[test]
fn mark_atoms_land_in_scene() {
    let mut m = mol("CCO").unwrap();
    let r = m
        .render(MolRenderOptions {
            mark_atoms: Some(vec![2]),
            color: Some("#0b6e4f".into()),
            ..Default::default()
        })
        .unwrap();
    let svg = r.to_svg();
    assert!(svg.contains("circle") || svg.contains("path"), "{svg}");
    assert!(!r.molecule.mark_atoms.is_empty());
}

#[test]
fn circle_bonds_land_in_svg() {
    use xpict::{CircleField, CircleSpec, StrokeSpec};

    let mut m = mol("CCO").unwrap();
    let r = m
        .render(MolRenderOptions {
            circle: Some(CircleField::One(CircleSpec {
                atoms: vec![],
                bonds: vec![(0, 1)],
                ring: None,
                fill: None,
                stroke: Some(StrokeSpec {
                    color: Some("#c00".into()),
                    width: Some(1.4),
                    dash: None,
                }),
                scale: None,
                z: None,
            })),
            ..Default::default()
        })
        .unwrap();
    let svg = r.to_svg();
    assert!(svg.contains("bond-circle"), "{svg}");
    assert!(svg.contains("#c00"), "{svg}");
}

#[test]
fn align_to_rendered_pose() {
    let mut a = mol("CCO").unwrap();
    let home = a.render(MolRenderOptions::default()).unwrap();
    let mut b = mol("CCCO").unwrap();
    let aligned = b
        .render(MolRenderOptions {
            align_to: Some(home.source.clone()),
            ..Default::default()
        })
        .unwrap();
    assert!(!aligned.frame_molblock.is_empty());
    assert_eq!(aligned.molecule.atoms.len(), 4);
}

fn align_pair(template: &str, query: &str) -> (xpict::Rendered, xpict::Rendered) {
    use xpict::{depict, AlignTo, DepictSpec, MolNode, Node};
    let out = depict(&DepictSpec::Group {
        id: None,
        align: false,
        children: vec![
            Node::Mol(MolNode {
                smiles: Some(template.into()),
                id: Some("m_0".into()),
                ..Default::default()
            }),
            Node::Mol(MolNode {
                smiles: Some(query.into()),
                id: Some("m_1".into()),
                align_to: Some(AlignTo::Ref("m_0".into())),
                ..Default::default()
            }),
        ],
        opts: None,
        color: None,
        scale: None,
        aromatic: None,
        debug_box: None,
        debug_align: None,
        pad: None,
    })
    .unwrap();
    assert_eq!(out.len(), 2, "expected template + query");
    (out[0].clone(), out[1].clone())
}

fn frame_xy(frame: &str) -> std::collections::HashMap<i32, (f64, f64)> {
    let v: serde_json::Value = serde_json::from_str(frame).expect("frame json");
    v["atoms"]
        .as_array()
        .expect("atoms")
        .iter()
        .map(|a| {
            (
                a["index"].as_i64().expect("index") as i32,
                (a["x"].as_f64().expect("x"), a["y"].as_f64().expect("y")),
            )
        })
        .collect()
}

fn align_tags(query: &xpict::Rendered) -> Vec<(i32, i32)> {
    query
        .molecule
        .align_debug
        .as_ref()
        .map(|d| d.tags.clone())
        .unwrap_or_default()
}

fn frame_median(template: &xpict::Rendered, query: &xpict::Rendered) -> f64 {
    let pref = frame_xy(template.frame());
    let qref = frame_xy(query.frame());
    let mut dists: Vec<f64> = align_tags(query)
        .into_iter()
        .filter_map(|(q, r)| {
            let (qx, qy) = qref.get(&q)?;
            let (rx, ry) = pref.get(&r)?;
            Some((*qx - *rx).hypot(*qy - *ry))
        })
        .collect();
    assert!(!dists.is_empty(), "no tagged frame pairs");
    dists.sort_by(|a, b| a.partial_cmp(b).unwrap());
    dists[dists.len() / 2]
}

fn assert_mcs_tags(query: &xpict::Rendered, min: usize, label: &str) {
    let n = align_tags(query).len();
    assert!(n >= min, "{label}: expected ≥{min} MCS tags, got {n}");
}

#[test]
fn align_mcs_ethyl_pentyl_both_ways() {
    let (e, p_on_e) = align_pair("c1ccccc1CC", "c1ccccc1CCCCC");
    assert_mcs_tags(&p_on_e, 8, "pentyl→ethyl");
    let (p, e_on_p) = align_pair("c1ccccc1CCCCC", "c1ccccc1CC");
    assert_mcs_tags(&e_on_p, 8, "ethyl→pentyl");
    let _ = (e, p);
}

#[test]
fn align_mcs_phenol_quinone() {
    let (ph, q_on_ph) = align_pair("c1ccc(O)cc1", "O=C1C=CC(=O)C=C1");
    assert_mcs_tags(&q_on_ph, 6, "quinone→phenol");
    assert!(
        frame_median(&ph, &q_on_ph) < 0.5,
        "quinone should sit on phenol pose"
    );
}

#[test]
fn align_mcs_aniline_quinone_imine() {
    let (a, im_on_a) = align_pair("Nc1ccccc1", "O=C1C=CC(=N)C=C1");
    assert_mcs_tags(&im_on_a, 6, "imine→aniline");
    assert!(
        frame_median(&a, &im_on_a) < 0.5,
        "imine should sit on aniline pose"
    );
}

#[test]
fn align_aliphatic_vs_quinone_still_renders() {
    let (_q, aligned) = align_pair("O=C1C=CC(=O)C=C1", "C1CCCCC1CCOCCCCCC");
    assert_eq!(aligned.molecule.atoms.len(), 15);
    assert!(!aligned.to_svg().is_empty());
}

/// C1CCCOC1 ← O=CCCCCO: MCS without an explicit atom_map.
#[test]
fn align_atom_map_thp_chain() {
    let (r, aligned) = align_pair("C1CCCOC1", "O=CCCCCO");
    assert_mcs_tags(&aligned, 4, "chain→THP");
    assert!(frame_median(&r, &aligned) < 0.5, "chain should sit on THP");
}

/// Asymmetric para-halo pair — F coincides under any valid embedding.
#[test]
fn align_asymmetric_para_halo() {
    let (t, q) = align_pair("Fc1ccc(Cl)cc1", "Fc1ccc(Br)cc1");
    assert_mcs_tags(&q, 7, "F-Cl→F-Br");
    assert!(frame_median(&t, &q) < 0.5, "para-halo pose");
}

/// Multiple queries onto one source must not rewrite the template frame.
#[test]
fn align_multi_query_leaves_template_frame() {
    let mut tmpl = mol("c1ccc(O)cc1").unwrap();
    let t0 = tmpl.render(MolRenderOptions::default()).unwrap();
    let pose = t0.frame().to_string();

    for smi in ["O=C1C=CC(=O)C=C1", "c1ccccc1CC", "Fc1ccccc1O"] {
        let (_t, aligned) = align_pair("c1ccc(O)cc1", smi);
        assert_mcs_tags(&aligned, 6, smi);
        let mut q = mol(smi).unwrap();
        let _ = q
            .render(MolRenderOptions {
                align_to: Some(t0.source.clone()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(pose, t0.frame());
    }
}

#[test]
fn depict_nested_group() {
    use xpict::{depict, DepictSpec, MolNode, Node};
    let out = depict(&DepictSpec::Group {
        id: None,
        align: false,
        children: vec![
            Node::Mol(MolNode {
                smiles: Some("CCO".into()),
                color: Some("#111".into()),
                ..Default::default()
            }),
            Node::Mol(MolNode {
                smiles: Some("CCCO".into()),
                ..Default::default()
            }),
        ],
        opts: None,
        color: None,
        scale: None,
        aromatic: None,
        debug_box: None,
        debug_align: None,
        pad: None,
    })
    .unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].molecule.atoms.len(), 3);
    assert_eq!(out[1].molecule.atoms.len(), 4);
    assert!(!out[0].to_svg().is_empty());
}

#[test]
fn depict_cx_markush_alias() {
    use xpict::{depict, DepictSpec, MolNode};
    let out = depict(&DepictSpec::Mol(MolNode {
        smiles: None,
        cxsmiles: Some("*C |$R1;$|".into()),
        molfile: None,
        id: None,
        color: None,
        shade: None,
        star_labels: None,
        scale: None,
        weight: None,
        align_to: None,
        ..Default::default()
    }))
    .unwrap();
    let svg = out[0].to_svg();
    assert!(
        svg.contains("data-text=\"R1\"") || svg.contains(">R1<"),
        "expected literal R1 from CX alias, got snippet {}",
        &svg[..svg.len().min(200)]
    );
}

#[test]
fn depict_star_labels() {
    use xpict::{depict, DepictSpec, MolNode};
    let out = depict(&DepictSpec::Mol(MolNode {
        smiles: Some("*C".into()),
        cxsmiles: None,
        molfile: None,
        id: None,
        color: None,
        shade: None,
        star_labels: Some(vec![Some("$R_1$".into())]),
        scale: None,
        weight: None,
        align_to: None,
        ..Default::default()
    }))
    .unwrap();
    let svg = out[0].to_svg();
    assert!(
        svg.contains("data-text=\"R₁\"") || svg.contains(">R₁<"),
        "expected R₁ from document star_labels, got snippet {}",
        &svg[..svg.len().min(200)]
    );
}

#[test]
fn empty_source_errors() {
    let err = Mol::from_source("   ").unwrap_err();
    assert!(matches!(err, Error::EmptySource));
}
