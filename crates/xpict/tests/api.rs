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

#[test]
fn empty_source_errors() {
    let err = Mol::from_source("   ").unwrap_err();
    assert!(matches!(err, Error::EmptySource));
}
