//! In-core EdgePlan processor: Chematic parse + CoordGen layout/align.
//!
//! Same role as the JS / `xpict` host processors, but never leaves Rust.

use std::collections::HashMap;

use chematic::core::{implicit_hcount, AtomIdx, BondOrder, Molecule};
use chematic::mol::{parse_mol, parse_mol_v3000};
use chematic::smiles::{self, parse_cxsmiles};

use crate::align_layout::layout_or_align;
use crate::cxsmiles::smiles_base;
use crate::edge::{
    CoordGenMoleculeResult, CoordMethod, EdgePlan, EdgeResult, EdgeTask, EdgeTaskResult,
    MolTemplate, MIN_MCS_ATOMS,
};
use crate::elements::element_symbol;
use crate::labels::atom_display_label;
use crate::layout_coordgen::{LayoutMolecule, LayoutResult};
use crate::metrics::SCALE;
use crate::scene::{AtomIn, BondIn, MoleculeIn};

#[derive(Clone)]
struct Parsed {
    layout: LayoutMolecule,
    atom_indices: Vec<i32>,
    paint_orders: Vec<f64>,
    /// Chematic implicit (or bracket-stored) H counts per atom.
    implicit_hs: Vec<u32>,
    /// Chematic atom-map numbers (`:n` in reaction SMILES), parallel to atoms.
    atom_mapnos: Vec<Option<u32>>,
}

fn ctab_bond_order(order: BondOrder, a_arom: bool, b_arom: bool) -> i32 {
    match order {
        BondOrder::Aromatic => 4,
        BondOrder::Double => 2,
        BondOrder::Triple => 3,
        BondOrder::Quadruple => 4,
        BondOrder::Zero => 0,
        BondOrder::Single | BondOrder::Up | BondOrder::Down | BondOrder::Dative
            if a_arom && b_arom =>
        {
            4
        }
        _ => 1,
    }
}

fn molecule_to_layout(mol: &Molecule) -> Result<Parsed, String> {
    let n = mol.atom_count();
    if n == 0 {
        return Err("empty molecule".into());
    }
    let mut atomic_numbers = Vec::with_capacity(n);
    let mut formal_charges = Vec::with_capacity(n);
    let mut implicit_hs = Vec::with_capacity(n);
    let mut atom_mapnos = Vec::with_capacity(n);
    let mut arom = Vec::with_capacity(n);
    for i in 0..n {
        let idx = AtomIdx(i as u32);
        let atom = mol.atom(idx);
        let z = if atom.wildcard {
            0
        } else {
            i32::from(atom.element.atomic_number())
        };
        atomic_numbers.push(z);
        formal_charges.push(i32::from(atom.charge));
        implicit_hs.push(u32::from(implicit_hcount(mol, idx)));
        atom_mapnos.push(atom.atom_map.map(u32::from));
        arom.push(atom.aromatic);
    }
    let mut bond_from = Vec::new();
    let mut bond_to = Vec::new();
    let mut bond_orders = Vec::new();
    let mut paint_orders = Vec::new();
    for (_bidx, bond) in mol.bonds() {
        let a = bond.atom1.0 as usize;
        let b = bond.atom2.0 as usize;
        let order = ctab_bond_order(
            bond.order,
            arom.get(a).copied().unwrap_or(false),
            arom.get(b).copied().unwrap_or(false),
        );
        bond_from.push(a as i32);
        bond_to.push(b as i32);
        bond_orders.push(order);
        paint_orders.push(order as f64);
    }
    let nb = bond_from.len();
    Ok(Parsed {
        layout: LayoutMolecule {
            atomic_numbers,
            formal_charges,
            bond_from,
            bond_to,
            bond_orders,
            bond_stereos: vec![0; nb],
            ..Default::default()
        },
        atom_indices: (0..n as i32).collect(),
        paint_orders,
        implicit_hs,
        atom_mapnos,
    })
}

