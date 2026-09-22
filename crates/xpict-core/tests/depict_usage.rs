//! Usage-facing depiction smoke tests (JSON ABI + paint).
//!
//! Python/JS call `depict_molecule` with `MoleculeIn` JSON. These cases are
//! the labels and bonds a chemist would notice first if broken.

use xpict_core::depict_molecule;
use xpict_core::scene::{AtomIn, BondIn, LayerName, MoleculeIn, Primitive, Scene};

fn atom(index: i32, element: &str, x: f64, y: f64, label: Option<&str>) -> AtomIn {
    atom_charged(index, element, x, y, label, 0)
}

fn atom_charged(
    index: i32,
    element: &str,
    x: f64,
    y: f64,
    label: Option<&str>,
    charge: i32,
) -> AtomIn {
    AtomIn {
        index,
        element: Some(element.into()),
        z: None,
        x,
        y,
        label: label.map(str::to_string),
        charge,
    }
}

fn label_path_d(scene: &Scene, needle: &str) -> Option<String> {
    scene.viewports[0]
        .layers
        .iter()
        .find(|l| l.name == LayerName::Labels)?
        .primitives
        .iter()
        .find_map(|p| match p {
            Primitive::Path {
                data_text: Some(t),
                d,
                ..
            } if t == needle || t.contains(needle) => Some(d.clone()),
            _ => None,
        })
}

fn bond(index: i32, begin: i32, end: i32, order: f64) -> BondIn {
    BondIn {
        index,
        begin,
        end,
        order,
        stereo: None,
        interior: None,
    }
}

fn label_texts(scene: &Scene) -> Vec<String> {
    scene.viewports[0]
        .layers
        .iter()
        .find(|l| l.name == LayerName::Labels)
        .expect("labels layer")
        .primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Path {
                data_text: Some(t),
                d,
                ..
            } if !d.is_empty() => Some(t.clone()),
            _ => None,
        })
        .collect()
}

fn bond_path_count(scene: &Scene) -> usize {
    scene.viewports[0]
        .layers
        .iter()
        .find(|l| l.name == LayerName::Bonds)
        .map(|l| l.primitives.len())
        .unwrap_or(0)
}

#[test]
fn ethanol_oh_and_single_bonds() {
    let mol = MoleculeIn {
        id: Some("etoh".into()),
        atoms: vec![
            atom(0, "C", 0.0, 0.0, None),
            atom(1, "C", 20.0, 0.0, None),
            atom(2, "O", 40.0, 10.0, Some("OH")),
        ],
        bonds: vec![bond(0, 0, 1, 1.0), bond(1, 1, 2, 1.0)],
        color: None,
        atom_shade: None,
        bond_shade: None,
        mark_atoms: vec![],
        mark_bonds: vec![],
        bold_labels: false,
    };
    let scene = depict_molecule(&mol);
    assert!(scene.width > 0.0 && scene.height > 0.0);
    assert_eq!(bond_path_count(&scene), 2);
    let texts = label_texts(&scene);
    assert!(
        texts.iter().any(|t| t == "OH" || t == "HO"),
        "expected OH/HO, got {texts:?}"
    );
}

#[test]
fn braced_rgroup_markup_subscripts_on_star() {
    // Callers must pass chem markup (``R_{1}`` / ``$R_1$``). Bare ``R1`` is
    // literal — do not invent ad-hoc CX alias rewriting in paint.
    let mol = MoleculeIn {
        id: Some("markush".into()),
        atoms: vec![
            atom(0, "C", 0.0, 0.0, None),
            atom(1, "*", 20.0, 0.0, Some("R_{1}")),
            atom(2, "Cl", -20.0, 0.0, Some("Cl")),
        ],
        bonds: vec![bond(0, 0, 1, 1.0), bond(1, 0, 2, 1.0)],
        color: None,
        atom_shade: None,
        bond_shade: None,
        mark_atoms: vec![],
        mark_bonds: vec![],
        bold_labels: false,
    };
    let texts = label_texts(&depict_molecule(&mol));
    assert!(
        texts.iter().any(|t| t == "R₁"),
        "R_{{1}} on * must become R₁, got {texts:?}"
    );
}

#[test]
fn amine_nh2_subscript_in_scene() {
    let mol = MoleculeIn {
        id: None,
        atoms: vec![
            atom(0, "C", 0.0, 0.0, None),
            atom(1, "N", 20.0, 0.0, Some("NH2")),
        ],
        bonds: vec![bond(0, 0, 1, 1.0)],
        color: None,
        atom_shade: None,
        bond_shade: None,
        mark_atoms: vec![],
        mark_bonds: vec![],
        bold_labels: false,
    };
    let texts = label_texts(&depict_molecule(&mol));
    assert!(
        texts.iter().any(|t| t == "NH₂" || t == "H₂N"),
        "expected NH₂/H₂N, got {texts:?}"
    );
}

