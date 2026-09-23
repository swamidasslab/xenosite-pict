use xpict::{mol, scene_to_svg, Error, Mol, MolRenderOptions};

#[test]
fn ethanol_svg_has_root_and_paths() {
    let mut m = mol("CCO").expect("parse");
    let rendered = m.render(MolRenderOptions::default()).expect("render");
    let svg = rendered.to_svg();
    assert!(svg.contains("<svg"), "{svg}");
    assert!(svg.contains("viewBox="), "{svg}");
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
    assert!(svg.starts_with("<?xml") || svg.contains("<svg"));
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
    assert!(labeled.is_some(), "CX R2 label missing: {:?}", r.molecule.atoms);
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
fn align_to_rendered_pose() {
    let mut a = mol("CCO").unwrap();
    let home = a.render(MolRenderOptions::default()).unwrap();
    let mut b = mol("CCCO").unwrap();
    let aligned = b
        .render(MolRenderOptions {
            align_to: Some(home.frame().to_string()),
            ..Default::default()
        })
        .unwrap();
    assert!(!aligned.frame_molblock.is_empty());
    assert_eq!(aligned.molecule.atoms.len(), 4);
}

fn near(ax: f64, ay: f64, bx: f64, by: f64, tol: f64) -> bool {
    (ax - bx).hypot(ay - by) < tol
}

/// MCS overlay: at least `min_hits` query atoms land on some template atom.
fn assert_mcs_overlay(query: &xpict::Rendered, template: &xpict::Rendered, min_hits: usize, label: &str) {
    let mut hits = 0usize;
    for a in &query.molecule.atoms {
        if template
            .molecule
            .atoms
            .iter()
            .any(|b| near(a.x, a.y, b.x, b.y, 0.2))
        {
            hits += 1;
        }
    }
    assert!(
        hits >= min_hits,
        "{label}: expected ≥{min_hits} MCS hits, got {hits}"
    );
}

#[test]
fn align_mcs_ethyl_pentyl_both_ways() {
    let mut ethyl = mol("c1ccccc1CC").unwrap();
    let e = ethyl.render(MolRenderOptions::default()).unwrap();
    let mut pentyl = mol("c1ccccc1CCCCC").unwrap();
    let p_on_e = pentyl
        .render(MolRenderOptions {
            align_to: Some(e.frame().to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_mcs_overlay(&p_on_e, &e, 8, "pentyl→ethyl");

    let mut pentyl2 = mol("c1ccccc1CCCCC").unwrap();
    let p = pentyl2.render(MolRenderOptions::default()).unwrap();
    let mut ethyl2 = mol("c1ccccc1CC").unwrap();
    let e_on_p = ethyl2
        .render(MolRenderOptions {
            align_to: Some(p.frame().to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_mcs_overlay(&e_on_p, &p, 8, "ethyl→pentyl");
}

#[test]
fn align_mcs_phenol_quinone() {
    let mut phenol = mol("c1ccc(O)cc1").unwrap();
    let ph = phenol.render(MolRenderOptions::default()).unwrap();
    let mut quinone = mol("O=C1C=CC(=O)C=C1").unwrap();
    let q_on_ph = quinone
        .render(MolRenderOptions {
            align_to: Some(ph.frame().to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_mcs_overlay(&q_on_ph, &ph, 6, "quinone→phenol");
}

/// Asymmetric para-halo pair — F coincides under any valid embedding.
#[test]
fn align_asymmetric_para_halo() {
    let mut tmpl = mol("Fc1ccc(Cl)cc1").unwrap();
    let t = tmpl.render(MolRenderOptions::default()).unwrap();
    let mut query = mol("Fc1ccc(Br)cc1").unwrap();
    let q = query
        .render(MolRenderOptions {
            align_to: Some(t.frame().to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_mcs_overlay(&q, &t, 7, "F-Cl→F-Br");
    let f_t = t
        .molecule
        .atoms
        .iter()
        .find(|a| a.symbol() == "F")
        .expect("template F");
    let f_q = q
        .molecule
        .atoms
        .iter()
        .find(|a| a.symbol() == "F")
        .expect("query F");
    assert!(
        near(f_t.x, f_t.y, f_q.x, f_q.y, 0.2),
        "asymmetric: F atoms should coincide"
    );
}

/// Multiple queries onto one pose must not rewrite the template frame.
#[test]
fn align_multi_query_leaves_template_frame() {
    let mut tmpl = mol("c1ccc(O)cc1").unwrap();
    let t0 = tmpl.render(MolRenderOptions::default()).unwrap();
    let frame = t0.frame().to_string();
    let before: Vec<(i32, f64, f64)> = t0
        .molecule
        .atoms
        .iter()
        .map(|a| (a.index, a.x, a.y))
        .collect();

    for smi in ["O=C1C=CC(=O)C=C1", "c1ccccc1CC", "Fc1ccccc1O"] {
        let mut q = mol(smi).unwrap();
        let aligned = q
            .render(MolRenderOptions {
                align_to: Some(frame.clone()),
                ..Default::default()
            })
            .unwrap();
        assert_mcs_overlay(&aligned, &t0, 6, smi);
        // Cached pose string is unchanged input — queries never write back.
        assert_eq!(frame, t0.frame());
    }

    // Align phenol onto the frozen frame: atoms must sit on the original pose
    // (any symmetric ring flip still overlays the same point set).
    let mut again = mol("c1ccc(O)cc1").unwrap();
    let on_frame = again
        .render(MolRenderOptions {
            align_to: Some(frame),
            ..Default::default()
        })
        .unwrap();
    for a in &on_frame.molecule.atoms {
        let hit = before
            .iter()
            .any(|&(_, x, y)| near(a.x, a.y, x, y, 0.2));
        assert!(hit, "atom {} not on original template pose", a.index);
    }
    let _ = before;
}

#[test]
fn depict_nested_group() {
    use xpict::{depict, DepictSpec, MolNode};
    let out = depict(&DepictSpec::Group {
        id: None,
        children: vec![
            MolNode {
                smiles: Some("CCO".into()),
                color: Some("#111".into()),
                ..Default::default()
            },
            MolNode {
                smiles: Some("CCCO".into()),
                ..Default::default()
            },
        ],
    })
    .unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].molecule.atoms.len(), 3);
    assert_eq!(out[1].molecule.atoms.len(), 4);
    assert!(!out[0].to_svg().is_empty());
}

#[test]
fn depict_cx_markush_alias() {
    use xpict::{depict, DepictSpec};
    let out = depict(&DepictSpec::Mol {
        smiles: None,
        cxsmiles: Some("*C |$R1;$|".into()),
        molfile: None,
        id: None,
        color: None,
        shade: None,
        star_labels: None,
    })
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
    use xpict::{depict, DepictSpec};
    let out = depict(&DepictSpec::Mol {
        smiles: Some("*C".into()),
        cxsmiles: None,
        molfile: None,
        id: None,
        color: None,
        shade: None,
        star_labels: Some(vec![Some("$R_1$".into())]),
    })
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
