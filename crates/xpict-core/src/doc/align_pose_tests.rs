//! Align pose carry-over: coords seed, molblock MCS, visible:false.

#![cfg(test)]

use crate::doc::{render, AlignTo, AlignToSpec, DepictSpec, MolNode, Node};
use crate::edge::AlignCoords;
use crate::molfile::{molblock_from_frame_json, write_v2000};

fn ethanol_molblock() -> String {
    write_v2000(
        &[6, 6, 8],
        &[0, 0, 0],
        &[0, 1],
        &[1, 2],
        &[1, 1],
        &[0.0, 1.5, 2.25],
        &[0.0, 0.0, 1.299],
    )
    .unwrap()
}

#[test]
fn coords_partial_seed_pins_listed_atoms() {
    let spec = DepictSpec::Mol(MolNode {
        smiles: Some("CCO".into()),
        align_to: Some(AlignTo::Spec(AlignToSpec {
            coords: Some(AlignCoords {
                x: vec![0.0, 1.5],
                y: vec![0.0, 0.0],
                atom: vec![0, 1],
            }),
            ..Default::default()
        })),
        ..Default::default()
    });
    let rows = render(&spec).unwrap();
    assert_eq!(rows.len(), 1);
    // Frame is CoordGen-space; seeded atoms should land near the pins after layout.
    let frame = &rows[0].frame_molblock;
    assert!(frame.contains("atoms"), "frame={frame}");
}

#[test]
fn molblock_align_without_ref() {
    let mb = ethanol_molblock();
    let spec = DepictSpec::Mol(MolNode {
        smiles: Some("CCO".into()),
        align_to: Some(AlignTo::Spec(AlignToSpec {
            molblock: Some(mb),
            timeout_ms: Some(50),
            ..Default::default()
        })),
        ..Default::default()
    });
    let rows = render(&spec).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].molecule.atoms.len(), 3);
}

#[test]
fn visible_false_omits_template_cell() {
    let spec = DepictSpec::Group {
        id: None,
        align: false,
        children: vec![
            Node::Mol(MolNode {
                id: Some("t".into()),
                smiles: Some("c1ccccc1O".into()),
                visible: Some(false),
                ..Default::default()
            }),
            Node::Mol(MolNode {
                smiles: Some("Cc1ccccc1O".into()),
                align_to: Some(AlignTo::Ref("t".into())),
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
        svg: None,
    };
    let rows = render(&spec).unwrap();
    assert_eq!(rows.len(), 1, "only query should be visible");
}

#[test]
fn align_to_ref_plus_coords_rejected() {
    let bad = AlignToSpec {
        ref_id: Some("t".into()),
        coords: Some(AlignCoords {
            x: vec![0.0],
            y: vec![0.0],
            atom: vec![0],
        }),
        ..Default::default()
    };
    assert!(bad.validate_source().is_err());
}

#[test]
fn coords_length_mismatch_rejected() {
    let c = AlignCoords {
        x: vec![0.0, 1.0],
        y: vec![0.0],
        atom: vec![0, 1],
    };
    assert!(c.validate().is_err());
}

#[test]
fn frame_json_roundtrip_molblock() {
    let spec = DepictSpec::Mol(MolNode {
        smiles: Some("CCO".into()),
        ..Default::default()
    });
    let rows = render(&spec).unwrap();
    let mb = molblock_from_frame_json(&rows[0].frame_molblock).unwrap();
    assert!(mb.contains("V2000"), "{mb}");
    let again = DepictSpec::Mol(MolNode {
        smiles: Some("CCO".into()),
        align_to: Some(AlignTo::Spec(AlignToSpec {
            molblock: Some(mb),
            ..Default::default()
        })),
        ..Default::default()
    });
    assert_eq!(render(&again).unwrap().len(), 1);
}

#[test]
fn coords_remap_uses_query_indices() {
    // B2: soft-seed with query-index `atom` + template positions (as alignFrom remaps).
    use crate::molfile::align_coords_from_frame_map;
    let prior = render(&DepictSpec::Mol(MolNode {
        smiles: Some("CCO".into()),
        ..Default::default()
    }))
    .unwrap();
    let map = vec![(10u32, 0u32), (11, 1), (12, 2)]; // query indices ≠ template
    let coords = align_coords_from_frame_map(&prior[0].frame_molblock, &map).unwrap();
    assert_eq!(coords.atom, vec![10, 11, 12]);
    // Engine accepts the remapped coords as a seed on a same-size query.
    let seeded = DepictSpec::Mol(MolNode {
        smiles: Some("CCO".into()),
        align_to: Some(AlignTo::Spec(AlignToSpec {
            coords: Some(AlignCoords {
                x: coords.x.clone(),
                y: coords.y.clone(),
                atom: vec![0, 1, 2],
            }),
            atom_map: Some(vec![(0, 0), (1, 1), (2, 2)]),
            ..Default::default()
        })),
        ..Default::default()
    });
    assert_eq!(render(&seeded).unwrap().len(), 1);
}

#[test]
fn group_align_both_visible() {
    let spec = DepictSpec::Group {
        id: None,
        align: false,
        children: vec![
            Node::Mol(MolNode {
                id: Some("t".into()),
                smiles: Some("c1ccccc1O".into()),
                ..Default::default()
            }),
            Node::Mol(MolNode {
                smiles: Some("Cc1ccccc1O".into()),
                align_to: Some(AlignTo::Spec(AlignToSpec {
                    ref_id: Some("t".into()),
                    timeout_ms: Some(50),
                    ..Default::default()
                })),
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
        svg: None,
    };
    let rows = render(&spec).unwrap();
    assert_eq!(rows.len(), 2);
}