fn parse_source(source: &str) -> Result<Parsed, String> {
    let text = source.trim();
    if text.is_empty() {
        return Err("empty structure".into());
    }
    if text.contains("V2000") || text.contains("V3000") || text.lines().count() >= 4 {
        let mol = if text.contains("V3000") {
            parse_mol_v3000(text)
                .map(|(m, _)| m)
                .map_err(|e| format!("molfile V3000: {e}"))?
        } else {
            parse_mol(text)
                .map(|(m, _)| m)
                .map_err(|e| format!("molfile: {e}"))?
        };
        return molecule_to_layout(&mol);
    }
    let mol = if text.contains('|') {
        parse_cxsmiles(text)
            .map(|cx| cx.mol)
            .map_err(|e| format!("cxsmiles: {e}"))?
    } else {
        smiles::parse(&smiles_base(text)).map_err(|e| format!("smiles: {e}"))?
    };
    molecule_to_layout(&mol)
}

fn scale_to_paint(coords: &LayoutResult, bonds: &[(i32, i32)]) -> (Vec<f64>, Vec<f64>) {
    let mut lens = Vec::new();
    for &(a, b) in bonds {
        let i = a as usize;
        let j = b as usize;
        if i < coords.x.len() && j < coords.x.len() {
            let dx = f64::from(coords.x[i] - coords.x[j]);
            let dy = f64::from(coords.y[i] - coords.y[j]);
            lens.push((dx * dx + dy * dy).sqrt());
        }
    }
    let mean = if lens.is_empty() {
        1.0
    } else {
        lens.iter().sum::<f64>() / lens.len() as f64
    };
    let s = SCALE / mean.max(1e-6);
    let ymax = coords.y.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let xs = coords.x.iter().map(|&x| f64::from(x) * s).collect();
    let ys = coords.y.iter().map(|&y| f64::from(ymax - y) * s).collect();
    (xs, ys)
}

fn to_molecule_in(id: &str, parsed: &Parsed, coords: &LayoutResult) -> MoleculeIn {
    let bonds_idx: Vec<(i32, i32)> = parsed
        .layout
        .bond_from
        .iter()
        .zip(parsed.layout.bond_to.iter())
        .map(|(&a, &b)| (a, b))
        .collect();
    let (xs, ys) = scale_to_paint(coords, &bonds_idx);
    let atoms: Vec<AtomIn> = (0..parsed.layout.n_atoms())
        .map(|i| {
            let z = parsed.layout.atomic_numbers[i] as u32;
            let charge = parsed.layout.formal_charges[i];
            let element = element_symbol(z).to_string();
            let hs = parsed.implicit_hs.get(i).copied().unwrap_or(0);
            // Charge stays on `AtomIn.charge`; `display_label` appends the
            // suffix so we do not bake it into the string twice.
            let label = atom_display_label(&element, hs, 0);
            AtomIn {
                index: parsed.atom_indices.get(i).copied().unwrap_or(i as i32),
                element: Some(element),
                z: Some(z),
                x: xs.get(i).copied().unwrap_or(0.0),
                y: ys.get(i).copied().unwrap_or(0.0),
                label,
                charge,
                mapno: parsed.atom_mapnos.get(i).copied().flatten(),
            }
        })
        .collect();
    let bonds: Vec<BondIn> = (0..parsed.layout.n_bonds())
        .map(|i| BondIn {
            index: i as i32,
            begin: parsed.layout.bond_from[i],
            end: parsed.layout.bond_to[i],
            order: parsed
                .paint_orders
                .get(i)
                .copied()
                .unwrap_or(parsed.layout.bond_orders[i] as f64),
            stereo: None,
            interior: None,
        })
        .collect();
    MoleculeIn {
        id: Some(id.to_string()),
        atoms,
        bonds,
        color: None,
        atom_shade: None,
        bond_shade: None,
        shade_vmin: None,
        shade_vmax: None,
        mark_atoms: vec![],
        mark_bonds: vec![],
        scale: 1.0,
        weight: 1.0,
        aromatic: crate::scene::AromaticStyle::default(),
        debug_box: false,
        debug_align: false,
        align_debug: None,
    }
}