#[test]
fn ammonium_charge_superscript() {
    let mol = MoleculeIn {
        id: None,
        atoms: vec![
            atom(0, "C", 0.0, 0.0, None),
            atom(1, "N", 20.0, 0.0, Some("NH4+")),
        ],
        bonds: vec![bond(0, 0, 1, 1.0)],
        color: None,
        atom_shade: None,
        bond_shade: None,
        mark_atoms: vec![],
        mark_bonds: vec![],
        bold_labels: false,
    };
    let texts = label_texts(&depict_molecule(&mol));
    assert!(
        texts.iter().any(|t| t.contains('₄') && (t.contains('⁺') || t.contains('+'))),
        "expected NH₄⁺, got {texts:?}"
    );
}

#[test]
fn markush_and_star_markup_labels() {
    let mol = MoleculeIn {
        id: Some("markush".into()),
        atoms: vec![
            atom(0, "C", 0.0, 0.0, None),
            atom(1, "*", 20.0, 0.0, Some("$R_1$")),
            atom(2, "*", 0.0, 20.0, Some("R^2")),
            atom(3, "*", -20.0, 0.0, Some(r"$\alpha$")),
            atom(4, "*", 0.0, -20.0, Some("*")),
        ],
        bonds: vec![
            bond(0, 0, 1, 1.0),
            bond(1, 0, 2, 1.0),
            bond(2, 0, 3, 1.0),
            bond(3, 0, 4, 1.0),
        ],
        color: None,
        atom_shade: None,
        bond_shade: None,
        mark_atoms: vec![],
        mark_bonds: vec![],
        bold_labels: false,
    };
    let texts = label_texts(&depict_molecule(&mol));
    let joined = texts.join("|");
    assert!(joined.contains('₁') || texts.iter().any(|t| t == "R₁"), "R₁ missing: {texts:?}");
    assert!(joined.contains('²') || texts.iter().any(|t| t == "R²"), "R² missing: {texts:?}");
    assert!(texts.iter().any(|t| t.contains('α')), "α missing: {texts:?}");
    assert!(texts.iter().any(|t| t == "*"), "star missing: {texts:?}");
}

#[test]
fn bare_underscore_alias_not_subscripted() {
    let mol = MoleculeIn {
        id: None,
        atoms: vec![
            atom(0, "C", 0.0, 0.0, None),
            atom(1, "*", 20.0, 0.0, Some("my_name")),
        ],
        bonds: vec![bond(0, 0, 1, 1.0)],
        color: None,
        atom_shade: None,
        bond_shade: None,
        mark_atoms: vec![],
        mark_bonds: vec![],
        bold_labels: false,
    };
    let texts = label_texts(&depict_molecule(&mol));
    assert!(
        texts.iter().any(|t| t == "my_name"),
        "bare _ must stay literal, got {texts:?}"
    );
}

#[test]
fn acetone_double_bond_and_carbonyl_label() {
    let mol = MoleculeIn {
        id: None,
        atoms: vec![
            atom(0, "C", -20.0, 8.0, None),
            atom(1, "C", 0.0, 0.0, None),
            atom(2, "O", 0.0, -20.0, Some("O")),
            atom(3, "C", 20.0, 8.0, None),
        ],
        bonds: vec![
            bond(0, 0, 1, 1.0),
            bond(1, 1, 2, 2.0),
            bond(2, 1, 3, 1.0),
        ],
        color: Some("#336699".into()),
        atom_shade: None,
        bond_shade: None,
        mark_atoms: vec![],
        mark_bonds: vec![],
        bold_labels: false,
    };
    let scene = depict_molecule(&mol);
    // Double bond → more than one stroke path (centered pair).
    assert!(
        bond_path_count(&scene) >= 3,
        "expected multi-stroke double + singles, got {}",
        bond_path_count(&scene)
    );
    let texts = label_texts(&scene);
    assert!(texts.iter().any(|t| t == "O"), "carbonyl O label: {texts:?}");
}

#[test]
fn json_abi_roundtrip_then_depict() {
    let mol = MoleculeIn {
        id: Some("json".into()),
        atoms: vec![
            atom(0, "C", 0.0, 0.0, None),
            atom(1, "O", 20.0, 0.0, Some("OH")),
            atom(2, "*", 0.0, 20.0, Some("$R_1$")),
        ],
        bonds: vec![bond(0, 0, 1, 1.0), bond(1, 0, 2, 1.0)],
        color: None,
        atom_shade: None,
        bond_shade: None,
        mark_atoms: vec![1],
        mark_bonds: vec![],
        bold_labels: true,
    };
    let json = serde_json::to_string(&mol).expect("serialize");
    let back: MoleculeIn = serde_json::from_str(&json).expect("deserialize");
    assert!(back.bold_labels);
    assert_eq!(back.atoms[2].label.as_deref(), Some("$R_1$"));
    let scene = depict_molecule(&back);
    let texts = label_texts(&scene);
    assert!(texts.iter().any(|t| t == "OH" || t == "HO"));
    assert!(texts.iter().any(|t| t == "R₁"));
    // Marks layer present when mark_atoms set.
    assert!(
        scene.viewports[0]
            .layers
            .iter()
            .any(|l| l.name == LayerName::Marks && !l.primitives.is_empty()),
        "expected mark primitives"
    );
}

