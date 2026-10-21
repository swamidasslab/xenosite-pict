//! In-core EdgePlan processor: Chematic parse + CoordGen layout/align.
//!
//! Same role as the JS / `xpict` host processors, but never leaves Rust.

use std::collections::HashMap;

use chematic::core::{implicit_hcount, AtomIdx, BondOrder, Molecule};
use chematic::mol::{parse_mol, parse_mol_v3000};
use chematic::smiles::{self, parse_cxsmiles};

use crate::cxsmiles::{cx_multicenters, smiles_base};
use crate::edge::{
    AlignMode, AlignOpts, CoordGenMoleculeResult, CoordMethod, EdgePlan, EdgeResult, EdgeTask,
    EdgeTaskResult, MolTemplate, MIN_MCS_ATOMS,
};
use crate::elements::element_symbol;
use crate::labels::atom_display_label;
use crate::layout_coordgen::{LayoutMolecule, LayoutResult};
use crate::layout_spoke::layout_or_align_with_spokes;
use crate::metrics::SCALE;
use crate::scene::{AtomIn, Attach, BondIn, MoleculeIn, SpokeIn};
use crate::spokes::detect_ring_spokes;

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

fn to_molecule_in(id: &str, parsed: &Parsed, coords: &LayoutResult, source: &str) -> MoleculeIn {
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
    let groups = cx_multicenters(source);
    let comps = detect_ring_spokes(parsed.layout.n_atoms(), &bonds_idx, &groups);
    let dummy_of: HashMap<i32, &crate::spokes::SpokeComp> =
        comps.iter().map(|c| (c.dummy, c)).collect();
    let mut atoms = atoms;
    for a in &mut atoms {
        if dummy_of.contains_key(&a.index) {
            a.label = Some(String::new());
        }
    }
    let endpt_of_dummy =
        |d: i32, other: i32| dummy_of.get(&d).is_some_and(|c| c.endpts.contains(&other));
    let mut bonds: Vec<BondIn> = Vec::new();
    for i in 0..parsed.layout.n_bonds() {
        let begin = parsed.layout.bond_from[i];
        let end = parsed.layout.bond_to[i];
        if endpt_of_dummy(begin, end) || endpt_of_dummy(end, begin) {
            continue;
        }
        bonds.push(BondIn {
            index: i as i32,
            begin,
            end,
            order: parsed
                .paint_orders
                .get(i)
                .copied()
                .unwrap_or(parsed.layout.bond_orders[i] as f64),
            stereo: None,
            interior: None,
        });
    }
    let spokes: Vec<SpokeIn> = comps
        .iter()
        .map(|c| SpokeIn {
            dummy: Some(c.dummy),
            atom: c.atom,
            endpts: c.endpts.clone(),
            attach: Attach::Any,
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
        spokes,
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
    let (coords, _) = layout_or_align_with_spokes(
        &parsed.layout,
        &parsed.atom_indices,
        source,
        None,
        None,
        None,
        None,
        None,
    )
    .map_err(|e| e.to_string())?;
    let molecule = to_molecule_in(id, &parsed, &coords, source);
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
    align: Option<&AlignOpts>,
) -> Result<(Pose, CoordMethod), String> {
    let parsed = parse_source(source)?;
    let dense: Option<Vec<(i32, i32)>> =
        atom_map.map(|pairs| pairs.iter().map(|&(q, r)| (q as i32, r as i32)).collect());
    let (coords, dbg) = layout_or_align_with_spokes(
        &parsed.layout,
        &parsed.atom_indices,
        source,
        Some(&parent.layout_ref),
        Some(&parent.parsed.atom_indices),
        dense.as_deref(),
        Some(parent_id),
        align,
    )
    .map_err(|e| e.to_string())?;
    let mut molecule = to_molecule_in(id, &parsed, &coords, source);
    molecule.align_debug = dbg;
    let method = if atom_map.is_some() {
        CoordMethod::AtomMap
    } else if matches!(align.map(AlignOpts::align_mode), Some(AlignMode::Greedy)) {
        CoordMethod::Greedy
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
    let mode = node
        .align
        .as_ref()
        .map(AlignOpts::align_mode)
        .unwrap_or(AlignMode::Exact);

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
        Some(_) if mode == AlignMode::Skip => match free_layout(source, &node.id) {
            Ok(pose) => {
                rows.push(CoordGenMoleculeResult {
                    id: node.id.clone(),
                    ok: true,
                    method: CoordMethod::Skip,
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

            match align_layout(source, &node.id, &parent, pid, use_map, node.align.as_ref()) {
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

    fn tmpl(
        id: &str,
        smiles: &str,
        align: Option<AlignOpts>,
        kids: Vec<MolTemplate>,
    ) -> MolTemplate {
        MolTemplate {
            id: id.into(),
            smiles: Some(smiles.into()),
            cxsmiles: None,
            molfile: None,
            align,
            template_for: kids,
        }
    }

    #[test]
    fn skip_mode_free_layouts_child() {
        let plan = EdgePlan::new_v1(vec![EdgeTask::CoordGen {
            roots: vec![tmpl(
                "a",
                "c1ccccc1",
                None,
                vec![tmpl(
                    "b",
                    "Cc1ccccc1",
                    Some(AlignOpts {
                        mode: Some(AlignMode::Skip),
                        ..Default::default()
                    }),
                    vec![],
                )],
            )],
        }]);
        let edge = process_edge_plan(&plan).unwrap();
        let EdgeTaskResult::CoordGen { molecules, ok } = &edge.results[0];
        assert!(ok);
        assert_eq!(molecules[0].method, CoordMethod::Free);
        assert_eq!(molecules[1].method, CoordMethod::Skip);
        assert!(molecules[1]
            .molecule
            .as_ref()
            .unwrap()
            .align_debug
            .is_none());
    }

    #[test]
    fn greedy_mode_aligns_toluene_to_anisole() {
        let plan = EdgePlan::new_v1(vec![EdgeTask::CoordGen {
            roots: vec![tmpl(
                "a",
                "COc1ccccc1",
                None,
                vec![tmpl(
                    "b",
                    "Cc1ccccc1",
                    Some(AlignOpts {
                        mode: Some(AlignMode::Greedy),
                        timeout_ms: Some(0),
                        ..Default::default()
                    }),
                    vec![],
                )],
            )],
        }]);
        let edge = process_edge_plan(&plan).unwrap();
        let EdgeTaskResult::CoordGen { molecules, ok } = &edge.results[0];
        assert!(ok);
        assert_eq!(molecules[1].method, CoordMethod::Greedy);
        let tags = molecules[1]
            .molecule
            .as_ref()
            .unwrap()
            .align_debug
            .as_ref()
            .expect("greedy should stamp align_debug")
            .tags
            .len();
        assert!(tags >= MIN_MCS_ATOMS as usize, "tags={tags}");
    }

    #[test]
    fn atom_map_still_skips_mcs() {
        let plan = EdgePlan::new_v1(vec![EdgeTask::CoordGen {
            roots: vec![tmpl(
                "a",
                "CCO",
                None,
                vec![tmpl(
                    "b",
                    "CCO",
                    Some(AlignOpts {
                        atom_map: Some(vec![(0, 0), (1, 1), (2, 2)]),
                        timeout_ms: Some(1),
                        ..Default::default()
                    }),
                    vec![],
                )],
            )],
        }]);
        let edge = process_edge_plan(&plan).unwrap();
        let EdgeTaskResult::CoordGen { molecules, ok } = &edge.results[0];
        assert!(ok);
        assert_eq!(molecules[1].method, CoordMethod::AtomMap);
    }

    fn xy(mol: &MoleculeIn, i: i32) -> (f64, f64) {
        let a = mol.atom(i).expect("atom");
        (a.x, a.y)
    }

    #[test]
    fn cc_star_on_benzene_has_chain_kink() {
        let mol = free_layout("CC*.c1ccccc1 |m:2:3.4.5.6.7.8|", "s")
            .unwrap()
            .molecule;
        assert_eq!(mol.spokes.len(), 1, "expected one ring-center spoke");
        let sp = &mol.spokes[0];
        assert_eq!(sp.atom, 1);
        let c = mol.centroid_of(&sp.endpts).expect("centroid");
        let attach = xy(&mol, sp.atom);
        let methyl = xy(&mol, 0);
        let turn = crate::spokes::kink_turn(c, attach, methyl);
        assert!(
            turn > 0.7 && turn < 1.5,
            "CC* should zigzag at CH2, not sit collinear; turn={turn:.3} rad"
        );
        let dummy = mol.atom(sp.dummy.expect("dummy")).unwrap();
        assert_eq!(dummy.label.as_deref(), Some(""));
        let disk = 0.25 * crate::metrics::BOND_PX;
        for &e in &sp.endpts {
            let (x, y) = xy(&mol, e);
            let d = crate::spokes::point_segment_dist(x, y, c.0, c.1, attach.0, attach.1);
            assert!(
                d + 1e-6 >= disk,
                "stem crosses ring atom {e}: d={d:.2} disk={disk:.2}"
            );
        }
        let ring: Vec<(i32, f64, f64)> = sp
            .endpts
            .iter()
            .map(|&i| {
                let (x, y) = xy(&mol, i);
                (i, x, y)
            })
            .collect();
        let bonds: Vec<(i32, i32)> = mol.bonds.iter().map(|b| (b.begin, b.end)).collect();
        let bis = crate::spokes::bisector_angles(c, &ring, &bonds);
        let stem_ang = (attach.1 - c.1).atan2(attach.0 - c.0);
        let best = bis
            .iter()
            .map(|&b| crate::spokes::ang_diff(stem_ang, b))
            .fold(f64::INFINITY, f64::min);
        assert!(
            best < 0.4,
            "stem should follow a ring-bond bisector, min Δθ={best:.3}"
        );
    }

    fn min_dist_to_ring(mol: &MoleculeIn, attach: (f64, f64), endpts: &[i32]) -> f64 {
        let set: HashMap<i32, (f64, f64)> = endpts
            .iter()
            .filter_map(|&i| mol.atom(i).map(|a| (i, (a.x, a.y))))
            .collect();
        let mut best = f64::INFINITY;
        for b in &mol.bonds {
            let Some(&pa) = set.get(&b.begin) else {
                continue;
            };
            let Some(&pb) = set.get(&b.end) else {
                continue;
            };
            best = best.min(crate::spokes::point_segment_dist(
                attach.0, attach.1, pa.0, pa.1, pb.0, pb.1,
            ));
        }
        best
    }

    fn aromatic_radii(scene: &crate::scene::Scene) -> Vec<f64> {
        scene
            .viewports
            .iter()
            .flat_map(|v| v.layers.iter())
            .flat_map(|l| l.primitives.iter())
            .filter_map(|p| match p {
                crate::scene::Primitive::Circle { r, class, .. }
                    if class.as_deref().is_some_and(|c| c.contains("aromatic")) =>
                {
                    Some(*r)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn o_star_sits_one_bond_outside_ring_and_keeps_aromatic_circle() {
        let source = "O*.c1ccccc1 |m:1:2.3.4.5.6.7|";
        let mol = free_layout(source, "s").unwrap().molecule;
        assert_eq!(mol.spokes.len(), 1);
        let sp = &mol.spokes[0];
        let c = mol.centroid_of(&sp.endpts).expect("centroid");
        let attach = xy(&mol, sp.atom);
        let ring: Vec<(i32, f64, f64)> = sp
            .endpts
            .iter()
            .map(|&i| {
                let (x, y) = xy(&mol, i);
                (i, x, y)
            })
            .collect();
        let bonds: Vec<(i32, i32)> = mol.bonds.iter().map(|b| (b.begin, b.end)).collect();
        let ang = (attach.1 - c.1).atan2(attach.0 - c.0);
        let exit = crate::spokes::ring_exit_along_ray(c, ang, &ring, &bonds);
        let from_c = (attach.0 - c.0).hypot(attach.1 - c.1);
        let from_ring = min_dist_to_ring(&mol, attach, &sp.endpts);
        let bond = crate::metrics::BOND_PX;
        assert!(
            from_c > 1.4 * bond,
            "O must sit past the ring, not on it: from_c={from_c:.1} exit={exit:.1}"
        );
        assert!(
            (from_c - (exit + bond)).abs() < 0.35 * bond,
            "stem should be ring-exit + 1 bond: from_c={from_c:.1} want={:.1}",
            exit + bond
        );
        assert!(
            from_ring > 0.7 * bond && from_ring < 1.4 * bond,
            "O should be ~1 bond off the ring, got {from_ring:.1} (bond={bond:.1})"
        );
        let dummy = xy(&mol, sp.dummy.expect("dummy"));
        let dummy_from_c = (dummy.0 - c.0).hypot(dummy.1 - c.1);
        assert!(
            dummy_from_c < 0.2 * bond,
            "stem dummy must sit at the centroid, d={dummy_from_c:.1}"
        );

        let spec: crate::doc::DepictSpec = serde_json::from_value(serde_json::json!({
            "type": "mol",
            "smiles": source,
        }))
        .unwrap();
        let painted = crate::doc::depict(&spec).unwrap();
        let rs = aromatic_radii(&painted[0].scene);
        assert!(
            rs.iter().any(|&r| r > 0.25 * bond),
            "aromatic circle vanished (radii={rs:?})"
        );
    }

    #[test]
    fn cl_star_on_ethylbenzene_stems_from_the_ring() {
        let mol = free_layout("Cl*.c1ccc(CC)cc1 |m:1:2.3.4.5.6.7|", "s")
            .unwrap()
            .molecule;
        assert_eq!(mol.spokes.len(), 1);
        let sp = &mol.spokes[0];
        assert_eq!(sp.atom, 0, "Cl is the substituent, not the dummy");
        let mut endpts = sp.endpts.clone();
        endpts.sort_unstable();
        assert_eq!(endpts, vec![2, 3, 4, 5, 8, 9]);
        let c = mol.centroid_of(&sp.endpts).expect("centroid");
        let dummy = xy(&mol, sp.dummy.expect("dummy"));
        let attach = xy(&mol, sp.atom);
        let bond = crate::metrics::BOND_PX;
        let dummy_from_c = (dummy.0 - c.0).hypot(dummy.1 - c.1);
        assert!(
            dummy_from_c < 0.2 * bond,
            "dummy must sit at the ring centroid, d={dummy_from_c:.1}"
        );
        let from_ring = min_dist_to_ring(&mol, attach, &sp.endpts);
        assert!(
            from_ring > 0.7 * bond && from_ring < 1.4 * bond,
            "Cl should be ~1 bond off the ring, got {from_ring:.1}"
        );
        let ethyl = xy(&mol, 6);
        let d_ethyl =
            crate::spokes::point_segment_dist(ethyl.0, ethyl.1, c.0, c.1, attach.0, attach.1);
        assert!(
            d_ethyl > 0.25 * bond,
            "stem must miss the ethyl, d={d_ethyl:.1}"
        );
    }

    #[test]
    fn star_cc_on_ethylbenzene_has_kink_at_first_carbon() {
        let mol = free_layout("c1ccccc1CC.*CC |m:8:0.1.2.3.4.5|", "s")
            .unwrap()
            .molecule;
        assert_eq!(mol.spokes.len(), 1);
        let sp = &mol.spokes[0];
        assert_eq!(sp.atom, 9);
        let c = mol.centroid_of(&sp.endpts).expect("centroid");
        let attach = xy(&mol, 9);
        let methyl = xy(&mol, 10);
        let turn = crate::spokes::kink_turn(c, attach, methyl);
        assert!(
            turn > 0.7 && turn < 1.5,
            "*CC must kink at the first carbon, turn={turn:.3} rad"
        );
        let dummy = xy(&mol, sp.dummy.expect("dummy"));
        let bond = crate::metrics::BOND_PX;
        assert!((dummy.0 - c.0).hypot(dummy.1 - c.1) < 0.2 * bond);
    }

    #[test]
    fn two_cc_star_spokes_take_different_slots() {
        let mol = free_layout("CC*.CC*.c1ccccc1 |m:2:6.7.8.9.10.11,5:6.7.8.9.10.11|", "s")
            .unwrap()
            .molecule;
        assert_eq!(mol.spokes.len(), 2);
        let c = mol.centroid_of(&mol.spokes[0].endpts).unwrap();
        let a0 = xy(&mol, mol.spokes[0].atom);
        let a1 = xy(&mol, mol.spokes[1].atom);
        let t0 = (a0.1 - c.1).atan2(a0.0 - c.0);
        let t1 = (a1.1 - c.1).atan2(a1.0 - c.0);
        assert!(
            crate::spokes::ang_diff(t0, t1) > 0.8,
            "two spokes must not share a slot: Δθ={:.3}",
            crate::spokes::ang_diff(t0, t1)
        );
        for sp in &mol.spokes {
            let attach = xy(&mol, sp.atom);
            let methyl_idx = if sp.atom == 1 { 0 } else { 3 };
            let methyl = xy(&mol, methyl_idx);
            let turn = crate::spokes::kink_turn(c, attach, methyl);
            assert!(
                turn > 0.7 && turn < 1.5,
                "spoke {} should zigzag at CH2, turn={turn:.3}",
                sp.atom
            );
        }
    }
}