fn frame_json(parsed: &Parsed, coords: &LayoutResult) -> String {
    let atoms: Vec<serde_json::Value> = (0..parsed.layout.n_atoms())
        .map(|i| {
            let z = parsed.layout.atomic_numbers[i] as u32;
            serde_json::json!({
                "index": parsed.atom_indices.get(i).copied().unwrap_or(i as i32),
                "element": element_symbol(z),
                "x": coords.x.get(i).copied().unwrap_or(0.0),
                "y": coords.y.get(i).copied().unwrap_or(0.0),
                "charge": parsed.layout.formal_charges[i],
            })
        })
        .collect();
    let bonds: Vec<serde_json::Value> = (0..parsed.layout.n_bonds())
        .map(|i| {
            serde_json::json!({
                "index": i,
                "begin": parsed.layout.bond_from[i],
                "end": parsed.layout.bond_to[i],
                "order": parsed.paint_orders.get(i).copied().unwrap_or(1.0),
            })
        })
        .collect();
    serde_json::json!({ "atoms": atoms, "bonds": bonds }).to_string()
}

fn with_coords(mut layout: LayoutMolecule, coords: &LayoutResult) -> LayoutMolecule {
    layout.input_x = Some(coords.x.clone());
    layout.input_y = Some(coords.y.clone());
    layout.has_template_match = None;
    layout.is_atom_fixed = None;
    layout
}

#[derive(Clone)]
struct Pose {
    parsed: Parsed,
    layout_ref: LayoutMolecule,
    molecule: MoleculeIn,
    frame: String,
}

fn free_layout(source: &str, id: &str) -> Result<Pose, String> {
    let parsed = parse_source(source)?;
    let (coords, _) = layout_or_align(&parsed.layout, &parsed.atom_indices, None, None, None, None)
        .map_err(|e| e.to_string())?;
    let molecule = to_molecule_in(id, &parsed, &coords);
    let frame = frame_json(&parsed, &coords);
    let layout_ref = with_coords(parsed.layout.clone(), &coords);
    Ok(Pose {
        parsed,
        layout_ref,
        molecule,
        frame,
    })
}

fn align_layout(
    source: &str,
    id: &str,
    parent: &Pose,
    parent_id: &str,
    atom_map: Option<&[(u32, u32)]>,
) -> Result<(Pose, CoordMethod), String> {
    let parsed = parse_source(source)?;
    let dense: Option<Vec<(i32, i32)>> =
        atom_map.map(|pairs| pairs.iter().map(|&(q, r)| (q as i32, r as i32)).collect());
    let (coords, dbg) = layout_or_align(
        &parsed.layout,
        &parsed.atom_indices,
        Some(&parent.layout_ref),
        Some(&parent.parsed.atom_indices),
        dense.as_deref(),
        Some(parent_id),
    )
    .map_err(|e| e.to_string())?;
    let mut molecule = to_molecule_in(id, &parsed, &coords);
    molecule.align_debug = dbg;
    let method = if atom_map.is_some() {
        CoordMethod::AtomMap
    } else {
        CoordMethod::Mcs
    };
    let frame = frame_json(&parsed, &coords);
    let layout_ref = with_coords(parsed.layout.clone(), &coords);
    Ok((
        Pose {
            parsed,
            layout_ref,
            molecule,
            frame,
        },
        method,
    ))
}

/// Process [`EdgePlan`] with Chematic + CoordGen (parity with JS host).
pub fn process_edge_plan(plan: &EdgePlan) -> Result<EdgeResult, String> {
    Ok(process_edge_plan_with_frames(plan)?.0)
}