#[test]
fn empty_molecule_does_not_panic() {
    let mol = MoleculeIn {
        id: None,
        atoms: vec![],
        bonds: vec![],
        color: None,
        atom_shade: None,
        bond_shade: None,
        mark_atoms: vec![],
        mark_bonds: vec![],
        bold_labels: false,
    };
    let scene = depict_molecule(&mol);
    assert_eq!(scene.viewports.len(), 1);
}

#[test]
fn element_helpers_match_common_symbols() {
    assert_eq!(xpict_core::element_symbol(1), "H");
    assert_eq!(xpict_core::element_symbol(6), "C");
    assert_eq!(xpict_core::element_symbol(8), "O");
    assert_eq!(xpict_core::atomic_number("C"), Some(6));
    assert_eq!(xpict_core::atomic_number("cl"), Some(17));
    assert_eq!(xpict_core::atomic_number("Xx"), None);
}

#[test]
fn kabsch_identity_and_rigid_align_smoke() {
    let pts = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)];
    let xf = xpict_core::kabsch_2d(&pts, &pts, true);
    assert!((xf.cos - 1.0).abs() < 1e-9);
    assert!(xf.sin.abs() < 1e-9);
    let template = vec![(0, 0.0, 0.0), (1, 10.0, 0.0)];
    let other = vec![(0, 5.0, 5.0), (1, 15.0, 5.0)];
    let mapping = vec![(0, 0), (1, 1)];
    let (out, _xf) = xpict_core::rigid_align_coords(&template, &other, &mapping);
    assert_eq!(out.len(), 2);
    // After align, first mapped point lands on template.
    assert!((out[0].1 - 0.0).abs() < 1e-6);
    assert!((out[0].2 - 0.0).abs() < 1e-6);
}

#[test]
fn west_oh_flips_to_ho_in_scene() {
    // Neighbor east of O → West orientation → "HO".
    let mol = MoleculeIn {
        id: None,
        atoms: vec![
            atom(0, "C", 40.0, 0.0, None),
            atom(1, "O", 0.0, 0.0, Some("OH")),
        ],
        bonds: vec![bond(0, 0, 1, 1.0)],
        color: None,
        atom_shade: None,
        bond_shade: None,
        mark_atoms: vec![],
        mark_bonds: vec![],
        bold_labels: false,
    };
    let texts = label_texts(&depict_molecule(&mol));
    assert!(
        texts.iter().any(|t| t == "HO"),
        "expected west-flipped HO, got {texts:?}"
    );
}

#[test]
fn bold_labels_change_oh_path_ink() {
    let mut mol = MoleculeIn {
        id: None,
        atoms: vec![
            atom(0, "C", 0.0, 0.0, None),
            atom(1, "O", 20.0, 0.0, Some("OH")),
        ],
        bonds: vec![bond(0, 0, 1, 1.0)],
        color: None,
        atom_shade: None,
        bond_shade: None,
        mark_atoms: vec![],
        mark_bonds: vec![],
        bold_labels: false,
    };
    let thin = depict_molecule(&mol);
    mol.bold_labels = true;
    let thick = depict_molecule(&mol);
    let d_thin = label_path_d(&thin, "OH").or_else(|| label_path_d(&thin, "HO"));
    let d_thick = label_path_d(&thick, "OH").or_else(|| label_path_d(&thick, "HO"));
    assert!(d_thin.is_some() && d_thick.is_some());
    assert_ne!(
        d_thin, d_thick,
        "bold_labels must thicken label glyph outlines, not only bonds"
    );
}

#[test]
fn formal_charge_on_nh2_and_silent_carbon() {
    let mol = MoleculeIn {
        id: None,
        atoms: vec![
            atom_charged(0, "C", 0.0, 0.0, None, 1),
            atom_charged(1, "N", 20.0, 0.0, Some("NH2"), 1),
            atom_charged(2, "N", 40.0, 0.0, Some("N+"), 1),
        ],
        bonds: vec![bond(0, 0, 1, 1.0), bond(1, 1, 2, 1.0)],
        color: None,
        atom_shade: None,
        bond_shade: None,
        mark_atoms: vec![],
        mark_bonds: vec![],
        bold_labels: false,
    };
    let texts = label_texts(&depict_molecule(&mol));
    assert!(
        texts.iter().any(|t| t.contains('C') && (t.contains('⁺') || t.contains('+'))),
        "charged C should show C⁺, got {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t.contains('₂') && (t.contains('⁺') || t.contains('+'))),
        "NH2+charge → NH₂⁺, got {texts:?}"
    );
    // Label already has +; AtomIn.charge must not double it.
    assert!(
        texts.iter().any(|t| t == "N⁺" || t == "N+"),
        "N+ must not become N++, got {texts:?}"
    );
    assert!(
        texts.iter().all(|t| !t.contains("++") && !t.contains("⁺⁺")),
        "no double charge, got {texts:?}"
    );
}