/// Like [`process_edge_plan`], plus pose frames keyed by mol id.
pub fn process_edge_plan_with_frames(
    plan: &EdgePlan,
) -> Result<(EdgeResult, HashMap<String, String>), String> {
    plan.validate()?;
    let mut task_results = Vec::new();
    let mut all_frames = HashMap::new();

    for task in &plan.tasks {
        match task {
            EdgeTask::CoordGen { roots } => {
                let mut rows = Vec::new();
                let mut poses: HashMap<String, Pose> = HashMap::new();

                for root in roots {
                    visit(root, None, &mut rows, &mut poses)?;
                }

                for (id, pose) in &poses {
                    all_frames.insert(id.clone(), pose.frame.clone());
                }
                let ok = rows.iter().all(|r| r.ok);
                task_results.push(EdgeTaskResult::CoordGen {
                    ok,
                    molecules: rows,
                });
            }
        }
    }

    Ok((EdgeResult::new_v1(task_results), all_frames))
}

fn visit(
    node: &MolTemplate,
    parent_id: Option<&str>,
    rows: &mut Vec<CoordGenMoleculeResult>,
    poses: &mut HashMap<String, Pose>,
) -> Result<(), String> {
    let source = node.source()?;
    let min_atoms = node
        .align
        .as_ref()
        .and_then(|a| a.min_atoms)
        .unwrap_or(MIN_MCS_ATOMS);
    let atom_map = node.align.as_ref().and_then(|a| a.atom_map.clone());

    match parent_id {
        None => match free_layout(source, &node.id) {
            Ok(pose) => {
                rows.push(CoordGenMoleculeResult {
                    id: node.id.clone(),
                    ok: true,
                    method: CoordMethod::Free,
                    used_map: None,
                    molecule: Some(pose.molecule.clone()),
                    error: None,
                });
                poses.insert(node.id.clone(), pose);
            }
            Err(e) => {
                rows.push(CoordGenMoleculeResult {
                    id: node.id.clone(),
                    ok: false,
                    method: CoordMethod::None,
                    used_map: None,
                    molecule: None,
                    error: Some(e),
                });
                return Ok(());
            }
        },
        Some(pid) => {
            let Some(parent) = poses.get(pid).cloned() else {
                match free_layout(source, &node.id) {
                    Ok(pose) => {
                        rows.push(CoordGenMoleculeResult {
                            id: node.id.clone(),
                            ok: true,
                            method: CoordMethod::None,
                            used_map: None,
                            molecule: Some(pose.molecule.clone()),
                            error: Some(format!(
                                "missing parent pose {pid}; fell back to unaligned coord gen"
                            )),
                        });
                        poses.insert(node.id.clone(), pose);
                    }
                    Err(e) => {
                        rows.push(CoordGenMoleculeResult {
                            id: node.id.clone(),
                            ok: false,
                            method: CoordMethod::None,
                            used_map: None,
                            molecule: None,
                            error: Some(e),
                        });
                    }
                }
                for child in &node.template_for {
                    visit(child, Some(node.id.as_str()), rows, poses)?;
                }
                return Ok(());
            };
            let map_ok = atom_map
                .as_ref()
                .map(|m| m.len() as u32 >= min_atoms)
                .unwrap_or(true);
            let use_map = if map_ok { atom_map.as_deref() } else { None };

            match align_layout(source, &node.id, &parent, pid, use_map) {
                Ok((pose, method)) => {
                    rows.push(CoordGenMoleculeResult {
                        id: node.id.clone(),
                        ok: true,
                        method,
                        used_map: atom_map.clone(),
                        molecule: Some(pose.molecule.clone()),
                        error: None,
                    });
                    poses.insert(node.id.clone(), pose);
                }
                Err(_) => match free_layout(source, &node.id) {
                    Ok(pose) => {
                        rows.push(CoordGenMoleculeResult {
                            id: node.id.clone(),
                            ok: true,
                            method: CoordMethod::None,
                            used_map: None,
                            molecule: Some(pose.molecule.clone()),
                            error: Some("align failed; fell back to unaligned coord gen".into()),
                        });
                        poses.insert(node.id.clone(), pose);
                    }
                    Err(e) => {
                        rows.push(CoordGenMoleculeResult {
                            id: node.id.clone(),
                            ok: false,
                            method: CoordMethod::None,
                            used_map: None,
                            molecule: None,
                            error: Some(e),
                        });
                        return Ok(());
                    }
                },
            }
        }
    }

    for child in &node.template_for {
        visit(child, Some(node.id.as_str()), rows, poses)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::align_cost::{cost_mapping, mapping_cost};
    use crate::align_layout::mol_graph_from_layout;
    use crate::edge::EdgeTask;
    use std::collections::HashMap;

    #[test]
    fn toluene_anisole_real_smiles_mcs_costs() {
        let anisole = parse_source("COc1ccccc1").unwrap();
        let toluene = parse_source("Cc1ccccc1").unwrap();
        eprintln!(
            "anisole bond_orders={:?} n_arom4={}",
            anisole.layout.bond_orders,
            anisole
                .layout
                .bond_orders
                .iter()
                .filter(|&&o| o >= 4)
                .count()
        );
        eprintln!(
            "toluene bond_orders={:?} n_arom4={}",
            toluene.layout.bond_orders,
            toluene
                .layout
                .bond_orders
                .iter()
                .filter(|&&o| o >= 4)
                .count()
        );
        let ref_g = mol_graph_from_layout(&anisole.layout);
        let query_g = mol_graph_from_layout(&toluene.layout);
        eprintln!(
            "anisole bond.aromatic={:?}",
            ref_g.bonds.iter().map(|b| b.aromatic).collect::<Vec<_>>()
        );
        eprintln!(
            "toluene bond.aromatic={:?}",
            query_g.bonds.iter().map(|b| b.aromatic).collect::<Vec<_>>()
        );

        // Production shifted map (methyl → ring).
        let bad: HashMap<u16, u16> = [(0, 3), (1, 2), (2, 7), (3, 6), (4, 5), (5, 4)]
            .into_iter()
            .collect();
        let bad_c = mapping_cost(&query_g, &ref_g, &bad);
        let res = cost_mapping(&ref_g, &query_g).expect("map");
        eprintln!("cost_mapping cost={} map={:?}", res.cost, res.map);
        eprintln!("bad_map cost={}", bad_c);
        assert!(
            res.cost <= bad_c,
            "search should not prefer shifted map: got {} > bad {}",
            res.cost,
            bad_c
        );
        // Pendant Me maps to O (exo atom on the ring ipso), not a ring carbon.
        assert_eq!(
            res.map.get(&0),
            Some(&1),
            "toluene methyl should map to anisole oxygen; map={:?}",
            res.map
        );
    }

    /// `mol("CCCCCC") + mol("C1CCCCC1CCCCC")` must align exactly like the
    /// same pentylcyclohexane spelled `CCCCCC1CCCCC1`: SMILES atom order is
    /// not chemistry, so the optimum cost and the chain coverage must agree.
    #[test]
    fn hexane_pentylcyclohexane_alignment_is_order_independent() {
        let hexane = mol_graph_from_layout(&parse_source("CCCCCC").unwrap().layout);
        let ring_first = mol_graph_from_layout(&parse_source("C1CCCCC1CCCCC").unwrap().layout);
        let chain_first = mol_graph_from_layout(&parse_source("CCCCCC1CCCCC1").unwrap().layout);

        // Hexane must lie along ipso + pentyl chain — never wrapped around
        // the cyclohexane (a zero-bond-cost tie that only atom order broke).
        // Ring atoms: indices 0..6 in ring-first, 5..11 in chain-first.
        let ring_first_ring = |a: u16| a < 6;
        let chain_first_ring = |a: u16| a >= 5;
        let ring_hits = |m: &HashMap<u16, u16>, keys: bool, in_ring: &dyn Fn(u16) -> bool| {
            m.iter()
                .filter(|(&q, &r)| in_ring(if keys { q } else { r }))
                .count()
        };

        let rf = cost_mapping(&ring_first, &hexane).expect("map");
        let cf = cost_mapping(&chain_first, &hexane).expect("map");
        assert_eq!(rf.cost, cf.cost, "ring_first {rf:?} vs chain_first {cf:?}");
        assert_eq!(rf.map.len(), 6, "ring_first: hexane must fully map; {rf:?}");
        assert_eq!(
            cf.map.len(),
            6,
            "chain_first: hexane must fully map; {cf:?}"
        );
        assert_eq!(
            ring_hits(&rf.map, false, &ring_first_ring),
            1,
            "ring_first {rf:?}"
        );
        assert_eq!(
            ring_hits(&cf.map, false, &chain_first_ring),
            1,
            "chain_first {cf:?}"
        );

        // `mol(hexane) + mol(pentylcyclohexane)`: hexane is the align ref.
        let rf = cost_mapping(&hexane, &ring_first).expect("map");
        let cf = cost_mapping(&hexane, &chain_first).expect("map");
        assert_eq!(rf.cost, cf.cost, "ring_first {rf:?} vs chain_first {cf:?}");
        assert_eq!(rf.map.len(), 6, "ring_first: six atoms should map; {rf:?}");
        assert_eq!(cf.map.len(), 6, "chain_first: six atoms should map; {cf:?}");
        assert_eq!(
            ring_hits(&rf.map, true, &ring_first_ring),
            1,
            "ring_first {rf:?}"
        );
        assert_eq!(
            ring_hits(&cf.map, true, &chain_first_ring),
            1,
            "chain_first {cf:?}"
        );
    }

    #[test]
    fn process_free_ethanol() {
        let plan = EdgePlan::new_v1(vec![EdgeTask::CoordGen {
            roots: vec![MolTemplate {
                id: "m_0".into(),
                smiles: Some("CCO".into()),
                cxsmiles: None,
                molfile: None,
                align: None,
                template_for: vec![],
            }],
        }]);
        let (edge, frames) = process_edge_plan_with_frames(&plan).unwrap();
        let EdgeTaskResult::CoordGen { molecules, ok } = &edge.results[0];
        assert!(ok);
        assert_eq!(molecules[0].molecule.as_ref().unwrap().atoms.len(), 3);
        assert!(frames.contains_key("m_0"));
    }

    /// Chematic→paint must append implicit H on heteroatoms (OH / NH2), not
    /// bare element symbols — matches RDKit host `atom_label`.
    #[test]
    fn chematic_labels_include_implicit_h_on_n_and_o() {
        let cases: &[(&str, &[&str])] = &[
            ("CCO", &["OH"]),
            ("CCN", &["NH2"]),
            ("c1ccccc1O", &["OH"]),
            ("c1ccccc1N", &["NH2"]),
            ("[NH4+]", &["NH4"]),
            ("[OH-]", &["OH"]),
        ];
        for &(smiles, want_labels) in cases {
            let mol = free_layout(smiles, "m").unwrap().molecule;
            let got: Vec<String> = mol.atoms.iter().filter_map(|a| a.label.clone()).collect();
            for want in want_labels {
                assert!(
                    got.iter().any(|g| g == *want),
                    "{smiles}: expected label {want:?} among {got:?}"
                );
            }
            // Bare heteroatom without H is wrong when H is present.
            if want_labels.iter().any(|w| w.contains('H')) {
                assert!(
                    !got.iter().any(|g| g == "O" || g == "N"),
                    "{smiles}: bare O/N label without H, got {got:?}"
                );
            }
        }
    }
}
